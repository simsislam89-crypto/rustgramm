//! Ядро Rustgramm: клиент Telegram (MTProto через grammers).
//! Все адреса дата-центров проходят через `rg-net`: адрес вне сетей Telegram
//! не попадает ни в сессию, ни в сокет.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use grammers_session::Session;
use grammers_session::types::{DcOption, PeerId, PeerInfo, UpdateState, UpdatesState};
use grammers_session::{BoxFuture, storages::SqliteSession};
use rg_net::Guard;

/// Обёртка над любым хранилищем сессии: чужие адреса DC отбрасываются.
pub struct GuardedSession<S: Session> {
    inner: S,
    guard: Guard,
    blocked: AtomicUsize,
}

impl<S: Session> GuardedSession<S> {
    pub fn new(inner: S, guard: Guard) -> Self {
        Self {
            inner,
            guard,
            blocked: AtomicUsize::new(0),
        }
    }

    /// Сколько раз был отклонён адрес вне Telegram.
    pub fn blocked(&self) -> usize {
        self.blocked.load(Ordering::Relaxed)
    }

    fn dc_ok(&self, dc: &DcOption) -> bool {
        self.guard.ip_allowed((*dc.ipv4.ip()).into()) && self.guard.ip_allowed((*dc.ipv6.ip()).into())
    }
}

impl<S: Session> Session for GuardedSession<S> {
    type Error = S::Error;

    fn home_dc_id(&self) -> Result<i32, Self::Error> {
        self.inner.home_dc_id()
    }

