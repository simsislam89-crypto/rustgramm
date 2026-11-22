//! Встроенный медиаплеер и Mini App (Windows): WebView2 — движок Edge из Windows 11.
//! - `VideoView`: аппаратное декодирование и звук для MP4/H.264, WebM/VP9, GIF;
//! - `InlineGifView`: автозапуск GIF в ленте;
//! - `AudioView`: надёжный аудиоплеер для музыки и голосовых (видимый offscreen WebView2, чтобы
//!   Chromium не глушил аудиопоток, с сохранением `WebContext` и явным `play()`/`muted=false`);
//! - `MiniAppView`: полноценный просмотрщик Telegram Mini Apps с JS-бриджем `TelegramWebviewProxy`
//!   (BackButton, MainButton, expand, close, open_link, open_tg_link, themeParams).

#![cfg(windows)]

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use wry::dpi::{PhysicalPosition, PhysicalSize};
use wry::raw_window_handle::HasWindowHandle;
use wry::{Rect, WebContext, WebView, WebViewBuilder};

pub struct VideoView {
    _context: WebContext,
    webview: WebView,
    bounds: (i32, i32, u32, u32),
    closed: Arc<AtomicBool>,
    dims: Arc<AtomicU64>,
    full: Arc<AtomicBool>,
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
    pub fn new(window: &impl HasWindowHandle, video: &Path, dir: &Path, loop_it: bool) -> Result<VideoView, String> {
        Self::new_url(window, &file_url(video), dir, loop_it)
    }

    pub fn new_url(window: &impl HasWindowHandle, src: &str, dir: &Path, loop_it: bool) -> Result<VideoView, String> {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        let html_path: PathBuf = dir.join("player.html");
        std::fs::write(&html_path, page(src, loop_it)).map_err(|e| e.to_string())?;
        let icons = dir.join("i");
        std::fs::create_dir_all(&icons).map_err(|e| e.to_string())?;
        for (file, name) in PLAYER_ICONS {
            super::icons::export_png(name, &icons.join(format!("{file}.png")));
        }
        let closed = Arc::new(AtomicBool::new(false));
        let flag = closed.clone();
        let dims = Arc::new(AtomicU64::new(0));
        let dims_w = dims.clone();
        let full = Arc::new(AtomicBool::new(false));
        let full_w = full.clone();
        let mut context = WebContext::new(Some(dir.join("webview")));
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
            .with_document_title_changed_handler(move |title| {
                let Some((_, body)) = title.strip_prefix("rg").and_then(|t| t.split_once('|')) else {
                    return;
                };
                if body == "close" {
                    flag.store(true, Ordering::Relaxed);
                } else if let Some(rest) = body.strip_prefix("size:") {
                    let mut it = rest.split(':').filter_map(|x| x.parse::<u64>().ok());
                    if let (Some(w), Some(h)) = (it.next(), it.next()) {
                        dims_w.store(w << 32 | h, Ordering::Relaxed);
                    }
                } else if let Some(f) = body.strip_prefix("fs:") {
                    full_w.store(f == "1", Ordering::Relaxed);
                }
            })
            .build_as_child(window)
            .map_err(|e| format!("WebView2 не запустился: {e}"))?;
        Ok(VideoView { _context: context, webview, bounds: (0, 0, 16, 9), closed, dims, full })
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

    pub fn aspect(&self) -> Option<f32> {
        let d = self.dims.load(Ordering::Relaxed);
        let (w, h) = ((d >> 32) as f32, (d & 0xffff_ffff) as f32);
        (w > 0.0 && h > 0.0).then(|| w / h)
    }

    pub fn is_full(&self) -> bool {
        self.full.load(Ordering::Relaxed)
    }

    pub fn wants_close(&self) -> bool {
        self.closed.load(Ordering::Relaxed)
    }

    pub fn stop(&self) {
        let _ = self.webview.evaluate_script("document.getElementById('v').pause()");
        let _ = self.webview.focus_parent();
    }
}

/// Встроенный в ленту мини-плеер для автозапуска GIF и коротких роликов без звука.
pub struct InlineGifView {
    _context: WebContext,
    webview: WebView,
    pub msg_id: i32,
    bounds: (i32, i32, u32, u32),
    clicked: Arc<AtomicBool>,
}

impl InlineGifView {
    pub fn new(window: &impl HasWindowHandle, msg_id: i32, video: &Path, dir: &Path) -> Result<Self, String> {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        let html_path = dir.join("inline.html");
        let src = file_url(video);
        let html = format!(
            "<!doctype html><html><head><meta charset=utf-8><style>\
            html,body{{margin:0;height:100%;background:transparent;overflow:hidden;cursor:pointer}}\
            video{{width:100%;height:100%;object-fit:contain;display:block;background:transparent}}\
            </style></head><body><video id=v src=\"{src}\" autoplay loop muted playsinline></video>\
            <script>var n=0;document.onclick=function(){{document.title='rg'+(++n)+'|open'}}</script>\
            </body></html>"
        );
        std::fs::write(&html_path, html).map_err(|e| e.to_string())?;
        let clicked = Arc::new(AtomicBool::new(false));
        let flag = clicked.clone();
        let mut context = WebContext::new(Some(dir.join("webview")));
        let webview = WebViewBuilder::new_with_web_context(&mut context)
            .with_url(file_url(&html_path))
            .with_autoplay(true)
            .with_transparent(true)
            .with_background_color((0, 0, 0, 0))
            .with_devtools(false)
            .with_bounds(Rect {
                position: PhysicalPosition::new(0, 0).into(),
                size: PhysicalSize::new(16u32, 16u32).into(),
            })
            .with_new_window_req_handler(|_, _| wry::NewWindowResponse::Deny)
            .with_document_title_changed_handler(move |title| {
                if title.ends_with("|open") {
                    flag.store(true, Ordering::Relaxed);
                }
            })
            .build_as_child(window)
            .map_err(|e| e.to_string())?;
        Ok(Self { _context: context, webview, msg_id, bounds: (0, 0, 16, 16), clicked })
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
        self.clicked.swap(false, Ordering::Relaxed)
    }

