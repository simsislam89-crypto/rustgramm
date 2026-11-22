//! Нативный звук и запись микрофона без сторонних крейтов:
//! - Синтез чистого WAV-звука уведомлений (в стиле Telegram) и гудка звонка;
//! - Надёжное воспроизведение через `winmm.dll` (`PlaySoundW` SND_MEMORY) — звук слышен всегда,
//!   даже если в Windows отключены системные звуки (`MessageBeep` в этом случае молчит!);
//! - Запись голосовых сообщений с микрофона (WinMM `waveIn*`, 16 кГц 16-бит mono PCM)
//!   с расчётом живого уровня громкости и 5-битной волны Telegram (`waveform`).

use std::path::Path;
#[cfg(windows)]
use std::sync::{Arc, Mutex};
#[cfg(windows)]
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::Instant;

/// Упаковывает 16-битные моно PCM-сэмплы в стандартный RIFF WAVE файл.
pub fn pcm16_to_wav(samples: &[i16], sample_rate: u32) -> Vec<u8> {
    let data_bytes = (samples.len() * 2) as u32;
    let file_size = 36 + data_bytes;
    let byte_rate = sample_rate * 2;
    let mut out = Vec::with_capacity(44 + samples.len() * 2);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&file_size.to_le_bytes());
    out.extend_from_slice(b"WAVE");
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&16u32.to_le_bytes()); // PCM chunk size
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM format
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&sample_rate.to_le_bytes());
    out.extend_from_slice(&byte_rate.to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes()); // block align
    out.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_bytes.to_le_bytes());
    for &s in samples {
        out.extend_from_slice(&s.to_le_bytes());
    }
    out
}

/// Генерирует мягкий двухтоновый звук уведомления (44.1 кГц WAV) с масштабированием громкости `0.0..=1.0`.
pub fn generate_chime_wav(volume: f32) -> Vec<u8> {
    let sr = 44_100u32;
    let vol = volume.clamp(0.0, 1.0);
    let dur_ms = 260usize;
    let total = (sr as usize * dur_ms) / 1000;
    let mut samples = Vec::with_capacity(total);
    let two_pi = std::f32::consts::TAU;
    for i in 0..total {
        let t = i as f32 / sr as f32;
        // Две чистые ноты (C6 -> G6) с мягкой огибающей без щелчков.
        let (freq, local_t) = if t < 0.105 { (1046.5_f32, t) } else { (1567.98_f32, t - 0.105) };
        let attack = (local_t / 0.008).min(1.0);
        let decay = (-local_t * 16.0).exp();
        let env = attack * decay;
        let wave = (two_pi * freq * t).sin() * 0.72 + (two_pi * freq * 2.0 * t).sin() * 0.18;
        let v = (wave * env * vol * 22_000.0).clamp(-32_767.0, 32_767.0) as i16;
        samples.push(v);
    }
    pcm16_to_wav(&samples, sr)
}

/// Генерирует мелодичный двойной гудок входящего/исходящего звонка.
pub fn generate_ringtone_wav(volume: f32) -> Vec<u8> {
    let sr = 22_050u32;
    let vol = volume.clamp(0.0, 1.0);
    let total = sr as usize; // 1.0 с
    let mut samples = Vec::with_capacity(total);
    let two_pi = std::f32::consts::TAU;
    for i in 0..total {
        let t = i as f32 / sr as f32;
        let burst = if t < 0.28 {
            Some(t)
        } else if (0.36..0.68).contains(&t) {
            Some(t - 0.36)
        } else {
            None
        };
        let v = if let Some(lt) = burst {
            let env = (lt / 0.015).min(1.0) * (-lt * 5.0).exp();
            let s = (two_pi * 659.25 * t).sin() * 0.55 + (two_pi * 880.0 * t).sin() * 0.35;
            (s * env * vol * 20_000.0).clamp(-32_767.0, 32_767.0) as i16
        } else {
            0
        };
        samples.push(v);
    }
    pcm16_to_wav(&samples, sr)
}

/// Считает массив пиков `0..=31` (63 столбика) для Telegram `DocumentAttributeAudio.waveform`.
pub fn compute_waveform_peaks(samples: &[i16], bars: usize) -> Vec<u8> {
    let n = bars.clamp(8, 100);
    if samples.is_empty() {
        return vec![4; n];
    }
    let chunk = (samples.len() / n).max(1);
    let mut rms_list = Vec::with_capacity(n);
    let mut max_rms = 1.0_f32;
    for i in 0..n {
        let start = i * chunk;
        let end = ((i + 1) * chunk).min(samples.len());
        if start >= end {
            rms_list.push(0.0);
            continue;
        }
        let sum_sq: f64 = samples[start..end]
            .iter()
            .map(|&s| {
                let f = f64::from(s);
                f * f
            })
            .sum();
        let rms = (sum_sq / (end - start) as f64).sqrt() as f32;
        if rms > max_rms {
            max_rms = rms;
        }
        rms_list.push(rms);
    }
    rms_list
        .into_iter()
        .map(|r| ((r / max_rms) * 31.0).round().clamp(1.0, 31.0) as u8)
        .collect()
}

