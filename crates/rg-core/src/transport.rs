//! Безопасный обфусцированный MTProto-транспорт поверх Исправленного Intermediate.
//!
//! В `grammers-mtproto 0.12.0`:
//! - `Abridged::unpack` проверяет `if header_len == 1 && len >= 4` вместо `len == 4`, из-за чего
//!   ~50% обычных MTProto-пакетов (у которых низкие 32 бита `auth_key_id` имеют старший бит 1,
//!   например `-467886238`) ошибочно падают с `Error::BadStatus` на первом же ответе сервера.
//! - `Intermediate::unpack` сравнивает `buffer.len() < len` вместо `buffer.len() < 4 + len`.
//!
//! `SafeIntermediate` устраняет оба бага и оборачивается в стандартную AES-256-CTR обфускацию
//! `Obfuscated::new(SafeIntermediate::new())`.

use grammers_mtproto::DequeBuffer;
use grammers_mtproto::transport::{Error, Obfuscated, Tagged, Transport, UnpackedOffset};

const MAX_PACKET_LEN: i32 = 16 * 1024 * 1024;

/// Безопасная реализация Telegram Intermediate MTProto transport.
pub struct SafeIntermediate {
    init: bool,
}

impl SafeIntermediate {
    const TAG: [u8; 4] = 0xee_ee_ee_ee_u32.to_le_bytes();

    #[must_use]
    pub fn new() -> Self {
        Self { init: false }
    }
}

impl Default for SafeIntermediate {
    fn default() -> Self {
        Self::new()
    }
}

impl Transport for SafeIntermediate {
    fn pack(&mut self, buffer: &mut DequeBuffer<u8>) {
        let len = buffer.len();
        assert_eq!(len % 4, 0, "MTProto payload must be 4-byte aligned");

        buffer.extend_front(&(len as i32).to_le_bytes());

        if !self.init {
            buffer.extend_front(&Self::TAG);
            self.init = true;
        }
    }

    fn unpack(&mut self, buffer: &mut [u8]) -> Result<UnpackedOffset, Error> {
        if buffer.len() < 4 {
            return Err(Error::MissingBytes);
        }

        let len = i32::from_le_bytes(buffer[0..4].try_into().unwrap());
        if len < 0 {
            if (-len) <= 1024 {
                return Err(Error::BadStatus {
                    status: (-len) as u32,
                });
            }
            return Err(Error::BadLen { got: len });
        }
        if !(4..=MAX_PACKET_LEN).contains(&len) {
            return Err(Error::BadLen { got: len });
        }

        let need = 4_usize + (len as usize);
        if buffer.len() < need {
            return Err(Error::MissingBytes);
        }

        // Реальные ошибки транспортного уровня передаются 4-байтным отрицательным кодом (e.g. -404/-429).
        if len == 4 {
            let data = i32::from_le_bytes(buffer[4..8].try_into().unwrap());
            if data < 0 {
                return Err(Error::BadStatus {
                    status: (-data) as u32,
                });
            }
        }

        Ok(UnpackedOffset {
            data_range: 4..need,
            next_offset: need,
        })
    }
}

impl Tagged for SafeIntermediate {
    fn init_tag(&mut self) -> [u8; 4] {
        self.init = true;
        Self::TAG
    }
}

/// Создаёт необфусцированный MTProto-транспорт `SafeIntermediate`.
pub fn plain_intermediate_transport() -> Box<dyn Transport + Send> {
    Box::new(SafeIntermediate::new())
}

/// Создаёт обфусцированный MTProto-транспорт `Obfuscated<SafeIntermediate>`.
pub fn obfuscated_transport() -> Box<dyn Transport + Send> {
    Box::new(Obfuscated::new(SafeIntermediate::new()))
}

