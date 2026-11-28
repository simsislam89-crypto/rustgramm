//! Ядро Rustgramm: клиент Telegram (MTProto через grammers).
//! Все адреса дата-центров проходят через `rg-net`: адрес вне сетей Telegram
//! не попадает ни в сессию, ни в сокет.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use grammers_session::Session;
use grammers_session::types::{ChannelKind, DcOption, PeerAuth, PeerId, PeerInfo, UpdateState, UpdatesState};
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
    /// Изолированный отпечаток устройства для анти-корреляции мультиаккаунта.
    pub device_model: Option<String>,
    pub system_version: Option<String>,
    pub app_version: Option<String>,
    pub lang_code: Option<String>,
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
            device_model: None,
            system_version: None,
            app_version: None,
            lang_code: None,
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

/// Клиент и канал обновлений, полученные после подключения.
pub struct Connected {
    pub session: Arc<GuardedSession<SqliteSession>>,
    pub client: grammers_client::Client,
    pub pool_handle: grammers_mtsender::SenderPoolFatHandle,
    pub updates: tokio::sync::mpsc::Receiver<grammers_session::updates::UpdatesLike>,
}

/// Сведения о клиенте для `initConnection`: одинаковые у всех пользователей и ничего не
/// говорят об устройстве. Модель, версия ОС и язык системы не отправляются
/// (grammers по умолчанию взял бы их из системы).
pub fn privacy_params(proxy_url: Option<String>) -> grammers_mtsender::ConnectionParams {
    grammers_mtsender::ConnectionParams {
        device_model: "Rustgramm".to_owned(),
        system_version: "Rustgramm OS".to_owned(),
        app_version: env!("CARGO_PKG_VERSION").to_owned(),
        system_lang_code: "en".to_owned(),
        lang_code: "en".to_owned(),
        proxy_url,
        updates_channel_capacity: std::num::NonZeroUsize::new(2048).expect("non-zero capacity"),
        transport: Box::new(|| crate::transport::obfuscated_transport()),
        ..grammers_mtsender::ConnectionParams::default()
    }
}

/// Преобразует сырых пользователей и чаты Telegram в записи `PeerInfo` и сохраняет их в кэше сессии.
/// Нужно вызывать после сырых `messages.getDialogs` / `auth.*`, чтобы `UpdatesReceiver` всегда имел
/// `access_hash` каналов и запись текущего пользователя для быстрого получения live-обновлений без потерь.
pub async fn cache_tl_peers<S: Session>(
    session: &S,
    users: &[grammers_client::tl::enums::User],
    chats: &[grammers_client::tl::enums::Chat],
) {
    use grammers_client::tl::enums::{Chat as C, User as U};
    let mut batch = Vec::with_capacity(users.len() + chats.len());
    for u in users {
        if let U::User(x) = u {
            batch.push(PeerInfo::User {
                id: x.id,
                auth: x.access_hash.map(PeerAuth::from_hash),
                bot: Some(x.bot),
                is_self: Some(x.is_self),
            });
        }
    }
    for c in chats {
        match c {
            C::Chat(x) => batch.push(PeerInfo::Chat { id: x.id }),
            C::Channel(x) => batch.push(PeerInfo::Channel {
                id: x.id,
                auth: x.access_hash.map(PeerAuth::from_hash),
                kind: Some(if x.broadcast {
                    ChannelKind::Broadcast
                } else if x.gigagroup {
                    ChannelKind::Gigagroup
                } else {
                    ChannelKind::Megagroup
                }),
            }),
            C::Forbidden(x) => batch.push(PeerInfo::Chat { id: x.id }),
            C::ChannelForbidden(x) => batch.push(PeerInfo::Channel {
                id: x.id,
                auth: Some(PeerAuth::from_hash(x.access_hash)),
                kind: None,
            }),
            _ => {}
        }
    }
    if !batch.is_empty() {
        let _ = session.cache_peers(batch).await;
    }
}