    fn set_home_dc_id(&self, dc_id: i32) -> BoxFuture<'_, Result<(), Self::Error>> {
        self.inner.set_home_dc_id(dc_id)
    }

    fn dc_option(&self, dc_id: i32) -> Result<Option<DcOption>, Self::Error> {
        match self.inner.dc_option(dc_id)? {
            Some(dc) if !self.dc_ok(&dc) => {
                self.blocked.fetch_add(1, Ordering::Relaxed);
                Ok(None)
            }
            other => Ok(other),
        }
    }

    fn set_dc_option(&self, dc_option: &DcOption) -> BoxFuture<'_, Result<(), Self::Error>> {
        if !self.dc_ok(dc_option) {
            self.blocked.fetch_add(1, Ordering::Relaxed);
            return Box::pin(async { Ok(()) });
        }
        self.inner.set_dc_option(dc_option)
    }

    fn peer(&self, peer: PeerId) -> BoxFuture<'_, Result<Option<PeerInfo>, Self::Error>> {
        self.inner.peer(peer)
    }

    fn cache_peer(&self, peer: PeerInfo) -> BoxFuture<'_, Result<(), Self::Error>> {
        self.inner.cache_peer(peer)
    }

    fn cache_peers(&self, peers: Vec<PeerInfo>) -> BoxFuture<'_, Result<(), Self::Error>> {
        self.inner.cache_peers(peers)
    }

    fn updates_state(&self) -> BoxFuture<'_, Result<UpdatesState, Self::Error>> {
        self.inner.updates_state()
    }

    fn set_update_state(&self, update: UpdateState) -> BoxFuture<'_, Result<(), Self::Error>> {
        self.inner.set_update_state(update)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum ConfigError {
    Missing(&'static str),
    BadApiId,
    ProxyNotAllowed(String),
}

/// Параметры подключения. `api_id` и `api_hash` — свои, с my.telegram.org, в код не зашиваются.
#[derive(Debug, Clone)]
pub struct Config {
    pub api_id: i32,
    pub api_hash: String,
    pub session_path: String,
    /// `socks5://host:port` — только явный выбор пользователя.
    pub proxy_url: Option<String>,
}

impl Config {
    /// Читает `RG_API_ID`, `RG_API_HASH`, `RG_SESSION`, `RG_PROXY` из переменных окружения.
    pub fn from_env() -> Result<Self, ConfigError> {
        Self::from_lookup(|k| std::env::var(k).ok())
    }

    pub fn from_lookup(get: impl Fn(&str) -> Option<String>) -> Result<Self, ConfigError> {
        let api_id = get("RG_API_ID")
            .ok_or(ConfigError::Missing("RG_API_ID"))?
            .trim()
            .parse::<i32>()
            .map_err(|_| ConfigError::BadApiId)?;
        let api_hash = get("RG_API_HASH").ok_or(ConfigError::Missing("RG_API_HASH"))?;
        Ok(Self {
            api_id,
            api_hash: api_hash.trim().to_owned(),
            session_path: get("RG_SESSION").unwrap_or_else(|| "rustgramm.session".to_owned()),
            proxy_url: get("RG_PROXY").filter(|p| !p.trim().is_empty()),
        })
    }

    /// Хост прокси добавляется в охрану как явное решение пользователя.
    pub fn guard(&self) -> Result<Guard, ConfigError> {
        let mut guard = Guard::new();
        if let Some(p) = &self.proxy_url {
            let rest = p
                .strip_prefix("socks5://")
                .ok_or_else(|| ConfigError::ProxyNotAllowed(p.clone()))?;
            let host = rg_net::host_of(rest).ok_or_else(|| ConfigError::ProxyNotAllowed(p.clone()))?;
            guard.allow_user_host(&host);
        }
        Ok(guard)
    }
}

/// Открывает сессию на диске и оборачивает её стражем.
pub async fn open_session(
    cfg: &Config,
) -> Result<Arc<GuardedSession<SqliteSession>>, Box<dyn std::error::Error>> {
    let guard = cfg.guard().map_err(|e| format!("{e:?}"))?;
    let inner = SqliteSession::open(&cfg.session_path).await?;
    Ok(Arc::new(GuardedSession::new(inner, guard)))
}

/// Подключение: пул отправителей с защищённой сессией. Сведений об устройстве не раскрываем.
pub async fn connect(
    cfg: &Config,
) -> Result<grammers_client::Client, Box<dyn std::error::Error>> {
    use grammers_mtsender::{ConnectionParams, SenderPool};
    let session = open_session(cfg).await?;
    let params = ConnectionParams {
        device_model: "Rustgramm".to_owned(),
        system_version: String::new(),
        app_version: env!("CARGO_PKG_VERSION").to_owned(),
        proxy_url: cfg.proxy_url.clone(),
        ..ConnectionParams::default()
    };
    let SenderPool { runner, handle, .. } =
        SenderPool::with_configuration(session, cfg.api_id, params);
    let client = grammers_client::Client::new(handle);
    tokio::spawn(runner.run());
    Ok(client)
}

#[cfg(test)]
mod tests {
    use super::*;
    use grammers_session::SessionData;
    use grammers_session::storages::MemorySession;

    fn env(pairs: &[(&'static str, &'static str)]) -> impl Fn(&str) -> Option<String> {
        let v: Vec<_> = pairs.to_vec();
        move |k| v.iter().find(|(a, _)| *a == k).map(|(_, b)| (*b).to_owned())
    }

    #[test]
    fn config_requires_own_api_keys() {
        assert_eq!(
            Config::from_lookup(env(&[])).unwrap_err(),
            ConfigError::Missing("RG_API_ID")
        );
        assert_eq!(
            Config::from_lookup(env(&[("RG_API_ID", "x"), ("RG_API_HASH", "h")])).unwrap_err(),
            ConfigError::BadApiId
        );
        let c = Config::from_lookup(env(&[("RG_API_ID", "123"), ("RG_API_HASH", "abc")])).unwrap();
        assert_eq!((c.api_id, c.api_hash.as_str()), (123, "abc"));
        assert!(c.proxy_url.is_none());
    }

    #[test]
    fn only_socks5_proxy_and_it_is_explicit() {
        let c = Config::from_lookup(env(&[
            ("RG_API_ID", "1"),
            ("RG_API_HASH", "h"),
            ("RG_PROXY", "socks5://u:p@127.0.0.1:1080"),
        ]))
        .unwrap();
        assert!(c.guard().unwrap().host_allowed("127.0.0.1"));
        let bad = Config {
            proxy_url: Some("http://evil.example:80".into()),
            ..c
        };
        assert!(matches!(bad.guard(), Err(ConfigError::ProxyNotAllowed(_))));
    }

    #[test]
    fn stock_dc_table_is_all_telegram() {
        let guard = Guard::new();
        let data = SessionData::default();
        assert_eq!(data.dc_options.len(), 5);
        for dc in data.dc_options.values() {
            assert!(guard.ip_allowed((*dc.ipv4.ip()).into()), "dc {} v4", dc.id);
            assert!(guard.ip_allowed((*dc.ipv6.ip()).into()), "dc {} v6", dc.id);
        }
    }

    #[test]
    fn foreign_dc_is_dropped() {
        let session = GuardedSession::new(MemorySession::from(SessionData::default()), Guard::new());
        let mut evil = SessionData::default().dc_options[&2].clone();
        evil.ipv4 = "8.8.8.8:443".parse().unwrap();
        let rt = tokio::runtime::Builder::new_current_thread().build().unwrap();
        rt.block_on(session.set_dc_option(&evil)).ok();
        assert_eq!(session.blocked(), 1);
        assert!(session.dc_option(2).unwrap().is_some());
    }
}