/// Выбирает транспорт MTProto (`Obfuscated<SafeIntermediate>` или обычный `SafeIntermediate`).
pub fn transport_for(obfuscated: bool) -> Box<dyn Transport + Send> {
    if obfuscated {
        obfuscated_transport()
    } else {
        plain_intermediate_transport()
    }
}

/// Формирует реальный незашифрованный пакет MTProto `req_pq_multi#be7e8ef1` для живой проверки DC.
/// Возвращает готовые к отправке байты и парсер транспорта для проверки `resPQ`.
pub fn build_req_pq_probe(obfuscated: bool) -> (Vec<u8>, Box<dyn Transport + Send>) {
    const REQ_PQ_MULTI_ID: u32 = 0xbe7e_8ef1;
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(1_760_000_000_i64, |d| d.as_secs() as i64);
    let msg_id = (secs << 32) & !3_i64;

    let mut payload = Vec::with_capacity(40);
    payload.extend_from_slice(&0_i64.to_le_bytes()); // auth_key_id = 0
    payload.extend_from_slice(&msg_id.to_le_bytes()); // message_id
    payload.extend_from_slice(&20_i32.to_le_bytes()); // message_data_length
    payload.extend_from_slice(&REQ_PQ_MULTI_ID.to_le_bytes());
    let nonce_hi = secs.wrapping_mul(0x517c_c1b7_2722_0a95_u64 as i64) as u64;
    let nonce_lo = (secs ^ 0x4d59_5df4_d0f3_3173_i64) as u64;
    payload.extend_from_slice(&nonce_hi.to_le_bytes());
    payload.extend_from_slice(&nonce_lo.to_le_bytes());

    let mut tr = transport_for(obfuscated);
    let mut buf = DequeBuffer::with_capacity(payload.len(), 80);
    buf.extend_back(&payload);
    tr.pack(&mut buf);
    (buf.as_ref().to_vec(), tr)
}

/// Проверяет, что сырые байты от сервера содержат корректный незашифрованный ответ `resPQ#05162463`.
/// Отбрасывает «мёртвые» сокеты (например IPv6, принимающий TCP и закрывающий соединение с 0 байтами).
pub fn is_valid_res_pq_reply(transport: &mut dyn Transport, raw: &mut [u8]) -> bool {
    const RES_PQ_ID: u32 = 0x0516_2463;
    let Ok(off) = transport.unpack(raw) else {
        return false;
    };
    let body = &raw[off.data_range];
    if body.len() < 24 || body[0..8] != [0_u8; 8] {
        return false;
    }
    let msg_len = i32::from_le_bytes(body[16..20].try_into().unwrap());
    if msg_len < 20 {
        return false;
    }
    u32::from_le_bytes(body[20..24].try_into().unwrap()) == RES_PQ_ID
}

#[cfg(test)]
mod tests {
    use super::*;
    use grammers_mtproto::transport::Abridged;

    #[test]
    fn reproduces_upstream_abridged_negative_auth_key_bug_and_safe_intermediate_passes() {
        // Обычный 40-байтный ответ MTProto, где первые 4 байта auth_key_id = -467886238 (из скриншота Android).
        let mut payload = vec![0xAA_u8; 40];
        payload[0..4].copy_from_slice(&(-467_886_238_i32).to_le_bytes());

        // Доказательство причины: upstream Abridged ошибочно падает с BadStatus { status: 467886238 }.
        let mut abridged = Abridged::new();
        let mut buf_abridged = DequeBuffer::with_capacity(payload.len(), 8);
        buf_abridged.extend_back(&payload);
        abridged.pack(&mut buf_abridged);
        assert_eq!(
            abridged.unpack(&mut buf_abridged.as_mut()[1..]),
            Err(Error::BadStatus { status: 467_886_238 })
        );

        // SafeIntermediate корректно упаковывает и распаковывает тот же 40-байтный пакет.
        let mut safe = SafeIntermediate::new();
        let mut buf_safe = DequeBuffer::with_capacity(payload.len(), 16);
        buf_safe.extend_back(&payload);
        safe.pack(&mut buf_safe);
        assert_eq!(&buf_safe.as_ref()[0..4], &0xee_ee_ee_ee_u32.to_le_bytes());
        let off = safe.unpack(&mut buf_safe.as_mut()[4..]).expect("must unpack 40 bytes");
        assert_eq!(&buf_safe.as_ref()[4 + off.data_range.start..4 + off.data_range.end], payload.as_slice());

        // В паре с Obfuscated транспорт также строится без паник и с корректным 64-байтным заголовком.
        let mut obf = Obfuscated::new(SafeIntermediate::new());
        let mut buf_obf = DequeBuffer::with_capacity(payload.len(), 80);
        buf_obf.extend_back(&payload);
        obf.pack(&mut buf_obf);
        assert_eq!(buf_obf.len(), 64 + 4 + payload.len());
    }