/// Сохраняет авторизованного пользователя из ответа `auth.Authorization` как `is_self = true`.
pub async fn cache_tl_auth_user<S: Session>(
    session: &S,
    auth: &grammers_client::tl::enums::auth::Authorization,
) {
    if let grammers_client::tl::enums::auth::Authorization::Authorization(a) = auth
        && let grammers_client::tl::enums::User::User(u) = &a.user
    {
        let _ = session
            .cache_peer(PeerInfo::User {
                id: u.id,
                auth: u.access_hash.map(PeerAuth::from_hash),
                bot: Some(u.bot),
                is_self: Some(true),
            })
            .await;
    }
}

/// Параметры `initConnection` с учётом изолированного отпечатка профиля аккаунта.
pub fn isolated_connection_params(cfg: &Config) -> grammers_mtsender::ConnectionParams {
    let mut p = privacy_params(cfg.proxy_url.clone());
    if let Some(d) = cfg.device_model.as_ref().filter(|s| !s.trim().is_empty()) {
        p.device_model.clone_from(d);
    }
    if let Some(s) = cfg.system_version.as_ref().filter(|s| !s.trim().is_empty()) {
        p.system_version.clone_from(s);
    }
    if let Some(v) = cfg.app_version.as_ref().filter(|s| !s.trim().is_empty()) {
        p.app_version.clone_from(v);
    }
    // Для Desktop API (2040) Telegram отклоняет слишком старые/semver-префиксы (`0.x`) на auth.sendCode.
    if cfg.api_id == 2040 {
        let major = p
            .app_version
            .trim()
            .split(|c: char| !c.is_ascii_digit())
            .next()
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(0);
        if major < 4 {
            p.app_version = "5.14.3 x64".to_string();
        }
    }
    if let Some(l) = cfg.lang_code.as_ref().filter(|s| !s.trim().is_empty()) {
        p.lang_code.clone_from(l);
        p.system_lang_code.clone_from(l);
    }
    p
}

/// Победитель живой MTProto-проверки DC (`req_pq_multi` -> `resPQ`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProbeWinner {
    pub dc_id: i32,
    pub addr: std::net::SocketAddr,
    pub use_ipv6: bool,
    pub use_obfuscated: bool,
    pub mode: crate::transport::TransportMode,
    pub rtt_ms: u32,
}

async fn probe_single_endpoint(
    dc_id: i32,
    addr: std::net::SocketAddr,
    use_ipv6: bool,
    mode: crate::transport::TransportMode,
    delay_ms: u64,
) -> Option<ProbeWinner> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    if delay_ms > 0 {
        tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
    }
    let started = std::time::Instant::now();
    let deadline = tokio::time::Duration::from_millis(2400);
    tokio::time::timeout(deadline, async {
        let mut stream = tokio::net::TcpStream::connect(addr).await.ok()?;
        let _ = stream.set_nodelay(true);
        let (pkt, mut tr) = crate::transport::build_req_pq_probe_mode(mode);
        stream.write_all(&pkt).await.ok()?;
        let _ = stream.flush().await;
        let mut buf = vec![0_u8; 512];
        let mut filled = 0_usize;
        while filled < buf.len() {
            let n = stream.read(&mut buf[filled..]).await.ok()?;
            if n == 0 {
                break;
            }
            let prev = filled;
            filled += n;
            // Расшифровываем единый накопительный буфер на месте, чтобы Obfuscated::decrypt_tail
            // не рассинхронизировался при приходе ответа частями.
            if crate::transport::is_valid_res_pq_reply(&mut *tr, &mut buf[..filled]) {
                return Some(ProbeWinner {
                    dc_id,
                    addr,
                    use_ipv6,
                    use_obfuscated: mode.is_obfuscated(),
                    mode,
                    rtt_ms: started.elapsed().as_millis() as u32,
                });
            }
            let _ = prev;
        }
        None
    })
    .await
    .ok()
    .flatten()
}

