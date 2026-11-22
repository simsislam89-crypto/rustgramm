//! Секретные чаты Telegram (E2E MTProto 2.0):
//! - Обмен ключами Диффи — Хеллмана 2048 бит (`messages.getDhConfig`, `requestEncryption`, `acceptEncryption`);
//! - Отпечаток ключа (`key_fingerprint`), визуальная сетка 8×8 и 4 проверочных эмодзи;
//! - Шифрование и расшифровка сообщений MTProto 2.0 (`AES-256-IGE` + `msg_key`);
//! - Кодирование/декодирование TL-слоя `decryptedMessageLayer17`, `decryptedMessage73` и `setMessageTTL`.

use num_bigint::BigUint;
use sha1::{Digest as _, Sha1};
use sha2::Sha256;

/// Параметры 2048-битной группы Диффи — Хеллмана от сервера Telegram.
#[derive(Debug, Clone)]
pub struct DhParams {
    pub g: i32,
    pub p: Vec<u8>,
}

/// 64 проверяемых символа для визуальной сверки E2E-ключа (как в Telegram Secret Chats / Calls).
pub const KEY_EMOJIS: [&str; 64] = [
    "🐶", "🐱", "🐭", "🐹", "🐰", "🦊", "🐻", "🐼", "🐨", "🐯", "🦁", "🐮", "🐷", "🐸", "🐵", "🐔",
    "🐧", "🐦", "🐤", "🦆", "🦅", "🦉", "🦇", "🐺", "🐗", "🐴", "🦄", "🐝", "🐛", "🦋", "🐌", "🐞",
    "🐢", "🐍", "🦎", "🐙", "🦑", "🦐", "🦀", "🐡", "🐠", "🐟", "🐬", "🐳", "🐋", "🦈", "🐊", "🐅",
    "🐆", "🦓", "🦍", "🐘", "🦏", "🐪", "🦒", "🦘", "🐎", "🐖", "🐏", "🐑", "🐐", "🦌", "🐕", "🐈",
];

/// Валидация параметров `p` и `g` через `grammers_crypto::check_dh_prime`.
pub fn validate_dh_params(g: i32, p: &[u8]) -> Result<(), String> {
    let prime = BigUint::from_bytes_be(p);
    let ok = grammers_crypto::check_dh_prime(&prime, g).map_err(|e| format!("DH проверка: {e}"))?;
    if !ok {
        return Err("Сервер вернул небезопасные DH-параметры".to_owned());
    }
    Ok(())
}

/// Генерирует криптостойкий 256-байтный секретный экспонент `a` и публичное значение `g^a mod p` (256 байт BE).
pub fn generate_dh_keypair(g: i32, p: &[u8], server_random: &[u8]) -> Result<(Vec<u8>, Vec<u8>), String> {
    if p.len() != 256 || g < 2 {
        return Err("Некорректные параметры DH".to_owned());
    }
    let prime = BigUint::from_bytes_be(p);
    let mut priv_bytes = [0u8; 256];
    getrandom::fill(&mut priv_bytes).map_err(|e| format!("CSPRNG: {e}"))?;
    for (i, b) in priv_bytes.iter_mut().enumerate() {
        if let Some(&sr) = server_random.get(i) {
            *b ^= sr;
        }
    }
    // Устанавливаем старший бит, чтобы экспонент был гарантированно большим.
    priv_bytes[0] |= 0x80;
    let exp = BigUint::from_bytes_be(&priv_bytes);
    let base = BigUint::from(g as u32);
    let pub_val = base.modpow(&exp, &prime);
    check_dh_value(&pub_val, &prime)?;
    Ok((priv_bytes.to_vec(), to_256_be(&pub_val)))
}

/// Вычисляет общий 256-байтный ключ `g_other ^ our_exp mod p`.
pub fn compute_shared_key(g_other: &[u8], our_exp: &[u8], p: &[u8]) -> Result<[u8; 256], String> {
    if p.len() != 256 || g_other.is_empty() || our_exp.is_empty() {
        return Err("Некорректная длина параметров DH".to_owned());
    }
    let prime = BigUint::from_bytes_be(p);
    let peer = BigUint::from_bytes_be(g_other);
    check_dh_value(&peer, &prime)?;
    let exp = BigUint::from_bytes_be(our_exp);
    let shared = peer.modpow(&exp, &prime);
    let vec = to_256_be(&shared);
    let mut out = [0u8; 256];
    out.copy_from_slice(&vec);
    Ok(out)
}