    pub fn stop(&self) {
        let _ = self.webview.evaluate_script("document.getElementById('v').pause()");
        let _ = self.webview.focus_parent();
    }
}

/// Встроенный аудиоплеер для музыки, голосовых сообщений и аудиофайлов.
/// Держит WebView2 видимым за пределами окна (`-4000, -4000`) + явный `a.muted = false; a.play()`,
/// чтобы Chromium не глушил звук из-за `IsVisible(false)` / политики автозапуска.
pub struct AudioView {
    _context: WebContext,
    webview: WebView,
    pos_ms: Arc<AtomicU64>,
    dur_ms: Arc<AtomicU64>,
    playing: Arc<AtomicBool>,
    ended: Arc<AtomicBool>,
    last_err: Arc<Mutex<Option<String>>>,
}

impl AudioView {
    pub fn new(window: &impl HasWindowHandle, audio: &Path, dir: &Path, volume: f32) -> Result<Self, String> {
        Self::new_url(window, &file_url(audio), dir, volume)
    }

    pub fn new_url(window: &impl HasWindowHandle, src: &str, dir: &Path, volume: f32) -> Result<Self, String> {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        let html_path = dir.join("audio.html");
        let vol = volume.clamp(0.05, 1.0);
        let html = format!(
            "<!doctype html><html><head><meta charset=utf-8></head><body>\
            <audio id=a src=\"{src}\" preload=auto autoplay></audio>\
            <script>\
            var a=document.getElementById('a'),n=0,started=false;\
            a.muted=false;a.volume={vol:.3};\
            function rep(ev){{\
              var p=Math.round((a.currentTime||0)*1000);\
              var d=Math.round((isFinite(a.duration)?a.duration:0)*1000);\
              var pl=a.paused?0:1;\
              document.title='rga'+(++n)+'|'+ev+'|'+p+'|'+d+'|'+pl;\
            }}\
            function kick(){{\
              a.muted=false;\
              var pr=a.play();\
              if(pr&&pr.then){{pr.then(function(){{started=true;rep('p')}}).catch(function(e){{document.title='rge'+(++n)+'|'+(e&&e.message?e.message:'play_err')}});}}\
            }}\
            a.onplay=function(){{started=true;rep('p')}};\
            a.onpause=function(){{rep('p')}};\
            a.onended=function(){{rep('e')}};\
            a.onloadedmetadata=function(){{rep('m');kick()}};\
            a.oncanplay=function(){{if(!started)kick()}};\
            a.onerror=function(){{var c=a.error?a.error.code:0;document.title='rge'+(++n)+'|media_err_'+c}};\
            kick();\
            setInterval(function(){{if(!a.paused)rep('t')}},250);\
            </script></body></html>"
        );
        std::fs::write(&html_path, html).map_err(|e| e.to_string())?;
        let pos_ms = Arc::new(AtomicU64::new(0));
        let dur_ms = Arc::new(AtomicU64::new(0));
        let playing = Arc::new(AtomicBool::new(true));
        let ended = Arc::new(AtomicBool::new(false));
        let last_err: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
        let (pw, dw, plw, ew, errw) = (
            pos_ms.clone(),
            dur_ms.clone(),
            playing.clone(),
            ended.clone(),
            last_err.clone(),
        );
        let mut context = WebContext::new(Some(dir.join("webview")));
        let webview = WebViewBuilder::new_with_web_context(&mut context)
            .with_url(file_url(&html_path))
            .with_autoplay(true)
            .with_visible(true)
            .with_devtools(false)
            .with_bounds(Rect {
                position: PhysicalPosition::new(-4000, -4000).into(),
                size: PhysicalSize::new(2u32, 2u32).into(),
            })
            .with_new_window_req_handler(|_, _| wry::NewWindowResponse::Deny)
            .with_document_title_changed_handler(move |title| {
                if let Some((_, msg)) = title.strip_prefix("rge").and_then(|t| t.split_once('|')) {
                    if let Ok(mut g) = errw.lock() {
                        *g = Some(format!("Аудио ({msg})"));
                    }
                    plw.store(false, Ordering::Relaxed);
                    return;
                }
                let Some(rest) = title.strip_prefix("rga").and_then(|t| t.split_once('|')).map(|(_, b)| b) else {
                    return;
                };
                let mut parts = rest.split('|');
                let ev = parts.next().unwrap_or("");
                if let Some(p) = parts.next().and_then(|s| s.parse::<u64>().ok()) {
                    pw.store(p, Ordering::Relaxed);
                }
                if let Some(d) = parts.next().and_then(|s| s.parse::<u64>().ok())
                    && d > 0
                {
                    dw.store(d, Ordering::Relaxed);
                }
                if let Some(pl) = parts.next() {
                    plw.store(pl == "1", Ordering::Relaxed);
                }
                if ev == "e" {
                    ew.store(true, Ordering::Relaxed);
                    plw.store(false, Ordering::Relaxed);
                }
            })
            .build_as_child(window)
            .map_err(|e| format!("Аудиоплеер: {e}"))?;
        let _ = webview.focus_parent();
        Ok(Self { _context: context, webview, pos_ms, dur_ms, playing, ended, last_err })
    }

