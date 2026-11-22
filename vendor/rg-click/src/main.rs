//! Быстрая Win32-утилита на Rust для автотестов и сверки интерфейса AyuGram и Rustgramm.
//! При запуске тестовых команд сворачивает все посторонние окна, оставляя только AyuGram и Rustgramm.
//!
//! Команды:
//!   rg-click prep                                      — свернуть все окна кроме AyuGram и Rustgramm
//!   rg-click side-by-side                              — свернуть лишнее, поставить AyuGram слева и Rustgramm справа
//!   rg-click list                                      — список видимых окон (HWND PID L,T,W,H TITLE)
//!   rg-click focus <окно>                              — свернуть лишнее и вывести окно на передний план
//!   rg-click move <x> <y>                              — навести курсор в экранные координаты
//!   rg-click click <x> <y> [delay_ms]                  — навести, подождать (по умолчанию 1000 мс) и кликнуть ЛКМ
//!   rg-click rclick <x> <y> [delay_ms]                 — клик ПКМ
//!   rg-click click-win <окно> <x|x%> <y|y%> [delay_ms] — клик в координатах относительно окна
//!   rg-click hover-win <окно> <x|x%> <y|y%> [delay_ms] [out.bmp] — наведение + снимок окна
//!   rg-click key <esc|enter|tab|space|f|bs>            — нажать клавишу
//!   rg-click type <текст>                              — ввести текст (Unicode)
//!   rg-click shot <окно|screen> <out.bmp>              — снимок окна или экрана
//!   rg-click crop <окно|screen> <x> <y> <w> <h> <out.bmp> — увеличенный фрагмент (zoom)
//!   rg-click frames <окно> <count> <interval_ms> <dir> — серия кадров анимации
//!   rg-click diff <a.bmp> <b.bmp> [diff.bmp]           — попиксельное сравнение двух снимков
//!   rg-click tour <dir>                                — обход интерфейса AyuGram и Rustgramm со снимками

use std::path::Path;

#[cfg(windows)]
mod win32 {
    use std::ffi::c_void;
    use std::path::Path;

    pub type Hwnd = isize;

    #[repr(C)]
    #[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
    pub struct Rect {
        pub left: i32,
        pub top: i32,
        pub right: i32,
        pub bottom: i32,
    }

    impl Rect {
        pub fn width(self) -> i32 {
            (self.right - self.left).max(0)
        }
        pub fn height(self) -> i32 {
            (self.bottom - self.top).max(0)
        }
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
    const SW_MINIMIZE: i32 = 6;
    const SW_RESTORE: i32 = 9;
    const SWP_SHOWWINDOW: u32 = 0x0040;
    const GWL_EXSTYLE: i32 = -20;
    const WS_EX_TOOLWINDOW: i32 = 0x0000_0080;
    const SRCCOPY: u32 = 0x00CC_0020;
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
        fn GetWindowLongW(hwnd: Hwnd, index: i32) -> i32;
        fn GetWindowTextLengthW(hwnd: Hwnd) -> i32;
        fn GetWindowTextW(hwnd: Hwnd, buf: *mut u16, max: i32) -> i32;
        fn GetClassNameW(hwnd: Hwnd, buf: *mut u16, max: i32) -> i32;
        fn GetWindowThreadProcessId(hwnd: Hwnd, pid: *mut u32) -> u32;
        fn GetWindowRect(hwnd: Hwnd, rect: *mut Rect) -> i32;
        fn ShowWindow(hwnd: Hwnd, cmd: i32) -> i32;
        fn SetWindowPos(hwnd: Hwnd, after: Hwnd, x: i32, y: i32, cx: i32, cy: i32, flags: u32) -> i32;
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

    #[derive(Clone, Debug)]
    pub struct WinInfo {
        pub hwnd: Hwnd,
        pub pid: u32,
        pub rect: Rect,
        pub title: String,
        pub class: String,
        pub iconic: bool,
    }

    pub fn init_dpi() {
        unsafe {
            SetProcessDPIAware();
        }
    }

    pub fn screen_size() -> (i32, i32) {
        unsafe { (GetSystemMetrics(SM_CXSCREEN).max(800), GetSystemMetrics(SM_CYSCREEN).max(600)) }
    }

