//! Оформление окна Windows и системная интеграция:
//! - тёмная рамка и скруглённые углы Windows 11, динамический Acrylic-блюр стекла (`set_window_blur`),
//! - защита от захвата экрана в режиме стримера (`set_capture_excluded` -> `WDA_EXCLUDEFROMCAPTURE`),
//! - смена значка приложения и баджа непрочитанных на лету (`set_window_icon`, `render_app_icon_rgba`),
//! - автозапуск Windows (`HKCU\Software\Microsoft\Windows\CurrentVersion\Run`) и регистрация `tg://`,
//! - сворачивание в трей и восстановление (`hide_to_tray`, `show_from_tray`).

/// Набор цветов 12 иконок приложения (первый — фирменный оранжевый Rustgramm `#F58220`).
pub const APP_ICON_PALETTE: [u32; 12] = [
    0xf58220, 0x8f6af0, 0x5865f2, 0x14150f, 0xd9423f, 0xf4f4f4, 0x3a3260, 0xf6f0f4, 0x1f8f86, 0x2a2840,
    0x6a4fb0, 0xd7332f,
];

/// Отрисовывает сглаженную RGBA-иконку приложения размера `size×size` для окна и системного трея
/// (с точкой/счётчиком непрочитанных и рамкой режима призрака, как в `tray_win.cpp`).
pub fn render_app_icon_rgba(
    icon_idx: usize,
    unread: i32,
    muted: bool,
    hide_badge: bool,
    ghost_active: bool,
    size: u32,
) -> Vec<u8> {
    let sz = size.max(16) as usize;
    let mut px = vec![0u8; sz * sz * 4];
    let rgb = APP_ICON_PALETTE[icon_idx.min(APP_ICON_PALETTE.len() - 1)];
    let br = ((rgb >> 16) & 0xff) as f32;
    let bg = ((rgb >> 8) & 0xff) as f32;
    let bb = (rgb & 0xff) as f32;
    let light_bg = (br * 0.299 + bg * 0.587 + bb * 0.114) > 165.0;
    let fg: [f32; 3] = if light_bg { [30.0, 32.0, 38.0] } else { [255.0, 255.0, 255.0] };
    // Тонкое кольцо акцента вокруг кружка (оранжевое в норме, фиолетово-синее в режиме призрака).
    let accent: [f32; 3] = if ghost_active { [138.0, 120.0, 255.0] } else { [245.0, 130.0, 32.0] };

    let half = sz as f32 * 0.5;
    let rad = half - 1.5;

    for y in 0..sz {
        for x in 0..sz {
            let nx = x as f32 + 0.5 - half;
            let ny = y as f32 + 0.5 - half;
            let dist = (nx * nx + ny * ny).sqrt();
            let cov = (rad - dist + 0.75).clamp(0.0, 1.0);
            if cov <= 0.0 {
                continue;
            }
            let mut r = br;
            let mut g = bg;
            let mut b = bb;
            // Тонкий ободок (1.5 px) у края.
            let ring_t = ((dist - (rad - 1.8)).clamp(0.0, 1.0) * cov).clamp(0.0, 1.0);
            if ghost_active || icon_idx == 0 {
                let mix = if ghost_active { 0.85 } else { 0.35 };
                r = r * (1.0 - ring_t * mix) + accent[0] * (ring_t * mix);
                g = g * (1.0 - ring_t * mix) + accent[1] * (ring_t * mix);
                b = b * (1.0 - ring_t * mix) + accent[2] * (ring_t * mix);
            }

            // Стильный логотип-самолёт/крыло внутри кружка (нормализованные координаты -1..1).
            let ux = nx / rad;
            let uy = ny / rad;
            // Треугольник бумажного самолёта: вершины (-0.52, 0.04), (0.50, -0.42), (0.18, 0.48).
            let plane = point_in_triangle(
                ux,
                uy,
                (-0.52, 0.04),
                (0.50, -0.42),
                (0.18, 0.48),
            );
            let cut = point_in_triangle(
                ux,
                uy,
                (-0.14, 0.14),
                (0.46, -0.36),
                (-0.02, 0.38),
            );
            let logo_alpha = if plane && !cut { 0.96 } else if plane && cut { 0.36 } else { 0.0 };
            if logo_alpha > 0.0 {
                r = r * (1.0 - logo_alpha) + fg[0] * logo_alpha;
                g = g * (1.0 - logo_alpha) + fg[1] * logo_alpha;
                b = b * (1.0 - logo_alpha) + fg[2] * logo_alpha;
            }

            let off = (y * sz + x) * 4;
            px[off] = r.round().clamp(0.0, 255.0) as u8;
            px[off + 1] = g.round().clamp(0.0, 255.0) as u8;
            px[off + 2] = b.round().clamp(0.0, 255.0) as u8;
            px[off + 3] = (cov * 255.0).round().clamp(0.0, 255.0) as u8;
        }
    }

    // Бейдж непрочитанных в правом нижнем углу (как в `tray_win.cpp:162-177`).
    if unread > 0 && !hide_badge {
        let bx = sz as f32 * 0.72;
        let by = sz as f32 * 0.72;
        let brad = (sz as f32 * 0.26).max(4.5);
        let badge_rgb: [f32; 3] = if muted { [124.0, 128.0, 138.0] } else { [245.0, 130.0, 32.0] };
        let label = if unread > 9 { "9+" } else { "" };
        for y in 0..sz {
            for x in 0..sz {
                let dx = x as f32 + 0.5 - bx;
                let dy = y as f32 + 0.5 - by;
                let d = (dx * dx + dy * dy).sqrt();
                let outer = (brad + 1.2 - d + 0.6).clamp(0.0, 1.0);
                if outer <= 0.0 {
                    continue;
                }
                let inner = (brad - d + 0.6).clamp(0.0, 1.0);
                let off = (y * sz + x) * 4;
                // Тёмная каёмка вокруг бейджа + оранжевый/серый круг.
                let border_alpha = outer;
                let old_a = f32::from(px[off + 3]) / 255.0;
                let out_a = old_a.max(border_alpha);
                let (tr, tg, tb) = if inner > 0.5 {
                    (badge_rgb[0], badge_rgb[1], badge_rgb[2])
                } else {
                    (20.0, 20.0, 24.0)
                };
                let mix = if inner > 0.0 { border_alpha } else { border_alpha * 0.85 };
                px[off] = (f32::from(px[off]) * (1.0 - mix) + tr * mix).clamp(0.0, 255.0) as u8;
                px[off + 1] = (f32::from(px[off + 1]) * (1.0 - mix) + tg * mix).clamp(0.0, 255.0) as u8;
                px[off + 2] = (f32::from(px[off + 2]) * (1.0 - mix) + tb * mix).clamp(0.0, 255.0) as u8;
                px[off + 3] = (out_a * 255.0).clamp(0.0, 255.0) as u8;
            }
        }
        // Белая точка или цифра внутри бейджа на иконках >= 32 px.
        if sz >= 24 {
            let text = if label.is_empty() {
                unread.to_string()
            } else {
                "!".to_owned()
            };
            draw_tiny_glyph(&mut px, sz, bx.round() as i32, by.round() as i32, &text);
        }
    }

    px
}

