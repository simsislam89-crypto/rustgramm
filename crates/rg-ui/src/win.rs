//! Оформление окна Windows: тёмная рамка, без системной подложки (прозрачность считает
//! приложение), скруглённые углы Windows 11. Порт `dress_window` из LLM Launcher.

/// Находит окно по заголовку (оно появляется через несколько кадров) и настраивает DWM.
#[cfg(windows)]
pub fn dress_window(title: &str) {
    use std::ffi::c_void;
    #[link(name = "user32")]
    unsafe extern "system" {
        fn FindWindowW(class: *const u16, title: *const u16) -> *mut c_void;
    }
    #[link(name = "dwmapi")]
    unsafe extern "system" {
        fn DwmSetWindowAttribute(hwnd: *mut c_void, attr: u32, value: *const c_void, size: u32) -> i32;
    }
    const DWMWA_USE_IMMERSIVE_DARK_MODE: u32 = 20;
    const DWMWA_WINDOW_CORNER_PREFERENCE: u32 = 33;
    const DWMWA_SYSTEMBACKDROP_TYPE: u32 = 38;
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
    let set = |attr: u32, value: i32| {
        // SAFETY: указатель на i32 живёт до конца вызова, размер 4 байта.
        let _ = unsafe { DwmSetWindowAttribute(hwnd, attr, (&raw const value).cast(), 4) };
    };
    set(DWMWA_USE_IMMERSIVE_DARK_MODE, 1);
    set(DWMWA_SYSTEMBACKDROP_TYPE, 1); // без Mica/Acrylic: полностью прозрачная поверхность
    set(DWMWA_WINDOW_CORNER_PREFERENCE, 2); // скруглённые углы
}

#[cfg(not(windows))]
pub fn dress_window(_title: &str) {}
