//! Нативный медиа-поток звонков (WebRTC STUN/ICE + шифрованный UDP-транспорт рефлекторов Telegram).
//! Использует общий 256-байтный DH-ключ звонка (`rg-core::calls` + AES-256-IGE из `rg-core::secret`)
//! для упаковки/расшифровки 20-мс голосовых кадров (16 кГц моно -> компактный IMA-ADPCM) и
//! замера реального RTT канала по `tokio::net::UdpSocket`.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tokio::net::UdpSocket;
use tokio::sync::mpsc;

use crate::calls::CallEndpoint;
use crate::secret::{decrypt_secret_payload, encrypt_secret_payload};

const STUN_MAGIC_COOKIE: u32 = 0x2112_A442;
const PKT_MAGIC: [u8; 2] = [0x52, 0x47];
const KIND_KEEPALIVE: u8 = 0;
const KIND_AUDIO_ADPCM: u8 = 1;

/// Параметры запуска нативного медиа-потока звонка.
#[derive(Clone)]
pub struct CallMediaConfig {
    pub shared_key: [u8; 256],
    pub endpoints: Vec<CallEndpoint>,
    pub p2p_allowed: bool,
    pub is_outgoing: bool,
    pub video: bool,
    /// Режим повышенной приватности: использовать только рефлекторы/TURN Telegram, скрывая локальный IP от собеседника.
    pub relay_only_privacy: bool,
}

/// Живая статистика UDP/WebRTC канала звонка для отображения в UI.
#[derive(Debug, Clone, Default)]
pub struct CallMediaStats {
    pub transport_label: String,
    pub connected: bool,
    pub rtt_ms: u32,
    pub tx_packets: u64,
    pub rx_packets: u64,
    pub tx_bytes: u64,
    pub rx_bytes: u64,
    pub mapped_addr: String,
    pub last_error: String,
}

/// Выбирает допустимые эндпоинты с учётом `p2p_allowed` и режима приватности (скрытие IP).
pub fn select_endpoints(endpoints: &[CallEndpoint], allow_p2p: bool) -> Vec<CallEndpoint> {
    let mut relays: Vec<CallEndpoint> = endpoints
        .iter()
        .filter(|e| !e.tcp && e.port > 0 && (allow_p2p || !e.is_webrtc || e.turn))
        .cloned()
        .collect();
    // Приоритет: UDP-рефлекторы Telegram с `peer_tag` и TURN, затем STUN.
    relays.sort_by_key(|e| {
        if !e.is_webrtc && !e.peer_tag.is_empty() {
            0u8
        } else if e.turn {
            1
        } else {
            2
        }
    });
    relays
}

/// Собирает STUN Binding Request (RFC 5389) с 96-битным идентификатором транзакции.
pub fn build_stun_binding_request(tx_id: [u8; 12]) -> [u8; 20] {
    let mut pkt = [0u8; 20];
    pkt[0..2].copy_from_slice(&0x0001u16.to_be_bytes()); // Binding Request
    pkt[2..4].copy_from_slice(&0u16.to_be_bytes()); // Длина атрибутов = 0
    pkt[4..8].copy_from_slice(&STUN_MAGIC_COOKIE.to_be_bytes());
    pkt[8..20].copy_from_slice(&tx_id);
    pkt
}

/// Проверяет, является ли пакет STUN Binding Success Response для данного `tx_id`.
pub fn is_stun_binding_response(pkt: &[u8], expected_tx: [u8; 12]) -> bool {
    pkt.len() >= 20
        && pkt[0..2] == 0x0101u16.to_be_bytes()
        && pkt[4..8] == STUN_MAGIC_COOKIE.to_be_bytes()
        && pkt[8..20] == expected_tx
}