fn point_in_triangle(px: f32, py: f32, a: (f32, f32), b: (f32, f32), c: (f32, f32)) -> bool {
    let sign = |p1: (f32, f32), p2: (f32, f32), p3: (f32, f32)| {
        (p1.0 - p3.0) * (p2.1 - p3.1) - (p2.0 - p3.0) * (p1.1 - p3.1)
    };
    let d1 = sign((px, py), a, b);
    let d2 = sign((px, py), b, c);
    let d3 = sign((px, py), c, a);
    let has_neg = d1 < 0.0 || d2 < 0.0 || d3 < 0.0;
    let has_pos = d1 > 0.0 || d2 > 0.0 || d3 > 0.0;
    !(has_neg && has_pos)
}

fn draw_tiny_glyph(px: &mut [u8], sz: usize, cx: i32, cy: i32, s: &str) {
    // 3x5 пиксельные цифры 1..9 и '!'.
    let bits: u16 = match s {
        "1" => 0b010_110_010_010_111,
        "2" => 0b111_001_111_100_111,
        "3" => 0b111_001_111_001_111,
        "4" => 0b101_101_111_001_001,
        "5" => 0b111_100_111_001_111,
        "6" => 0b111_100_111_101_111,
        "7" => 0b111_001_001_010_010,
        "8" => 0b111_101_111_101_111,
        "9" => 0b111_101_111_001_111,
        _ => 0b010_010_010_000_010,
    };
    for r in 0..5 {
        for c in 0..3 {
            let bit = 14 - (r * 3 + c);
            if (bits >> bit) & 1 == 1 {
                let x = cx - 1 + c;
                let y = cy - 2 + r;
                if x >= 0 && y >= 0 && (x as usize) < sz && (y as usize) < sz {
                    let off = (y as usize * sz + x as usize) * 4;
                    px[off] = 255;
                    px[off + 1] = 255;
                    px[off + 2] = 255;
                    px[off + 3] = 255;
                }
            }
        }
    }
}