/// Дополнительные официальные IP-адреса Telegram DC1..DC5 для устойчивой связности на мобильных сетях.
fn dc_ipv4_candidates(dc_id: i32, primary: std::net::SocketAddrV4) -> Vec<std::net::SocketAddrV4> {
    use std::net::{Ipv4Addr, SocketAddrV4};
    let mut out = vec![primary];
    let extras: &[Ipv4Addr] = match dc_id {
        1 => &[Ipv4Addr::new(149, 154, 175, 50), Ipv4Addr::new(149, 154, 175, 53)],
        2 => &[Ipv4Addr::new(149, 154, 167, 51), Ipv4Addr::new(149, 154, 167, 41)],
        3 => &[Ipv4Addr::new(149, 154, 175, 100)],
        4 => &[Ipv4Addr::new(149, 154, 167, 91), Ipv4Addr::new(149, 154, 167, 92)],
        5 => &[Ipv4Addr::new(91, 108, 56, 130), Ipv4Addr::new(91, 108, 56, 104)],
        _ => &[],
    };
    for ip in extras {
        if *ip != *primary.ip() {
            out.push(SocketAddrV4::new(*ip, 443));
        }
    }
    out
}

/// Живая многопоточная MTProto-проверка всех официальных DC Telegram (`req_pq_multi` -> `resPQ`):
/// отбирает по одному самому быстрому живому эндпоинту на каждый DC и порт (`443`, опционально `80`/`5222`).
pub async fn probe_reachable_dcs(
    preferred_dc: i32,
    force_ipv4: bool,
    allow_alt_ports: bool,
) -> Vec<ProbeWinner> {
    use crate::transport::TransportMode;
    let data = grammers_session::SessionData::default();
    let (tx, mut rx) = tokio::sync::mpsc::channel::<ProbeWinner>(64);
    let mut handles = Vec::new();
    let ports: &[u16] = if allow_alt_ports { &[443, 80, 5222] } else { &[443, 80] };
    for (&dc_id, dc) in &data.dc_options {
        let is_pref = dc_id == preferred_dc;
        let base_delay = if is_pref { 0_u64 } else { 70_u64 };
        for v4 in dc_ipv4_candidates(dc_id, dc.ipv4) {
            for &port in ports {
                let addr = std::net::SocketAddr::V4(std::net::SocketAddrV4::new(*v4.ip(), port));
                let port_delay = match port {
                    443 => 0_u64,
                    80 => if allow_alt_ports { 40 } else { 420 },
                    _ => 180,
                };
                for (mode, mode_delay) in [
                    (TransportMode::ObfuscatedIntermediate, 0_u64),
                    (TransportMode::ObfuscatedAbridged, 140),
                    (TransportMode::PlainIntermediate, 320),
                    (TransportMode::Full, 480),
                ] {
                    let tx_c = tx.clone();
                    let d = base_delay + port_delay + mode_delay;
                    handles.push(tokio::spawn(async move {
                        if let Some(win) = probe_single_endpoint(dc_id, addr, false, mode, d).await {
                            let _ = tx_c.send(win).await;
                        }
                    }));
                }
            }
        }
        if !force_ipv4 {
            let v6: std::net::SocketAddr = dc.ipv6.into();
            let tx_c = tx.clone();
            handles.push(tokio::spawn(async move {
                if let Some(win) = probe_single_endpoint(dc_id, v6, true, TransportMode::ObfuscatedIntermediate, 700).await {
                    let _ = tx_c.send(win).await;
                }
            }));
        }
    }
    drop(tx);
    let mut best_by_dc: std::collections::HashMap<i32, ProbeWinner> = std::collections::HashMap::new();
    let total_deadline = tokio::time::Instant::now() + std::time::Duration::from_millis(2800);
    let mut after_first: Option<tokio::time::Instant> = None;
    loop {
        let wait_until = match after_first {
            Some(t) => total_deadline.min(t),
            None => total_deadline,
        };
        let Ok(msg) = tokio::time::timeout_at(wait_until, rx.recv()).await else {
            break;
        };
        let Some(w) = msg else { break };
        if after_first.is_none() {
            after_first = Some(tokio::time::Instant::now() + std::time::Duration::from_millis(650));
        }
        best_by_dc
            .entry(w.dc_id)
            .and_modify(|cur| {
                if w.rtt_ms < cur.rtt_ms {
                    *cur = w;
                }
            })
            .or_insert(w);
        if best_by_dc.contains_key(&preferred_dc) && best_by_dc.len() >= 4 {
            break;
        }
    }
    for h in handles {
        h.abort();
    }
    let mut list: Vec<ProbeWinner> = best_by_dc.into_values().collect();
    list.sort_by_key(|w| (i32::from(w.dc_id != preferred_dc), w.rtt_ms));
    list
}