    pub fn set_playing(&self, play: bool) {
        let js = if play {
            "var a=document.getElementById('a');if(a){a.muted=false;a.play().catch(function(){})}"
        } else {
            "var a=document.getElementById('a');if(a)a.pause()"
        };
        let _ = self.webview.evaluate_script(js);
        self.playing.store(play, Ordering::Relaxed);
    }

    pub fn seek_secs(&self, secs: f32) {
        let s = secs.max(0.0);
        let _ = self.webview.evaluate_script(&format!("document.getElementById('a').currentTime={s:.2}"));
        self.pos_ms.store((s * 1000.0) as u64, Ordering::Relaxed);
    }

    pub fn set_volume(&self, vol: f32) {
        let v = vol.clamp(0.0, 1.0);
        let _ = self.webview.evaluate_script(&format!("var a=document.getElementById('a');if(a){{a.muted=false;a.volume={v:.3}}}"));
    }

    pub fn pos_secs(&self) -> f32 {
        self.pos_ms.load(Ordering::Relaxed) as f32 / 1000.0
    }

    pub fn dur_secs(&self) -> f32 {
        self.dur_ms.load(Ordering::Relaxed) as f32 / 1000.0
    }

    pub fn is_playing(&self) -> bool {
        self.playing.load(Ordering::Relaxed)
    }

    pub fn take_ended(&self) -> bool {
        self.ended.swap(false, Ordering::Relaxed)
    }