// =========================================================================
// Windows WinMM (`winmm.dll`): воспроизведение WAV + запись микрофона `waveIn*`
// =========================================================================

#[cfg(windows)]
mod win_audio {
    use super::*;
    use std::ffi::c_void;
    use std::sync::OnceLock;

    const SND_ASYNC: u32 = 0x0001;
    const SND_NODEFAULT: u32 = 0x0002;
    const SND_MEMORY: u32 = 0x0004;
    const SND_LOOP: u32 = 0x0008;

    const WAVE_FORMAT_PCM: u16 = 1;
    const WAVE_MAPPER: u32 = u32::MAX;
    const WHDR_DONE: u32 = 0x0000_0001;

    #[repr(C)]
    struct WaveFormatEx {
        w_format_tag: u16,
        n_channels: u16,
        n_samples_per_sec: u32,
        n_avg_bytes_per_sec: u32,
        n_block_align: u16,
        w_bits_per_sample: u16,
        cb_size: u16,
    }

    #[repr(C)]
    struct WaveHdr {
        lp_data: *mut u8,
        dw_buffer_length: u32,
        dw_bytes_recorded: u32,
        dw_user: usize,
        dw_flags: u32,
        dw_loops: u32,
        lp_next: *mut WaveHdr,
        reserved: usize,
    }

    #[link(name = "winmm")]
    unsafe extern "system" {
        fn PlaySoundW(psz_sound: *const c_void, hmod: *mut c_void, fdw_sound: u32) -> i32;
        fn waveInOpen(
            phwi: *mut *mut c_void,
            u_device_id: u32,
            pwfx: *const WaveFormatEx,
            dw_callback: usize,
            dw_instance: usize,
            fdw_open: u32,
        ) -> u32;
        fn waveInPrepareHeader(hwi: *mut c_void, pwh: *mut WaveHdr, cbwh: u32) -> u32;
        fn waveInUnprepareHeader(hwi: *mut c_void, pwh: *mut WaveHdr, cbwh: u32) -> u32;
        fn waveInAddBuffer(hwi: *mut c_void, pwh: *mut WaveHdr, cbwh: u32) -> u32;
        fn waveInStart(hwi: *mut c_void) -> u32;
        fn waveInStop(hwi: *mut c_void) -> u32;
        fn waveInReset(hwi: *mut c_void) -> u32;
        fn waveInClose(hwi: *mut c_void) -> u32;
    }

    // Держим последний WAV-буфер живым, пока `PlaySoundW(SND_ASYNC | SND_MEMORY)` его проигрывает.
    static ACTIVE_SOUND: OnceLock<Mutex<Vec<u8>>> = OnceLock::new();

    pub fn play_notify_chime(volume: u8) {
        if volume == 0 {
            return;
        }
        let wav = generate_chime_wav(f32::from(volume.min(100)) / 100.0);
        let slot = ACTIVE_SOUND.get_or_init(|| Mutex::new(Vec::new()));
        if let Ok(mut guard) = slot.lock() {
            unsafe {
                // Останавливаем предыдущий буфер до замены памяти.
                PlaySoundW(std::ptr::null(), std::ptr::null_mut(), 0);
            }
            *guard = wav;
            unsafe {
                PlaySoundW(
                    guard.as_ptr().cast(),
                    std::ptr::null_mut(),
                    SND_MEMORY | SND_ASYNC | SND_NODEFAULT,
                );
            }
        }
    }

    pub fn play_ringtone(volume: u8) {
        if volume == 0 {
            return;
        }
        let wav = generate_ringtone_wav(f32::from(volume.min(100)) / 100.0);
        let slot = ACTIVE_SOUND.get_or_init(|| Mutex::new(Vec::new()));
        if let Ok(mut guard) = slot.lock() {
            unsafe {
                PlaySoundW(std::ptr::null(), std::ptr::null_mut(), 0);
            }
            *guard = wav;
            unsafe {
                PlaySoundW(
                    guard.as_ptr().cast(),
                    std::ptr::null_mut(),
                    SND_MEMORY | SND_ASYNC | SND_NODEFAULT | SND_LOOP,
                );
            }
        }
    }

    pub fn stop_ringtone() {
        unsafe {
            PlaySoundW(std::ptr::null(), std::ptr::null_mut(), 0);
        }
    }