#[cfg(windows)]
mod imp {
    use std::ffi::c_void;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};

    static HWND_VAL: AtomicIsize = AtomicIsize::new(0);
    static WANT_BLUR: AtomicBool = AtomicBool::new(false);
    static APPLIED_BLUR: AtomicBool = AtomicBool::new(false);
    static WANT_CAPTURE_EXCLUDED: AtomicBool = AtomicBool::new(false);
    static APPLIED_CAPTURE_EXCLUDED: AtomicBool = AtomicBool::new(false);
    type IconCacheEntry = (usize, i32, bool, bool, isize);
    static ICON_CACHE: Mutex<Option<IconCacheEntry>> = Mutex::new(None);

    #[repr(C)]
    struct Margins {
        left: i32,
        right: i32,
        top: i32,
        bottom: i32,
    }

    #[repr(C)]
    struct FlashWInfo {
        cb_size: u32,
        hwnd: *mut c_void,
        flags: u32,
        count: u32,
        timeout: u32,
    }

    #[repr(C)]
    struct IconInfo {
        f_icon: i32,
        x_hotspot: u32,
        y_hotspot: u32,
        hbm_mask: *mut c_void,
        hbm_color: *mut c_void,
    }

    #[link(name = "user32")]
    unsafe extern "system" {
        fn FindWindowW(class: *const u16, title: *const u16) -> *mut c_void;
        fn FlashWindowEx(pfwi: *const FlashWInfo) -> i32;
        fn SetWindowDisplayAffinity(hwnd: *mut c_void, dw_affinity: u32) -> i32;
        fn ShowWindow(hwnd: *mut c_void, cmd_show: i32) -> i32;
        fn IsWindowVisible(hwnd: *mut c_void) -> i32;
        fn IsIconic(hwnd: *mut c_void) -> i32;
        fn SetForegroundWindow(hwnd: *mut c_void) -> i32;
        fn BringWindowToTop(hwnd: *mut c_void) -> i32;
        fn SendMessageW(hwnd: *mut c_void, msg: u32, wparam: usize, lparam: isize) -> isize;
        fn SetWindowPos(
            hwnd: *mut c_void,
            insert_after: *mut c_void,
            x: i32,
            y: i32,
            cx: i32,
            cy: i32,
            flags: u32,
        ) -> i32;
        fn CreateIconIndirect(piconinfo: *const IconInfo) -> *mut c_void;
        pub(crate) fn DestroyIcon(hicon: *mut c_void) -> i32;
    }

    #[link(name = "gdi32")]
    unsafe extern "system" {
        fn CreateBitmap(width: i32, height: i32, planes: u32, bpp: u32, bits: *const c_void) -> *mut c_void;
        fn DeleteObject(obj: *mut c_void) -> i32;
    }

    #[link(name = "dwmapi")]
    unsafe extern "system" {
        fn DwmSetWindowAttribute(hwnd: *mut c_void, attr: u32, value: *const c_void, size: u32) -> i32;
        fn DwmExtendFrameIntoClientArea(hwnd: *mut c_void, margins: *const Margins) -> i32;
    }

    #[link(name = "advapi32")]
    unsafe extern "system" {
        fn RegCreateKeyExW(
            hkey: isize,
            sub_key: *const u16,
            reserved: u32,
            class: *mut u16,
            options: u32,
            sam_desired: u32,
            sec_attr: *const c_void,
            result: *mut isize,
            disposition: *mut u32,
        ) -> i32;
        fn RegOpenKeyExW(
            hkey: isize,
            sub_key: *const u16,
            options: u32,
            sam_desired: u32,
            result: *mut isize,
        ) -> i32;
        fn RegQueryValueExW(
            hkey: isize,
            value_name: *const u16,
            reserved: *mut u32,
            val_type: *mut u32,
            data: *mut u8,
            cb_data: *mut u32,
        ) -> i32;
        fn RegSetValueExW(
            hkey: isize,
            value_name: *const u16,
            reserved: u32,
            val_type: u32,
            data: *const u8,
            cb_data: u32,
        ) -> i32;
        fn RegDeleteValueW(hkey: isize, value_name: *const u16) -> i32;
        fn RegCloseKey(hkey: isize) -> i32;
    }

    const DWMWA_USE_IMMERSIVE_DARK_MODE: u32 = 20;
    const DWMWA_WINDOW_CORNER_PREFERENCE: u32 = 33;
    const DWMWA_SYSTEMBACKDROP_TYPE: u32 = 38;
    const WDA_NONE: u32 = 0x0000_0000;
    const WDA_MONITOR: u32 = 0x0000_0001;
    const WDA_EXCLUDEFROMCAPTURE: u32 = 0x0000_0011;
    const SW_HIDE: i32 = 0;
    const SW_SHOW: i32 = 5;
    const SW_RESTORE: i32 = 9;
    const SWP_NOSIZE: u32 = 0x0001;
    const SWP_NOMOVE: u32 = 0x0002;
    const SWP_NOZORDER: u32 = 0x0004;
    const SWP_NOACTIVATE: u32 = 0x0010;
    const SWP_FRAMECHANGED: u32 = 0x0020;
    const WM_SETICON: u32 = 0x0080;
    const ICON_SMALL: usize = 0;
    const ICON_BIG: usize = 1;
    const HKEY_CURRENT_USER: isize = 0x8000_0001_u32 as i32 as isize;
    const KEY_READ: u32 = 0x0002_0019;
    const KEY_WRITE: u32 = 0x0002_0006;
    const REG_SZ: u32 = 1;

    pub(crate) fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    /// Создаёт Win32 `HICON` из RGBA-буфера `w×h` (сверху вниз). Освобождается через `DestroyIcon`.
    pub(crate) fn rgba_to_hicon(rgba: &[u8], w: u32, h: u32) -> *mut c_void {
        if rgba.len() < (w * h * 4) as usize || w == 0 || h == 0 {
            return std::ptr::null_mut();
        }
        let mut bgra = vec![0u8; (w * h * 4) as usize];
        let (rgba_px, _) = rgba.as_chunks::<4>();
        let (bgra_px, _) = bgra.as_chunks_mut::<4>();
        for (src, dst) in rgba_px.iter().zip(bgra_px.iter_mut()) {
            dst[0] = src[2];
            dst[1] = src[1];
            dst[2] = src[0];
            dst[3] = src[3];
        }
        let row_bytes = (w as usize).div_ceil(16) * 2;
        let and_mask = vec![0u8; row_bytes * h as usize];
        // SAFETY: буферы валидны на время вызова CreateBitmap; созданные HBITMAP удаляются после CreateIconIndirect.
        unsafe {
            let hbm_color = CreateBitmap(w as i32, h as i32, 1, 32, bgra.as_ptr().cast());
            let hbm_mask = CreateBitmap(w as i32, h as i32, 1, 1, and_mask.as_ptr().cast());
            if hbm_color.is_null() || hbm_mask.is_null() {
                if !hbm_color.is_null() {
                    DeleteObject(hbm_color);
                }
                if !hbm_mask.is_null() {
                    DeleteObject(hbm_mask);
                }
                return std::ptr::null_mut();
            }
            let info = IconInfo {
                f_icon: 1,
                x_hotspot: 0,
                y_hotspot: 0,
                hbm_mask,
                hbm_color,
            };
            let icon = CreateIconIndirect(&raw const info);
            DeleteObject(hbm_color);
            DeleteObject(hbm_mask);
            icon
        }
    }

    fn apply_blur(hwnd: *mut c_void, blur: bool) {
        if hwnd.is_null() {
            return;
        }
        let m = if blur {
            Margins { left: -1, right: -1, top: -1, bottom: -1 }
        } else {
            Margins { left: 0, right: 0, top: 0, bottom: 0 }
        };
        // SAFETY: структура `Margins` живёт до конца вызова.
        let _ = unsafe { DwmExtendFrameIntoClientArea(hwnd, &raw const m) };
        let backdrop: i32 = if blur { 3 } else { 1 };
        // SAFETY: указатель на i32 живёт до конца вызова, размер 4 байта.
        let _ = unsafe { DwmSetWindowAttribute(hwnd, DWMWA_SYSTEMBACKDROP_TYPE, (&raw const backdrop).cast(), 4) };
        // Уведомляем DWM о смене клиентской рамки — предотвращает двоение и артефакты прозрачности.
        // SAFETY: валидный HWND, без изменения позиции и размера.
        let _ = unsafe {
            SetWindowPos(
                hwnd,
                std::ptr::null_mut(),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
            )
        };
        APPLIED_BLUR.store(blur, Ordering::Relaxed);
    }

    fn apply_capture_excluded(hwnd: *mut c_void, excluded: bool) {
        if hwnd.is_null() {
            return;
        }
        let affinity = if excluded { WDA_EXCLUDEFROMCAPTURE } else { WDA_NONE };
        // SAFETY: валидный HWND главного окна.
        let ok = unsafe { SetWindowDisplayAffinity(hwnd, affinity) };
        if ok == 0 && excluded {
            // Запасной вариант для старых сборок Windows 10 (чёрный прямоугольник вместо невидимости).
            let _ = unsafe { SetWindowDisplayAffinity(hwnd, WDA_MONITOR) };
        }
        APPLIED_CAPTURE_EXCLUDED.store(excluded, Ordering::Relaxed);
    }

    pub fn dress_window(title: &str) {
        let wtitle = wide(title);
        let mut hwnd = std::ptr::null_mut();
        for _ in 0..120 {
            // SAFETY: строка заканчивается нулём и живёт до конца вызова.
            hwnd = unsafe { FindWindowW(std::ptr::null(), wtitle.as_ptr()) };
            if !hwnd.is_null() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        if hwnd.is_null() {
            return;
        }
        HWND_VAL.store(hwnd as isize, Ordering::Release);
        let set = |attr: u32, value: i32| {
            // SAFETY: указатель на i32 живёт до конца вызова, размер 4 байта.
            let _ = unsafe { DwmSetWindowAttribute(hwnd, attr, (&raw const value).cast(), 4) };
        };
        set(DWMWA_USE_IMMERSIVE_DARK_MODE, 1);
        set(DWMWA_WINDOW_CORNER_PREFERENCE, 2); // скруглённые углы Windows 11
        apply_blur(hwnd, WANT_BLUR.load(Ordering::Relaxed));
        if WANT_CAPTURE_EXCLUDED.load(Ordering::Relaxed) {
            apply_capture_excluded(hwnd, true);
        }
    }

    /// Переключает системное размытие Acrylic за прозрачным окном на лету.
    pub fn set_window_blur(enabled: bool) {
        WANT_BLUR.store(enabled, Ordering::Relaxed);
        let raw = HWND_VAL.load(Ordering::Acquire);
        if raw != 0 && APPLIED_BLUR.load(Ordering::Relaxed) != enabled {
            apply_blur(raw as *mut c_void, enabled);
        }
    }

    /// Режим стримера (`streamer_mode_win.cpp`): исключает окно из захвата экрана (OBS, скриншоты).
    pub fn set_capture_excluded(excluded: bool) {
        WANT_CAPTURE_EXCLUDED.store(excluded, Ordering::Relaxed);
        let raw = HWND_VAL.load(Ordering::Acquire);
        if raw != 0 && APPLIED_CAPTURE_EXCLUDED.load(Ordering::Relaxed) != excluded {
            apply_capture_excluded(raw as *mut c_void, excluded);
        }
    }

    /// Обновляет значок окна на панели задач и в Alt+Tab с выбранной иконкой и баджем.
    pub fn set_window_icon(icon_idx: usize, unread: i32, muted: bool, hide_badge: bool) {
        let raw = HWND_VAL.load(Ordering::Acquire);
        if raw == 0 {
            return;
        }
        let Ok(mut guard) = ICON_CACHE.lock() else {
            return;
        };
        let shown_unread = if hide_badge { 0 } else { unread.min(10) };
        if let Some((ci, cu, cm, cb, _)) = *guard
            && ci == icon_idx
            && cu == shown_unread
            && cm == muted
            && cb == hide_badge
        {
            return;
        }
        let rgba = super::render_app_icon_rgba(icon_idx, shown_unread, muted, hide_badge, false, 64);
        let hicon = rgba_to_hicon(&rgba, 64, 64);
        if hicon.is_null() {
            return;
        }
        // SAFETY: HWND валиден; старый HICON освобождается после замены в окне.
        unsafe {
            let hwnd = raw as *mut c_void;
            SendMessageW(hwnd, WM_SETICON, ICON_SMALL, hicon as isize);
            SendMessageW(hwnd, WM_SETICON, ICON_BIG, hicon as isize);
            if let Some((_, _, _, _, old)) = guard.take()
                && old != 0
            {
                DestroyIcon(old as *mut c_void);
            }
        }
        *guard = Some((icon_idx, shown_unread, muted, hide_badge, hicon as isize));
    }

    pub fn is_window_visible() -> bool {
        let raw = HWND_VAL.load(Ordering::Acquire);
        if raw == 0 {
            return true;
        }
        // SAFETY: валидный HWND или 0.
        unsafe { IsWindowVisible(raw as *mut c_void) != 0 && IsIconic(raw as *mut c_void) == 0 }
    }

    pub fn hide_to_tray() {
        let raw = HWND_VAL.load(Ordering::Acquire);
        if raw != 0 {
            // SAFETY: валидный HWND.
            unsafe {
                ShowWindow(raw as *mut c_void, SW_HIDE);
            }
        }
    }

    pub fn show_from_tray() {
        let raw = HWND_VAL.load(Ordering::Acquire);
        if raw != 0 {
            // SAFETY: валидный HWND.
            unsafe {
                let hwnd = raw as *mut c_void;
                ShowWindow(hwnd, SW_SHOW);
                if IsIconic(hwnd) != 0 {
                    ShowWindow(hwnd, SW_RESTORE);
                }
                BringWindowToTop(hwnd);
                SetForegroundWindow(hwnd);
            }
        }
    }

    /// Проверяет, прописан ли автозапуск в реестре Windows (`HKCU\...\Run\Rustgramm`).
    pub fn autostart_enabled() -> bool {
        let sub = wide(r"Software\Microsoft\Windows\CurrentVersion\Run");
        let name = wide("Rustgramm");
        let mut hkey: isize = 0;
        // SAFETY: константный корневой ключ HKCU и нуль-терминированные UTF-16 строки.
        unsafe {
            if RegOpenKeyExW(HKEY_CURRENT_USER, sub.as_ptr(), 0, KEY_READ, &raw mut hkey) != 0 {
                return false;
            }
            let mut sz: u32 = 0;
            let rc = RegQueryValueExW(hkey, name.as_ptr(), std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null_mut(), &raw mut sz);
            RegCloseKey(hkey);
            rc == 0 && sz > 2
        }
    }

    /// Включает или отключает автозапуск с Windows в `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`.
    pub fn set_autostart(enabled: bool, minimized: bool) -> Result<(), String> {
        let sub = wide(r"Software\Microsoft\Windows\CurrentVersion\Run");
        let name = wide("Rustgramm");
        let mut hkey: isize = 0;
        // SAFETY: нуль-терминированные UTF-16 строки и валидные out-параметры.
        unsafe {
            let rc = RegCreateKeyExW(
                HKEY_CURRENT_USER,
                sub.as_ptr(),
                0,
                std::ptr::null_mut(),
                0,
                KEY_WRITE,
                std::ptr::null(),
                &raw mut hkey,
                std::ptr::null_mut(),
            );
            if rc != 0 {
                return Err(format!("RegCreateKeyExW failed ({rc})"));
            }
            let res = if enabled {
                let exe = std::env::current_exe().map_err(|e| e.to_string())?;
                let cmd = if minimized {
                    format!("\"{}\" --autostart", exe.to_string_lossy())
                } else {
                    format!("\"{}\"", exe.to_string_lossy())
                };
                let val = wide(&cmd);
                let bytes = (val.len() * 2) as u32;
                let wrc = RegSetValueExW(hkey, name.as_ptr(), 0, REG_SZ, val.as_ptr().cast(), bytes);
                if wrc != 0 { Err(format!("RegSetValueExW failed ({wrc})")) } else { Ok(()) }
            } else {
                let drc = RegDeleteValueW(hkey, name.as_ptr());
                // ERROR_FILE_NOT_FOUND (2) считается уже удалённым.
                if drc != 0 && drc != 2 { Err(format!("RegDeleteValueW failed ({drc})")) } else { Ok(()) }
            };
            RegCloseKey(hkey);
            res
        }
    }

    /// Регистрирует обработчик ссылок `tg://` для текущего пользователя напрямую через Registry API.
    pub fn register_tg_url_scheme() -> Result<(), String> {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let set_sz = |sub: &str, name: Option<&str>, data: &str| -> Result<(), String> {
            let wsub = wide(sub);
            let wname = name.map(wide);
            let wdata = wide(data);
            let mut hkey: isize = 0;
            // SAFETY: все строки нуль-терминированы в UTF-16.
            unsafe {
                let rc = RegCreateKeyExW(
                    HKEY_CURRENT_USER,
                    wsub.as_ptr(),
                    0,
                    std::ptr::null_mut(),
                    0,
                    KEY_WRITE,
                    std::ptr::null(),
                    &raw mut hkey,
                    std::ptr::null_mut(),
                );
                if rc != 0 {
                    return Err(format!("RegCreateKeyExW({sub})={rc}"));
                }
                let ptr = wname.as_ref().map_or(std::ptr::null(), Vec::as_ptr);
                let wrc = RegSetValueExW(hkey, ptr, 0, REG_SZ, wdata.as_ptr().cast(), (wdata.len() * 2) as u32);
                RegCloseKey(hkey);
                if wrc != 0 { Err(format!("RegSetValueExW({sub})={wrc}")) } else { Ok(()) }
            }
        };
        set_sz(r"Software\Classes\tg", None, "URL:Telegram Link")?;
        set_sz(r"Software\Classes\tg", Some("URL Protocol"), "")?;
        let cmd = format!("\"{}\" \"%1\"", exe.to_string_lossy());
        set_sz(r"Software\Classes\tg\shell\open\command", None, &cmd)?;
        Ok(())
    }

    /// Мигает значком окна на панели задач Windows до активации окна.
    pub fn flash_taskbar() {
        let raw = HWND_VAL.load(Ordering::Acquire);
        if raw == 0 {
            return;
        }
        let info = FlashWInfo {
            cb_size: std::mem::size_of::<FlashWInfo>() as u32,
            hwnd: raw as *mut c_void,
            flags: 2 | 12,
            count: 3,
            timeout: 0,
        };
        // SAFETY: структура валидна на время вызова.
        let _ = unsafe { FlashWindowEx(&raw const info) };
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn CreateMutexW(sec: *const c_void, initial_owner: i32, name: *const u16) -> *mut c_void;
        fn GetLastError() -> u32;
        fn CloseHandle(h: *mut c_void) -> i32;
    }

    const ERROR_ALREADY_EXISTS: u32 = 183;

    /// Удерживает именованный мьютекс Windows, запрещая одновременный запуск двух копий Rustgramm.
    pub struct SingleInstanceGuard(*mut c_void);

    // SAFETY: хэндл мьютекса закрывается только при Drop в конце жизни процесса.
    unsafe impl Send for SingleInstanceGuard {}

    impl Drop for SingleInstanceGuard {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe {
                    CloseHandle(self.0);
                }
                self.0 = std::ptr::null_mut();
            }
        }
    }

    /// Проверяет, запущен ли уже другой экземпляр Rustgramm. Если да — фокусирует его окно и возвращает `None`.
    pub fn acquire_single_instance() -> Option<SingleInstanceGuard> {
        if std::env::var_os("RG_SECOND_INSTANCE").is_some() || std::env::var_os("RG_DEMO").is_some() {
            return Some(SingleInstanceGuard(std::ptr::null_mut()));
        }
        let name = wide("Local\\Rustgramm-SingleInstance-Mutex");
        // SAFETY: `name` нуль-терминирован в UTF-16.
        let handle = unsafe { CreateMutexW(std::ptr::null(), 0, name.as_ptr()) };
        if handle.is_null() {
            return Some(SingleInstanceGuard(std::ptr::null_mut()));
        }
        let err = unsafe { GetLastError() };
        if err == ERROR_ALREADY_EXISTS {
            unsafe {
                CloseHandle(handle);
            }
            let wtitle = wide("Rustgramm");
            let hwnd = unsafe { FindWindowW(std::ptr::null(), wtitle.as_ptr()) };
            if !hwnd.is_null() {
                unsafe {
                    ShowWindow(hwnd, SW_SHOW);
                    if IsIconic(hwnd) != 0 {
                        ShowWindow(hwnd, SW_RESTORE);
                    }
                    BringWindowToTop(hwnd);
                    SetForegroundWindow(hwnd);
                }
            }
            return None;
        }
        Some(SingleInstanceGuard(handle))
    }

    /// Проигрывает чистый двухтоновый WAV-звук уведомления через WinMM (работает даже при отключённых системных звуках Windows).
    pub fn play_notify_sound(volume: u8) {
        crate::audio_io::play_notify_chime(volume);
    }
}