    #[test]
    fn safe_intermediate_detects_real_4byte_transport_errors() {
        let mut safe = SafeIntermediate::new();
        for code in [404_i32, 429_i32] {
            let mut pkt = Vec::with_capacity(8);
            pkt.extend_from_slice(&4_i32.to_le_bytes());
            pkt.extend_from_slice(&(-code).to_le_bytes());
            assert_eq!(
                safe.unpack(&mut pkt),
                Err(Error::BadStatus { status: code as u32 })
            );
        }
    }

    #[test]
    fn safe_intermediate_handles_partial_buffers_without_panic() {
        let mut safe = SafeIntermediate::new();
        // Длина = 16 байт, но передано ровно 16 байт (не хватает 4 байт для 4 + 16 = 20).
        let mut partial = Vec::with_capacity(16);
        partial.extend_from_slice(&16_i32.to_le_bytes());
        partial.extend_from_slice(&[0u8; 12]);
        assert_eq!(safe.unpack(&mut partial), Err(Error::MissingBytes));
    }

    #[test]
    fn req_pq_probe_roundtrip_and_rejects_empty_dead_ipv6_reply() {
        let (obf_pkt, mut obf_tr) = build_req_pq_probe(true);
        assert!(obf_pkt.len() >= 68);
        assert!(!is_valid_res_pq_reply(&mut *obf_tr, &mut []));

        let (plain_pkt, mut plain_tr) = build_req_pq_probe(false);
        assert_eq!(plain_pkt.len(), 48);
        assert_eq!(&plain_pkt[0..4], &0xee_ee_ee_ee_u32.to_le_bytes());
        assert!(!is_valid_res_pq_reply(&mut *plain_tr, &mut []));

        // Синтетический ответ resPQ#05162463 через SafeIntermediate должен проходить проверку.
        let mut srv_tr = SafeIntermediate::new();
        let mut res_pq = vec![0_u8; 64];
        res_pq[16..20].copy_from_slice(&44_i32.to_le_bytes());
        res_pq[20..24].copy_from_slice(&0x0516_2463_u32.to_le_bytes());
        let mut wire = DequeBuffer::with_capacity(res_pq.len(), 8);
        wire.extend_back(&res_pq);
        srv_tr.pack(&mut wire);
        // Срезаем первые 4 байта серверного init-тега, т.к. серверный Intermediate шлёт только length+body.
        let mut raw_reply = wire.as_ref()[4..].to_vec();
        assert!(is_valid_res_pq_reply(&mut *plain_tr, &mut raw_reply));

        // Испорченный конструктор (не resPQ) должен отклоняться.
        res_pq[20..24].copy_from_slice(&0xdead_beef_u32.to_le_bytes());
        let mut bad_wire = DequeBuffer::with_capacity(res_pq.len(), 8);
        bad_wire.extend_back(&res_pq);
        srv_tr.pack(&mut bad_wire);
        assert!(!is_valid_res_pq_reply(&mut *plain_tr, &mut bad_wire.as_mut()[..]));
    }
}
