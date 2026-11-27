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
        transport: Box::new(|| {
            Box::new(grammers_mtproto::transport::Obfuscated::new(
                grammers_mtproto::transport::Abridged::new(),
            ))
        }),
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

/// Быстрая гонка TCP-подключений к официальным DC Telegram (IPv4 + IPv6) без прокси,
/// чтобы выбрать первый живой маршрут и не ждать 45с на заблокированном адресе.
pub async fn probe_reachable_dc(preferred_dc: i32) -> Option<(i32, bool)> {
    let data = grammers_session::SessionData::default();
    let (tx, mut rx) = tokio::sync::mpsc::channel::<(i32, bool)>(16);
    let mut handles = Vec::new();
    for (&dc_id, dc) in &data.dc_options {
        let v4: std::net::SocketAddr = dc.ipv4.into();
        let tx4 = tx.clone();
        let delay4 = if dc_id == preferred_dc { 0 } else { 120 };
        handles.push(tokio::spawn(async move {
            if delay4 > 0 {
                tokio::time::sleep(std::time::Duration::from_millis(delay4)).await;
            }
            if tokio::time::timeout(
                std::time::Duration::from_millis(2400),
                tokio::net::TcpStream::connect(v4),
            )
            .await
            .is_ok_and(|r| r.is_ok())
            {
                let _ = tx4.send((dc_id, false)).await;
            }
        }));
        let v6: std::net::SocketAddr = dc.ipv6.into();
        let tx6 = tx.clone();
        handles.push(tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(260)).await;
            if tokio::time::timeout(
                std::time::Duration::from_millis(2400),
                tokio::net::TcpStream::connect(v6),
            )
            .await
            .is_ok_and(|r| r.is_ok())
            {
                let _ = tx6.send((dc_id, true)).await;
            }
        }));
    }
    drop(tx);
    let winner = tokio::time::timeout(std::time::Duration::from_millis(2600), rx.recv())
        .await
        .ok()
        .flatten();
    for h in handles {
        h.abort();
    }
    winner
}

/// Подключение: пул отправителей с защищённой сессией. Сведений об устройстве не раскрываем.
pub async fn connect_with_updates(cfg: &Config) -> Result<Connected, Box<dyn std::error::Error>> {
    use grammers_mtsender::SenderPool;
    let session = open_session(cfg).await?;
    let mut params = isolated_connection_params(cfg);
    if cfg.proxy_url.is_none() {
        let home_dc = session.home_dc_id().unwrap_or(2);
        if let Some((dc_id, use_ipv6)) = probe_reachable_dc(home_dc).await {
            params.use_ipv6 = use_ipv6;
            let has_key = session
                .dc_option(home_dc)
                .ok()
                .flatten()
                .and_then(|d| d.auth_key)
                .is_some();
            if !has_key && dc_id != home_dc {
                let _ = session.set_home_dc_id(dc_id).await;
            }
        }
    }
    let SenderPool {
        runner,
        handle,
        updates,
    } = SenderPool::with_configuration(session.clone(), cfg.api_id, params);
    let pool_handle = handle.clone();
    let client = grammers_client::Client::new(handle);
    tokio::spawn(runner.run());
    Ok(Connected {
        session,
        client,
        pool_handle,
        updates,
    })
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
}