/// Извлекает `XOR-MAPPED-ADDRESS` (IPv4) из ответа STUN (если сервер его вернул).
pub fn parse_stun_xor_mapped_addr(pkt: &[u8]) -> Option<SocketAddr> {
    if pkt.len() < 20 || pkt[4..8] != STUN_MAGIC_COOKIE.to_be_bytes() {
        return None;
    }
    let msg_len = u16::from_be_bytes([pkt[2], pkt[3]]) as usize;
    let end = (20 + msg_len).min(pkt.len());
    let mut pos = 20usize;
    while pos + 4 <= end {
        let attr_type = u16::from_be_bytes([pkt[pos], pkt[pos + 1]]);
        let attr_len = u16::from_be_bytes([pkt[pos + 2], pkt[pos + 3]]) as usize;
        pos += 4;
        if pos + attr_len > end {
            break;
        }
        // XOR-MAPPED-ADDRESS (0x0020), family 0x01 (IPv4)
        if attr_type == 0x0020 && attr_len >= 8 && pkt[pos + 1] == 0x01 {
            let xport = u16::from_be_bytes([pkt[pos + 2], pkt[pos + 3]]) ^ (STUN_MAGIC_COOKIE >> 16) as u16;
            let xip = u32::from_be_bytes([pkt[pos + 4], pkt[pos + 5], pkt[pos + 6], pkt[pos + 7]])
                ^ STUN_MAGIC_COOKIE;
            return Some(SocketAddr::new(IpAddr::V4(Ipv4Addr::from(xip)), xport));
        }
        let pad = (4 - (attr_len % 4)) % 4;
        pos = (pos + attr_len + pad).min(end);
    }
    None
}

// --- Компактный IMA-ADPCM кодер/декодер (4:1 сжатие 16-бит PCM для 20-мс UDP кадров) ---

const IMA_STEP_TABLE: [i32; 89] = [
    7, 8, 9, 10, 11, 12, 13, 14, 16, 17, 19, 21, 23, 25, 28, 31, 34, 37, 41, 45, 50, 55, 60, 66,
    73, 80, 88, 97, 107, 118, 130, 143, 157, 173, 190, 209, 230, 253, 279, 307, 337, 371, 408, 449,
    494, 544, 598, 658, 724, 796, 876, 963, 1060, 1166, 1282, 1411, 1552, 1707, 1878, 2066, 2272,
    2499, 2749, 3024, 3327, 3660, 4026, 4428, 4871, 5358, 5894, 6484, 7132, 7845, 8630, 9493,
    10442, 11487, 12635, 13899, 15289, 16818, 18500, 20350, 22385, 24623, 27086, 29794, 32767,
];

const IMA_INDEX_TABLE: [i32; 16] = [-1, -1, -1, -1, 2, 4, 6, 8, -1, -1, -1, -1, 2, 4, 6, 8];

/// Сжимает PCM16-кадр в компактный блок `[pred: i16 LE, index: u8, count: u16 LE, nibbles...]`.
pub fn encode_adpcm_frame(samples: &[i16]) -> Vec<u8> {
    if samples.is_empty() {
        return Vec::new();
    }
    let mut pred = i32::from(samples[0]);
    let init_delta = samples
        .get(1)
        .map_or(0, |&s| (i32::from(s) - pred).abs());
    let mut index = IMA_STEP_TABLE
        .iter()
        .position(|&st| st >= init_delta / 2)
        .unwrap_or(0) as i32;
    let mut out = Vec::with_capacity(5 + samples.len().div_ceil(2));
    out.extend_from_slice(&samples[0].to_le_bytes());
    out.push(index as u8);
    out.extend_from_slice(&(samples.len() as u16).to_le_bytes());

    let mut low_nibble: Option<u8> = None;
    for &s in &samples[1..] {
        let step = IMA_STEP_TABLE[index as usize];
        let mut diff = i32::from(s) - pred;
        let mut code = 0u8;
        if diff < 0 {
            code = 8;
            diff = -diff;
        }
        let mut vpdiff = step >> 3;
        if diff >= step {
            code |= 4;
            diff -= step;
            vpdiff += step;
        }
        if diff >= (step >> 1) {
            code |= 2;
            diff -= step >> 1;
            vpdiff += step >> 1;
        }
        if diff >= (step >> 2) {
            code |= 1;
            vpdiff += step >> 2;
        }
        if (code & 8) != 0 {
            pred -= vpdiff;
        } else {
            pred += vpdiff;
        }
        pred = pred.clamp(-32_768, 32_767);
        index = (index + IMA_INDEX_TABLE[code as usize]).clamp(0, 88);

        if let Some(lo) = low_nibble.take() {
            out.push(lo | (code << 4));
        } else {
            low_nibble = Some(code & 0x0f);
        }
    }
    if let Some(lo) = low_nibble {
        out.push(lo);
    }
    out
}

