//! Голосовые и видеозвонки Telegram (MTProto `phone.*` сигналинг и E2E проверка ключа).

use grammers_client::tl;
use sha2::{Digest as _, Sha256};

use crate::secret::key_emojis;

/// Текущее состояние звонка.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CallState {
    /// Исходящий вызов, ждём ответа собеседника.
    RingingOut,
    /// Входящий вызов, можно принять или отклонить.
    RingingIn,
    /// Идёт обмен DH-ключами / установка канала.
    Connecting,
    /// Звонок установлен (E2E-ключ подтверждён).
    Active,
    /// Звонок завершён с причиной.
    Ended(String),
}

/// Входящее или исходящее событие звонка из MTProto (`Update::PhoneCall`).
#[derive(Debug, Clone)]
pub enum CallEvent {
    Incoming {
        call_id: i64,
        access_hash: i64,
        admin_id: i64,
        video: bool,
        g_a_hash: Vec<u8>,
    },
    Waiting {
        call_id: i64,
        access_hash: i64,
        received: bool,
    },
    Accepted {
        call_id: i64,
        access_hash: i64,
        g_b: Vec<u8>,
    },
    Active {
        call_id: i64,
        access_hash: i64,
        g_a_or_b: Vec<u8>,
        key_fingerprint: i64,
        video: bool,
        p2p_allowed: bool,
        relays: usize,
    },
    Discarded {
        call_id: i64,
        reason: String,
        duration: i32,
    },
}

/// Запись истории звонков в рамках сессии.
#[derive(Debug, Clone)]
pub struct CallLogEntry {
    pub peer_name: String,
    pub peer_id: i64,
    pub video: bool,
    pub outgoing: bool,
    pub duration: u32,
    pub status: String,
    pub timestamp: i64,
}

/// Параметры протокола звонков Telegram (`PhoneCallProtocol`), совместимые с современными клиентами.
pub fn default_call_protocol() -> tl::enums::PhoneCallProtocol {
    tl::enums::PhoneCallProtocol::Protocol(tl::types::PhoneCallProtocol {
        udp_p2p: true,
        udp_reflector: true,
        min_layer: 65,
        max_layer: 92,
        library_versions: vec!["4.0.0".to_owned(), "2.4.4".to_owned()],
    })
}

/// Хеш `g_a_hash = SHA256(g_a)` для `phone.requestCall`.
pub fn ga_hash(g_a: &[u8]) -> [u8; 32] {
    let mut out = [0u8; 32];
    out.copy_from_slice(&Sha256::digest(g_a));
    out
}

/// Четыре эмодзи E2E-проверки звонка (`SHA256(key || g_a)`).
pub fn call_emojis(key: &[u8; 256], g_a: &[u8]) -> [&'static str; 4] {
    key_emojis(key, g_a)
}

/// Человекочитаемая причина завершения звонка.
pub fn discard_reason_label(r: Option<&tl::enums::PhoneCallDiscardReason>) -> &'static str {
    use tl::enums::PhoneCallDiscardReason as R;
    match r {
        Some(R::Missed) => "Пропущенный вызов",
        Some(R::Busy) => "Абонент занят",
        Some(R::Hangup) => "Звонок завершён",
        Some(R::Disconnect) => "Связь прервана",
        Some(R::MigrateConferenceCall(_)) => "Переведён в конференцию",
        None => "Звонок завершён",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ga_hash_and_call_emojis_match_expected_lengths() {
        let g_a = [0x11u8; 256];
        let h = ga_hash(&g_a);
        assert_eq!(h.len(), 32);
        let key = [0x22u8; 256];
        let em = call_emojis(&key, &g_a);
        assert_eq!(em.len(), 4);
        assert!(em.iter().all(|s| !s.is_empty()));
    }

    #[test]
    fn default_protocol_enables_p2p_and_reflector() {
        let tl::enums::PhoneCallProtocol::Protocol(p) = default_call_protocol();
        assert!(p.udp_p2p && p.udp_reflector);
        assert!(p.max_layer >= 92);
    }
}