    extern "system" fn enum_cb(hwnd: Hwnd, lparam: isize) -> i32 {
        unsafe {
            if IsWindowVisible(hwnd) == 0 {
                return 1;
            }
            let ex = GetWindowLongW(hwnd, GWL_EXSTYLE);
            if (ex & WS_EX_TOOLWINDOW) != 0 {
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
            let mut cbuf = [0u16; 128];
            let cgot = GetClassNameW(hwnd, cbuf.as_mut_ptr(), cbuf.len() as i32);
            let class = if cgot > 0 {
                String::from_utf16_lossy(&cbuf[..cgot as usize])
            } else {
                String::new()
            };
            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, &raw mut pid);
            let mut rect = Rect::default();
            GetWindowRect(hwnd, &raw mut rect);
            let iconic = IsIconic(hwnd) != 0;
            let list = &mut *(lparam as *mut Vec<WinInfo>);
            list.push(WinInfo { hwnd, pid, rect, title, class, iconic });
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
            .find(|w| w.title.to_lowercase().contains(&low) || w.class.to_lowercase().contains(&low))
    }

    /// Сворачивает все видимые окна, кроме AyuGram и Rustgramm, и разворачивает целевые.
    pub fn prep_workspace() -> (Option<WinInfo>, Option<WinInfo>, usize) {
        let mut minimized = 0usize;
        for w in list_windows() {
            if crate::is_shell_window(&w.title, &w.class) {
                continue;
            }
            if crate::is_target_window(&w.title) {
                unsafe {
                    if IsIconic(w.hwnd) != 0 {
                        ShowWindow(w.hwnd, SW_RESTORE);
                    }
                }
            } else if !w.iconic {
                unsafe {
                    ShowWindow(w.hwnd, SW_MINIMIZE);
                }
                minimized += 1;
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(120));
        let ayu = find_window("ayugram");
        let rg = find_window("rustgramm");
        (ayu, rg, minimized)
    }

    /// Ставит AyuGram на левую половину экрана, а Rustgramm — на правую (одинаковый размер 1:1).
    pub fn side_by_side() -> (Option<WinInfo>, Option<WinInfo>, usize) {
        let (_, _, minimized) = prep_workspace();
        let (sw, sh) = screen_size();
        let half_w = sw / 2;
        let usable_h = (sh - 48).max(500);
        if let Some(a) = find_window("ayugram") {
            unsafe {
                ShowWindow(a.hwnd, SW_RESTORE);
                SetWindowPos(a.hwnd, 0, 0, 0, half_w, usable_h, SWP_SHOWWINDOW);
            }
        }
        if let Some(r) = find_window("rustgramm") {
            unsafe {
                ShowWindow(r.hwnd, SW_RESTORE);
                SetWindowPos(r.hwnd, 0, half_w, 0, half_w, usable_h, SWP_SHOWWINDOW);
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(180));
        (find_window("ayugram"), find_window("rustgramm"), minimized)
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

    /// Снимает 24-битный BGR-буфер окна или экрана (снизу вверх, как в BMP).
    pub fn capture_raw(target: &str) -> Result<(u32, u32, Vec<u8>), String> {
        unsafe {
            let (hwnd, w, h, use_print) = if target.eq_ignore_ascii_case("screen") {
                let (sw, sh) = screen_size();
                (0isize, sw, sh, false)
            } else {
                let win = find_window(target).ok_or_else(|| format!("Окно «{target}» не найдено"))?;
                let w = win.rect.width().max(1);
                let h = win.rect.height().max(1);
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
            let stride = crate::bmp_stride(w as u32);
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
            Ok((w as u32, h as u32, pixels))
        }
    }

    pub fn capture_bmp(target: &str, out: &Path) -> Result<(u32, u32), String> {
        let (w, h, pixels) = capture_raw(target)?;
        crate::write_bmp(out, w, h, &pixels)?;
        Ok((w, h))
    }
}

/// Должно ли окно оставаться открытым во время тестов (только AyuGram и Rustgramm).
pub fn is_target_window(title: &str) -> bool {
    let low = title.to_lowercase();
    low.contains("ayugram") || low.contains("rustgramm")
}

/// Системные окна оболочки Windows, которые не нужно сворачивать.
pub fn is_shell_window(title: &str, class: &str) -> bool {
    let t = title.trim();
    matches!(
        class,
        "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd" | "DV2ControlHost"
    ) || t.eq_ignore_ascii_case("Program Manager")
}

/// Разбирает координату внутри окна: пиксели (`42`) или проценты (`50%`).
pub fn resolve_coord(spec: &str, span: i32) -> Option<i32> {
    let s = spec.trim();
    if let Some(pct) = s.strip_suffix('%') {
        let p: f32 = pct.trim().parse().ok()?;
        Some(((span as f32) * (p / 100.0)).round() as i32)
    } else {
        s.parse::<i32>().ok()
    }
}

pub fn bmp_stride(w: u32) -> usize {
    ((w as usize * 3) + 3) & !3
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

pub fn write_bmp(path: &Path, w: u32, h: u32, pixels: &[u8]) -> Result<(), String> {
    use std::io::Write;
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        let _ = std::fs::create_dir_all(parent);
    }
    let header = bmp_header(w, h, pixels.len() as u32);
    let mut f = std::fs::File::create(path).map_err(|e| e.to_string())?;
    f.write_all(&header).map_err(|e| e.to_string())?;
    f.write_all(pixels).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn read_bmp(path: &Path) -> Result<(u32, u32, Vec<u8>), String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    if bytes.len() < 54 || &bytes[0..2] != b"BM" {
        return Err("Некорректный BMP-файл".to_owned());
    }
    let offset = u32::from_le_bytes(bytes[10..14].try_into().unwrap()) as usize;
    let w = u32::from_le_bytes(bytes[18..22].try_into().unwrap());
    let h = u32::from_le_bytes(bytes[22..26].try_into().unwrap());
    let bpp = u16::from_le_bytes(bytes[28..30].try_into().unwrap());
    if bpp != 24 || offset > bytes.len() {
        return Err("Поддерживается только 24-битный BMP".to_owned());
    }
    Ok((w, h, bytes[offset..].to_vec()))
}

/// Вырезает прямоугольник `(cx, cy, cw, ch)` из 24-битного BMP-буфера (координаты сверху слева).
pub fn crop_pixels(w: u32, h: u32, pixels: &[u8], cx: u32, cy: u32, cw: u32, ch: u32) -> (u32, u32, Vec<u8>) {
    let cx = cx.min(w.saturating_sub(1));
    let cy = cy.min(h.saturating_sub(1));
    let cw = cw.min(w - cx).max(1);
    let ch = ch.min(h - cy).max(1);
    let src_stride = bmp_stride(w);
    let dst_stride = bmp_stride(cw);
    let mut out = vec![0u8; dst_stride * ch as usize];
    for row in 0..ch {
        // В BMP строки хранятся снизу вверх.
        let src_y = h - 1 - (cy + row);
        let dst_y = ch - 1 - row;
        let src_off = src_y as usize * src_stride + cx as usize * 3;
        let dst_off = dst_y as usize * dst_stride;
        let bytes = cw as usize * 3;
        if src_off + bytes <= pixels.len() {
            out[dst_off..dst_off + bytes].copy_from_slice(&pixels[src_off..src_off + bytes]);
        }
    }
    (cw, ch, out)
}

/// Сравнивает два 24-битных BMP-буфера: возвращает (процент отличающихся пикселей, средняя разница 0..255, тепловая карта).
pub fn diff_buffers(w1: u32, h1: u32, p1: &[u8], w2: u32, h2: u32, p2: &[u8]) -> (f32, f32, u32, u32, Vec<u8>) {
    let w = w1.min(w2).max(1);
    let h = h1.min(h2).max(1);
    let s1 = bmp_stride(w1);
    let s2 = bmp_stride(w2);
    let so = bmp_stride(w);
    let mut out = vec![0u8; so * h as usize];
    let mut diff_px = 0u64;
    let mut sum_err = 0u64;
    let total = u64::from(w) * u64::from(h);
    for y in 0..h {
        let r1 = (h1 - 1 - y) as usize * s1;
        let r2 = (h2 - 1 - y) as usize * s2;
        let ro = (h - 1 - y) as usize * so;
        for x in 0..w as usize {
            let i1 = r1 + x * 3;
            let i2 = r2 + x * 3;
            let io = ro + x * 3;
            let (b1, g1, r_1) = (p1[i1], p1[i1 + 1], p1[i1 + 2]);
            let (b2, g2, r_2) = (p2[i2], p2[i2 + 1], p2[i2 + 2]);
            let db = b1.abs_diff(b2);
            let dg = g1.abs_diff(g2);
            let dr = r_1.abs_diff(r_2);
            let err = (u16::from(db) + u16::from(dg) + u16::from(dr)) / 3;
            sum_err += u64::from(err);
            if err > 12 {
                diff_px += 1;
                // Подсвечиваем расхождения ярко-оранжевым поверх приглушённого оригинала.
                out[io] = 32;
                out[io + 1] = 130;
                out[io + 2] = 245;
            } else {
                out[io] = b1 / 2;
                out[io + 1] = g1 / 2;
                out[io + 2] = r_1 / 2;
            }
        }
    }
    let pct = (diff_px as f32 * 100.0) / (total.max(1) as f32);
    let mae = (sum_err as f32) / (total.max(1) as f32);
    (pct, mae, w, h, out)
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
        eprintln!(
            "Использование: rg-click <prep|side-by-side|list|focus|move|click|rclick|click-win|hover-win|key|type|shot|crop|frames|diff|tour> ..."
        );
        std::process::exit(2);
    };
    match cmd {
        "prep" => {
            let (ayu, rg, min) = win32::prep_workspace();
            println!(
                "PREP minimized={min} ayu={} rustgramm={}",
                ayu.as_ref().map_or("нет".to_owned(), |w| format!("{},{} {}x{}", w.rect.left, w.rect.top, w.rect.width(), w.rect.height())),
                rg.as_ref().map_or("нет".to_owned(), |w| format!("{},{} {}x{}", w.rect.left, w.rect.top, w.rect.width(), w.rect.height())),
            );
        }
        "side-by-side" => {
            let (ayu, rg, min) = win32::side_by_side();
            println!(
                "SIDE_BY_SIDE minimized={min} ayu={} rustgramm={}",
                ayu.as_ref().map_or("нет".to_owned(), |w| format!("{},{} {}x{}", w.rect.left, w.rect.top, w.rect.width(), w.rect.height())),
                rg.as_ref().map_or("нет".to_owned(), |w| format!("{},{} {}x{}", w.rect.left, w.rect.top, w.rect.width(), w.rect.height())),
            );
        }
        "list" => {
            for w in win32::list_windows() {
                println!(
                    "{}\t{}\t{},{},{},{}\t{}",
                    w.hwnd,
                    w.pid,
                    w.rect.left,
                    w.rect.top,
                    w.rect.width(),
                    w.rect.height(),
                    w.title
                );
            }
        }
        "focus" => {
            win32::prep_workspace();
            let q = args.get(1).map(String::as_str).unwrap_or("");
            if let Some(w) = win32::find_window(q) {
                win32::focus_window(w.hwnd);
                println!(
                    "OK {} {},{},{}x{} {}",
                    w.hwnd,
                    w.rect.left,
                    w.rect.top,
                    w.rect.width(),
                    w.rect.height(),
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
            win32::prep_workspace();
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
        "click-win" | "hover-win" => {
            win32::prep_workspace();
            let (Some(target), Some(xs), Some(ys)) = (args.get(1), args.get(2), args.get(3)) else {
                eprintln!("rg-click {cmd} <окно> <x|x%> <y|y%> [delay_ms] [out.bmp]");
                std::process::exit(2);
            };
            let Some(w) = win32::find_window(target) else {
                eprintln!("Окно не найдено: {target}");
                std::process::exit(1);
            };
            win32::focus_window(w.hwnd);
            std::thread::sleep(std::time::Duration::from_millis(120));
            let (Some(rx), Some(ry)) = (resolve_coord(xs, w.rect.width()), resolve_coord(ys, w.rect.height())) else {
                eprintln!("Неверные координаты: {xs} {ys}");
                std::process::exit(2);
            };
            let delay = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(1000);
            let sx = w.rect.left + rx;
            let sy = w.rect.top + ry;
            if cmd == "click-win" {
                win32::mouse_click(sx, sy, false, delay);
            } else {
                win32::move_mouse(sx, sy);
                std::thread::sleep(std::time::Duration::from_millis(delay));
                if let Some(out) = args.get(5)
                    && let Err(e) = win32::capture_bmp(target, Path::new(out))
                {
                    eprintln!("Ошибка снимка: {e}");
                    std::process::exit(1);
                }
            }
            println!("OK {cmd} ({sx},{sy})");
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
            win32::prep_workspace();
            let (Some(target), Some(out)) = (args.get(1), args.get(2)) else {
                eprintln!("rg-click shot <заголовок|screen> <out.bmp>");
                std::process::exit(2);
            };
            match win32::capture_bmp(target, Path::new(out)) {
                Ok((w, h)) => println!("OK {w}x{h} -> {out}"),
                Err(e) => {
                    eprintln!("Ошибка снимка: {e}");
                    std::process::exit(1);
                }
            }
        }
        "crop" => {
            win32::prep_workspace();
            let (Some(target), Some(cx), Some(cy), Some(cw), Some(ch), Some(out)) = (
                args.get(1),
                args.get(2).and_then(|s| s.parse().ok()),
                args.get(3).and_then(|s| s.parse().ok()),
                args.get(4).and_then(|s| s.parse().ok()),
                args.get(5).and_then(|s| s.parse().ok()),
                args.get(6),
            ) else {
                eprintln!("rg-click crop <окно|screen> <x> <y> <w> <h> <out.bmp>");
                std::process::exit(2);
            };
            match win32::capture_raw(target) {
                Ok((w, h, px)) => {
                    let (ow, oh, cropped) = crop_pixels(w, h, &px, cx, cy, cw, ch);
                    if let Err(e) = write_bmp(Path::new(out), ow, oh, &cropped) {
                        eprintln!("Ошибка записи: {e}");
                        std::process::exit(1);
                    }
                    println!("OK crop {ow}x{oh} -> {out}");
                }
                Err(e) => {
                    eprintln!("Ошибка снимка: {e}");
                    std::process::exit(1);
                }
            }
        }
        "frames" => {
            win32::prep_workspace();
            let (Some(target), Some(count), Some(interval), Some(dir)) = (
                args.get(1),
                args.get(2).and_then(|s| s.parse::<usize>().ok()),
                args.get(3).and_then(|s| s.parse::<u64>().ok()),
                args.get(4),
            ) else {
                eprintln!("rg-click frames <окно> <count> <interval_ms> <out_dir>");
                std::process::exit(2);
            };
            let dir = Path::new(dir);
            let _ = std::fs::create_dir_all(dir);
            for i in 0..count.min(60) {
                let p = dir.join(format!("frame_{i:02}.bmp"));
                let _ = win32::capture_bmp(target, &p);
                if interval > 0 {
                    std::thread::sleep(std::time::Duration::from_millis(interval));
                }
            }
            println!("OK frames {} -> {}", count.min(60), dir.display());
        }
        "diff" => {
            let (Some(a), Some(b)) = (args.get(1), args.get(2)) else {
                eprintln!("rg-click diff <a.bmp> <b.bmp> [diff.bmp]");
                std::process::exit(2);
            };
            let ((w1, h1, p1), (w2, h2, p2)) = match (read_bmp(Path::new(a)), read_bmp(Path::new(b))) {
                (Ok(x), Ok(y)) => (x, y),
                (Err(e), _) | (_, Err(e)) => {
                    eprintln!("Ошибка чтения BMP: {e}");
                    std::process::exit(1);
                }
            };
            let (pct, mae, dw, dh, out) = diff_buffers(w1, h1, &p1, w2, h2, &p2);
            if let Some(dst) = args.get(3) {
                let _ = write_bmp(Path::new(dst), dw, dh, &out);
            }
            println!("DIFF pct={pct:.2}% mae={mae:.2} size={dw}x{dh}");
        }
        "tour" => {
            let out_dir = Path::new(args.get(1).map(String::as_str).unwrap_or("dist/ui_tour"));
            let _ = std::fs::create_dir_all(out_dir);
            // Если Rustgramm не запущен, но есть dist/Rustgramm.exe — запускаем один экземпляр.
            if win32::find_window("rustgramm").is_none() && Path::new("dist/Rustgramm.exe").exists() {
                let _ = std::process::Command::new("dist/Rustgramm.exe").spawn();
                std::thread::sleep(std::time::Duration::from_millis(2000));
            }
            let (ayu, rg, min) = win32::side_by_side();
            let _ = win32::capture_bmp("screen", &out_dir.join("00_side_by_side.bmp"));
            for (tag, win_opt) in [("ayu", ayu), ("rg", rg)] {
                let Some(w) = win_opt else { continue };
                win32::focus_window(w.hwnd);
                std::thread::sleep(std::time::Duration::from_millis(250));
                let _ = win32::capture_bmp(&w.title, &out_dir.join(format!("01_main_{tag}.bmp")));
                // 1) Открыть левое меню (гамбургер).
                win32::mouse_click(w.rect.left + 28, w.rect.top + 52, false, 900);
                std::thread::sleep(std::time::Duration::from_millis(350));
                let _ = win32::capture_bmp(&w.title, &out_dir.join(format!("02_drawer_{tag}.bmp")));
                // 2) Закрыть шторку по Esc.
                win32::send_vk(0x1B);
                std::thread::sleep(std::time::Duration::from_millis(300));
                // 3) Снимок шапки и поиска крупным планом.
                if let Ok((rw, rh, px)) = win32::capture_raw(&w.title) {
                    let (cw, ch, cr) = crop_pixels(rw, rh, &px, 0, 0, 380, 120);
                    let _ = write_bmp(&out_dir.join(format!("03_header_crop_{tag}.bmp")), cw, ch, &cr);
                }
            }
            println!("TOUR_OK minimized={min} dir={}", out_dir.display());
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
    fn target_and_shell_window_filters() {
        assert!(is_target_window("AyuGram Desktop"));
        assert!(is_target_window("Rustgramm"));
        assert!(!is_target_window("Google Chrome"));
        assert!(is_shell_window("Program Manager", "Progman"));
        assert!(is_shell_window("", "Shell_TrayWnd"));
        assert!(!is_shell_window("Блокнот", "Notepad"));
    }

    #[test]
    fn resolves_pixel_and_percent_coords() {
        assert_eq!(resolve_coord("40", 800), Some(40));
        assert_eq!(resolve_coord("50%", 800), Some(400));
        assert_eq!(resolve_coord("25.5%", 1000), Some(255));
        assert_eq!(resolve_coord("abc", 800), None);
    }

    #[test]
    fn bmp_roundtrip_crop_and_diff() {
        let dir = std::env::temp_dir().join("rg-click-test");
        let _ = std::fs::create_dir_all(&dir);
        let p1 = dir.join("a.bmp");
        let p2 = dir.join("b.bmp");
        let w = 20u32;
        let h = 10u32;
        let stride = bmp_stride(w);
        let buf1 = vec![40u8; stride * h as usize];
        let mut buf2 = buf1.clone();
        // Меняем один пиксель в верхнем левом углу.
        let top_row = (h - 1) as usize * stride;
        buf2[top_row] = 250;
        buf2[top_row + 1] = 250;
        buf2[top_row + 2] = 250;
        write_bmp(&p1, w, h, &buf1).unwrap();
        write_bmp(&p2, w, h, &buf2).unwrap();
        let (rw, rh, loaded1) = read_bmp(&p1).unwrap();
        assert_eq!((rw, rh), (w, h));
        let (cw, ch, cropped) = crop_pixels(w, h, &loaded1, 2, 2, 8, 5);
        assert_eq!((cw, ch), (8, 5));
        assert_eq!(cropped.len(), bmp_stride(8) * 5);
        let (pct, mae, dw, dh, _) = diff_buffers(w, h, &buf1, w, h, &buf2);
        assert_eq!((dw, dh), (w, h));
        assert!(pct > 0.0 && pct < 2.0, "pct={pct}");
        assert!(mae > 0.0);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
