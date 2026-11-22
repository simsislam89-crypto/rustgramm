//! Встроенный видеоплеер (Windows): WebView2 — движок Edge из Windows 11 — в дочернем окне
//! поверх области просмотра. Это аппаратное декодирование, звук, перемотка, полноэкранный режим
//! и все форматы Chromium (MP4/H.264, WebM/VP9 с прозрачностью, GIF). Страница — локальный
//! HTML-файл с тегом `<video>`, файл ролика читается прямо с диска, ничего не уходит в сеть.
//! Окно живёт, только пока открыт просмотрщик (около 100 МБ памяти).

#![cfg(windows)]

use std::path::{Path, PathBuf};

use wry::dpi::{PhysicalPosition, PhysicalSize};
use wry::raw_window_handle::HasWindowHandle;
use wry::{Rect, WebView, WebViewBuilder};

pub struct VideoView {
    webview: WebView,
    bounds: (i32, i32, u32, u32),
    /// Страница плеера просила закрыть просмотр (Esc внутри окна).
    closed: std::sync::Arc<std::sync::atomic::AtomicBool>,
    /// Размер кадра `ширина << 32 | высота` (0, пока метаданные не прочитаны).
    dims: std::sync::Arc<std::sync::atomic::AtomicU64>,
    /// Страница включила полноэкранный режим.
    full: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

/// Значки плеера: имя файла страницы → значок набора Telegram.
const PLAYER_ICONS: [(&str, &str); 8] = [
    ("play", "player/player_play"),
    ("pause", "player/player_pause"),
    ("vol_on", "player/player_volume_on"),
    ("vol_off", "player/player_volume_off"),
    ("pip", "player/player_pip"),
    ("fullscreen", "player/player_fullscreen"),
    ("speed", "player/player_speed"),
    ("settings", "player/player_settings"),
];

/// Адрес `file:///` для локального пути (пробелы и не-ASCII кодируются процентами).
pub fn file_url(path: &Path) -> String {
    let mut s = String::from("file:///");
    for c in path.to_string_lossy().replace('\\', "/").chars() {
        if c.is_ascii_alphanumeric() || matches!(c, '/' | ':' | '-' | '_' | '.' | '~') {
            s.push(c);
        } else {
            let mut buf = [0u8; 4];
            for b in c.encode_utf8(&mut buf).bytes() {
                s.push_str(&format!("%{b:02X}"));
            }
        }
    }
    s
}

/// HTML страницы плеера для файла `src` (в виде адреса).
fn page(src: &str, loop_it: bool) -> String {
    include_str!("../assets/player.html")
        .replace("{SRC}", src)
        .replace("{LOOP}", if loop_it { "loop" } else { "" })
}

impl VideoView {
    /// Создаёт плеер для локального файла `video`.
    pub fn new(window: &impl HasWindowHandle, video: &Path, dir: &Path, loop_it: bool) -> Result<VideoView, String> {
        Self::new_url(window, &file_url(video), dir, loop_it)
    }

    /// Создаёт плеер для произвольного локального адреса (`file:///` или loopback-потока).
    pub fn new_url(window: &impl HasWindowHandle, src: &str, dir: &Path, loop_it: bool) -> Result<VideoView, String> {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        let html_path: PathBuf = dir.join("player.html");
        std::fs::write(&html_path, page(src, loop_it)).map_err(|e| e.to_string())?;
        let icons = dir.join("i");
        std::fs::create_dir_all(&icons).map_err(|e| e.to_string())?;
        for (file, name) in PLAYER_ICONS {
            super::icons::export_png(name, &icons.join(format!("{file}.png")));
        }
        let closed = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flag = closed.clone();
        let dims = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
        let dims_w = dims.clone();
        let full = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let full_w = full.clone();
        let mut context = wry::WebContext::new(Some(dir.join("webview")));
        let webview = WebViewBuilder::new_with_web_context(&mut context)
            .with_url(file_url(&html_path))
            .with_autoplay(true)
            .with_background_color((0, 0, 0, 255))
            .with_devtools(false)
            .with_bounds(Rect {
                position: PhysicalPosition::new(0, 0).into(),
                size: PhysicalSize::new(16u32, 9u32).into(),
            })
            .with_new_window_req_handler(|_, _| wry::NewWindowResponse::Deny)
            // Страница сообщает о себе через заголовок (`rgN|команда`): IPC wry разбирает адрес
            // страницы как `Uri` и молча отбрасывает сообщения с адресов `file:///`.
            .with_document_title_changed_handler(move |title| {
                use std::sync::atomic::Ordering::Relaxed;
                let Some((_, body)) = title.strip_prefix("rg").and_then(|t| t.split_once('|')) else {
                    return;
                };
                if body == "close" {
                    flag.store(true, Relaxed);
                } else if let Some(rest) = body.strip_prefix("size:") {
                    let mut it = rest.split(':').filter_map(|x| x.parse::<u64>().ok());
                    if let (Some(w), Some(h)) = (it.next(), it.next()) {
                        dims_w.store(w << 32 | h, Relaxed);
                    }
                } else if let Some(f) = body.strip_prefix("fs:") {
                    full_w.store(f == "1", Relaxed);
                }
            })
            .build_as_child(window)
            .map_err(|e| format!("WebView2 не запустился: {e}"))?;
        Ok(VideoView { webview, bounds: (0, 0, 16, 9), closed, dims, full })
    }

