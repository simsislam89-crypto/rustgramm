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
    /// Создаёт плеер для `video`; страница записывается в `dir`, данные WebView2 — в `dir/webview`.
    pub fn new(window: &impl HasWindowHandle, video: &Path, dir: &Path, loop_it: bool) -> Result<VideoView, String> {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        let html_path: PathBuf = dir.join("player.html");
        std::fs::write(&html_path, page(&file_url(video), loop_it)).map_err(|e| e.to_string())?;
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
