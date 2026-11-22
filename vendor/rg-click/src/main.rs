//! Локальная Win32-утилита для тестирования окон Rustgramm и AyuGram на рабочем столе.
//! Команды:
//!   rg-click list                          — список видимых окон (HWND PID L T W H TITLE)
//!   rg-click focus <часть_заголовка>       — вывести окно на передний план
//!   rg-click move <x> <y>                  — навести курсор
//!   rg-click click <x> <y> [delay_ms]      — навести, подождать (по умолчанию 1000 мс для egui) и кликнуть ЛКМ
//!   rg-click rclick <x> <y> [delay_ms]     — клик ПКМ
//!   rg-click key <esc|enter|tab|space|f>   — нажать клавишу
//!   rg-click type <текст>                  — ввести текст (Unicode)
//!   rg-click shot <заголовок|screen> <out.bmp> — сохранить снимок окна или экрана

#[cfg(windows)]
mod win32 {
    use std::ffi::c_void;
    use std::io::Write;
    use std::path::Path;

    pub type Hwnd = isize;

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    pub struct Rect {
        pub left: i32,
        pub top: i32,
        pub right: i32,
        pub bottom: i32,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct MouseInput {
        dx: i32,
        dy: i32,
        mouse_data: u32,
        dw_flags: u32,
        time: u32,
        dw_extra_info: usize,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct KeybdInput {
        w_vk: u16,
        w_scan: u16,
        dw_flags: u32,
        time: u32,
        dw_extra_info: usize,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    union InputUnion {
        mi: MouseInput,
        ki: KeybdInput,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct Input {
        r#type: u32,
        u: InputUnion,
    }

    #[repr(C)]
    struct BitmapInfoHeader {
        bi_size: u32,
        bi_width: i32,
        bi_height: i32,
        bi_planes: u16,
        bi_bit_count: u16,
        bi_compression: u32,
        bi_size_image: u32,
        bi_x_pels_per_meter: i32,
        bi_y_pels_per_meter: i32,
        bi_clr_used: u32,
        bi_clr_important: u32,
    }

    #[repr(C)]
    struct BitmapInfo {
        bmi_header: BitmapInfoHeader,
        bmi_colors: [u32; 1],
    }

    const INPUT_MOUSE: u32 = 0;
    const INPUT_KEYBOARD: u32 = 1;
    const MOUSEEVENTF_LEFTDOWN: u32 = 0x0002;
    const MOUSEEVENTF_LEFTUP: u32 = 0x0004;
    const MOUSEEVENTF_RIGHTDOWN: u32 = 0x0008;
    const MOUSEEVENTF_RIGHTUP: u32 = 0x0010;
    const KEYEVENTF_KEYUP: u32 = 0x0002;
    const KEYEVENTF_UNICODE: u32 = 0x0004;
    const SW_RESTORE: i32 = 9;
    const SRCCOPY: u32 = 0x00CC0020;
    const PW_RENDERFULLCONTENT: u32 = 2;
    const DIB_RGB_COLORS: u32 = 0;
    const SM_CXSCREEN: i32 = 0;
    const SM_CYSCREEN: i32 = 1;

    #[link(name = "user32")]
    unsafe extern "system" {
        fn SetProcessDPIAware() -> i32;
        fn EnumWindows(cb: extern "system" fn(Hwnd, isize) -> i32, lparam: isize) -> i32;
        fn IsWindowVisible(hwnd: Hwnd) -> i32;
        fn IsIconic(hwnd: Hwnd) -> i32;
        fn GetWindowTextLengthW(hwnd: Hwnd) -> i32;
        fn GetWindowTextW(hwnd: Hwnd, buf: *mut u16, max: i32) -> i32;
        fn GetWindowThreadProcessId(hwnd: Hwnd, pid: *mut u32) -> u32;
        fn GetWindowRect(hwnd: Hwnd, rect: *mut Rect) -> i32;
        fn ShowWindow(hwnd: Hwnd, cmd: i32) -> i32;
        fn SetForegroundWindow(hwnd: Hwnd) -> i32;
        fn SetCursorPos(x: i32, y: i32) -> i32;
        fn SendInput(c: u32, inputs: *const Input, cb_size: i32) -> u32;
        fn GetDC(hwnd: Hwnd) -> *mut c_void;
        fn ReleaseDC(hwnd: Hwnd, hdc: *mut c_void) -> i32;
        fn PrintWindow(hwnd: Hwnd, hdc: *mut c_void, flags: u32) -> i32;
        fn GetSystemMetrics(index: i32) -> i32;
    }

    #[link(name = "gdi32")]
    unsafe extern "system" {
        fn CreateCompatibleDC(hdc: *mut c_void) -> *mut c_void;
        fn CreateCompatibleBitmap(hdc: *mut c_void, w: i32, h: i32) -> *mut c_void;
        fn SelectObject(hdc: *mut c_void, obj: *mut c_void) -> *mut c_void;
        fn BitBlt(
            dst: *mut c_void,
            x: i32,
            y: i32,
            w: i32,
            h: i32,
            src: *mut c_void,
            sx: i32,
            sy: i32,
            rop: u32,
        ) -> i32;
        fn GetDIBits(
            hdc: *mut c_void,
            hbmp: *mut c_void,
            start: u32,
            lines: u32,
            bits: *mut c_void,
            bmi: *mut BitmapInfo,
            usage: u32,
        ) -> i32;
        fn DeleteObject(obj: *mut c_void) -> i32;
        fn DeleteDC(hdc: *mut c_void) -> i32;
    }

    pub struct WinInfo {
        pub hwnd: Hwnd,
        pub pid: u32,
        pub rect: Rect,
        pub title: String,
    }

    pub fn init_dpi() {
        unsafe {
            SetProcessDPIAware();
        }
    }

    extern "system" fn enum_cb(hwnd: Hwnd, lparam: isize) -> i32 {
        unsafe {
            if IsWindowVisible(hwnd) == 0 {
                return 1;
            }
            let len = GetWindowTextLengthW(hwnd);
            if len <= 0 {
                return 1;
            }
            let mut buf = vec![0u16; (len + 1) as usize];
            let got = GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32);
            if got <= 0 {
                return 1;
            }
            let title = String::from_utf16_lossy(&buf[..got as usize]);
            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, &raw mut pid);
            let mut rect = Rect::default();
            GetWindowRect(hwnd, &raw mut rect);
            let list = &mut *(lparam as *mut Vec<WinInfo>);
            list.push(WinInfo { hwnd, pid, rect, title });
        }
        1
    }

    pub fn list_windows() -> Vec<WinInfo> {
        let mut out = Vec::new();
        unsafe {
            EnumWindows(enum_cb, (&raw mut out) as isize);
        }
        out
    }

    pub fn find_window(needle: &str) -> Option<WinInfo> {
        let low = needle.to_lowercase();
        list_windows()
            .into_iter()
            .find(|w| w.title.to_lowercase().contains(&low))
    }

    pub fn focus_window(hwnd: Hwnd) {
        unsafe {
            if IsIconic(hwnd) != 0 {
                ShowWindow(hwnd, SW_RESTORE);
            }
            SetForegroundWindow(hwnd);
        }
    }

    pub fn move_mouse(x: i32, y: i32) {
        unsafe {
            SetCursorPos(x, y);
        }
    }

    pub fn mouse_click(x: i32, y: i32, right: bool, delay_ms: u64) {
        move_mouse(x, y);
        if delay_ms > 0 {
            std::thread::sleep(std::time::Duration::from_millis(delay_ms));
        }
        let (down, up) = if right {
            (MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP)
        } else {
            (MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP)
        };
        let inputs = [
            Input {
                r#type: INPUT_MOUSE,
                u: InputUnion {
                    mi: MouseInput {
                        dx: 0,
                        dy: 0,
                        mouse_data: 0,
                        dw_flags: down,
                        time: 0,
                        dw_extra_info: 0,
                    },
                },
            },
            Input {
                r#type: INPUT_MOUSE,
                u: InputUnion {
                    mi: MouseInput {
                        dx: 0,
                        dy: 0,
                        mouse_data: 0,
                        dw_flags: up,
                        time: 0,
                        dw_extra_info: 0,
                    },
                },
            },
        ];
        unsafe {
            SendInput(
                inputs.len() as u32,
                inputs.as_ptr(),
                std::mem::size_of::<Input>() as i32,
            );
        }
    }

    pub fn send_vk(vk: u16) {
        let inputs = [
            Input {
                r#type: INPUT_KEYBOARD,
                u: InputUnion {
                    ki: KeybdInput {
                        w_vk: vk,
                        w_scan: 0,
                        dw_flags: 0,
                        time: 0,
                        dw_extra_info: 0,
                    },
                },
            },
            Input {
                r#type: INPUT_KEYBOARD,
                u: InputUnion {
                    ki: KeybdInput {
                        w_vk: vk,
                        w_scan: 0,
                        dw_flags: KEYEVENTF_KEYUP,
                        time: 0,
                        dw_extra_info: 0,
                    },
                },
            },
        ];
        unsafe {
            SendInput(
                inputs.len() as u32,
                inputs.as_ptr(),
                std::mem::size_of::<Input>() as i32,
            );
        }
    }

    pub fn send_text(text: &str) {
        let mut inputs = Vec::new();
        for unit in text.encode_utf16() {
            inputs.push(Input {
                r#type: INPUT_KEYBOARD,
                u: InputUnion {
                    ki: KeybdInput {
                        w_vk: 0,
                        w_scan: unit,
                        dw_flags: KEYEVENTF_UNICODE,
                        time: 0,
                        dw_extra_info: 0,
                    },
                },
            });
            inputs.push(Input {
                r#type: INPUT_KEYBOARD,
                u: InputUnion {
                    ki: KeybdInput {
                        w_vk: 0,
                        w_scan: unit,
                        dw_flags: KEYEVENTF_UNICODE | KEYEVENTF_KEYUP,
                        time: 0,
                        dw_extra_info: 0,
                    },
                },
            });
        }
        if !inputs.is_empty() {
            unsafe {
                SendInput(
                    inputs.len() as u32,
                    inputs.as_ptr(),
                    std::mem::size_of::<Input>() as i32,
                );
            }
        }
    }

    pub fn capture_bmp(target: &str, out: &Path) -> Result<(i32, i32), String> {
        unsafe {
            let (hwnd, w, h, use_print) = if target.eq_ignore_ascii_case("screen") {
                (0isize, GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN), false)
            } else {
                let win = find_window(target).ok_or_else(|| format!("Окно «{target}» не найдено"))?;
                let w = (win.rect.right - win.rect.left).max(1);
                let h = (win.rect.bottom - win.rect.top).max(1);
                (win.hwnd, w, h, true)
            };
            let src_dc = GetDC(if use_print { hwnd } else { 0 });
            if src_dc.is_null() {
                return Err("GetDC вернул NULL".to_owned());
            }
            let mem_dc = CreateCompatibleDC(src_dc);
            let bmp = CreateCompatibleBitmap(src_dc, w, h);
            let old = SelectObject(mem_dc, bmp);
            let ok = if use_print {
                PrintWindow(hwnd, mem_dc, PW_RENDERFULLCONTENT) != 0
                    || BitBlt(mem_dc, 0, 0, w, h, src_dc, 0, 0, SRCCOPY) != 0
            } else {
                BitBlt(mem_dc, 0, 0, w, h, src_dc, 0, 0, SRCCOPY) != 0
            };
            let mut bi = BitmapInfo {
                bmi_header: BitmapInfoHeader {
                    bi_size: std::mem::size_of::<BitmapInfoHeader>() as u32,
                    bi_width: w,
                    bi_height: h,
                    bi_planes: 1,
                    bi_bit_count: 24,
                    bi_compression: 0,
                    bi_size_image: 0,
                    bi_x_pels_per_meter: 0,
                    bi_y_pels_per_meter: 0,
                    bi_clr_used: 0,
                    bi_clr_important: 0,
                },
                bmi_colors: [0],
            };
            let stride = ((w * 3 + 3) & !3) as usize;
            let mut pixels = vec![0u8; stride * h as usize];
            if ok {
                GetDIBits(
                    mem_dc,
                    bmp,
                    0,
                    h as u32,
                    pixels.as_mut_ptr().cast(),
                    &raw mut bi,
                    DIB_RGB_COLORS,
                );
            }
            SelectObject(mem_dc, old);
            DeleteObject(bmp);
            DeleteDC(mem_dc);
            ReleaseDC(if use_print { hwnd } else { 0 }, src_dc);
            if !ok {
                return Err("Не удалось снять изображение окна".to_owned());
            }
            let header = crate::bmp_header(w as u32, h as u32, pixels.len() as u32);
            let mut f = std::fs::File::create(out).map_err(|e| e.to_string())?;
            f.write_all(&header).map_err(|e| e.to_string())?;
            f.write_all(&pixels).map_err(|e| e.to_string())?;
            Ok((w, h))
        }
    }
}

/// Заголовок 24-битного BMP-файла (54 байта).
pub fn bmp_header(w: u32, h: u32, data_len: u32) -> [u8; 54] {
    let mut b = [0u8; 54];
    b[0] = b'B';
    b[1] = b'M';
    b[2..6].copy_from_slice(&(54 + data_len).to_le_bytes());
    b[10..14].copy_from_slice(&54u32.to_le_bytes());
    b[14..18].copy_from_slice(&40u32.to_le_bytes());
    b[18..22].copy_from_slice(&w.to_le_bytes());
    b[22..26].copy_from_slice(&h.to_le_bytes());
    b[26..28].copy_from_slice(&1u16.to_le_bytes());
    b[28..30].copy_from_slice(&24u16.to_le_bytes());
    b[34..38].copy_from_slice(&data_len.to_le_bytes());
    b
}

/// Перевод имени клавиши в виртуальный код Windows.
pub fn parse_key(name: &str) -> Option<u16> {
    match name.to_ascii_lowercase().as_str() {
        "esc" | "escape" => Some(0x1B),
        "enter" | "return" => Some(0x0D),
        "tab" => Some(0x09),
        "space" => Some(0x20),
        "backspace" | "bs" => Some(0x08),
        "up" => Some(0x26),
        "down" => Some(0x28),
        "left" => Some(0x25),
        "right" => Some(0x27),
        "f" => Some(0x46),
        _ => None,
    }
}

#[cfg(windows)]
fn main() {
    win32::init_dpi();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(cmd) = args.first().map(String::as_str) else {
        eprintln!("Использование: rg-click <list|focus|move|click|rclick|key|type|shot> ...");
        std::process::exit(2);
    };
    match cmd {
        "list" => {
            for w in win32::list_windows() {
                println!(
                    "{}\t{}\t{},{},{},{}\t{}",
                    w.hwnd,
                    w.pid,
                    w.rect.left,
                    w.rect.top,
                    w.rect.right - w.rect.left,
                    w.rect.bottom - w.rect.top,
                    w.title
                );
            }
        }
        "focus" => {
            let q = args.get(1).map(String::as_str).unwrap_or("");
            if let Some(w) = win32::find_window(q) {
                win32::focus_window(w.hwnd);
                println!(
                    "OK {} {},{},{}x{} {}",
                    w.hwnd,
                    w.rect.left,
                    w.rect.top,
                    w.rect.right - w.rect.left,
                    w.rect.bottom - w.rect.top,
                    w.title
                );
            } else {
                eprintln!("Окно не найдено: {q}");
                std::process::exit(1);
            }
        }
        "move" => {
            let (Some(x), Some(y)) = (
                args.get(1).and_then(|s| s.parse().ok()),
                args.get(2).and_then(|s| s.parse().ok()),
            ) else {
                eprintln!("rg-click move <x> <y>");
                std::process::exit(2);
            };
            win32::move_mouse(x, y);
        }
        "click" | "rclick" => {
            let (Some(x), Some(y)) = (
                args.get(1).and_then(|s| s.parse().ok()),
                args.get(2).and_then(|s| s.parse().ok()),
            ) else {
                eprintln!("rg-click {cmd} <x> <y> [delay_ms]");
                std::process::exit(2);
            };
            let delay = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(1000);
            win32::mouse_click(x, y, cmd == "rclick", delay);
        }
        "key" => {
            let Some(vk) = args.get(1).and_then(|k| parse_key(k)) else {
                eprintln!("Неизвестная клавиша");
                std::process::exit(2);
            };
            win32::send_vk(vk);
        }
        "type" => {
            let text = args[1..].join(" ");
            win32::send_text(&text);
        }
        "shot" => {
            let (Some(target), Some(out)) = (args.get(1), args.get(2)) else {
                eprintln!("rg-click shot <заголовок|screen> <out.bmp>");
                std::process::exit(2);
            };
            match win32::capture_bmp(target, std::path::Path::new(out)) {
                Ok((w, h)) => println!("OK {w}x{h} -> {out}"),
                Err(e) => {
                    eprintln!("Ошибка снимка: {e}");
                    std::process::exit(1);
                }
            }
        }
        _ => {
            eprintln!("Неизвестная команда: {cmd}");
            std::process::exit(2);
        }
    }
}

#[cfg(not(windows))]
fn main() {
    eprintln!("rg-click поддерживается только на Windows");
    std::process::exit(1);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_names_map_to_vk() {
        assert_eq!(parse_key("esc"), Some(0x1B));
        assert_eq!(parse_key("ENTER"), Some(0x0D));
        assert_eq!(parse_key("space"), Some(0x20));
        assert_eq!(parse_key("unknown"), None);
    }

    #[test]
    fn bmp_header_is_valid() {
        let h = bmp_header(100, 50, 15_000);
        assert_eq!(&h[0..2], b"BM");
        assert_eq!(u32::from_le_bytes(h[2..6].try_into().unwrap()), 15_054);
        assert_eq!(u32::from_le_bytes(h[18..22].try_into().unwrap()), 100);
        assert_eq!(u32::from_le_bytes(h[22..26].try_into().unwrap()), 50);
    }
}