/// Проверяет условие безопасности Telegram: `2^(2048-64) <= val <= p - 2^(2048-64)`.
fn check_dh_value(val: &BigUint, prime: &BigUint) -> Result<(), String> {
    let one = BigUint::from(1u32);
    if val <= &one || val >= &(prime - &one) {
        return Err("Небезопасное DH-значение вне (1, p-1)".to_owned());
    }
    let bound = BigUint::from(1u32) << (2048 - 64);
    if val < &bound || val > &(prime - &bound) {
        return Err("DH-значение слишком близко к краям группы".to_owned());
    }
    Ok(())
}

fn to_256_be(v: &BigUint) -> Vec<u8> {
    let raw = v.to_bytes_be();
    if raw.len() >= 256 {
        raw[raw.len() - 256..].to_vec()
    } else {
        let mut out = vec![0u8; 256];
        out[256 - raw.len()..].copy_from_slice(&raw);
        out
    }
}

/// 64-битный отпечаток ключа (`key_fingerprint`): младшие 8 байт `SHA1(auth_key)` в little-endian.
pub fn key_fingerprint(key: &[u8; 256]) -> i64 {
    let h = Sha1::digest(key);
    i64::from_le_bytes(h[12..20].try_into().unwrap_or([0; 8]))
}

/// 4 проверочных эмодзи E2E-ключа (алгоритм Telegram: `SHA256(key || g_a)` -> 4×64-битных индекса).
pub fn key_emojis(key: &[u8; 256], g_a: &[u8]) -> [&'static str; 4] {
    let mut hasher = Sha256::new();
    hasher.update(key);
    hasher.update(g_a);
    let h = hasher.finalize();
    let mut out = ["🐶"; 4];
    for (i, slot) in out.iter_mut().enumerate() {
        let chunk = u64::from_be_bytes(h[i * 8..(i + 1) * 8].try_into().unwrap_or([0; 8]));
        *slot = KEY_EMOJIS[(chunk % KEY_EMOJIS.len() as u64) as usize];
    }
    out
}

/// Визуальная сетка 8×8 (значения `0..=3` для 4 оттенков) для сверки ключа секретного чата.
pub fn key_visual_grid(key: &[u8; 256]) -> [[u8; 8]; 8] {
    let h1 = Sha1::digest(key);
    let h2 = Sha256::digest(key);
    let mut bits = [0u8; 32];
    bits[..16].copy_from_slice(&h1[..16]);
    bits[16..32].copy_from_slice(&h2[..16]);
    let mut grid = [[0u8; 8]; 8];
    for idx in 0..64 {
        let byte = bits[idx / 4];
        let shift = (idx % 4) * 2;
        grid[idx / 8][idx % 8] = (byte >> shift) & 0x03;
    }
    grid
}