    pub fn take_error(&self) -> Option<String> {
        self.last_err.lock().ok().and_then(|mut g| g.take())
    }

    pub fn stop(&self) {
        let _ = self.webview.evaluate_script("var a=document.getElementById('a');if(a)a.pause()");
        let _ = self.webview.focus_parent();
    }
}

/// Состояние встроенного Telegram Mini App (обновляется через JS-бридж `TelegramWebviewProxy`).
#[derive(Debug, Clone, Default)]
pub struct MiniAppBridgeState {
    pub wants_close: bool,
    pub expanded: bool,
    pub back_visible: bool,
    pub main_btn_visible: bool,
    pub main_btn_text: String,
    pub main_btn_color: Option<(u8, u8, u8)>,
    pub header_color: Option<(u8, u8, u8)>,
    pub open_link: Option<String>,
    pub open_tg_link: Option<String>,
}

fn parse_hex_rgb(s: &str) -> Option<(u8, u8, u8)> {
    let h = s.trim().trim_start_matches('#');
    if h.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&h[0..2], 16).ok()?;
    let g = u8::from_str_radix(&h[2..4], 16).ok()?;
    let b = u8::from_str_radix(&h[4..6], 16).ok()?;
    Some((r, g, b))
}

const MINIAPP_BRIDGE_JS: &str = r#"(function(){
  if(window.__rgBridgeInstalled) return;
  window.__rgBridgeInstalled = true;
  function sendToHost(ev, data){
    try {
      if(window.ipc && window.ipc.postMessage){
        window.ipc.postMessage(JSON.stringify({event: ev, data: data || {}}));
      }
    } catch(_){}
  }
  window.TelegramWebviewProxy = {
    postEvent: function(ev, raw){
      var d = raw;
      if(typeof raw === 'string' && raw.length){ try { d = JSON.parse(raw); } catch(_){} }
      sendToHost(ev, d);
    }
  };
  window.external = window.external || {};
  window.external.notify = function(msg){
    try {
      var o = JSON.parse(msg);
      sendToHost(o.eventType || o.event || '', o.eventData || o.data || {});
    } catch(_){}
  };
  window.__rgReceive = function(ev, payload){
    try {
      if(window.Telegram && window.Telegram.WebView && window.Telegram.WebView.receiveEvent){
        window.Telegram.WebView.receiveEvent(ev, payload || {});
      }
      window.dispatchEvent(new CustomEvent('telegram-webview-event', {detail:{event:ev, data:payload||{}}}));
    } catch(_){}
  };
  setTimeout(function(){
    window.__rgReceive('theme_changed', {theme_params:{
      bg_color:'#17171b', secondary_bg_color:'#1f1f24', text_color:'#f5f5f7',
      hint_color:'#9b9ba6', link_color:'#f58220', button_color:'#f58220', button_text_color:'#ffffff',
      header_bg_color:'#17171b', accent_text_color:'#f58220', section_bg_color:'#1f1f24'
    }});
    window.__rgReceive('viewport_changed', {height: window.innerHeight, is_state_stable: true, is_expanded: true});
  }, 80);
})();"#;

/// Встроенное окно Mini App / WebApp бота (WebView2 + `TelegramWebviewProxy` IPC + изолированный профиль).
pub struct MiniAppView {
    _context: WebContext,
    webview: WebView,
    bounds: (i32, i32, u32, u32),
    bridge: Arc<Mutex<MiniAppBridgeState>>,
}