    /// Живой рекордер микрофона на базе WinMM `waveIn*` (16 кГц, 16-бит моно).
    pub struct VoiceRecorder {
        stop_flag: Arc<AtomicBool>,
        samples: Arc<Mutex<Vec<i16>>>,
        level_bits: Arc<AtomicU32>,
        bars: Arc<Mutex<Vec<f32>>>,
        started: Instant,
        handle: Option<std::thread::JoinHandle<()>>,
    }

    impl VoiceRecorder {
        pub fn start() -> Result<Self, String> {
            let stop_flag = Arc::new(AtomicBool::new(false));
            let samples = Arc::new(Mutex::new(Vec::<i16>::with_capacity(16_000 * 10)));
            let level_bits = Arc::new(AtomicU32::new(0));
            let bars = Arc::new(Mutex::new(Vec::<f32>::with_capacity(64)));

            let (tx, rx) = std::sync::mpsc::channel::<Result<(), String>>();
            let sf = stop_flag.clone();
            let sm = samples.clone();
            let lb = level_bits.clone();
            let br = bars.clone();

            let handle = std::thread::Builder::new()
                .name("rg-voice-rec".into())
                .spawn(move || {
                    const SR: u32 = 16_000;
                    const CHUNK_SAMPLES: usize = 800; // 50 мс на буфер
                    let fmt = WaveFormatEx {
                        w_format_tag: WAVE_FORMAT_PCM,
                        n_channels: 1,
                        n_samples_per_sec: SR,
                        n_avg_bytes_per_sec: SR * 2,
                        n_block_align: 2,
                        w_bits_per_sample: 16,
                        cb_size: 0,
                    };
                    let mut hwi: *mut c_void = std::ptr::null_mut();
                    let rc = unsafe { waveInOpen(&mut hwi, WAVE_MAPPER, &fmt, 0, 0, 0) };
                    if rc != 0 || hwi.is_null() {
                        let _ = tx.send(Err(format!("Микрофон недоступен (WinMM код {rc})")));
                        return;
                    }
                    let mut bufs: Vec<Box<[u8]>> = (0..4)
                        .map(|_| vec![0u8; CHUNK_SAMPLES * 2].into_boxed_slice())
                        .collect();
                    let mut hdrs: Vec<Box<WaveHdr>> = bufs
                        .iter_mut()
                        .map(|b| {
                            Box::new(WaveHdr {
                                lp_data: b.as_mut_ptr(),
                                dw_buffer_length: b.len() as u32,
                                dw_bytes_recorded: 0,
                                dw_user: 0,
                                dw_flags: 0,
                                dw_loops: 0,
                                lp_next: std::ptr::null_mut(),
                                reserved: 0,
                            })
                        })
                        .collect();
                    let hdr_size = std::mem::size_of::<WaveHdr>() as u32;
                    for h in &mut hdrs {
                        unsafe {
                            waveInPrepareHeader(hwi, h.as_mut(), hdr_size);
                            waveInAddBuffer(hwi, h.as_mut(), hdr_size);
                        }
                    }
                    let start_rc = unsafe { waveInStart(hwi) };
                    if start_rc != 0 {
                        unsafe {
                            waveInClose(hwi);
                        }
                        let _ = tx.send(Err(format!("Не удалось запустить запись ({start_rc})")));
                        return;
                    }
                    let _ = tx.send(Ok(()));

                    while !sf.load(Ordering::Relaxed) {
                        for h in &mut hdrs {
                            let flags = unsafe { std::ptr::read_volatile(&h.dw_flags) };
                            if (flags & WHDR_DONE) != 0 {
                                let bytes = unsafe { std::ptr::read_volatile(&h.dw_bytes_recorded) } as usize;
                                if bytes >= 2 {
                                    let slice = unsafe { std::slice::from_raw_parts(h.lp_data, bytes) };
                                    let mut sum_sq = 0.0_f64;
                                    let count = bytes / 2;
                                    if let Ok(mut guard) = sm.lock() {
                                        guard.reserve(count);
                                        for &[lo, hi] in slice.as_chunks::<2>().0 {
                                            let s = i16::from_le_bytes([lo, hi]);
                                            guard.push(s);
                                            let f = f64::from(s);
                                            sum_sq += f * f;
                                        }
                                    }
                                    let rms = ((sum_sq / count.max(1) as f64).sqrt() / 12_000.0)
                                        .clamp(0.0, 1.0) as f32;
                                    lb.store(rms.to_bits(), Ordering::Relaxed);
                                    if let Ok(mut bg) = br.lock() {
                                        bg.push(rms);
                                        if bg.len() > 48 {
                                            bg.remove(0);
                                        }
                                    }
                                }
                                h.dw_bytes_recorded = 0;
                                unsafe {
                                    waveInAddBuffer(hwi, h.as_mut(), hdr_size);
                                }
                            }
                        }
                        std::thread::sleep(std::time::Duration::from_millis(16));
                    }

                    unsafe {
                        waveInStop(hwi);
                        waveInReset(hwi);
                        for h in &mut hdrs {
                            waveInUnprepareHeader(hwi, h.as_mut(), hdr_size);
                        }
                        waveInClose(hwi);
                    }
                })
                .map_err(|e| format!("Поток записи: {e}"))?;

            match rx.recv_timeout(std::time::Duration::from_secs(2)) {
                Ok(Ok(())) => Ok(Self {
                    stop_flag,
                    samples,
                    level_bits,
                    bars,
                    started: Instant::now(),
                    handle: Some(handle),
                }),
                Ok(Err(e)) => Err(e),
                Err(_) => Err("Таймаут инициализации микрофона".to_owned()),
            }
        }