    /// Ставит плеер на прямоугольник `(x, y, ширина, высота)` в физических пикселях окна.
    pub fn place(&mut self, bounds: (i32, i32, u32, u32)) {
        if bounds != self.bounds {
            self.bounds = bounds;
            let _ = self.webview.set_bounds(Rect {
                position: PhysicalPosition::new(bounds.0, bounds.1).into(),
                size: PhysicalSize::new(bounds.2.max(1), bounds.3.max(1)).into(),
            });
        }
    }

    /// Пропорции кадра (ширина / высота), когда они известны.
    pub fn aspect(&self) -> Option<f32> {
        let d = self.dims.load(std::sync::atomic::Ordering::Relaxed);
        let (w, h) = ((d >> 32) as f32, (d & 0xffff_ffff) as f32);
        (w > 0.0 && h > 0.0).then(|| w / h)
    }

    /// Включён ли полноэкранный режим страницы.
    pub fn is_full(&self) -> bool {
        self.full.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Просила ли страница закрыть просмотр.
    pub fn wants_close(&self) -> bool {
        self.closed.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Останавливает воспроизведение и возвращает фокус главному окну.
    pub fn stop(&self) {
        let _ = self.webview.evaluate_script("document.getElementById('v').pause()");
        let _ = self.webview.focus_parent();
    }
}

/// Встроенный в ленту мини-плеер для автозапуска GIF и коротких роликов без звука.
pub struct InlineGifView {
    webview: WebView,
    pub msg_id: i32,
    bounds: (i32, i32, u32, u32),
    clicked: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl InlineGifView {
    pub fn new(window: &impl HasWindowHandle, msg_id: i32, video: &Path, dir: &Path) -> Result<Self, String> {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        let html_path = dir.join("inline.html");
        let src = file_url(video);
        let html = format!(
            "<!doctype html><html><head><meta charset=utf-8><style>\
            html,body{{margin:0;height:100%;background:#141418;overflow:hidden;cursor:pointer}}\
            video{{width:100%;height:100%;object-fit:cover;display:block}}\
            </style></head><body><video id=v src=\"{src}\" autoplay loop muted playsinline></video>\
            <script>var n=0;document.onclick=function(){{document.title='rg'+(++n)+'|open'}}</script>\
            </body></html>"
        );
        std::fs::write(&html_path, html).map_err(|e| e.to_string())?;
        let clicked = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flag = clicked.clone();
        let mut context = wry::WebContext::new(Some(dir.join("webview")));
        let webview = WebViewBuilder::new_with_web_context(&mut context)
            .with_url(file_url(&html_path))
            .with_autoplay(true)
            .with_background_color((20, 20, 24, 255))
            .with_devtools(false)
            .with_bounds(Rect {
                position: PhysicalPosition::new(0, 0).into(),
                size: PhysicalSize::new(16u32, 16u32).into(),
            })
            .with_new_window_req_handler(|_, _| wry::NewWindowResponse::Deny)
            .with_document_title_changed_handler(move |title| {
                if title.ends_with("|open") {
                    flag.store(true, std::sync::atomic::Ordering::Relaxed);
                }
            })
            .build_as_child(window)
            .map_err(|e| e.to_string())?;
        Ok(Self { webview, msg_id, bounds: (0, 0, 16, 16), clicked })
    }

    pub fn place(&mut self, bounds: (i32, i32, u32, u32)) {
        if bounds != self.bounds {
            self.bounds = bounds;
            let _ = self.webview.set_bounds(Rect {
                position: PhysicalPosition::new(bounds.0, bounds.1).into(),
                size: PhysicalSize::new(bounds.2.max(1), bounds.3.max(1)).into(),
            });
        }
    }

    pub fn take_clicked(&self) -> bool {
        self.clicked.swap(false, std::sync::atomic::Ordering::Relaxed)
    }

    pub fn stop(&self) {
        let _ = self.webview.evaluate_script("document.getElementById('v').pause()");
        let _ = self.webview.focus_parent();
    }
}

/// Встроенный скрытый аудиоплеер для музыки в профиле и голосовых/аудио в чатах
/// (поддерживает M4A/AAC, MP3, OGG/Opus, FLAC, WAV; создаётся только во время воспроизведения).
pub struct AudioView {
    webview: WebView,
    /// Текущая позиция (мс).
    pos_ms: std::sync::Arc<std::sync::atomic::AtomicU64>,
    /// Длительность (мс).
    dur_ms: std::sync::Arc<std::sync::atomic::AtomicU64>,
    /// Идёт ли воспроизведение прямо сейчас.
    playing: std::sync::Arc<std::sync::atomic::AtomicBool>,
    /// Закончился ли трек.
    ended: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl AudioView {
    pub fn new(window: &impl HasWindowHandle, audio: &Path, dir: &Path, volume: f32) -> Result<Self, String> {
        Self::new_url(window, &file_url(audio), dir, volume)
    }

    pub fn new_url(window: &impl HasWindowHandle, src: &str, dir: &Path, volume: f32) -> Result<Self, String> {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        let html_path = dir.join("audio.html");
        let vol = volume.clamp(0.0, 1.0);
        let html = format!(
            "<!doctype html><html><head><meta charset=utf-8></head><body>\
            <audio id=a src=\"{src}\" autoplay></audio>\
            <script>\
            var a=document.getElementById('a'),n=0;a.volume={vol:.3};\
            function rep(ev){{\
              var p=Math.round((a.currentTime||0)*1000);\
              var d=Math.round((isFinite(a.duration)?a.duration:0)*1000);\
              var pl=a.paused?0:1;\
              document.title='rga'+(++n)+'|'+ev+'|'+p+'|'+d+'|'+pl;\
            }}\
            a.onplay=function(){{rep('p')}};\
            a.onpause=function(){{rep('p')}};\
            a.onended=function(){{rep('e')}};\
            a.onloadedmetadata=function(){{rep('m')}};\
            setInterval(function(){{if(!a.paused)rep('t')}},250);\
            </script></body></html>"
        );
        std::fs::write(&html_path, html).map_err(|e| e.to_string())?;
        let pos_ms = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
        let dur_ms = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
        let playing = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
        let ended = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let (pw, dw, plw, ew) = (pos_ms.clone(), dur_ms.clone(), playing.clone(), ended.clone());
        let mut context = wry::WebContext::new(Some(dir.join("webview")));
        let webview = WebViewBuilder::new_with_web_context(&mut context)
            .with_url(file_url(&html_path))
            .with_autoplay(true)
            .with_visible(false)
            .with_devtools(false)
            .with_bounds(Rect {
                position: PhysicalPosition::new(-100, -100).into(),
                size: PhysicalSize::new(1u32, 1u32).into(),
            })
            .with_new_window_req_handler(|_, _| wry::NewWindowResponse::Deny)
            .with_document_title_changed_handler(move |title| {
                use std::sync::atomic::Ordering::Relaxed;
                let Some(rest) = title.strip_prefix("rga").and_then(|t| t.split_once('|')).map(|(_, b)| b) else {
                    return;
                };
                let mut parts = rest.split('|');
                let ev = parts.next().unwrap_or("");
                if let Some(p) = parts.next().and_then(|s| s.parse::<u64>().ok()) {
                    pw.store(p, Relaxed);
                }
                if let Some(d) = parts.next().and_then(|s| s.parse::<u64>().ok())
                    && d > 0
                {
                    dw.store(d, Relaxed);
                }
                if let Some(pl) = parts.next() {
                    plw.store(pl == "1", Relaxed);
                }
                if ev == "e" {
                    ew.store(true, Relaxed);
                    plw.store(false, Relaxed);
                }
            })
            .build_as_child(window)
            .map_err(|e| format!("Аудиоплеер: {e}"))?;
        let _ = webview.focus_parent();
        Ok(Self { webview, pos_ms, dur_ms, playing, ended })
    }

    pub fn set_playing(&self, play: bool) {
        let js = if play { "document.getElementById('a').play()" } else { "document.getElementById('a').pause()" };
        let _ = self.webview.evaluate_script(js);
        self.playing.store(play, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn seek_secs(&self, secs: f32) {
        let s = secs.max(0.0);
        let _ = self.webview.evaluate_script(&format!("document.getElementById('a').currentTime={s:.2}"));
        self.pos_ms.store((s * 1000.0) as u64, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn set_volume(&self, vol: f32) {
        let v = vol.clamp(0.0, 1.0);
        let _ = self.webview.evaluate_script(&format!("document.getElementById('a').volume={v:.3}"));
    }

    pub fn pos_secs(&self) -> f32 {
        self.pos_ms.load(std::sync::atomic::Ordering::Relaxed) as f32 / 1000.0
    }

    pub fn dur_secs(&self) -> f32 {
        self.dur_ms.load(std::sync::atomic::Ordering::Relaxed) as f32 / 1000.0
    }

    pub fn is_playing(&self) -> bool {
        self.playing.load(std::sync::atomic::Ordering::Relaxed)
    }

    pub fn take_ended(&self) -> bool {
        self.ended.swap(false, std::sync::atomic::Ordering::Relaxed)
    }

    pub fn stop(&self) {
        let _ = self.webview.evaluate_script("document.getElementById('a').pause()");
        let _ = self.webview.focus_parent();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_url_encodes_spaces_and_cyrillic() {
        let u = file_url(Path::new("C:\\Users\\Иван Петров\\a b.mp4"));
        assert!(u.starts_with("file:///C:/Users/"));
        assert!(!u.contains(' ') && u.is_ascii());
        assert!(u.ends_with("a%20b.mp4"));
    }
}
