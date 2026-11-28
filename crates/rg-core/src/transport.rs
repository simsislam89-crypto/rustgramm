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

/// Создаёт обфусцированный MTProto-транспорт `Obfuscated<SafeIntermediate>`.
pub fn obfuscated_transport() -> Box<dyn Transport + Send> {
    Box::new(Obfuscated::new(SafeIntermediate::new()))
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
}
