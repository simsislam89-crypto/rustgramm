//! Снимок сообщений (`ayu/features/message_shot/message_shot.cpp` + `message_shot_box.cpp`):
//! - поддержка одного или нескольких выбранных сообщений,
//! - выбор фоновой темы (Тёмная, OLED, Ночной Telegram, Оранжевый закат, Прозрачная),
//! - настройки: фон, дата/время, реакции, украшения, цветные ответы, раскрытие спойлеров,
//! - чёткий рендер пузырей, заголовка, ответа, текста, реакций в RGBA PNG и копирование в буфер.

use rg_core::engine::MessageRow;
use rg_settings::AyuSettings;

use super::theme::hhmm;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ShotTheme {
    Night,
    Oled,
    TelegramBlue,
    RustgrammSunset,
    Transparent,
}

impl ShotTheme {
    pub const ALL: [Self; 5] = [
        Self::Night,
        Self::Oled,
        Self::TelegramBlue,
        Self::RustgrammSunset,
        Self::Transparent,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Self::Night => "Тёмная",
            Self::Oled => "OLED",
            Self::TelegramBlue => "Ночной Telegram",
            Self::RustgrammSunset => "Оранжевый закат",
            Self::Transparent => "Прозрачный фон",
        }
    }

    pub fn colors(self) -> ShotPalette {
        match self {
            Self::Night => ShotPalette {
                bg_top: [26, 28, 36],
                bg_bot: [18, 20, 26],
                bubble_in: [36, 40, 52],
                bubble_out: [62, 46, 36],
                reply_bar: [245, 130, 32],
                author: [245, 148, 64],
                text: [242, 244, 248],
                subtext: [152, 158, 174],
            },
            Self::Oled => ShotPalette {
                bg_top: [6, 6, 8],
                bg_bot: [0, 0, 0],
                bubble_in: [24, 24, 28],
                bubble_out: [52, 34, 20],
                reply_bar: [245, 130, 32],
                author: [245, 152, 66],
                text: [245, 245, 248],
                subtext: [142, 144, 156],
            },
            Self::TelegramBlue => ShotPalette {
                bg_top: [22, 33, 52],
                bg_bot: [14, 22, 38],
                bubble_in: [28, 40, 58],
                bubble_out: [43, 82, 120],
                reply_bar: [100, 181, 246],
                author: [100, 181, 246],
                text: [242, 246, 252],
                subtext: [150, 170, 194],
            },
            Self::RustgrammSunset => ShotPalette {
                bg_top: [44, 26, 22],
                bg_bot: [24, 16, 24],
                bubble_in: [38, 30, 36],
                bubble_out: [82, 44, 24],
                reply_bar: [245, 130, 32],
                author: [255, 162, 76],
                text: [248, 242, 238],
                subtext: [186, 154, 140],
            },
            Self::Transparent => ShotPalette {
                bg_top: [0, 0, 0],
                bg_bot: [0, 0, 0],
                bubble_in: [30, 32, 40],
                bubble_out: [58, 42, 30],
                reply_bar: [245, 130, 32],
                author: [245, 148, 64],
                text: [242, 244, 248],
                subtext: [152, 158, 174],
            },
        }
    }
}

#[derive(Clone, Copy)]
pub struct ShotPalette {
    pub bg_top: [u8; 3],
    pub bg_bot: [u8; 3],
    pub bubble_in: [u8; 3],
    pub bubble_out: [u8; 3],
    pub reply_bar: [u8; 3],
    pub author: [u8; 3],
    pub text: [u8; 3],
    pub subtext: [u8; 3],
}

pub struct ShotInput<'a> {
    pub chat_title: &'a str,
    pub messages: &'a [&'a MessageRow],
    pub ayu: &'a AyuSettings,
    pub theme: ShotTheme,
}

