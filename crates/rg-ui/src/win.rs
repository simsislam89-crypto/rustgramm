//! Оформление окна Windows: тёмная рамка, скруглённые углы Windows 11 и динамический
//! Acrylic-блюр стекла (включается на лету, когда прозрачность окна < 100% и включён блюр).

#[cfg(windows)]
mod imp {
    use std::ffi::c_void;
    use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};

    static HWND_VAL: AtomicIsize = AtomicIsize::new(0);
    static WANT_BLUR: AtomicBool = AtomicBool::new(false);
    static APPLIED_BLUR: AtomicBool = AtomicBool::new(false);

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

    #[link(name = "user32")]
    unsafe extern "system" {
        fn FindWindowW(class: *const u16, title: *const u16) -> *mut c_void;
        fn FlashWindowEx(pfwi: *const FlashWInfo) -> i32;
        fn MessageBeep(utype: u32) -> i32;
    }
    #[link(name = "dwmapi")]
    unsafe extern "system" {
        fn DwmSetWindowAttribute(hwnd: *mut c_void, attr: u32, value: *const c_void, size: u32) -> i32;
        fn DwmExtendFrameIntoClientArea(hwnd: *mut c_void, margins: *const Margins) -> i32;
    }

    const DWMWA_USE_IMMERSIVE_DARK_MODE: u32 = 20;
    const DWMWA_WINDOW_CORNER_PREFERENCE: u32 = 33;
    const DWMWA_SYSTEMBACKDROP_TYPE: u32 = 38;

    fn apply_blur(hwnd: *mut c_void, blur: bool) {
        if hwnd.is_null() {
            return;
        }
        let m = Margins { left: -1, right: -1, top: -1, bottom: -1 };
        // SAFETY: структура `Margins` живёт до конца вызова.
        let _ = unsafe { DwmExtendFrameIntoClientArea(hwnd, &raw const m) };
        // 3 = DWMSBT_TRANSIENTWINDOW (Acrylic frosted glass), 1 = DWMSBT_NONE (без размытия).
        let backdrop: i32 = if blur { 3 } else { 1 };
        // SAFETY: указатель на i32 живёт до конца вызова, размер 4 байта.
        let _ = unsafe { DwmSetWindowAttribute(hwnd, DWMWA_SYSTEMBACKDROP_TYPE, (&raw const backdrop).cast(), 4) };
        APPLIED_BLUR.store(blur, Ordering::Relaxed);
    }

    pub fn dress_window(title: &str) {
        let wide: Vec<u16> = title.encode_utf16().chain(std::iter::once(0)).collect();
        let mut hwnd = std::ptr::null_mut();
        for _ in 0..120 {
            // SAFETY: строка заканчивается нулём и живёт до конца вызова.
            hwnd = unsafe { FindWindowW(std::ptr::null(), wide.as_ptr()) };
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
    }

    /// Переключает системное размытие Acrylic за прозрачным окном на лету.
    pub fn set_window_blur(enabled: bool) {
        WANT_BLUR.store(enabled, Ordering::Relaxed);
        let raw = HWND_VAL.load(Ordering::Acquire);
        if raw != 0 && APPLIED_BLUR.load(Ordering::Relaxed) != enabled {
            apply_blur(raw as *mut c_void, enabled);
        }
    }

    /// Мигает значком окна на панели задач Windows до активации окна.
    pub fn flash_taskbar() {
        let raw = HWND_VAL.load(Ordering::Acquire);
        if raw == 0 {
            return;
        }
        // FLASHW_TRAY (2) | FLASHW_TIMERNOFG (12) = 14
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

    /// Проигрывает короткий системный звук уведомления Windows.
    pub fn play_notify_sound() {
        // MB_ICONASTERISK (0x40) — мягкий звук уведомления.
        // SAFETY: системный вызов не принимает указателей.
        let _ = unsafe { MessageBeep(0x0000_0040) };
    }
}

#[cfg(windows)]
pub use imp::{dress_window, flash_taskbar, play_notify_sound, set_window_blur};

#[cfg(not(windows))]
pub fn dress_window(_title: &str) {}

#[cfg(not(windows))]
pub fn set_window_blur(_enabled: bool) {}

#[cfg(not(windows))]
pub fn flash_taskbar() {}

#[cfg(not(windows))]
pub fn play_notify_sound() {}