/// Декодирует ADPCM-блок обратно в PCM16 сэмплы.
pub fn decode_adpcm_frame(data: &[u8]) -> Option<Vec<i16>> {
    if data.len() < 5 {
        return None;
    }
    let first = i16::from_le_bytes([data[0], data[1]]);
    let mut pred = i32::from(first);
    let mut index = i32::from(data[2]).clamp(0, 88);
    let count = usize::from(u16::from_le_bytes([data[3], data[4]]));
    if count == 0 || count > 4096 {
        return None;
    }
    let mut out = Vec::with_capacity(count);
    out.push(first);
    let nibbles = &data[5..];
    for i in 0..(count - 1) {
        let byte = *nibbles.get(i / 2)?;
        let code = if i % 2 == 0 { byte & 0x0f } else { (byte >> 4) & 0x0f };
        let step = IMA_STEP_TABLE[index as usize];
        let mut vpdiff = step >> 3;
        if (code & 4) != 0 {
            vpdiff += step;
        }
        if (code & 2) != 0 {
            vpdiff += step >> 1;
        }
        if (code & 1) != 0 {
            vpdiff += step >> 2;
        }
        if (code & 8) != 0 {
            pred -= vpdiff;
        } else {
            pred += vpdiff;
        }
        pred = pred.clamp(-32_768, 32_767);
        index = (index + IMA_INDEX_TABLE[code as usize]).clamp(0, 88);
        out.push(pred as i16);
    }
    Some(out)
}

/// Упаковывает и шифрует голосовой/служебный UDP-кадр по MTProto 2.0 (`peer_tag` + AES-256-IGE).
pub fn seal_media_packet(
    shared_key: &[u8; 256],
    is_outgoing: bool,
    peer_tag: &[u8],
    kind: u8,
    seq: u32,
    ts_ms: u32,
    payload: &[u8],
) -> Vec<u8> {
    let mut plain = Vec::with_capacity(11 + payload.len());
    plain.extend_from_slice(&PKT_MAGIC);
    plain.push(kind);
    plain.extend_from_slice(&seq.to_le_bytes());
    plain.extend_from_slice(&ts_ms.to_le_bytes());
    plain.extend_from_slice(payload);
    let encrypted = encrypt_secret_payload(shared_key, &plain, is_outgoing);
    if peer_tag.is_empty() {
        encrypted
    } else {
        let mut out = Vec::with_capacity(peer_tag.len() + encrypted.len());
        out.extend_from_slice(peer_tag);
        out.extend_from_slice(&encrypted);
        out
    }
}

/// Расшифровывает входящий UDP-пакет звонка (с учётом опционального 16-байтного `peer_tag` рефлектора).
pub fn open_media_packet(
    shared_key: &[u8; 256],
    sender_is_outgoing: bool,
    peer_tag: &[u8],
    packet: &[u8],
) -> Option<(u8, u32, u32, Vec<u8>)> {
    let body = if !peer_tag.is_empty() && packet.starts_with(peer_tag) {
        &packet[peer_tag.len()..]
    } else {
        packet
    };
    let plain = decrypt_secret_payload(shared_key, body, sender_is_outgoing).ok()?;
    if plain.len() < 11 || plain[0..2] != PKT_MAGIC {
        return None;
    }
    let kind = plain[2];
    let seq = u32::from_le_bytes(plain[3..7].try_into().ok()?);
    let ts_ms = u32::from_le_bytes(plain[7..11].try_into().ok()?);
    Some((kind, seq, ts_ms, plain[11..].to_vec()))
}

/// Управляемый сеанс нативного UDP/WebRTC медиа-потока звонка.
pub struct CallMediaSession {
    stop: Arc<AtomicBool>,
    stats: Arc<Mutex<CallMediaStats>>,
    tx_audio: Option<mpsc::Sender<Vec<i16>>>,
}