/// Строит готовый RGBA PNG-кадр снимка выбранных сообщений.
pub fn render_message_shot(input: &ShotInput<'_>) -> (u32, u32, Vec<u8>) {
    let pal = input.theme.colors();
    let pad = 28i32;
    let width = 560u32;
    let bubble_max_w = (width as i32 - pad * 2 - 44).max(260);
    let show_bg = input.ayu.show_background && input.theme != ShotTheme::Transparent;
    let show_header = input.ayu.show_header_decorations;

    struct LaidOut<'a> {
        msg: &'a MessageRow,
        sender: String,
        body: String,
        reply: Option<String>,
        meta: String,
        reactions: String,
        bw: i32,
        bh: i32,
        text_h: i32,
        reply_h: i32,
        react_h: i32,
    }

    let mut items = Vec::with_capacity(input.messages.len());
    let mut y_cursor = pad;
    if show_header && !input.chat_title.is_empty() {
        y_cursor += 32;
    }

    for msg in input.messages {
        let sender = if msg.outgoing {
            "Вы".to_owned()
        } else if !msg.sender.is_empty() {
            msg.sender.clone()
        } else {
            input.chat_title.to_owned()
        };
        let mut body = msg.text.clone();
        if let Some(m) = &msg.media {
            let mlabel = if m.name.is_empty() { "Медиа" } else { m.name.as_str() };
            if body.is_empty() {
                body = format!("[{mlabel}]");
            } else {
                body = format!("[{mlabel}]\n{body}");
            }
        }
        if !input.ayu.reveal_spoilers && !msg.entities.is_empty() {
            body = mask_spoilers(&body, msg);
        }
        if body.is_empty() {
            body = "Сообщение".to_owned();
        }
        let reply = msg.reply_to.as_ref().map(|(_, r)| {
            let clean: String = r.chars().take(68).collect();
            if r.chars().count() > 68 { format!("{clean}…") } else { clean }
        });
        let text_w_max = bubble_max_w - 28;
        let (_tw, text_h) = text_measure(&body, 15, text_w_max);
        let reply_h = if reply.is_some() { 26 } else { 0 };
        let reactions = if input.ayu.show_reactions && msg.reactions > 0 {
            format!("❤ {}", msg.reactions)
        } else {
            String::new()
        };
        let react_h = if reactions.is_empty() { 0 } else { 24 };
        let meta = if input.ayu.show_date {
            format!("{}  {}", hhmm(msg.date), if msg.edits > 0 { "изм." } else { "" })
        } else if msg.edit_date.is_some() {
            "изменено".to_owned()
        } else {
            String::new()
        };
        let sender_h = if !msg.outgoing { 22 } else { 0 };
        let meta_h = if meta.is_empty() { 8 } else { 20 };
        let bw = (_tw.max(150) + 32).clamp(170, bubble_max_w);
        let bh = 12 + sender_h + reply_h + text_h.max(20) + react_h + meta_h;
        y_cursor += bh + 12;
        items.push(LaidOut {
            msg,
            sender,
            body,
            reply,
            meta,
            reactions,
            bw,
            bh,
            text_h: text_h.max(20),
            reply_h,
            react_h,
        });
    }

    let total_h = (y_cursor + pad).clamp(120, 2400) as u32;
    let w = width as usize;
    let h = total_h as usize;
    let mut canvas = ShotCanvas::new(w, h);

    if show_bg {
        canvas.fill_gradient(pal.bg_top, pal.bg_bot);
    }

    let mut cy = pad;
    if show_header && !input.chat_title.is_empty() {
        canvas.draw_text(pad, cy, w as i32 - pad * 2, 24, input.chat_title, 14, true, pal.subtext);
        cy += 32;
    }

    for it in &items {
        let bx = if it.msg.outgoing {
            width as i32 - pad - it.bw
        } else {
            pad
        };
        let bcol = if it.msg.outgoing { pal.bubble_out } else { pal.bubble_in };
        let radius = input.ayu.bubble_radius().clamp(4, 18) as i32;
        canvas.fill_round_rect(bx, cy, it.bw, it.bh, radius, bcol);

        let mut iy = cy + 10;
        let ix = bx + 14;
        let iw = it.bw - 28;
        if !it.msg.outgoing {
            canvas.draw_text(ix, iy, iw, 20, &it.sender, 13, true, pal.author);
            iy += 22;
        }
        if let Some(rep) = &it.reply {
            let bar_col = if input.ayu.show_colorful_replies { pal.reply_bar } else { pal.subtext };
            canvas.fill_round_rect(ix, iy, 3, it.reply_h - 4, 2, bar_col);
            canvas.draw_text(ix + 10, iy + 2, (iw - 10).max(40), it.reply_h - 4, rep, 12, false, pal.subtext);
            iy += it.reply_h;
        }
        canvas.draw_text(ix, iy, iw, it.text_h + 6, &it.body, 15, false, pal.text);
        iy += it.text_h + 4;
        if !it.reactions.is_empty() {
            canvas.draw_text(ix, iy, iw, 20, &it.reactions, 12, false, pal.author);
            iy += it.react_h;
        }
        if !it.meta.is_empty() {
            canvas.draw_text_right(bx, iy, it.bw - 12, 16, &it.meta, 11, pal.subtext);
        }
        cy += it.bh + 12;
    }

    (width, total_h, canvas.into_rgba())
}