        pub fn elapsed_secs(&self) -> f32 {
            self.started.elapsed().as_secs_f32()
        }

        pub fn live_level(&self) -> f32 {
            f32::from_bits(self.level_bits.load(Ordering::Relaxed))
        }

        pub fn recent_bars(&self) -> Vec<f32> {
            self.bars.lock().map(|g| g.clone()).unwrap_or_default()
        }

        pub fn finish_to_wav(mut self, dest: &Path) -> Result<(u32, Vec<u8>), String> {
            self.stop_flag.store(true, Ordering::Relaxed);
            if let Some(h) = self.handle.take() {
                let _ = h.join();
            }
            let data = self.samples.lock().map(|g| g.clone()).unwrap_or_default();
            if data.len() < 1600 {
                return Err("Запись слишком короткая".to_owned());
            }
            let dur = ((data.len() as f32) / 16_000.0).round().max(1.0) as u32;
            let peaks = compute_waveform_peaks(&data, 63);
            let wav = pcm16_to_wav(&data, 16_000);
            if let Some(parent) = dest.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            std::fs::write(dest, &wav).map_err(|e| format!("Сохранение голосового: {e}"))?;
            Ok((dur, peaks))
        }

        pub fn cancel(mut self) {
            self.stop_flag.store(true, Ordering::Relaxed);
            if let Some(h) = self.handle.take() {
                let _ = h.join();
            }
        }
    }

    impl Drop for VoiceRecorder {
        fn drop(&mut self) {
            self.stop_flag.store(true, Ordering::Relaxed);
            if let Some(h) = self.handle.take() {
                let _ = h.join();
            }
        }
    }
}

#[cfg(not(windows))]
mod win_audio {
    use super::*;

    pub fn play_notify_chime(_volume: u8) {}
    pub fn play_ringtone(_volume: u8) {}
    pub fn stop_ringtone() {}

    pub struct VoiceRecorder {
        started: Instant,
    }

    impl VoiceRecorder {
        pub fn start() -> Result<Self, String> {
            Err("Запись микрофона на Android требует Android AudioRecord JNI (доступно на Windows ПК)".to_owned())
        }
        pub fn elapsed_secs(&self) -> f32 {
            self.started.elapsed().as_secs_f32()
        }
        pub fn live_level(&self) -> f32 {
            0.0
        }
        pub fn recent_bars(&self) -> Vec<f32> {
            Vec::new()
        }
        pub fn finish_to_wav(self, _dest: &Path) -> Result<(u32, Vec<u8>), String> {
            Err("Не поддерживается на этой платформе".to_owned())
        }
        pub fn cancel(self) {}
    }
}

pub use win_audio::{VoiceRecorder, play_notify_chime, play_ringtone, stop_ringtone};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chime_and_ringtone_wavs_have_valid_riff_headers() {
        let chime = generate_chime_wav(0.8);
        assert!(chime.len() > 44);
        assert_eq!(&chime[0..4], b"RIFF");
        assert_eq!(&chime[8..12], b"WAVE");
        assert_eq!(&chime[12..16], b"fmt ");
        assert_eq!(&chime[36..40], b"data");

        let silent = generate_chime_wav(0.0);
        assert!(silent[44..].iter().all(|&b| b == 0));

        let ring = generate_ringtone_wav(0.7);
        assert_eq!(&ring[0..4], b"RIFF");
        assert!(ring.len() > 20_000);
    }

    #[test]
    fn waveform_peaks_scale_into_five_bit_range() {
        let mut pcm = Vec::with_capacity(6300);
        for i in 0..6300 {
            let amp = if i < 3150 { 4000.0 } else { 20_000.0 };
            pcm.push(((i as f32 * 0.2).sin() * amp) as i16);
        }
        let peaks = compute_waveform_peaks(&pcm, 63);
        assert_eq!(peaks.len(), 63);
        assert!(peaks.iter().all(|&p| (1..=31).contains(&p)));
        assert!(peaks[50] > peaks[10]);
    }
}
