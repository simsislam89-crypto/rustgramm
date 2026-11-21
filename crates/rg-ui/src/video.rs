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
    /// Страница плеера просила закрыть просмотр (Esc или крестик внутри окна).
    closed: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

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
    format!(
        r#"<!doctype html><html><head><meta charset="utf-8">
<style>html,body{{margin:0;height:100%;background:#000;overflow:hidden}}
video{{position:fixed;inset:0;width:100%;height:100%;object-fit:contain;background:#000;outline:none}}
#x{{position:fixed;top:8px;right:8px;width:32px;height:32px;border:0;border-radius:16px;background:rgba(0,0,0,.45);color:#fff;font:18px sans-serif;cursor:pointer;opacity:0;transition:opacity .15s;z-index:5}}
body:hover #x{{opacity:1}}</style>
</head><body><video id="v" src="{src}" controls autoplay {lp} playsinline></video><button id="x" title="Закрыть (Esc)">&#10005;</button>
<script>
var v=document.getElementById('v');
document.getElementById('x').onclick=function(){{window.ipc.postMessage('close')}};
document.addEventListener('keydown',function(e){{
  if(e.code==='Escape'){{window.ipc.postMessage('close')}}
  else if(e.code==='Space'){{e.preventDefault();v.paused?v.play():v.pause()}}
  else if(e.code==='ArrowRight'){{v.currentTime+=5}}
  else if(e.code==='ArrowLeft'){{v.currentTime-=5}}
  else if(e.code==='KeyM'){{v.muted=!v.muted}}
  else if(e.code==='KeyF'){{document.fullscreenElement?document.exitFullscreen():v.requestFullscreen()}}
}});
</script></body></html>"#,
        lp = if loop_it { "loop" } else { "" },
    )
}

impl VideoView {
    /// Создаёт плеер для `video`; страница записывается в `dir`, данные WebView2 — в `dir/webview`.
    pub fn new(window: &impl HasWindowHandle, video: &Path, dir: &Path, loop_it: bool) -> Result<VideoView, String> {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        let html_path: PathBuf = dir.join("player.html");
        std::fs::write(&html_path, page(&file_url(video), loop_it)).map_err(|e| e.to_string())?;
        let closed = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flag = closed.clone();
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
            .with_ipc_handler(move |req| {
                if req.body() == "close" {
                    flag.store(true, std::sync::atomic::Ordering::Relaxed);
                }
            })
            .build_as_child(window)
            .map_err(|e| format!("WebView2 не запустился: {e}"))?;
        Ok(VideoView { webview, bounds: (0, 0, 16, 9), closed })
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