/// Копирует готовый снимок в системный буфер обмена (на ПК).
#[cfg(not(target_os = "android"))]
pub fn copy_shot_to_clipboard(width: u32, height: u32, rgba: &[u8]) -> Result<(), String> {
    let mut cb = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    cb.set_image(arboard::ImageData {
        width: width as usize,
        height: height as usize,
        bytes: std::borrow::Cow::Borrowed(rgba),
    })
    .map_err(|e| e.to_string())
}

#[cfg(target_os = "android")]
pub fn copy_shot_to_clipboard(_width: u32, _height: u32, _rgba: &[u8]) -> Result<(), String> {
    Err("Буфер изображений поддерживается только на ПК".to_owned())
}

fn mask_spoilers(text: &str, msg: &MessageRow) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut masked = vec![false; chars.len()];
    for e in &msg.entities {
        if e.kind == rg_core::engine::EntityKind::Spoiler {
            for flag in masked.iter_mut().take(e.end.min(chars.len())).skip(e.start) {
                *flag = true;
            }
        }
    }
    chars
        .into_iter()
        .enumerate()
        .map(|(i, c)| if masked[i] && !c.is_whitespace() { '░' } else { c })
        .collect()
}

fn text_measure(text: &str, size: i32, max_w: i32) -> (i32, i32) {
    #[cfg(windows)]
    {
        win_gdi::measure_text(text, size, max_w)
    }
    #[cfg(not(windows))]
    {
        let approx_cw = (size as f32 * 0.56).round() as i32;
        let line_h = size + 5;
        let mut lines = 0i32;
        let mut max_line_w = 0i32;
        let per_line = (max_w / approx_cw.max(6)).max(8);
        for raw_line in text.split('\n') {
            let cnt = raw_line.chars().count() as i32;
            let wrapped = (cnt + per_line - 1) / per_line;
            lines += wrapped.max(1);
            max_line_w = max_line_w.max((cnt.min(per_line)) * approx_cw);
        }
        (max_line_w.min(max_w), lines * line_h)
    }
}

struct ShotCanvas {
    w: usize,
    h: usize,
    #[cfg(windows)]
    gdi: Option<win_gdi::DIBCanvas>,
    #[cfg(not(windows))]
    rgba: Vec<u8>,
}

impl ShotCanvas {
    fn new(w: usize, h: usize) -> Self {
        #[cfg(windows)]
        {
            Self { w, h, gdi: win_gdi::DIBCanvas::new(w, h) }
        }
        #[cfg(not(windows))]
        {
            Self { w, h, rgba: vec![0u8; w * h * 4] }
        }
    }

    fn fill_gradient(&mut self, top: [u8; 3], bot: [u8; 3]) {
        #[cfg(windows)]
        if let Some(c) = &mut self.gdi {
            c.fill_gradient(top, bot);
        }
        #[cfg(not(windows))]
        for y in 0..self.h {
            let t = y as f32 / (self.h.saturating_sub(1).max(1)) as f32;
            let r = (f32::from(top[0]) * (1.0 - t) + f32::from(bot[0]) * t) as u8;
            let g = (f32::from(top[1]) * (1.0 - t) + f32::from(bot[1]) * t) as u8;
            let b = (f32::from(top[2]) * (1.0 - t) + f32::from(bot[2]) * t) as u8;
            for x in 0..self.w {
                let off = (y * self.w + x) * 4;
                self.rgba[off] = r;
                self.rgba[off + 1] = g;
                self.rgba[off + 2] = b;
                self.rgba[off + 3] = 255;
            }
        }
    }