/// Живая MTProto-проверка официальных DC Telegram (`req_pq_multi` -> `resPQ`):
/// отбрасывает IPv6/заблокированные маршруты, принимающие TCP и обрывающие поток с 0 байтами,
/// и выбирает первый реально отвечающий дата-центр с самым низким RTT.
pub async fn probe_reachable_dc(preferred_dc: i32) -> Option<ProbeWinner> {
    probe_reachable_dcs(preferred_dc, false, false).await.into_iter().next()
}

/// Подключение с гибкой стратегией (подсказка DC, IPv4-фоллбэк, порты 80/5222 и выбор `TransportMode`).
pub async fn connect_with_strategy(
    cfg: &Config,
    preferred_dc_override: Option<i32>,
    force_ipv4: bool,
    allow_alt_ports: bool,
    transport_override: Option<crate::transport::TransportMode>,
    clear_unauth_keys: bool,
) -> Result<(Connected, crate::transport::TransportMode), Box<dyn std::error::Error>> {
    use crate::transport::TransportMode;
    use grammers_mtsender::SenderPool;
    let session = open_session(cfg).await?;
    let mut params = isolated_connection_params(cfg);
    if force_ipv4 {
        params.use_ipv6 = false;
    }
    let mut chosen_mode = transport_override.unwrap_or(TransportMode::ObfuscatedIntermediate);
    let is_logged_in = session.peer(PeerId::self_user()).await.ok().flatten().is_some();
    if clear_unauth_keys && !is_logged_in {
        for dc_id in 1..=5 {
            if let Ok(Some(mut opt)) = session.dc_option(dc_id)
                && opt.auth_key.is_some()
            {
                opt.auth_key = None;
                let _ = session.set_dc_option(&opt).await;
            }
        }
    }
    if cfg.proxy_url.is_none() {
        let session_home = session.home_dc_id().unwrap_or(2);
        let preferred_dc = preferred_dc_override.unwrap_or(session_home);
        let winners = probe_reachable_dcs(preferred_dc, force_ipv4, allow_alt_ports).await;
        for w in &winners {
            if let std::net::SocketAddr::V4(v4) = w.addr
                && let Ok(Some(mut opt)) = session.dc_option(w.dc_id)
            {
                opt.ipv4 = v4;
                let _ = session.set_dc_option(&opt).await;
            }
        }
        if let Some(win) = winners.first().copied() {
            if transport_override.is_none() {
                chosen_mode = win.mode;
            }
            crate::diag::info(
                "connect",
                &format!(
                    "DC probe winner: dc={} addr={} ipv6={} mode={} rtt={}ms (discovered {} DCs)",
                    win.dc_id,
                    win.addr,
                    win.use_ipv6,
                    chosen_mode.label(),
                    win.rtt_ms,
                    winners.len(),
                ),
            );
            params.use_ipv6 = if force_ipv4 { false } else { win.use_ipv6 };
            let has_key = session
                .dc_option(session_home)
                .ok()
                .flatten()
                .and_then(|d| d.auth_key)
                .is_some();
            if !has_key && win.dc_id != session_home {
                let _ = session.set_home_dc_id(win.dc_id).await;
            }
        }
    }
    params.transport = Box::new(move || chosen_mode.build());
    let SenderPool {
        runner,
        handle,
        updates,
    } = SenderPool::with_configuration(session.clone(), cfg.api_id, params);
    let pool_handle = handle.clone();
    let client = grammers_client::Client::new(handle);
    tokio::spawn(runner.run());
    Ok((
        Connected {
            session,
            client,
            pool_handle,
            updates,
        },
        chosen_mode,
    ))
}

/// Подключение: пул отправителей с защищённой сессией и подсказкой DC / форсированным IPv4.
pub async fn connect_with_updates_hint(
    cfg: &Config,
    preferred_dc_override: Option<i32>,
    force_ipv4: bool,
) -> Result<Connected, Box<dyn std::error::Error>> {
    Ok(connect_with_strategy(cfg, preferred_dc_override, force_ipv4, false, None, false)
        .await?
        .0)
}