/// Шестнадцатеричная строка отпечатка ключа (удобно сверять глазами).
pub fn key_hex_fingerprint(key: &[u8; 256]) -> String {
    let h = Sha256::digest(key);
    h[..16]
        .chunks(2)
        .map(|c| format!("{:02X}{:02X}", c[0], c[1]))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Вычисляет `aes_key` и `aes_iv` по спецификации MTProto 2.0 для секретных чатов.
fn secret_aes_key_iv(auth_key: &[u8; 256], msg_key: &[u8; 16], x: usize) -> ([u8; 32], [u8; 32]) {
    let mut ha = Sha256::new();
    ha.update(msg_key);
    ha.update(&auth_key[x..x + 36]);
    let sha256_a = ha.finalize();

    let mut hb = Sha256::new();
    hb.update(&auth_key[40 + x..40 + x + 36]);
    hb.update(msg_key);
    let sha256_b = hb.finalize();

    let mut aes_key = [0u8; 32];
    aes_key[0..8].copy_from_slice(&sha256_a[0..8]);
    aes_key[8..24].copy_from_slice(&sha256_b[8..24]);
    aes_key[24..32].copy_from_slice(&sha256_a[24..32]);

    let mut aes_iv = [0u8; 32];
    aes_iv[0..8].copy_from_slice(&sha256_b[0..8]);
    aes_iv[8..24].copy_from_slice(&sha256_a[8..24]);
    aes_iv[24..32].copy_from_slice(&sha256_b[24..32]);

    (aes_key, aes_iv)
}

/// Шифрует TL-полезную нагрузку секретного чата по MTProto 2.0:
/// `key_fingerprint(8) || msg_key(16) || AES_256_IGE(len_u32 || plaintext || random_padding)`.
pub fn encrypt_secret_payload(auth_key: &[u8; 256], plaintext_tl: &[u8], is_initiator: bool) -> Vec<u8> {
    let x = if is_initiator { 0 } else { 8 };
    let mut buf = Vec::with_capacity(4 + plaintext_tl.len() + 32);
    buf.extend_from_slice(&(plaintext_tl.len() as u32).to_le_bytes());
    buf.extend_from_slice(plaintext_tl);
    // MTProto 2.0 требует 12..1024 байт случайного паддинга и кратность 16 байтам.
    let rem = buf.len() % 16;
    let pad_len = if rem <= 4 { 16 - rem } else { 32 - rem };
    let mut pad = [0u8; 32];
    let _ = getrandom::fill(&mut pad);
    buf.extend_from_slice(&pad[..pad_len]);

    let mut mk_hash = Sha256::new();
    mk_hash.update(&auth_key[88 + x..88 + x + 32]);
    mk_hash.update(&buf);
    let mk_large = mk_hash.finalize();
    let mut msg_key = [0u8; 16];
    msg_key.copy_from_slice(&mk_large[8..24]);

    let (aes_key, aes_iv) = secret_aes_key_iv(auth_key, &msg_key, x);
    grammers_crypto::aes::ige_encrypt(&mut buf, &aes_key, &aes_iv);

    let fp = key_fingerprint(auth_key).to_le_bytes();
    let mut out = Vec::with_capacity(24 + buf.len());
    out.extend_from_slice(&fp);
    out.extend_from_slice(&msg_key);
    out.extend_from_slice(&buf);
    out
}

/// Расшифровывает входящий пакет секретного чата по MTProto 2.0 с проверкой `key_fingerprint` и `msg_key`.
pub fn decrypt_secret_payload(
    auth_key: &[u8; 256],
    data: &[u8],
    sender_is_initiator: bool,
) -> Result<Vec<u8>, String> {
    if data.len() < 40 || !(data.len() - 24).is_multiple_of(16) {
        return Err("Некорректный размер шифротекста секретного чата".to_owned());
    }
    let expected_fp = key_fingerprint(auth_key).to_le_bytes();
    if data[..8] != expected_fp {
        return Err("Отпечаток ключа секретного чата не совпадает".to_owned());
    }
    let mut msg_key = [0u8; 16];
    msg_key.copy_from_slice(&data[8..24]);
    let mut cipher = data[24..].to_vec();

    // Пробуем ожидаемую сторону (`x`), а при рассогласовании проверяем вторую (совместимость MTProto 2.0).
    let primary_x = if sender_is_initiator { 0 } else { 8 };
    let fallback_x = if sender_is_initiator { 8 } else { 0 };
    for x in [primary_x, fallback_x] {
        let mut plain = cipher.clone();
        let (aes_key, aes_iv) = secret_aes_key_iv(auth_key, &msg_key, x);
        grammers_crypto::aes::ige_decrypt(&mut plain, &aes_key, &aes_iv);
        let mut mk_hash = Sha256::new();
        mk_hash.update(&auth_key[88 + x..88 + x + 32]);
        mk_hash.update(&plain);
        let check = mk_hash.finalize();
        if check[8..24] == msg_key {
            let msg_len = u32::from_le_bytes(plain[..4].try_into().unwrap_or([0; 4])) as usize;
            if msg_len + 4 > plain.len() || plain.len() - (msg_len + 4) > 1024 {
                return Err("Некорректная длина расшифрованного сообщения".to_owned());
            }
            return Ok(plain[4..4 + msg_len].to_vec());
        }
        cipher = data[24..].to_vec();
    }
    Err("Проверка msg_key секретного чата не пройдена".to_owned())
}

// --- Компактный TL-кодер/декодер для `DecryptedMessageLayer` (слой 143) ---

const TL_DECRYPTED_LAYER: u32 = 0x1be3_1789;
const TL_DECRYPTED_MSG_73: u32 = 0x91cc_4674;
const TL_DECRYPTED_MSG_SERVICE: u32 = 0x7316_4160;
const TL_ACTION_SET_TTL: u32 = 0xa173_3aec;

fn write_tl_bytes(out: &mut Vec<u8>, bytes: &[u8]) {
    if bytes.len() < 254 {
        out.push(bytes.len() as u8);
        out.extend_from_slice(bytes);
        let pad = (4 - ((bytes.len() + 1) % 4)) % 4;
        out.resize(out.len() + pad, 0);
    } else {
        out.push(254);
        let len = bytes.len();
        out.push((len & 0xff) as u8);
        out.push(((len >> 8) & 0xff) as u8);
        out.push(((len >> 16) & 0xff) as u8);
        out.extend_from_slice(bytes);
        let pad = (4 - (bytes.len() % 4)) % 4;
        out.resize(out.len() + pad, 0);
    }
}

fn read_tl_bytes<'a>(buf: &'a [u8], pos: &mut usize) -> Option<&'a [u8]> {
    let first = *buf.get(*pos)?;
    if first < 254 {
        let len = usize::from(first);
        let start = *pos + 1;
        let end = start + len;
        if end > buf.len() {
            return None;
        }
        let pad = (4 - ((len + 1) % 4)) % 4;
        *pos = (end + pad).min(buf.len());
        Some(&buf[start..end])
    } else {
        if *pos + 4 > buf.len() {
            return None;
        }
        let len = usize::from(buf[*pos + 1])
            | (usize::from(buf[*pos + 2]) << 8)
            | (usize::from(buf[*pos + 3]) << 16);
        let start = *pos + 4;
        let end = start + len;
        if end > buf.len() {
            return None;
        }
        let pad = (4 - (len % 4)) % 4;
        *pos = (end + pad).min(buf.len());
        Some(&buf[start..end])
    }
}