    fn fill_round_rect(&mut self, x: i32, y: i32, w: i32, h: i32, r: i32, col: [u8; 3]) {
        #[cfg(windows)]
        if let Some(c) = &mut self.gdi {
            c.fill_round_rect(x, y, w, h, r, col);
        }
        #[cfg(not(windows))]
        {
            let r_f = r as f32;
            let x0 = x.max(0);
            let y0 = y.max(0);
            let x1 = (x + w).min(self.w as i32);
            let y1 = (y + h).min(self.h as i32);
            for py in y0..y1 {
                for px in x0..x1 {
                    let lx = (px - x) as f32;
                    let ly = (py - y) as f32;
                    let cx = lx.clamp(r_f, w as f32 - r_f);
                    let cy = ly.clamp(r_f, h as f32 - r_f);
                    let d = (lx - cx).hypot(ly - cy);
                    if d <= r_f + 0.5 {
                        let off = (py as usize * self.w + px as usize) * 4;
                        self.rgba[off] = col[0];
                        self.rgba[off + 1] = col[1];
                        self.rgba[off + 2] = col[2];
                        self.rgba[off + 3] = 255;
                    }
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_text(&mut self, x: i32, y: i32, w: i32, h: i32, text: &str, size: i32, bold: bool, col: [u8; 3]) {
        #[cfg(windows)]
        if let Some(c) = &mut self.gdi {
            c.draw_text(x, y, w, h, text, size, bold, false, col);
        }
        #[cfg(not(windows))]
        let _ = (x, y, w, h, text, size, bold, col);
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_text_right(&mut self, x: i32, y: i32, w: i32, h: i32, text: &str, size: i32, col: [u8; 3]) {
        #[cfg(windows)]
        if let Some(c) = &mut self.gdi {
            c.draw_text(x, y, w, h, text, size, false, true, col);
        }
        #[cfg(not(windows))]
        let _ = (x, y, w, h, text, size, col);
    }

    fn into_rgba(self) -> Vec<u8> {
        #[cfg(windows)]
        {
            if let Some(c) = self.gdi {
                return c.into_rgba();
            }
            vec![0u8; self.w * self.h * 4]
        }
        #[cfg(not(windows))]
        {
            self.rgba
        }
    }
}

#[cfg(windows)]
mod win_gdi {
    use std::ffi::c_void;

    #[repr(C)]
    struct BitmapInfoHeader {
        bi_size: u32,
        bi_width: i32,
        bi_height: i32,
        bi_planes: u16,
        bi_bit_count: u16,
        bi_compression: u32,
        bi_size_image: u32,
        bi_x_pels: i32,
        bi_y_pels: i32,
        bi_clr_used: u32,
        bi_clr_important: u32,
    }

    #[repr(C)]
    struct Rect {
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
    }

    #[link(name = "gdi32")]
    unsafe extern "system" {
        fn CreateCompatibleDC(hdc: *mut c_void) -> *mut c_void;
        fn DeleteDC(hdc: *mut c_void) -> i32;
        fn CreateDIBSection(
            hdc: *mut c_void,
            pbmi: *const BitmapInfoHeader,
            usage: u32,
            ppv_bits: *mut *mut c_void,
            h_section: *mut c_void,
            offset: u32,
        ) -> *mut c_void;
        fn SelectObject(hdc: *mut c_void, h: *mut c_void) -> *mut c_void;
        fn DeleteObject(ho: *mut c_void) -> i32;
        fn CreateFontW(
            c_height: i32,
            c_width: i32,
            c_escapement: i32,
            c_orientation: i32,
            c_weight: i32,
            b_italic: u32,
            b_underline: u32,
            b_strike_out: u32,
            i_char_set: u32,
            i_out_precision: u32,
            i_clip_precision: u32,
            i_quality: u32,
            i_pitch_and_family: u32,
            psz_face_name: *const u16,
        ) -> *mut c_void;
        fn SetBkMode(hdc: *mut c_void, mode: i32) -> i32;
        fn SetTextColor(hdc: *mut c_void, color: u32) -> u32;
        fn GdiFlush() -> i32;
    }

    #[link(name = "user32")]
    unsafe extern "system" {
        fn DrawTextW(hdc: *mut c_void, lpch_text: *const u16, cch_text: i32, lprc: *mut Rect, format: u32) -> i32;
    }

    const TRANSPARENT: i32 = 1;
    const DT_WORDBREAK: u32 = 0x0010;
    const DT_CALCRECT: u32 = 0x0400;
    const DT_NOPREFIX: u32 = 0x0800;
    const DT_RIGHT: u32 = 0x0002;
    const ANTIALIASED_QUALITY: u32 = 4;

    fn wide_no_nul(s: &str) -> Vec<u16> {
        s.encode_utf16().collect()
    }

    fn font_handle(size: i32, bold: bool) -> *mut c_void {
        let face: Vec<u16> = "Segoe UI\0".encode_utf16().collect();
        let weight = if bold { 700 } else { 400 };
        // SAFETY: константная UTF-16 строка названия шрифта.
        unsafe {
            CreateFontW(
                -(size + 3),
                0,
                0,
                0,
                weight,
                0,
                0,
                0,
                1,
                0,
                0,
                ANTIALIASED_QUALITY,
                0,
                face.as_ptr(),
            )
        }
    }

    pub(super) fn measure_text(text: &str, size: i32, max_w: i32) -> (i32, i32) {
        let wt = wide_no_nul(text);
        if wt.is_empty() {
            return (0, size + 6);
        }
        // SAFETY: локальный memory DC освобождается сразу после DrawTextW(DT_CALCRECT).
        unsafe {
            let hdc = CreateCompatibleDC(std::ptr::null_mut());
            let font = font_handle(size, false);
            let old = SelectObject(hdc, font);
            let mut rc = Rect { left: 0, top: 0, right: max_w.max(60), bottom: 0 };
            DrawTextW(
                hdc,
                wt.as_ptr(),
                wt.len() as i32,
                &raw mut rc,
                DT_WORDBREAK | DT_CALCRECT | DT_NOPREFIX,
            );
            SelectObject(hdc, old);
            DeleteObject(font);
            DeleteDC(hdc);
            ((rc.right - rc.left).max(16), (rc.bottom - rc.top).max(size + 6))
        }
    }

    pub(super) struct DIBCanvas {
        w: usize,
        h: usize,
        hdc: *mut c_void,
        hbmp: *mut c_void,
        old_bmp: *mut c_void,
        bits: *mut u8,
    }

    impl DIBCanvas {
        pub(super) fn new(w: usize, h: usize) -> Option<Self> {
            let bmi = BitmapInfoHeader {
                bi_size: std::mem::size_of::<BitmapInfoHeader>() as u32,
                bi_width: w as i32,
                bi_height: -(h as i32), // top-down DIB
                bi_planes: 1,
                bi_bit_count: 32,
                bi_compression: 0,
                bi_size_image: 0,
                bi_x_pels: 0,
                bi_y_pels: 0,
                bi_clr_used: 0,
                bi_clr_important: 0,
            };
            let mut bits: *mut c_void = std::ptr::null_mut();
            // SAFETY: создаётся совместимый DC и top-down 32-bpp DIB.
            unsafe {
                let hdc = CreateCompatibleDC(std::ptr::null_mut());
                if hdc.is_null() {
                    return None;
                }
                let hbmp = CreateDIBSection(hdc, &raw const bmi, 0, &raw mut bits, std::ptr::null_mut(), 0);
                if hbmp.is_null() || bits.is_null() {
                    DeleteDC(hdc);
                    return None;
                }
                let old_bmp = SelectObject(hdc, hbmp);
                let byte_len = w * h * 4;
                std::ptr::write_bytes(bits.cast::<u8>(), 0, byte_len);
                Some(Self { w, h, hdc, hbmp, old_bmp, bits: bits.cast() })
            }
        }

        pub(super) fn fill_gradient(&mut self, top: [u8; 3], bot: [u8; 3]) {
            // SAFETY: буфер принадлежит этому DIBSection размером w * h * 4.
            unsafe {
                GdiFlush();
                let buf = std::slice::from_raw_parts_mut(self.bits, self.w * self.h * 4);
                for y in 0..self.h {
                    let t = y as f32 / (self.h.saturating_sub(1).max(1)) as f32;
                    let r = (f32::from(top[0]) * (1.0 - t) + f32::from(bot[0]) * t) as u8;
                    let g = (f32::from(top[1]) * (1.0 - t) + f32::from(bot[1]) * t) as u8;
                    let b = (f32::from(top[2]) * (1.0 - t) + f32::from(bot[2]) * t) as u8;
                    let row = &mut buf[y * self.w * 4..(y + 1) * self.w * 4];
                    for px in row.as_chunks_mut::<4>().0 {
                        px[0] = b;
                        px[1] = g;
                        px[2] = r;
                        px[3] = 255;
                    }
                }
            }
        }

        pub(super) fn fill_round_rect(&mut self, x: i32, y: i32, w: i32, h: i32, r: i32, col: [u8; 3]) {
            // SAFETY: запись в пиксели сглаженного прямоугольника с сохранением альфы GDI.
            unsafe {
                GdiFlush();
                let buf = std::slice::from_raw_parts_mut(self.bits, self.w * self.h * 4);
                let r_f = (r as f32).min(w as f32 * 0.5).min(h as f32 * 0.5);
                let x0 = x.max(0);
                let y0 = y.max(0);
                let x1 = (x + w).min(self.w as i32);
                let y1 = (y + h).min(self.h as i32);
                for py in y0..y1 {
                    for px in x0..x1 {
                        let lx = (px - x) as f32 + 0.5;
                        let ly = (py - y) as f32 + 0.5;
                        let cx = lx.clamp(r_f, w as f32 - r_f);
                        let cy = ly.clamp(r_f, h as f32 - r_f);
                        let cov = (r_f - (lx - cx).hypot(ly - cy) + 0.6).clamp(0.0, 1.0);
                        if cov <= 0.0 {
                            continue;
                        }
                        let off = (py as usize * self.w + px as usize) * 4;
                        let a_old = f32::from(buf[off + 3]) / 255.0;
                        let inv = 1.0 - cov;
                        buf[off] = (f32::from(buf[off]) * inv + f32::from(col[2]) * cov) as u8;
                        buf[off + 1] = (f32::from(buf[off + 1]) * inv + f32::from(col[1]) * cov) as u8;
                        buf[off + 2] = (f32::from(buf[off + 2]) * inv + f32::from(col[0]) * cov) as u8;
                        buf[off + 3] = ((cov + a_old * inv) * 255.0).clamp(0.0, 255.0) as u8;
                    }
                }
            }
        }

        #[allow(clippy::too_many_arguments)]
        pub(super) fn draw_text(
            &mut self,
            x: i32,
            y: i32,
            w: i32,
            h: i32,
            text: &str,
            size: i32,
            bold: bool,
            right: bool,
            col: [u8; 3],
        ) {
            let wt = wide_no_nul(text);
            if wt.is_empty() {
                return;
            }
            // GDI DrawText оставляет alpha-байты в 32-bpp DIB равными 0; восстанавливаем alpha для записанных пикселей.
            let x0 = x.max(0) as usize;
            let y0 = y.max(0) as usize;
            let x1 = ((x + w).clamp(0, self.w as i32)) as usize;
            let y1 = ((y + h).clamp(0, self.h as i32)) as usize;
            // SAFETY: валидный memory DC и буфер того же DIBSection.
            unsafe {
                GdiFlush();
                let buf = std::slice::from_raw_parts_mut(self.bits, self.w * self.h * 4);
                let mut prev = vec![0u32; (x1.saturating_sub(x0)) * (y1.saturating_sub(y0))];
                for (row_i, py) in (y0..y1).enumerate() {
                    for (col_i, px) in (x0..x1).enumerate() {
                        let off = (py * self.w + px) * 4;
                        prev[row_i * (x1 - x0) + col_i] = u32::from_le_bytes([buf[off], buf[off + 1], buf[off + 2], buf[off + 3]]);
                        buf[off + 3] = 0x7f; // маркер: GDI при отрисовке занулит alpha на изменённых пикселях
                    }
                }
                let font = font_handle(size, bold);
                let old = SelectObject(self.hdc, font);
                SetBkMode(self.hdc, TRANSPARENT);
                let colorref = u32::from(col[0]) | (u32::from(col[1]) << 8) | (u32::from(col[2]) << 16);
                SetTextColor(self.hdc, colorref);
                let mut rc = Rect { left: x, top: y, right: x + w, bottom: y + h };
                let mut fmt = DT_WORDBREAK | DT_NOPREFIX;
                if right {
                    fmt |= DT_RIGHT;
                }
                DrawTextW(self.hdc, wt.as_ptr(), wt.len() as i32, &raw mut rc, fmt);
                SelectObject(self.hdc, old);
                DeleteObject(font);
                GdiFlush();
                for (row_i, py) in (y0..y1).enumerate() {
                    for (col_i, px) in (x0..x1).enumerate() {
                        let off = (py * self.w + px) * 4;
                        let was = prev[row_i * (x1 - x0) + col_i].to_le_bytes();
                        if buf[off + 3] != 0x7f || buf[off] != was[0] || buf[off + 1] != was[1] || buf[off + 2] != was[2] {
                            buf[off + 3] = 255;
                        } else {
                            buf[off + 3] = was[3];
                        }
                    }
                }
            }
        }

        pub(super) fn into_rgba(self) -> Vec<u8> {
            let mut out = vec![0u8; self.w * self.h * 4];
            // SAFETY: чтение всех пикселей перед освобождением в Drop.
            unsafe {
                GdiFlush();
                let buf = std::slice::from_raw_parts(self.bits, self.w * self.h * 4);
                for (s, d) in buf.as_chunks::<4>().0.iter().zip(out.as_chunks_mut::<4>().0.iter_mut()) {
                    d[0] = s[2];
                    d[1] = s[1];
                    d[2] = s[0];
                    d[3] = s[3];
                }
            }
            out
        }
    }

    impl Drop for DIBCanvas {
        fn drop(&mut self) {
            // SAFETY: освобождение HBITMAP и memory DC строго по окончании использования.
            unsafe {
                SelectObject(self.hdc, self.old_bmp);
                DeleteObject(self.hbmp);
                DeleteDC(self.hdc);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_single_and_multi_message_shot_with_nonempty_pixels() {
        let mut m1 = MessageRow::plain(1, "Привет! Проверяем снимок сообщения.".to_owned(), false, 1_760_000_100);
        m1.sender = "Алиса".to_owned();
        let mut m2 = MessageRow::plain(2, "Ответ с датой и длинным текстом для переноса строк!".to_owned(), true, 1_760_000_160);
        m2.reply_to = Some((1, "Привет! Проверяем".to_owned()));
        let ayu = AyuSettings { show_date: true, ..AyuSettings::default() };
        let (w, h, rgba) = render_message_shot(&ShotInput {
            chat_title: "Тестовый чат",
            messages: &[&m1, &m2],
            ayu: &ayu,
            theme: ShotTheme::Night,
        });
        assert_eq!(w, 560);
        assert!(h >= 120);
        assert_eq!(rgba.len(), (w * h * 4) as usize);
        assert!(rgba.iter().skip(3).step_by(4).all(|&a| a == 255));
    }

    #[test]
    fn transparent_theme_keeps_outer_margin_transparent() {
        let m1 = MessageRow::plain(1, "Без фона".to_owned(), true, 1_760_000_100);
        let ayu = AyuSettings::default();
        let (_w, _h, rgba) = render_message_shot(&ShotInput {
            chat_title: "",
            messages: &[&m1],
            ayu: &ayu,
            theme: ShotTheme::Transparent,
        });
        // Левый верхний пиксель отступа (0, 0) должен быть прозрачным.
        assert_eq!(rgba[3], 0);
    }

    #[test]
    fn all_five_shot_themes_render_non_empty_images() {
        let m = MessageRow::plain(7, "Тест палитр снимка".to_owned(), false, 1_760_000_200);
        let ayu = AyuSettings::default();
        for theme in ShotTheme::ALL {
            let (w, h, rgba) = render_message_shot(&ShotInput {
                chat_title: "Канал",
                messages: &[&m],
                ayu: &ayu,
                theme,
            });
            assert_eq!(w, 560);
            assert!(h >= 80);
            assert_eq!(rgba.len(), (w * h * 4) as usize);
        }
    }

    #[test]
    fn empty_messages_shot_renders_safely() {
        let ayu = AyuSettings::default();
        let (w, h, rgba) = render_message_shot(&ShotInput {
            chat_title: "Пусто",
            messages: &[],
            ayu: &ayu,
            theme: ShotTheme::Oled,
        });
        assert_eq!(w, 560);
        assert!(h >= 60);
        assert_eq!(rgba.len(), (w * h * 4) as usize);
    }
}