/// Подключение: пул отправителей с защищённой сессией. Сведений об устройстве не раскрываем.
pub async fn connect_with_updates(cfg: &Config) -> Result<Connected, Box<dyn std::error::Error>> {
    connect_with_updates_hint(cfg, None, false).await
}

/// Подключение без обработки обновлений.
pub async fn connect(cfg: &Config) -> Result<grammers_client::Client, Box<dyn std::error::Error>> {
    Ok(connect_with_updates(cfg).await?.client)
}

pub mod auth;
pub mod call_media;
pub mod calls;
pub mod diag;
pub mod engine;
pub mod media;
pub mod qr;
pub mod rich_msg;
pub mod secret;
pub mod transport;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn isolated_connection_params_normalizes_old_desktop_version() {
        let cfg = Config {
            api_id: 2040,
            api_hash: "0".into(),
            session_path: "s".into(),
            proxy_url: None,
            device_model: None,
            system_version: None,
            app_version: Some("0.23.0".into()),
            lang_code: None,
        };
        let p = isolated_connection_params(&cfg);
        assert_eq!(p.app_version, "5.14.3 x64");
    }

    #[test]
    fn isolated_connection_params_keeps_modern_desktop_version() {
        let cfg = Config {
            api_id: 2040,
            api_hash: "0".into(),
            session_path: "s".into(),
            proxy_url: None,
            device_model: None,
            system_version: None,
            app_version: Some("5.15.0 x64".into()),
            lang_code: None,
        };
        let p = isolated_connection_params(&cfg);
        assert_eq!(p.app_version, "5.15.0 x64");
    }
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
    fn client_info_reveals_nothing_about_the_device() {
        let p = privacy_params(Some("socks5://127.0.0.1:1080".to_owned()));
        assert_eq!(p.device_model, "Rustgramm");
        assert_eq!(p.system_version, "Rustgramm OS");
        assert_eq!((p.system_lang_code.as_str(), p.lang_code.as_str()), ("en", "en"));
        assert_eq!(p.proxy_url.as_deref(), Some("socks5://127.0.0.1:1080"));
        // Версия приложения — только наша, а не системная.
        assert_eq!(p.app_version, env!("CARGO_PKG_VERSION"));
        assert!(!p.use_ipv6);
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

    #[test]
    fn cache_tl_peers_persists_channel_hash_and_self_user() {
        use grammers_client::tl;
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        let inner = rt.block_on(SqliteSession::open(":memory:")).unwrap();
        let session = GuardedSession::new(inner, Guard::new());
        let self_user = tl::enums::User::User(tl::types::User {
            is_self: true,
            contact: false,
            mutual_contact: false,
            deleted: false,
            bot: false,
            bot_chat_history: false,
            bot_nochats: false,
            verified: false,
            restricted: false,
            min: false,
            bot_inline_geo: false,
            support: false,
            scam: false,
            apply_min_photo: false,
            fake: false,
            bot_attach_menu: false,
            premium: false,
            attach_menu_enabled: false,
            bot_can_edit: false,
            close_friend: false,
            stories_hidden: false,
            stories_unavailable: false,
            contact_require_premium: false,
            bot_business: false,
            bot_has_main_app: false,
            bot_forum_view: false,
            bot_forum_can_manage_topics: false,
            bot_can_manage_bots: false,
            bot_guestchat: false,
            bot_guard: false,
            bot_forum_sidebar: false,
            id: 777_001,
            access_hash: Some(987_654),
            first_name: Some("Tester".into()),
            last_name: None,
            username: None,
            phone: None,
            photo: None,
            status: None,
            bot_info_version: None,
            restriction_reason: None,
            bot_inline_placeholder: None,
            lang_code: None,
            emoji_status: None,
            usernames: None,
            stories_max_id: None,
            color: None,
            profile_color: None,
            bot_active_users: None,
            bot_verification_icon: None,
            send_paid_messages_stars: None,
            linked_community_id: None,
        });
        let channel = tl::enums::Chat::Channel(tl::types::Channel {
            creator: false,
            left: false,
            broadcast: true,
            verified: false,
            megagroup: false,
            restricted: false,
            signatures: false,
            min: false,
            scam: false,
            has_link: true,
            has_geo: false,
            slowmode_enabled: false,
            call_active: false,
            call_not_empty: false,
            fake: false,
            gigagroup: false,
            noforwards: false,
            join_to_send: false,
            join_request: false,
            forum: false,
            stories_hidden: false,
            stories_hidden_min: false,
            stories_unavailable: false,
            signature_profiles: false,
            autotranslation: false,
            broadcast_messages_allowed: false,
            monoforum: false,
            forum_tabs: false,
            in_news: false,
            forum_sidebar: false,
            id: 222_333,
            access_hash: Some(555_666),
            title: "News".into(),
            username: None,
            photo: tl::enums::ChatPhoto::Empty,
            date: 1_700_000_000,
            restriction_reason: None,
            admin_rights: None,
            banned_rights: None,
            default_banned_rights: None,
            participants_count: None,
            usernames: None,
            stories_max_id: None,
            color: None,
            profile_color: None,
            emoji_status: None,
            level: None,
            subscription_until_date: None,
            bot_verification_icon: None,
            send_paid_messages_stars: None,
            linked_monoforum_id: None,
            linked_community_id: None,
            pinned_msg_id: None,
        });
        rt.block_on(cache_tl_peers(&session, &[self_user], &[channel]));
        let cached_self = rt.block_on(session.peer(PeerId::self_user())).unwrap();
        assert!(cached_self.is_some());
        let cached_chan = rt
            .block_on(session.peer(PeerId::channel_unchecked(222_333)))
            .unwrap();
        match cached_chan {
            Some(PeerInfo::Channel { id, auth, kind }) => {
                assert_eq!(id, 222_333);
                assert_eq!(auth, Some(PeerAuth::from_hash(555_666)));
                assert_eq!(kind, Some(ChannelKind::Broadcast));
            }
            other => panic!("unexpected channel peer: {other:?}"),
        }
        assert!(privacy_params(None).updates_channel_capacity.get() >= 2048);
    }

    #[test]
    fn obfuscated_transport_builds_fresh_non_forbidden_handshake() {
        let p = privacy_params(None);
        let cfg = Config {
            api_id: 2040,
            api_hash: "0".into(),
            session_path: "s".into(),
            proxy_url: None,
            device_model: None,
            system_version: None,
            app_version: Some("0.25.0".into()),
            lang_code: None,
        };
        let iso = isolated_connection_params(&cfg);
        assert_eq!(iso.app_version, "5.14.3 x64");
        let _t1 = (p.transport)();
        let _t2 = (iso.transport)();
    }

    #[test]
    fn dc_ipv4_candidates_include_official_fallbacks_and_split_probe_works() {
        use grammers_mtproto::transport::Transport;
        let c1 = dc_ipv4_candidates(1, "149.154.175.53:443".parse().unwrap());
        assert!(c1.iter().any(|a| a.ip().to_string() == "149.154.175.50"));
        assert!(c1.iter().any(|a| a.ip().to_string() == "149.154.175.53"));
        let (_pkt, mut tr) = crate::transport::build_req_pq_probe_mode(crate::transport::TransportMode::PlainIntermediate);
        let mut srv = crate::transport::SafeIntermediate::new();
        let mut res_pq = vec![0_u8; 64];
        res_pq[16..20].copy_from_slice(&44_i32.to_le_bytes());
        res_pq[20..24].copy_from_slice(&0x0516_2463_u32.to_le_bytes());
        let mut wire = grammers_mtproto::DequeBuffer::with_capacity(res_pq.len(), 8);
        wire.extend_back(&res_pq);
        srv.pack(&mut wire);
        let raw = &wire.as_ref()[4..];
        let mut acc = raw[..16].to_vec();
        assert!(!crate::transport::is_valid_res_pq_reply(&mut *tr, &mut acc));
        acc.extend_from_slice(&raw[16..]);
        assert!(crate::transport::is_valid_res_pq_reply(&mut *tr, &mut acc));
    }
}