fn read_u32(buf: &[u8], pos: &mut usize) -> Option<u32> {
    if *pos + 4 > buf.len() {
        return None;
    }
    let v = u32::from_le_bytes(buf[*pos..*pos + 4].try_into().ok()?);
    *pos += 4;
    Some(v)
}

fn read_i64(buf: &[u8], pos: &mut usize) -> Option<i64> {
    if *pos + 8 > buf.len() {
        return None;
    }
    let v = i64::from_le_bytes(buf[*pos..*pos + 8].try_into().ok()?);
    *pos += 8;
    Some(v)
}

/// Расшифрованное содержимое секретного сообщения.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SecretPayload {
    Text {
        random_id: i64,
        ttl: i32,
        text: String,
    },
    SetTtl {
        random_id: i64,
        ttl: i32,
    },
    Unknown,
}

/// Упаковывает текстовое сообщение секретного чата в `decryptedMessageLayer17(decryptedMessage73)`.
pub fn encode_secret_text(random_id: i64, ttl: i32, text: &str, in_seq: i32, out_seq: i32) -> Vec<u8> {
    let mut rnd = [0u8; 15];
    let _ = getrandom::fill(&mut rnd);
    let mut out = Vec::with_capacity(64 + text.len());
    out.extend_from_slice(&TL_DECRYPTED_LAYER.to_le_bytes());
    write_tl_bytes(&mut out, &rnd);
    out.extend_from_slice(&143i32.to_le_bytes());
    out.extend_from_slice(&in_seq.to_le_bytes());
    out.extend_from_slice(&out_seq.to_le_bytes());
    // decryptedMessage#91cc4674 flags:# random_id:long ttl:int message:string ...
    out.extend_from_slice(&TL_DECRYPTED_MSG_73.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes()); // flags = 0
    out.extend_from_slice(&random_id.to_le_bytes());
    out.extend_from_slice(&ttl.to_le_bytes());
    write_tl_bytes(&mut out, text.as_bytes());
    out
}

/// Упаковывает сервисное сообщение смены таймера самоуничтожения (`setMessageTTL`).
pub fn encode_secret_set_ttl(random_id: i64, ttl: i32, in_seq: i32, out_seq: i32) -> Vec<u8> {
    let mut rnd = [0u8; 15];
    let _ = getrandom::fill(&mut rnd);
    let mut out = Vec::with_capacity(64);
    out.extend_from_slice(&TL_DECRYPTED_LAYER.to_le_bytes());
    write_tl_bytes(&mut out, &rnd);
    out.extend_from_slice(&143i32.to_le_bytes());
    out.extend_from_slice(&in_seq.to_le_bytes());
    out.extend_from_slice(&out_seq.to_le_bytes());
    out.extend_from_slice(&TL_DECRYPTED_MSG_SERVICE.to_le_bytes());
    out.extend_from_slice(&random_id.to_le_bytes());
    out.extend_from_slice(&TL_ACTION_SET_TTL.to_le_bytes());
    out.extend_from_slice(&ttl.to_le_bytes());
    out
}