impl MiniAppView {
    pub fn new(
        window: &impl HasWindowHandle,
        url: &str,
        dir: &Path,
        spoof_android: bool,
    ) -> Result<Self, String> {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        let bridge = Arc::new(Mutex::new(MiniAppBridgeState::default()));
        let bw = bridge.clone();
        let mut context = WebContext::new(Some(dir.join("miniapp_webview")));
        let mut builder = WebViewBuilder::new_with_web_context(&mut context)
            .with_url(url)
            .with_autoplay(true)
            .with_background_color((23, 23, 27, 255))
            .with_devtools(false)
            .with_initialization_script(MINIAPP_BRIDGE_JS)
            .with_ipc_handler(move |req| {
                let body = req.body();
                let Ok(v) = serde_json::from_str::<serde_json::Value>(body) else { return };
                let ev = v.get("event").and_then(serde_json::Value::as_str).unwrap_or("");
                let data = v.get("data");
                let Ok(mut st) = bw.lock() else { return };
                match ev {
                    "web_app_close" => st.wants_close = true,
                    "web_app_expand" => st.expanded = true,
                    "web_app_setup_back_button" => {
                        st.back_visible = data
                            .and_then(|d| d.get("is_visible"))
                            .and_then(serde_json::Value::as_bool)
                            .unwrap_or(false);
                    }
                    "web_app_setup_main_button" => {
                        if let Some(d) = data {
                            st.main_btn_visible = d.get("is_visible").and_then(serde_json::Value::as_bool).unwrap_or(false);
                            if let Some(t) = d.get("text").and_then(serde_json::Value::as_str) {
                                st.main_btn_text = t.to_owned();
                            }
                            if let Some(c) = d.get("color").and_then(serde_json::Value::as_str) {
                                st.main_btn_color = parse_hex_rgb(c);
                            }
                        }
                    }
                    "web_app_set_header_color" => {
                        if let Some(c) = data
                            .and_then(|d| d.get("color"))
                            .and_then(serde_json::Value::as_str)
                        {
                            st.header_color = parse_hex_rgb(c);
                        }
                    }
                    "web_app_open_link" => {
                        if let Some(u) = data.and_then(|d| d.get("url")).and_then(serde_json::Value::as_str) {
                            st.open_link = Some(u.to_owned());
                        }
                    }
                    "web_app_open_tg_link" => {
                        if let Some(p) = data.and_then(|d| d.get("path_full")).and_then(serde_json::Value::as_str) {
                            st.open_tg_link = Some(format!("https://t.me/{}", p.trim_start_matches('/')));
                        }
                    }
                    _ => {}
                }
            })
            .with_bounds(Rect {
                position: PhysicalPosition::new(0, 0).into(),
                size: PhysicalSize::new(16u32, 9u32).into(),
            });
        if spoof_android {
            builder = builder.with_user_agent(
                "Mozilla/5.0 (Linux; Android 14; Pixel 8 Pro) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36 Telegram-Android/11.2.0",
            );
        }
        let webview = builder
            .build_as_child(window)
            .map_err(|e| format!("Mini App WebView2: {e}"))?;
        Ok(Self {
            _context: context,
            webview,
            bounds: (0, 0, 0, 0),
            bridge,
        })
    }

    pub fn place(&mut self, rect: (i32, i32, u32, u32)) {
        if self.bounds == rect {
            return;
        }
        self.bounds = rect;
        let (x, y, w, h) = rect;
        let _ = self.webview.set_bounds(Rect {
            position: PhysicalPosition::new(x, y).into(),
            size: PhysicalSize::new(w.max(1), h.max(1)).into(),
        });
        let _ = self.webview.evaluate_script(&format!(
            "if(window.__rgReceive)window.__rgReceive('viewport_changed',{{height:{},is_state_stable:true,is_expanded:true}})",
            h
        ));
    }

    pub fn bridge_snapshot(&self) -> MiniAppBridgeState {
        self.bridge.lock().map(|g| g.clone()).unwrap_or_default()
    }

    pub fn take_links(&self) -> (Option<String>, Option<String>) {
        if let Ok(mut g) = self.bridge.lock() {
            (g.open_link.take(), g.open_tg_link.take())
        } else {
            (None, None)
        }
    }

    pub fn emit_back_pressed(&self) {
        let _ = self.webview.evaluate_script("if(window.__rgReceive)window.__rgReceive('back_button_pressed',{})");
    }

    pub fn emit_main_button_pressed(&self) {
        let _ = self.webview.evaluate_script("if(window.__rgReceive)window.__rgReceive('main_button_pressed',{})");
    }

    pub fn reload(&self) {
        let _ = self.webview.evaluate_script("location.reload()");
    }

    pub fn stop(&self) {
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

    #[test]
    fn parses_hex_rgb_colors() {
        assert_eq!(parse_hex_rgb("#F58220"), Some((0xF5, 0x82, 0x20)));
        assert_eq!(parse_hex_rgb("17171b"), Some((0x17, 0x17, 0x1b)));
        assert_eq!(parse_hex_rgb("bad"), None);
    }
}
