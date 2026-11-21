//! Сетевой страж: любое исходящее соединение клиента сначала проходит `Guard::check`.
//! По умолчанию разрешены только серверы Telegram. Остальное — только по явному
//! решению пользователя (`allow_user_host`), которое видно в настройках.

use std::net::Ipv4Addr;

/// Домены Telegram (хост равен домену или является его поддоменом).
pub const TELEGRAM_DOMAINS: &[&str] = &[
    "telegram.org",
    "t.me",
    "telegram.me",
    "telegra.ph",
    "telesco.pe",
    "cdn-telegram.org",
    "telegram-cdn.org",
];

/// Подсети дата-центров Telegram (AS62041, AS59930, AS44907, AS211157).
pub const TELEGRAM_NETS: &[(Ipv4Addr, u8)] = &[
    (Ipv4Addr::new(149, 154, 160, 0), 20),
    (Ipv4Addr::new(91, 108, 4, 0), 22),
    (Ipv4Addr::new(91, 108, 8, 0), 22),
    (Ipv4Addr::new(91, 108, 12, 0), 22),
    (Ipv4Addr::new(91, 108, 16, 0), 22),
    (Ipv4Addr::new(91, 108, 56, 0), 22),
    (Ipv4Addr::new(95, 161, 64, 0), 20),
    (Ipv4Addr::new(185, 76, 151, 0), 24),
];

#[derive(Debug, PartialEq, Eq)]
pub enum Denied {
    BadUrl,
    NotHttps,
    ForeignHost(String),
}

#[derive(Default, Debug, Clone)]
pub struct Guard {
    user_hosts: Vec<String>,
}

impl Guard {
    pub fn new() -> Self {
        Self::default()
    }

    /// Явное разрешение пользователя (например, свой прокси или переводчик).
    pub fn allow_user_host(&mut self, host: &str) {
        let host = host.trim().trim_end_matches('.').to_ascii_lowercase();
        if !host.is_empty() && !self.user_hosts.contains(&host) {
            self.user_hosts.push(host);
        }
    }

    pub fn user_hosts(&self) -> &[String] {
        &self.user_hosts
    }

    /// Проверка хоста или IPv4-адреса (для MTProto, порт не важен).
    pub fn host_allowed(&self, host: &str) -> bool {
        let host = host.trim().trim_end_matches('.').to_ascii_lowercase();
        if host.is_empty() {
            return false;
        }
        if let Ok(ip) = host.parse::<Ipv4Addr>() {
            return in_telegram_nets(ip) || self.user_hosts.contains(&host);
        }
        domain_match(&host, TELEGRAM_DOMAINS)
            || self.user_hosts.iter().any(|h| h == &host)
    }

    /// Проверка URL: только https и только разрешённый хост.
    pub fn check(&self, url: &str) -> Result<(), Denied> {
        let (scheme, rest) = url.split_once("://").ok_or(Denied::BadUrl)?;
        let host = host_of(rest).ok_or(Denied::BadUrl)?;
        if !scheme.eq_ignore_ascii_case("https") {
            return Err(Denied::NotHttps);
        }
        if self.host_allowed(&host) {
            Ok(())
        } else {
            Err(Denied::ForeignHost(host))
        }
    }
}

/// Хост из части URL после `://` (без логина, порта, пути).
pub fn host_of(rest: &str) -> Option<String> {
    let authority = rest.split(['/', '?', '#']).next()?;
    let authority = authority.rsplit('@').next()?;
    let host = match authority.strip_prefix('[') {
        Some(v6) => v6.split(']').next()?,
        None => authority.split(':').next()?,
    };
    if host.is_empty() {
        None
    } else {
        Some(host.to_ascii_lowercase())
    }
}

fn domain_match(host: &str, domains: &[&str]) -> bool {
    domains
        .iter()
        .any(|d| host == *d || host.strip_suffix(d).is_some_and(|p| p.ends_with('.')))
}

fn in_telegram_nets(ip: Ipv4Addr) -> bool {
    let v = u32::from(ip);
    TELEGRAM_NETS.iter().any(|(net, bits)| {
        let mask = u32::MAX << (32 - u32::from(*bits));
        v & mask == u32::from(*net) & mask
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn telegram_is_allowed() {
        let g = Guard::new();
        assert!(g.check("https://t.me/durov").is_ok());
        assert!(g.check("https://web.telegram.org/k/").is_ok());
        assert!(g.host_allowed("149.154.167.51"));
        assert!(g.host_allowed("91.108.56.130"));
    }

    #[test]
    fn foreign_is_denied() {
        let g = Guard::new();
        for u in [
            "https://sentry.radolyn.com/api/2/minidump/",
            "https://update.ayugram.one/rc/current/desktop2",
            "https://api.exteragram.app/api/v1/profiles/compact",
            "https://dpaste.com/api/v2/",
            "https://cdn.jsdelivr.net/gh/x",
            "https://translate.yandex.net/api",
        ] {
            assert!(matches!(g.check(u), Err(Denied::ForeignHost(_))), "{u}");
        }
    }

    #[test]
    fn lookalikes_are_denied() {
        let g = Guard::new();
        assert!(g.check("https://evil-t.me/").is_err());
        assert!(g.check("https://t.me.evil.com/").is_err());
        assert!(g.check("https://telegram.org@evil.com/").is_err());
        assert!(!g.host_allowed("149.154.159.255"));
        assert!(!g.host_allowed("8.8.8.8"));
    }

    #[test]
    fn plain_http_denied() {
        assert_eq!(Guard::new().check("http://t.me/"), Err(Denied::NotHttps));
        assert_eq!(Guard::new().check("t.me"), Err(Denied::BadUrl));
    }

    #[test]
    fn user_host_is_explicit() {
        let mut g = Guard::new();
        assert!(g.check("https://translate.example.org/").is_err());
        g.allow_user_host("Translate.Example.org.");
        assert!(g.check("https://translate.example.org/x").is_ok());
        assert!(g.check("https://other.example.org/").is_err());
    }
}