impl CallMediaSession {
    /// Запускает асинхронный UDP-транспорт в текущем Tokio-рантайме.
    /// `rx_playback` получает расшифрованные входящие PCM16 кадры (16 кГц моно).
    pub fn start(
        cfg: CallMediaConfig,
        rx_playback: mpsc::Sender<Vec<i16>>,
    ) -> Result<Self, String> {
        let allow_p2p = cfg.p2p_allowed && !cfg.relay_only_privacy;
        let endpoints = select_endpoints(&cfg.endpoints, allow_p2p);
        let stop = Arc::new(AtomicBool::new(false));
        let first_label = endpoints
            .first()
            .map_or("MTProto Signaling", CallEndpoint::transport_kind)
            .to_owned();
        let stats = Arc::new(Mutex::new(CallMediaStats {
            transport_label: first_label,
            ..Default::default()
        }));
        let (tx_audio, mut rx_mic) = mpsc::channel::<Vec<i16>>(32);

        let stop_bg = stop.clone();
        let stats_bg = stats.clone();

        tokio::spawn(async move {
            let Ok(sock) = UdpSocket::bind("0.0.0.0:0").await else {
                if let Ok(mut g) = stats_bg.lock() {
                    g.last_error = "Не удалось открыть UDP-сокет".to_owned();
                }
                return;
            };
            let primary = endpoints
                .iter()
                .find_map(|e| e.socket_addr_v4().map(|a| (a, e.clone())));

            let mut stun_tx = [0u8; 12];
            let _ = getrandom::fill(&mut stun_tx);
            let mut seq = 1u32;
            let started = Instant::now();
            let mut probe_tick = tokio::time::interval(Duration::from_millis(900));
            let mut last_probe_at = Instant::now();
            let mut buf = vec![0u8; 2048];

            // Первичный STUN / Relay handshake для всех кандидатов.
            for ep in endpoints.iter().take(4) {
                if let Some(addr) = ep.socket_addr_v4() {
                    if ep.is_webrtc && ep.stun {
                        let req = build_stun_binding_request(stun_tx);
                        if let Ok(n) = sock.send_to(&req, addr).await
                            && let Ok(mut g) = stats_bg.lock()
                        {
                            g.tx_packets += 1;
                            g.tx_bytes += n as u64;
                        }
                    } else {
                        let pkt = seal_media_packet(
                            &cfg.shared_key,
                            cfg.is_outgoing,
                            &ep.peer_tag,
                            KIND_KEEPALIVE,
                            seq,
                            0,
                            b"rg-ping",
                        );
                        seq = seq.wrapping_add(1);
                        if let Ok(n) = sock.send_to(&pkt, addr).await
                            && let Ok(mut g) = stats_bg.lock()
                        {
                            g.tx_packets += 1;
                            g.tx_bytes += n as u64;
                        }
                    }
                }
            }

            while !stop_bg.load(Ordering::Relaxed) {
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_millis(30)), if stop_bg.load(Ordering::Relaxed) => break,
                    _ = probe_tick.tick() => {
                        if stop_bg.load(Ordering::Relaxed) {
                            break;
                        }
                        if let Some((addr, ref ep)) = primary {
                            last_probe_at = Instant::now();
                            let ts_ms = started.elapsed().as_millis() as u32;
                            if ep.is_webrtc && ep.stun {
                                let req = build_stun_binding_request(stun_tx);
                                if let Ok(n) = sock.send_to(&req, addr).await
                                    && let Ok(mut g) = stats_bg.lock()
                                {
                                    g.tx_packets += 1;
                                    g.tx_bytes += n as u64;
                                }
                            } else {
                                let pkt = seal_media_packet(
                                    &cfg.shared_key,
                                    cfg.is_outgoing,
                                    &ep.peer_tag,
                                    KIND_KEEPALIVE,
                                    seq,
                                    ts_ms,
                                    b"rg-ka",
                                );
                                seq = seq.wrapping_add(1);
                                if let Ok(n) = sock.send_to(&pkt, addr).await
                                    && let Ok(mut g) = stats_bg.lock()
                                {
                                    g.tx_packets += 1;
                                    g.tx_bytes += n as u64;
                                }
                            }
                        }
                    }
                    Some(pcm) = rx_mic.recv() => {
                        if stop_bg.load(Ordering::Relaxed) {
                            break;
                        }
                        if let Some((addr, ref ep)) = primary {
                            let encoded = encode_adpcm_frame(&pcm);
                            let ts_ms = started.elapsed().as_millis() as u32;
                            let pkt = seal_media_packet(
                                &cfg.shared_key,
                                cfg.is_outgoing,
                                &ep.peer_tag,
                                KIND_AUDIO_ADPCM,
                                seq,
                                ts_ms,
                                &encoded,
                            );
                            seq = seq.wrapping_add(1);
                            if let Ok(n) = sock.send_to(&pkt, addr).await
                                && let Ok(mut g) = stats_bg.lock()
                            {
                                g.tx_packets += 1;
                                g.tx_bytes += n as u64;
                            }
                        }
                    }
                    recv_res = sock.recv_from(&mut buf) => {
                        let Ok((n, _from)) = recv_res else { continue };
                        let pkt = &buf[..n];
                        let rtt = last_probe_at.elapsed().as_millis().min(9999) as u32;
                        if is_stun_binding_response(pkt, stun_tx) {
                            let mapped = parse_stun_xor_mapped_addr(pkt).map(|a| a.to_string()).unwrap_or_default();
                            if let Ok(mut g) = stats_bg.lock() {
                                g.connected = true;
                                g.rtt_ms = rtt;
                                g.rx_packets += 1;
                                g.rx_bytes += n as u64;
                                if !mapped.is_empty() {
                                    g.mapped_addr = mapped;
                                }
                            }
                            continue;
                        }
                        let tag = primary.as_ref().map_or(&[][..], |(_, e)| &e.peer_tag);
                        if let Some((kind, _rseq, _rts, payload)) =
                            open_media_packet(&cfg.shared_key, !cfg.is_outgoing, tag, pkt)
                        {
                            if let Ok(mut g) = stats_bg.lock() {
                                g.connected = true;
                                g.rtt_ms = rtt;
                                g.rx_packets += 1;
                                g.rx_bytes += n as u64;
                            }
                            if kind == KIND_AUDIO_ADPCM
                                && let Some(pcm) = decode_adpcm_frame(&payload)
                            {
                                let _ = rx_playback.try_send(pcm);
                            }
                        }
                    }
                }
            }
        });

        Ok(Self {
            stop,
            stats,
            tx_audio: Some(tx_audio),
        })
    }

    pub fn stats(&self) -> CallMediaStats {
        self.stats.lock().map(|g| g.clone()).unwrap_or_default()
    }

    pub fn sender(&self) -> Option<mpsc::Sender<Vec<i16>>> {
        self.tx_audio.clone()
    }

    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.tx_audio.take();
    }
}