#[cfg(windows)]
pub(crate) use imp::{DestroyIcon, rgba_to_hicon, wide};
#[cfg(windows)]
pub use imp::{
    SingleInstanceGuard, acquire_single_instance, autostart_enabled, dress_window, flash_taskbar,
    hide_to_tray, is_window_visible, play_notify_sound, register_tg_url_scheme, set_autostart,
    set_capture_excluded, set_window_blur, set_window_icon, show_from_tray,
};

#[cfg(not(windows))]
pub fn dress_window(_title: &str) {}
#[cfg(not(windows))]
pub fn set_window_blur(_enabled: bool) {}
#[cfg(not(windows))]
pub fn set_capture_excluded(_excluded: bool) {}
#[cfg(not(windows))]
pub fn set_window_icon(_icon_idx: usize, _unread: i32, _muted: bool, _hide_badge: bool) {}
#[cfg(not(windows))]
pub fn is_window_visible() -> bool {
    true
}
#[cfg(not(windows))]
pub fn hide_to_tray() {}
#[cfg(not(windows))]
pub fn show_from_tray() {}
#[cfg(not(windows))]
pub fn autostart_enabled() -> bool {
    false
}
#[cfg(not(windows))]
pub fn set_autostart(_enabled: bool, _minimized: bool) -> Result<(), String> {
    Ok(())
}
#[cfg(not(windows))]
pub fn register_tg_url_scheme() -> Result<(), String> {
    Ok(())
}
#[cfg(not(windows))]
pub fn flash_taskbar() {}
#[cfg(not(windows))]
pub struct SingleInstanceGuard;
#[cfg(not(windows))]
pub fn acquire_single_instance() -> Option<SingleInstanceGuard> {
    Some(SingleInstanceGuard)
}
#[cfg(not(windows))]
pub fn play_notify_sound(_volume: u8) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icon_rgba_size_and_badge_diff() {
        let plain = render_app_icon_rgba(0, 0, false, false, false, 32);
        let badged = render_app_icon_rgba(0, 5, false, false, false, 32);
        let hidden = render_app_icon_rgba(0, 5, false, true, false, 32);
        assert_eq!(plain.len(), 32 * 32 * 4);
        assert_eq!(plain, hidden);
        assert_ne!(plain, badged);
    }

    #[test]
    fn ghost_and_palette_produce_distinct_icons() {
        let a = render_app_icon_rgba(0, 0, false, false, false, 32);
        let b = render_app_icon_rgba(4, 0, false, false, false, 32);
        let ghost = render_app_icon_rgba(0, 0, false, false, true, 32);
        assert_ne!(a, b);
        assert_ne!(a, ghost);
    }
}