/// Декодирует расшифрованный TL-буфер секретного сообщения (слой 17+ или базовое сообщение).
pub fn decode_secret_payload(buf: &[u8]) -> SecretPayload {
    let mut pos = 0usize;
    let Some(mut cid) = read_u32(buf, &mut pos) else {
        return SecretPayload::Unknown;
    };
    if cid == TL_DECRYPTED_LAYER {
        let _ = read_tl_bytes(buf, &mut pos); // random_bytes
        let _ = read_u32(buf, &mut pos); // layer
        let _ = read_u32(buf, &mut pos); // in_seq_no
        let _ = read_u32(buf, &mut pos); // out_seq_no
        let Some(inner) = read_u32(buf, &mut pos) else {
            return SecretPayload::Unknown;
        };
        cid = inner;
    }
    match cid {
        TL_DECRYPTED_MSG_73 => {
            let _flags = read_u32(buf, &mut pos).unwrap_or(0);
            let random_id = read_i64(buf, &mut pos).unwrap_or(0);
            let ttl = read_u32(buf, &mut pos).unwrap_or(0) as i32;
            let text = read_tl_bytes(buf, &mut pos)
                .map(|b| String::from_utf8_lossy(b).into_owned())
                .unwrap_or_default();
            SecretPayload::Text { random_id, ttl, text }
        }
        TL_DECRYPTED_MSG_SERVICE => {
            let random_id = read_i64(buf, &mut pos).unwrap_or(0);
            let action = read_u32(buf, &mut pos).unwrap_or(0);
            if action == TL_ACTION_SET_TTL {
                let ttl = read_u32(buf, &mut pos).unwrap_or(0) as i32;
                SecretPayload::SetTtl { random_id, ttl }
            } else {
                SecretPayload::Unknown
            }
        }
        // Совместимость со старым слоем `decryptedMessage17#204d3878` и `decryptedMessage45#36b091de`.
        0x204d_3878 => {
            let random_id = read_i64(buf, &mut pos).unwrap_or(0);
            let ttl = read_u32(buf, &mut pos).unwrap_or(0) as i32;
            let text = read_tl_bytes(buf, &mut pos)
                .map(|b| String::from_utf8_lossy(b).into_owned())
                .unwrap_or_default();
            SecretPayload::Text { random_id, ttl, text }
        }
        0x36b0_91de => {
            let _flags = read_u32(buf, &mut pos).unwrap_or(0);
            let random_id = read_i64(buf, &mut pos).unwrap_or(0);
            let ttl = read_u32(buf, &mut pos).unwrap_or(0) as i32;
            let text = read_tl_bytes(buf, &mut pos)
                .map(|b| String::from_utf8_lossy(b).into_owned())
                .unwrap_or_default();
            SecretPayload::Text { random_id, ttl, text }
        }
        _ => SecretPayload::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mtproto2_secret_encrypt_decrypt_roundtrip_both_sides() {
        let mut key = [0u8; 256];
        for (i, b) in key.iter_mut().enumerate() {
            *b = ((i * 37 + 13) & 0xff) as u8;
        }
        let tl = encode_secret_text(424242, 30, "Секретное сообщение E2E!", 0, 2);
        let ct = encrypt_secret_payload(&key, &tl, true);
        let pt = decrypt_secret_payload(&key, &ct, true).unwrap();
        assert_eq!(pt, tl);
        assert_eq!(
            decode_secret_payload(&pt),
            SecretPayload::Text {
                random_id: 424242,
                ttl: 30,
                text: "Секретное сообщение E2E!".to_owned(),
            }
        );

        let ttl_tl = encode_secret_set_ttl(999, 60, 2, 4);
        let ct2 = encrypt_secret_payload(&key, &ttl_tl, false);
        let pt2 = decrypt_secret_payload(&key, &ct2, false).unwrap();
        assert_eq!(
            decode_secret_payload(&pt2),
            SecretPayload::SetTtl {
                random_id: 999,
                ttl: 60,
            }
        );
    }

    #[test]
    fn tampered_secret_ciphertext_is_rejected() {
        let key = [0x5Au8; 256];
        let tl = encode_secret_text(1, 0, "hello", 0, 0);
        let mut ct = encrypt_secret_payload(&key, &tl, true);
        let last = ct.len() - 1;
        ct[last] ^= 0xFF;
        assert!(decrypt_secret_payload(&key, &ct, true).is_err());
    }

    #[test]
    fn key_visual_grid_and_emojis_are_deterministic() {
        let key = [0x33u8; 256];
        let g_a = [0x77u8; 256];
        let e1 = key_emojis(&key, &g_a);
        let e2 = key_emojis(&key, &g_a);
        assert_eq!(e1, e2);
        let grid = key_visual_grid(&key);
        for row in grid {
            for cell in row {
                assert!(cell <= 3);
            }
        }
        assert_eq!(key_hex_fingerprint(&key).split_whitespace().count(), 8);
    }
}