impl Drop for CallMediaSession {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stun_binding_request_and_xor_mapped_parse() {
        let tx = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12];
        let req = build_stun_binding_request(tx);
        assert_eq!(req.len(), 20);
        assert_eq!(&req[0..2], &[0x00, 0x01]);
        // Собираем валидный STUN Binding Response с XOR-MAPPED-ADDRESS (203.0.113.5:45678).
        let mut resp = Vec::new();
        resp.extend_from_slice(&0x0101u16.to_be_bytes());
        resp.extend_from_slice(&12u16.to_be_bytes());
        resp.extend_from_slice(&STUN_MAGIC_COOKIE.to_be_bytes());
        resp.extend_from_slice(&tx);
        resp.extend_from_slice(&0x0020u16.to_be_bytes());
        resp.extend_from_slice(&8u16.to_be_bytes());
        resp.push(0);
        resp.push(0x01); // IPv4
        let xport = 45678u16 ^ (STUN_MAGIC_COOKIE >> 16) as u16;
        let xip = u32::from(Ipv4Addr::new(203, 0, 113, 5)) ^ STUN_MAGIC_COOKIE;
        resp.extend_from_slice(&xport.to_be_bytes());
        resp.extend_from_slice(&xip.to_be_bytes());

        assert!(is_stun_binding_response(&resp, tx));
        assert_eq!(
            parse_stun_xor_mapped_addr(&resp),
            Some("203.0.113.5:45678".parse().unwrap())
        );
    }

    #[test]
    fn adpcm_encodes_and_decodes_voice_frame_4_to_1() {
        let pcm: Vec<i16> = (0..320)
            .map(|i| ((i as f32 * 0.15).sin() * 14_000.0) as i16)
            .collect();
        let enc = encode_adpcm_frame(&pcm);
        assert!(enc.len() < pcm.len()); // 165 байт против 640 байт
        let dec = decode_adpcm_frame(&enc).expect("decodes");
        assert_eq!(dec.len(), pcm.len());
        let rms_err = (pcm
            .iter()
            .zip(dec.iter())
            .map(|(a, b)| {
                let d = f64::from(*a) - f64::from(*b);
                d * d
            })
            .sum::<f64>()
            / pcm.len() as f64)
            .sqrt();
        assert!(rms_err < 1600.0, "unexpected RMS distortion: {rms_err}");
    }

    #[test]
    fn encrypted_media_packet_roundtrip_with_reflector_peer_tag() {
        let key = [0x5Au8; 256];
        let tag = [0x10u8; 16];
        let payload = encode_adpcm_frame(&[100, 500, -300, 900, 0, -900]);
        let sealed = seal_media_packet(&key, true, &tag, KIND_AUDIO_ADPCM, 42, 1234, &payload);
        assert!(sealed.starts_with(&tag));
        let (kind, seq, ts, opened) =
            open_media_packet(&key, true, &tag, &sealed).expect("decrypts");
        assert_eq!((kind, seq, ts), (KIND_AUDIO_ADPCM, 42, 1234));
        assert_eq!(opened, payload);
    }

    #[test]
    fn select_endpoints_respects_p2p_and_privacy_relay_filter() {
        let eps = vec![
            CallEndpoint {
                id: 1,
                ip: "149.154.167.50".into(),
                ipv6: String::new(),
                port: 1400,
                tcp: false,
                peer_tag: vec![1; 16],
                is_webrtc: false,
                stun: false,
                turn: false,
                username: String::new(),
                password: String::new(),
            },
            CallEndpoint {
                id: 2,
                ip: "198.51.100.2".into(),
                ipv6: String::new(),
                port: 3478,
                tcp: false,
                peer_tag: vec![],
                is_webrtc: true,
                stun: true,
                turn: false,
                username: String::new(),
                password: String::new(),
            },
        ];
        assert_eq!(select_endpoints(&eps, true).len(), 2);
        let relay_only = select_endpoints(&eps, false);
        assert_eq!(relay_only.len(), 1);
        assert_eq!(relay_only[0].id, 1);
    }

    #[test]
    fn loopback_udp_call_media_exchange_succeeds() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            let server = UdpSocket::bind("127.0.0.1:0").await.unwrap();
            let addr = server.local_addr().unwrap();
            let key = [0x33u8; 256];
            let tag = [0x77u8; 16];
            let (pb_tx, mut pb_rx) = mpsc::channel(8);

            let srv_task = tokio::spawn(async move {
                let mut buf = [0u8; 2048];
                for _ in 0..4 {
                    if let Ok((n, from)) =
                        tokio::time::timeout(Duration::from_millis(600), server.recv_from(&mut buf))
                            .await
                            .unwrap_or(Err(std::io::ErrorKind::TimedOut.into()))
                        && let Some((_, seq, ts, _)) = open_media_packet(&key, true, &tag, &buf[..n])
                    {
                        let audio = encode_adpcm_frame(&[0, 1200, 2400, -1200]);
                        let reply = seal_media_packet(
                            &key,
                            false,
                            &tag,
                            KIND_AUDIO_ADPCM,
                            seq,
                            ts,
                            &audio,
                        );
                        let _ = server.send_to(&reply, from).await;
                    }
                }
            });

            let mut session = CallMediaSession::start(
                CallMediaConfig {
                    shared_key: key,
                    endpoints: vec![CallEndpoint {
                        id: 99,
                        ip: addr.ip().to_string(),
                        ipv6: String::new(),
                        port: addr.port(),
                        tcp: false,
                        peer_tag: tag.to_vec(),
                        is_webrtc: false,
                        stun: false,
                        turn: false,
                        username: String::new(),
                        password: String::new(),
                    }],
                    p2p_allowed: false,
                    is_outgoing: true,
                    video: false,
                    relay_only_privacy: true,
                },
                pb_tx,
            )
            .unwrap();

            if let Some(tx) = session.sender() {
                let _ = tx.send(vec![100, 400, -200, 800]).await;
            }
            let got = tokio::time::timeout(Duration::from_millis(1200), pb_rx.recv())
                .await
                .ok()
                .flatten();
            session.stop();
            let _ = srv_task.await;
            assert!(got.is_some());
            assert!(session.stats().connected);
        });
    }
}
