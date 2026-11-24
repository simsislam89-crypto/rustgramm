//! Оформление Rustgramm: чёрно-белая «бумага» с оранжевыми основными кнопками и ступенями
//! прозрачности. На старте окно непрозрачное (100%); ползунок в настройках меняет прозрачность
//! и фонов, и кнопок на лету, а при прозрачности < 100% включает размытие стекла (блюр).

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Pos2, Rect,
    Sense, Stroke, Vec2,
};

/// Серый тон из палитры Telegram `0xRRGGBB`: тот же свет, но без цвета (для шкал в `kit`).
pub const fn navy(hex: u32) -> Color32 {
    let r = (hex >> 16) & 0xff;
    let g = (hex >> 8) & 0xff;
    let b = hex & 0xff;
    let l = ((r * 54 + g * 183 + b * 18) >> 8) as u8;
    Color32::from_rgb(l, l, l)
}

/// То же для акцентных цветов Telegram.
pub const fn tg(hex: u32) -> Color32 {
    navy(hex)
}

// ---- чернила и бумага
pub const WINDOW_BG: Color32 = Color32::from_rgb(14, 14, 16);
pub const WINDOW_OVER: Color32 = Color32::from_rgb(30, 30, 34);
pub const SUBTEXT: Color32 = Color32::from_rgb(134, 134, 142);
pub const ACCENT: Color32 = Color32::from_rgb(238, 238, 241);
pub const BUTTON: Color32 = Color32::from_rgb(46, 46, 52);
pub const BUTTON_OVER: Color32 = Color32::from_rgb(62, 62, 70);
pub const WHITE: Color32 = Color32::from_rgb(238, 238, 241);
pub const INK_DARK: Color32 = Color32::from_rgb(10, 10, 12);
// список чатов
pub const DIALOG_OVER: Color32 = Color32::from_rgb(24, 24, 28);
pub const DIALOG_ACTIVE: Color32 = Color32::from_rgb(46, 46, 52);
pub const DIALOG_DATE: Color32 = Color32::from_rgb(150, 150, 158);
pub const DIALOG_TEXT: Color32 = Color32::from_rgb(134, 134, 142);
pub const UNREAD: Color32 = Color32::from_rgb(238, 238, 241);
pub const UNREAD_MUTED: Color32 = Color32::from_rgb(74, 74, 80);
pub const FILTER_INPUT: Color32 = Color32::from_rgb(26, 26, 30);
pub const TITLE_BG: Color32 = Color32::from_rgb(20, 20, 23);
pub const SIDEBAR_BG: Color32 = Color32::from_rgb(8, 8, 10);
// лента
pub const CHAT_BG: Color32 = Color32::from_rgb(8, 8, 10);
pub const MSG_IN_BG: Color32 = Color32::from_rgb(26, 26, 30);
pub const MSG_OUT_BG: Color32 = Color32::from_rgb(56, 56, 63);
pub const MSG_IN_DATE: Color32 = Color32::from_rgb(120, 120, 128);
pub const MSG_OUT_DATE: Color32 = Color32::from_rgb(172, 172, 180);
pub const MSG_OUT_TEXT: Color32 = Color32::from_rgb(244, 244, 246);
pub const OUT_TICK: Color32 = Color32::from_rgb(214, 214, 220);
pub const SERVICE_BG: Color32 = Color32::from_rgb(34, 34, 38);
pub const COMPOSE_BG: Color32 = Color32::from_rgb(14, 14, 16);
pub const DANGER: Color32 = Color32::from_rgb(242, 108, 108);
pub const GREEN: Color32 = Color32::from_rgb(104, 224, 146);
/// Основные кнопки и главный акцент (оранжевый).
pub const ORANGE: Color32 = Color32::from_rgb(245, 130, 32);
pub const ORANGE_OVER: Color32 = Color32::from_rgb(255, 148, 54);
pub const ORANGE_ACTIVE: Color32 = Color32::from_rgb(218, 110, 20);
pub const ORANGE_DISABLED: Color32 = Color32::from_rgb(122, 68, 24);
pub const ACCENT_TEXT: Color32 = Color32::WHITE;
/// Тонкая линия-разделитель стекла.
pub const LINE: Color32 = Color32::from_rgba_premultiplied(22, 22, 22, 22);

// Размеры (px) из dialogs.style, window.style и chat.style.
pub const DIALOG_ROW_HEIGHT: f32 = 62.0;
pub const DIALOG_PHOTO: f32 = 48.0;
pub const TOP_BAR_HEIGHT: f32 = 54.0;
pub const MSG_MAX_WIDTH: f32 = 430.0;
pub const MSG_PAD_X: f32 = 11.0;
pub const MSG_PAD_Y: f32 = 8.0;
pub const MSG_RADIUS: u8 = 15;
pub const COLUMN_LEFT_MIN: f32 = 260.0;
pub const COLUMN_LEFT_MAX: f32 = 540.0;
pub const COLUMN_LEFT_NORMAL: f32 = 380.0;
/// Уже этого окно показывает одну колонку (телефон): список чатов или чат.
pub const NARROW_WIDTH: f32 = 720.0;

// ---- прозрачность окна и блюр
/// Непрозрачность фонов и кнопок, 0..=255 (70% = 178 по умолчанию, матовое стекло на старте).
static ALPHA: AtomicU8 = AtomicU8::new(178);
/// Последняя альфа, применённая к глобальному стилю egui (кнопки, окна, поля).
static VISUAL_ALPHA: AtomicU8 = AtomicU8::new(0);
/// Включён ли блюр-эффект в стиле iOS.
static BLUR: AtomicBool = AtomicBool::new(true);
/// Скорость анимаций, проценты (0 — мгновенно).
static ANIM: std::sync::atomic::AtomicU16 = std::sync::atomic::AtomicU16::new(100);

/// Живая палитра темы (по умолчанию равна стандартным константам, меняется при импорте `.tdesktop-theme`/`.attheme` или в конструкторе).
#[derive(Clone, Copy)]
pub struct LivePalette {
    pub accent: Color32,
    pub window_bg: Color32,
    pub sidebar_bg: Color32,
    pub chat_bg: Color32,
    pub msg_in_bg: Color32,
    pub msg_out_bg: Color32,
    pub text_primary: Color32,
    pub text_secondary: Color32,
    pub button_bg: Color32,
    pub customized: bool,
}

impl Default for LivePalette {
    fn default() -> Self {
        Self {
            accent: ORANGE,
            window_bg: WINDOW_BG,
            sidebar_bg: SIDEBAR_BG,
            chat_bg: CHAT_BG,
            msg_in_bg: MSG_IN_BG,
            msg_out_bg: MSG_OUT_BG,
            text_primary: WHITE,
            text_secondary: SUBTEXT,
            button_bg: BUTTON,
            customized: false,
        }
    }
}

static LIVE_PALETTE: std::sync::RwLock<LivePalette> = std::sync::RwLock::new(LivePalette {
    accent: ORANGE,
    window_bg: WINDOW_BG,
    sidebar_bg: SIDEBAR_BG,
    chat_bg: CHAT_BG,
    msg_in_bg: MSG_IN_BG,
    msg_out_bg: MSG_OUT_BG,
    text_primary: WHITE,
    text_secondary: SUBTEXT,
    button_bg: BUTTON,
    customized: false,
});

static PALETTE_REV: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
static VISUAL_PALETTE_REV: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static PALETTE_CUSTOMIZED: AtomicBool = AtomicBool::new(false);
static LOCAL_TZ: std::sync::OnceLock<jiff::tz::TimeZone> = std::sync::OnceLock::new();

pub fn sync_custom_theme(s: &rg_settings::CustomThemeSettings) {
    let parse_or = |hex: &str, def: Color32| -> (Color32, bool) {
        if hex.trim().is_empty() {
            (def, false)
        } else if let Some(c) = crate::theme_pack::parse_hex_color(hex) {
            (c, true)
        } else {
            (def, false)
        }
    };
    let (accent, c1) = parse_or(&s.accent_hex, ORANGE);
    let (window_bg, c2) = parse_or(&s.window_bg_hex, WINDOW_BG);
    let (sidebar_bg, c3) = parse_or(&s.sidebar_bg_hex, SIDEBAR_BG);
    let (chat_bg, c4) = parse_or(&s.chat_bg_hex, CHAT_BG);
    let (msg_in_bg, c5) = parse_or(&s.msg_in_bg_hex, MSG_IN_BG);
    let (msg_out_bg, c6) = parse_or(&s.msg_out_bg_hex, MSG_OUT_BG);
    let (text_primary, c7) = parse_or(&s.text_primary_hex, WHITE);
    let (text_secondary, c8) = parse_or(&s.text_secondary_hex, SUBTEXT);
    let (button_bg, c9) = parse_or(&s.button_bg_hex, BUTTON);
    let customized = c1 || c2 || c3 || c4 || c5 || c6 || c7 || c8 || c9;
    PALETTE_CUSTOMIZED.store(customized, Ordering::Relaxed);
    if let Ok(mut g) = LIVE_PALETTE.write() {
        let next = LivePalette {
            accent,
            window_bg,
            sidebar_bg,
            chat_bg,
            msg_in_bg,
            msg_out_bg,
            text_primary,
            text_secondary,
            button_bg,
            customized,
        };
        if g.accent != next.accent
            || g.window_bg != next.window_bg
            || g.sidebar_bg != next.sidebar_bg
            || g.chat_bg != next.chat_bg
            || g.msg_in_bg != next.msg_in_bg
            || g.msg_out_bg != next.msg_out_bg
            || g.text_primary != next.text_primary
            || g.text_secondary != next.text_secondary
            || g.button_bg != next.button_bg
            || g.customized != next.customized
        {
            *g = next;
            PALETTE_REV.fetch_add(1, Ordering::Relaxed);
        }
    }
}

fn resolve_theme_color(c: Color32) -> Color32 {
    if !PALETTE_CUSTOMIZED.load(Ordering::Relaxed) {
        return c;
    }
    let p = palette();
    if !p.customized {
        return c;
    }
    if c == WINDOW_BG {
        p.window_bg
    } else if c == SIDEBAR_BG {
        p.sidebar_bg
    } else if c == CHAT_BG {
        p.chat_bg
    } else if c == MSG_IN_BG {
        p.msg_in_bg
    } else if c == MSG_OUT_BG {
        p.msg_out_bg
    } else if c == BUTTON {
        p.button_bg
    } else if c == ORANGE {
        p.accent
    } else if c == WHITE {
        p.text_primary
    } else if c == SUBTEXT {
        p.text_secondary
    } else {
        c
    }
}

pub fn palette() -> LivePalette {
    if !PALETTE_CUSTOMIZED.load(Ordering::Relaxed) {
        return LivePalette::default();
    }
    LIVE_PALETTE.read().map(|g| *g).unwrap_or_default()
}

pub fn c_orange() -> Color32 {
    palette().accent
}
pub fn c_window_bg() -> Color32 {
    palette().window_bg
}
pub fn c_sidebar_bg() -> Color32 {
    palette().sidebar_bg
}
pub fn c_chat_bg() -> Color32 {
    palette().chat_bg
}
pub fn c_msg_in() -> Color32 {
    palette().msg_in_bg
}
pub fn c_msg_out() -> Color32 {
    palette().msg_out_bg
}
pub fn c_white() -> Color32 {
    palette().text_primary
}
pub fn c_subtext() -> Color32 {
    palette().text_secondary
}
pub fn c_button() -> Color32 {
    palette().button_bg
}

/// Задать скорость анимаций в процентах (0 — выключены, 200 — вдвое быстрее).
pub fn set_anim_speed(pct: u8) {
    ANIM.store(u16::from(pct.min(200)), Ordering::Relaxed);
}
/// Включить или выключить размытие стекла.
pub fn set_blur(on: bool) {
    BLUR.store(on, Ordering::Relaxed);
}
pub fn blur_on() -> bool {
    BLUR.load(Ordering::Relaxed)
}
/// Процент непрозрачности по умолчанию (70% — мягкое матовое стекло на старте).
pub const DEFAULT_OPACITY_PCT: u8 = 70;

/// Задать непрозрачность фонов и кнопок в процентах (15..=100). На Android всегда 100% без альфа-блендинга.
pub fn set_opacity(pct: u8) {
    #[cfg(target_os = "android")]
    {
        let _ = pct;
        ALPHA.store(255, Ordering::Relaxed);
    }
    #[cfg(not(target_os = "android"))]
    {
        let pct = pct.clamp(15, 100);
        ALPHA.store((u16::from(pct) * 255 / 100) as u8, Ordering::Relaxed);
    }
}

/// Текущая непрозрачность окна (0.15..=1.0).
pub fn opacity_f32() -> f32 {
    f32::from(ALPHA.load(Ordering::Relaxed)) / 255.0
}

/// Единая подложка окна: рисуется один раз под всеми панелями, чтобы на стыках не было швов.
pub const BACKDROP: Color32 = Color32::from_rgb(10, 10, 12);

/// Лёгкий светлый налёт поверх подложки: панели списка, шапки, поле ввода.
pub fn tint_panel() -> Color32 {
    Color32::from_white_alpha((7.0 * opacity_f32()).round() as u8)
}

/// Тёмный налёт ленты сообщений.
pub fn tint_chat() -> Color32 {
    Color32::from_black_alpha((70.0 * opacity_f32()).round() as u8)
}

/// Налёт заголовка окна.
pub fn tint_title() -> Color32 {
    Color32::from_white_alpha((11.0 * opacity_f32()).round() as u8)
}

/// Фон с учётом прозрачности окна.
pub fn t(c: Color32) -> Color32 {
    with_alpha(resolve_theme_color(c), ALPHA.load(Ordering::Relaxed))
}

/// Фон кнопки с учётом прозрачности окна: альфа кнопки масштабируется вместе с окном.
pub fn t_btn(c: Color32) -> Color32 {
    let c = resolve_theme_color(c);
    let a = ALPHA.load(Ordering::Relaxed);
    let out = ((u16::from(c.a()) * u16::from(a)) / 255) as u8;
    with_alpha(c, out)
}

/// Фон окон и меню: чуть плотнее основного, но тоже становится прозрачнее с ползунком.
pub fn t2(c: Color32) -> Color32 {
    let c = resolve_theme_color(c);
    let a = ALPHA.load(Ordering::Relaxed);
    let out = if a >= 245 {
        a
    } else {
        ((u16::from(a) * 108) / 100).clamp(44, 255) as u8
    };
    with_alpha(c, out)
}

fn with_alpha(c: Color32, a: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), a)
}

/// Плавная кривая iOS (быстрый старт, шёлковая остановка без перелёта).
pub fn ios_spring(k: f32) -> f32 {
    let k = k.clamp(0.0, 1.0);
    1.0 - (1.0 - k).powi(4)
}

/// Упругая пружинная кривая iOS с мягким перелётом (~5%) для кнопок, попапов и иконок.
pub fn ios_spring_bounce(k: f32) -> f32 {
    let t = k.clamp(0.0, 1.0);
    if t <= 0.0 {
        return 0.0;
    }
    if t >= 1.0 {
        return 1.0;
    }
    let decay = (-6.2 * t).exp();
    let osc = (t * std::f32::consts::PI * 1.65).cos();
    (1.0 - decay * osc).clamp(0.0, 1.08)
}

/// Пружинный масштаб интерактивных иконок/кнопок (наведение + нажатие).
pub fn spring_scale(hover: f32, active: f32) -> f32 {
    let h = ios_spring_bounce(hover);
    let a = ios_spring(active);
    (1.0 + 0.14 * h - 0.08 * a).clamp(0.88, 1.22)
}

/// Многослойное матовое стекло для левой шторки, всплывающих окон и модальных карточек.
pub fn paint_frosted_glass_panel(p: &egui::Painter, rect: Rect, radius: f32, raw: f32) {
    let k = ios_spring(raw.clamp(0.0, 1.0));
    let cr = CornerRadius::same(radius.clamp(0.0, 32.0) as u8);
    let op = opacity_f32();
    let base = resolve_theme_color(WINDOW_BG);
    // Плотность стекла: достаточно прозрачное для эффекта матовости, но текст читается идеально.
    let glass_a = ((150.0 + 82.0 * op) * k).clamp(0.0, 242.0) as u8;
    p.rect_filled(
        rect,
        cr,
        Color32::from_rgba_unmultiplied(base.r(), base.g(), base.b(), glass_a),
    );
    if blur_on() {
        // Верхняя морозная дымка (эффект преломления матового стекла).
        let top_h = (rect.height() * 0.42).clamp(24.0, 180.0);
        let top_rect = Rect::from_min_size(rect.min, Vec2::new(rect.width(), top_h));
        let frost_a = (16.0 * k * (0.55 + 0.45 * op)) as u8;
        p.rect_filled(
            top_rect,
            CornerRadius {
                nw: cr.nw,
                ne: cr.ne,
                sw: 0,
                se: 0,
            },
            Color32::from_rgba_unmultiplied(215, 224, 240, frost_a),
        );
        // Нижняя глубина для объёма.
        let bot_h = (rect.height() * 0.25).clamp(16.0, 100.0);
        let bot_rect = Rect::from_min_max(
            Pos2::new(rect.left(), rect.bottom() - bot_h),
            rect.right_bottom(),
        );
        p.rect_filled(
            bot_rect,
            CornerRadius {
                nw: 0,
                ne: 0,
                sw: cr.sw,
                se: cr.se,
            },
            Color32::from_black_alpha((26.0 * k) as u8),
        );
    }
    paint_modal_subblur(p, rect, radius, raw);
}

/// Стильный QR-код в стиле Telegram: высокий уровень коррекции ошибок `EcLevel::H` (30%),
/// чисто-белая тихая зона >= 4 модулей, высококонтрастные чёрные поисковые глаза и модули
/// (100% сканирование камерой любого телефона), кэш растра `ColorImage`/`TextureHandle` для 0 лагов.
pub fn render_qr_image(url: &str, badge_accent: Color32) -> Option<egui::ColorImage> {
    let code = qrcode::QrCode::with_error_correction_level(url.as_bytes(), qrcode::EcLevel::H).ok()?;
    let n = code.width();
    if n == 0 {
        return None;
    }
    const IMG_SIDE: usize = 512;
    let quiet_mods = 4usize;
    let total_mods = n + quiet_mods * 2;
    let mut pixels = vec![Color32::WHITE; IMG_SIDE * IMG_SIDE];

    let cell = IMG_SIDE as f32 / total_mods as f32;
    let origin = quiet_mods as f32 * cell;
    let mid = (n as f32 - 1.0) * 0.5;
    let center_clear_r = (n as f32 * 0.145).max(3.2);
    let badge_px_r = (center_clear_r * cell * 0.90).clamp(24.0, 44.0);
    let center_px = IMG_SIDE as f32 * 0.5;

    let is_finder = |x: usize, y: usize| -> bool {
        (x < 7 && y < 7) || (x + 7 >= n && y < 7) || (x < 7 && y + 7 >= n)
    };

    let dark = Color32::from_rgb(10, 12, 16);
    let pure_black = Color32::BLACK;

    let fill_disk = |pixels: &mut [Color32], cx: f32, cy: f32, r: f32, col: Color32| {
        let x0 = ((cx - r - 1.0).floor() as isize).clamp(0, IMG_SIDE as isize - 1) as usize;
        let x1 = ((cx + r + 1.0).ceil() as isize).clamp(0, IMG_SIDE as isize - 1) as usize;
        let y0 = ((cy - r - 1.0).floor() as isize).clamp(0, IMG_SIDE as isize - 1) as usize;
        let y1 = ((cy + r + 1.0).ceil() as isize).clamp(0, IMG_SIDE as isize - 1) as usize;
        let r2 = r * r;
        for py in y0..=y1 {
            let dy = py as f32 + 0.5 - cy;
            for px in x0..=x1 {
                let dx = px as f32 + 0.5 - cx;
                if dx * dx + dy * dy <= r2 {
                    pixels[py * IMG_SIDE + px] = col;
                }
            }
        }
    };

    let fill_round_rect = |pixels: &mut [Color32], rx0: f32, ry0: f32, rx1: f32, ry1: f32, rad: f32, col: Color32| {
        let x0 = (rx0.floor() as isize).clamp(0, IMG_SIDE as isize - 1) as usize;
        let x1 = (rx1.ceil() as isize).clamp(0, IMG_SIDE as isize - 1) as usize;
        let y0 = (ry0.floor() as isize).clamp(0, IMG_SIDE as isize - 1) as usize;
        let y1 = (ry1.ceil() as isize).clamp(0, IMG_SIDE as isize - 1) as usize;
        let r = rad.max(0.0);
        let r2 = r * r;
        let ix0 = rx0 + r;
        let ix1 = rx1 - r;
        let iy0 = ry0 + r;
        let iy1 = ry1 - r;
        for py in y0..=y1 {
            let fy = py as f32 + 0.5;
            if fy < ry0 || fy > ry1 {
                continue;
            }
            for px in x0..=x1 {
                let fx = px as f32 + 0.5;
                if fx < rx0 || fx > rx1 {
                    continue;
                }
                let dx = if fx < ix0 {
                    ix0 - fx
                } else if fx > ix1 {
                    fx - ix1
                } else {
                    0.0
                };
                let dy = if fy < iy0 {
                    iy0 - fy
                } else if fy > iy1 {
                    fy - iy1
                } else {
                    0.0
                };
                if dx * dx + dy * dy <= r2 {
                    pixels[py * IMG_SIDE + px] = col;
                }
            }
        }
    };

    // Модули данных: контрастные тёмные округлые точки с перемычками (легко читаются любыми камерами).
    let mod_r = (cell * 0.45).max(1.4);
    for y in 0..n {
        for x in 0..n {
            if is_finder(x, y) || code[(x, y)] != qrcode::Color::Dark {
                continue;
            }
            let dx = x as f32 - mid;
            let dy = y as f32 - mid;
            if (dx * dx + dy * dy).sqrt() < center_clear_r {
                continue;
            }
            let cx = origin + (x as f32 + 0.5) * cell;
            let cy = origin + (y as f32 + 0.5) * cell;
            fill_disk(&mut pixels, cx, cy, mod_r, dark);
            if x + 1 < n && !is_finder(x + 1, y) && code[(x + 1, y)] == qrcode::Color::Dark {
                let dx2 = (x + 1) as f32 - mid;
                if (dx2 * dx2 + dy * dy).sqrt() >= center_clear_r {
                    fill_round_rect(
                        &mut pixels,
                        cx,
                        cy - mod_r * 0.84,
                        cx + cell,
                        cy + mod_r * 0.84,
                        1.0,
                        dark,
                    );
                }
            }
            if y + 1 < n && !is_finder(x, y + 1) && code[(x, y + 1)] == qrcode::Color::Dark {
                let dy2 = (y + 1) as f32 - mid;
                if (dx * dx + dy2 * dy2).sqrt() >= center_clear_r {
                    fill_round_rect(
                        &mut pixels,
                        cx - mod_r * 0.84,
                        cy,
                        cx + mod_r * 0.84,
                        cy + cell,
                        1.0,
                        dark,
                    );
                }
            }
        }
    }

    // 3 поисковых глаза (strict 7x7 / 5x5 white / 3x3 black — максимальная надёжность сканирования).
    for (fx, fy) in [(0usize, 0usize), (n - 7, 0), (0, n - 7)] {
        let x0 = origin + fx as f32 * cell;
        let y0 = origin + fy as f32 * cell;
        fill_round_rect(&mut pixels, x0, y0, x0 + 7.0 * cell, y0 + 7.0 * cell, cell * 1.4, pure_black);
        fill_round_rect(
            &mut pixels,
            x0 + cell,
            y0 + cell,
            x0 + 6.0 * cell,
            y0 + 6.0 * cell,
            cell * 0.9,
            Color32::WHITE,
        );
        fill_round_rect(
            &mut pixels,
            x0 + 2.0 * cell,
            y0 + 2.0 * cell,
            x0 + 5.0 * cell,
            y0 + 5.0 * cell,
            cell * 0.65,
            pure_black,
        );
    }

    // Белый ободок и центральный синий/акцентный кружок медальона.
    fill_disk(&mut pixels, center_px, center_px, badge_px_r + 5.0, Color32::WHITE);
    fill_disk(&mut pixels, center_px, center_px, badge_px_r, badge_accent);

    Some(egui::ColorImage {
        size: [IMG_SIDE, IMG_SIDE],
        source_size: Vec2::new(IMG_SIDE as f32, IMG_SIDE as f32),
        pixels,
    })
}

/// Рисует стильный QR-код с кэшированием GPU-текстуры (1 текстура в кадр, без лагов и CPU-пересчёта).
#[allow(deprecated)]
pub fn draw_styled_qr(ui: &mut egui::Ui, url: &str, size: f32, accent: Color32, pop_k: f32) -> bool {
    if url.is_empty() {
        return false;
    }
    let cache_id = egui::Id::new("rg-qr-tex-cache-v2");
    let badge_col = Color32::from_rgb(42, 171, 238);
    let tex: Option<egui::TextureHandle> = ui.ctx().data(|d| {
        d.get_temp::<(String, Color32, egui::TextureHandle)>(cache_id)
            .filter(|(u, c, _)| u == url && *c == badge_col)
            .map(|(_, _, t)| t)
    });
    let tex = match tex {
        Some(t) => t,
        None => {
            let Some(img) = render_qr_image(url, badge_col) else {
                return false;
            };
            let handle = ui.ctx().load_texture("rg-qr-code", img, egui::TextureOptions::LINEAR);
            ui.ctx()
                .data_mut(|d| d.insert_temp(cache_id, (url.to_owned(), badge_col, handle.clone())));
            handle
        }
    };

    let scale = 0.92 + 0.08 * ios_spring_bounce(pop_k);
    let (outer, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    let card = Rect::from_center_size(outer.center(), Vec2::splat(size * scale));
    let p = ui.painter();

    p.rect_filled(
        card.expand(4.0),
        CornerRadius::same(24),
        Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), (26.0 * pop_k) as u8),
    );
    p.rect_filled(
        card,
        CornerRadius::same(20),
        Color32::from_rgba_unmultiplied(255, 255, 255, (255.0 * pop_k.clamp(0.25, 1.0)) as u8),
    );
    let inner = card.shrink(6.0 * scale);
    p.image(
        tex.id(),
        inner,
        Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
        Color32::from_white_alpha((255.0 * pop_k.clamp(0.25, 1.0)) as u8),
    );
    p.rect_stroke(
        card,
        CornerRadius::same(20),
        Stroke::new(1.2, Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 110)),
        egui::StrokeKind::Inside,
    );

    // Чёткий векторный бумажный самолётик поверх центрального медальона QR.
    let c = inner.center();
    let s = (inner.width() * 0.044).clamp(7.0, 14.0);
    p.add(egui::Shape::convex_polygon(
        vec![
            c + Vec2::new(-s * 1.10, 0.1 * s),
            c + Vec2::new(s * 0.96, -s * 0.80),
            c + Vec2::new(s * 0.44, s * 0.92),
            c + Vec2::new(-s * 0.14, s * 0.24),
        ],
        Color32::WHITE,
        Stroke::NONE,
    ));
    true
}

/// Полупрозрачная вуаль под модальными окнами в стиле AyuGram (лёгкое затемнение фона без пятен).
pub fn paint_frosted_scrim(p: &egui::Painter, rect: Rect, raw: f32) {
    let raw = raw.clamp(0.0, 1.0);
    let k = ios_spring(raw);
    let op = opacity_f32();
    let dark_a = (84.0 * k * (0.6 + 0.4 * op)) as u8;
    p.rect_filled(rect, 0.0, Color32::from_black_alpha(dark_a));
    if blur_on() {
        let frost_a = (12.0 * k * op) as u8;
        p.rect_filled(rect, 0.0, Color32::from_rgba_unmultiplied(24, 30, 40, frost_a));
    }
}

/// Мягкая внешняя тень-ореол и стеклянная кромка вокруг модальной карточки
/// (рисует ТОЛЬКО снаружи `StrokeKind::Outside`, не затемняя содержимое самой карточки!).
pub fn paint_modal_subblur(p: &egui::Painter, rect: Rect, radius: f32, raw: f32) {
    let k = ios_spring(raw.clamp(0.0, 1.0));
    if blur_on() {
        for (spread, dark_a) in [(2.0, 46.0), (6.0, 28.0), (12.0, 14.0)] {
            let r = rect.expand(spread * k);
            let cr = CornerRadius::same((radius + spread * 0.5).clamp(4.0, 36.0) as u8);
            p.rect_stroke(
                r,
                cr,
                Stroke::new(3.0, Color32::from_black_alpha((dark_a * k) as u8)),
                egui::StrokeKind::Outside,
            );
        }
    }
    p.rect_stroke(
        rect,
        CornerRadius::same(radius.clamp(2.0, 32.0) as u8),
        Stroke::new(1.0, Color32::from_white_alpha((26.0 * k * opacity_f32()) as u8)),
        egui::StrokeKind::Inside,
    );
    paint_glass_rim(p, rect, radius);
}

/// Рисует тонкий верхний стеклянный блик на карточке или модальном окне.
pub fn paint_glass_rim(p: &egui::Painter, rect: Rect, radius: f32) {
    let a = (38.0 * opacity_f32()) as u8;
    p.line_segment(
        [
            Pos2::new(rect.left() + radius, rect.top() + 0.5),
            Pos2::new(rect.right() - radius, rect.top() + 0.5),
        ],
        Stroke::new(1.0, Color32::from_white_alpha(a)),
    );
}

/// Фон области чата: 0 — тёмный, 1 — мягкий градиент, 2 — тонкий узор Telegram (+ опциональные обои темы).
pub fn paint_chat_backdrop(p: &egui::Painter, rect: Rect, style: u8) {
    paint_chat_backdrop_ex(p, rect, style, None, 45);
}

/// Расширенная отрисовка фона чата с поддержкой кастомной картинки обоев из `.tdesktop-theme` / `.attheme`.
#[allow(deprecated)]
pub fn paint_chat_backdrop_ex(
    p: &egui::Painter,
    rect: Rect,
    style: u8,
    wallpaper: Option<&egui::TextureHandle>,
    wallpaper_dim: u8,
) {
    let pal = palette();
    if pal.customized {
        p.rect_filled(rect, 0.0, t(pal.chat_bg));
    } else {
        p.rect_filled(rect, 0.0, tint_chat());
    }
    let op = opacity_f32();
    if let Some(tex) = wallpaper {
        let dim = ((f32::from(wallpaper_dim.min(95)) / 100.0) * 230.0 * op) as u8;
        p.image(
            tex.id(),
            rect,
            Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
            Color32::from_white_alpha((255.0 * op) as u8),
        );
        if dim > 0 {
            p.rect_filled(rect, 0.0, Color32::from_black_alpha(dim));
        }
        return;
    }
    match style {
        1 => {
            let ac = pal.accent;
            let bands = 12;
            let bh = rect.height() / bands as f32;
            for i in 0..bands {
                let f = 1.0 - i as f32 / bands as f32;
                let a = (24.0 * f * f * op) as u8;
                let r = Rect::from_min_size(
                    Pos2::new(rect.left(), rect.top() + i as f32 * bh),
                    Vec2::new(rect.width(), bh + 1.0),
                );
                p.rect_filled(r, 0.0, Color32::from_rgba_unmultiplied(ac.r(), ac.g(), ac.b(), a));
            }
        }
        2 => {
            // Оптимизированный геометрический узор в духе Telegram: одна повторяемая тайл-текстура вместо 1,600 фигур/кадр.
            let tile_id = egui::Id::new("rg-bg-pattern-tile");
            let tex: egui::TextureHandle = match p.ctx().data(|d| d.get_temp(tile_id)) {
                Some(t) => t,
                None => {
                    const S: usize = 64;
                    let mut px = vec![Color32::TRANSPARENT; S * S];
                    let c = Color32::from_white_alpha(18);
                    let ring = |px: &mut [Color32], cx: f32, cy: f32, r: f32| {
                        for y in 0..S {
                            for x in 0..S {
                                let d = ((x as f32 - cx).powi(2) + (y as f32 - cy).powi(2)).sqrt();
                                if (d - r).abs() <= 0.85 {
                                    px[y * S + x] = c;
                                }
                            }
                        }
                    };
                    ring(&mut px, 16.0, 16.0, 4.5);
                    ring(&mut px, 48.0, 48.0, 4.5);
                    px[16 * S + 48] = c;
                    px[48 * S + 16] = c;
                    let ci = egui::ColorImage { size: [S, S], source_size: Vec2::new(S as f32, S as f32), pixels: px };
                    let t = p.ctx().load_texture("rg-bg-tile", ci, egui::TextureOptions::LINEAR_REPEAT);
                    p.ctx().data_mut(|d| d.insert_temp(tile_id, t.clone()));
                    t
                }
            };
            let u = rect.width() / 64.0;
            let v = rect.height() / 64.0;
            p.image(
                tex.id(),
                rect,
                Rect::from_min_max(Pos2::ZERO, Pos2::new(u, v)),
                Color32::from_white_alpha((255.0 * op) as u8),
            );
        }
        _ => {}
    }
}

/// Основная оранжевая кнопка: фон прозрачнеет вместе с процентом прозрачности окна.
pub fn orange_button(ui: &mut egui::Ui, text: &str, min_size: Vec2) -> egui::Response {
    orange_button_enabled(ui, true, text, min_size)
}

/// Основная оранжевая кнопка с состоянием активности `enabled`.
pub fn orange_button_enabled(ui: &mut egui::Ui, enabled: bool, text: &str, min_size: Vec2) -> egui::Response {
    let galley = ui
        .painter()
        .layout_no_wrap(text.to_owned(), FontId::proportional(14.5), Color32::WHITE);
    let size = Vec2::new(
        (galley.size().x + 28.0).max(min_size.x),
        (galley.size().y + 14.0).max(min_size.y),
    );
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let (rect, resp) = ui.allocate_exact_size(size, sense);
    let hover = tween(ui.ctx(), ("obtn", resp.id), f32::from(u8::from(enabled && resp.hovered())), 0.12);
    let press = tween(ui.ctx(), ("obtn-p", resp.id), f32::from(u8::from(enabled && resp.is_pointer_button_down_on())), 0.08);
    let draw_r = rect.shrink(press * 1.4);
    let accent = c_orange();
    let accent_over = lerp_color(accent, Color32::WHITE, 0.14);
    let accent_act = lerp_color(accent, Color32::BLACK, 0.15);
    let accent_dis = lerp_color(accent, Color32::BLACK, 0.55);
    let base = if !enabled {
        accent_dis
    } else if press > 0.35 {
        lerp_color(accent_over, accent_act, press)
    } else {
        lerp_color(accent, accent_over, hover)
    };
    let fill = t_btn(base);
    let p = ui.painter();
    p.rect_filled(draw_r, CornerRadius::same(10), fill);
    if press > 0.05 {
        let rad = draw_r.width() * 0.55 * press;
        p.with_clip_rect(draw_r)
            .circle_filled(draw_r.center(), rad, Color32::from_white_alpha((32.0 * press) as u8));
    }
    if enabled {
        paint_glass_rim(p, draw_r, 10.0);
    }
    let text_c = if enabled { Color32::WHITE } else { Color32::from_white_alpha(150) };
    p.text(draw_r.center(), Align2::CENTER_CENTER, text, FontId::proportional(14.5), text_c);
    if enabled {
        resp.on_hover_cursor(egui::CursorIcon::PointingHand)
    } else {
        resp
    }
}

/// Обновляет глобальный стиль кнопок и окон egui при изменении прозрачности окна или темы.
pub fn sync_visuals(ctx: &egui::Context) {
    let a = ALPHA.load(Ordering::Relaxed);
    let pr = PALETTE_REV.load(Ordering::Relaxed);
    let alpha_changed = VISUAL_ALPHA.swap(a, Ordering::Relaxed) != a;
    let pal_changed = VISUAL_PALETTE_REV.swap(pr, Ordering::Relaxed) != pr;
    if !alpha_changed && !pal_changed {
        return;
    }
    let op = f32::from(a) / 255.0;
    let text_c = c_white();
    ctx.global_style_mut(|s| {
        let v = &mut s.visuals;
        v.window_fill = t2(WINDOW_BG);
        v.override_text_color = Some(text_c);
        v.hyperlink_color = c_orange();
        v.extreme_bg_color = Color32::from_rgba_unmultiplied(0, 0, 0, (90.0 * op) as u8);
        v.faint_bg_color = Color32::from_rgba_unmultiplied(255, 255, 255, (10.0 * op) as u8);
        let fill = Color32::from_rgba_unmultiplied(255, 255, 255, (18.0 * op) as u8);
        let hover = Color32::from_rgba_unmultiplied(255, 255, 255, (30.0 * op) as u8);
        let line = Color32::from_rgba_unmultiplied(255, 255, 255, (28.0 * op).max(10.0) as u8);
        v.widgets.inactive.bg_fill = fill;
        v.widgets.inactive.weak_bg_fill = fill;
        v.widgets.inactive.bg_stroke = Stroke::new(1.0, line);
        v.widgets.hovered.bg_fill = hover;
        v.widgets.hovered.weak_bg_fill = hover;
        v.widgets.hovered.bg_stroke = Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, (44.0 * op).max(14.0) as u8));
        v.widgets.active.bg_fill = hover;
        v.widgets.active.weak_bg_fill = hover;
        v.widgets.open = v.widgets.active;
    });
}

/// Плавный переход к `target` за `dur` секунд (всегда запрашивает перерисовку, пока идёт движение).
pub fn tween(ctx: &egui::Context, id: impl std::hash::Hash + std::fmt::Debug, target: f32, dur: f32) -> f32 {
    let id = egui::Id::NULL.with(id);
    let speed = f32::from(ANIM.load(Ordering::Relaxed)) / 100.0;
    let dur = if speed <= 0.0 { 0.0 } else { dur / speed };
    let val = ctx.animate_value_with_time(id, target, dur);
    if (val - target).abs() > 0.001 {
        ctx.request_repaint();
    }
    val
}

/// Экспоненциальное замедление: быстрый старт, мягкая посадка.
pub fn ease_out(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    if t >= 1.0 { 1.0 } else { 1.0 - 2f32.powf(-10.0 * t) }
}

/// Кривая появления, как у Telegram Desktop: быстрый старт и мягкая посадка без перелёта
/// (длительности 100–200 мс берутся из `.style` Telegram: `universalDuration` 120,
/// `dialogsFilterSlideDuration` 200, `emojiPanDuration` 200, `historyToDownDuration` 150).
/// Имя прежнее: так же вызывается во всех окнах и панелях.
pub fn spring(k: f32) -> f32 {
    let k = k.clamp(0.0, 1.0);
    1.0 - (1.0 - k).powi(3)
}

/// Линейная смесь цветов.
pub fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let f = |x: u8, y: u8| (f32::from(x) + (f32::from(y) - f32::from(x)) * t).round() as u8;
    Color32::from_rgba_unmultiplied(f(a.r(), b.r()), f(a.g(), b.g()), f(a.b(), b.b()), f(a.a(), b.a()))
}

pub const FAM_DISPLAY: &str = "display";

pub fn apply(ctx: &egui::Context) {
    install_fonts(ctx);
    let mut v = egui::Visuals::dark();
    v.panel_fill = Color32::TRANSPARENT;
    v.window_fill = t2(WINDOW_BG);
    v.extreme_bg_color = Color32::from_rgba_unmultiplied(0, 0, 0, 90);
    v.faint_bg_color = Color32::from_rgba_unmultiplied(255, 255, 255, 9);
    v.hyperlink_color = ACCENT;
    v.selection.bg_fill = Color32::from_rgba_unmultiplied(255, 255, 255, 46);
    v.selection.stroke = Stroke::new(1.0, WHITE);
    v.override_text_color = Some(WHITE);
    v.text_options.font_hinting = true;
    v.text_options.subpixel_binning = false;
    let line = Color32::from_rgba_unmultiplied(255, 255, 255, 22);
    let soft = Color32::from_rgba_unmultiplied(255, 255, 255, 12);
    let hover = Color32::from_rgba_unmultiplied(255, 255, 255, 15);
    let fill = Color32::from_rgba_unmultiplied(255, 255, 255, 9);
    v.widgets.noninteractive.bg_fill = Color32::TRANSPARENT;
    v.widgets.noninteractive.weak_bg_fill = Color32::TRANSPARENT;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, soft);
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, WHITE);
    v.widgets.inactive.bg_fill = fill;
    v.widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0, line);
    v.widgets.inactive.fg_stroke = Stroke::new(1.0, WHITE);
    v.widgets.hovered.bg_fill = hover;
    v.widgets.hovered.weak_bg_fill = hover;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 32));
    v.widgets.hovered.fg_stroke = Stroke::new(1.0, WHITE);
    v.widgets.active.bg_fill = hover;
    v.widgets.active.weak_bg_fill = hover;
    v.widgets.active.bg_stroke = Stroke::new(1.0, WHITE);
    v.widgets.active.fg_stroke = Stroke::new(1.0, WHITE);
    v.widgets.open = v.widgets.active;
    let r = CornerRadius::same(10);
    v.widgets.inactive.corner_radius = r;
    v.widgets.hovered.corner_radius = r;
    v.widgets.active.corner_radius = r;
    v.widgets.noninteractive.corner_radius = r;
    v.window_stroke = Stroke::new(1.0, line);
    v.window_corner_radius = CornerRadius::same(14);
    ctx.set_visuals(v);
    ctx.global_style_mut(|s| {
        s.spacing.item_spacing = Vec2::new(8.0, 4.0);
        s.spacing.button_padding = Vec2::new(14.0, 7.0);
        s.spacing.scroll.floating = true;
        s.spacing.scroll.floating_allocated_width = 9.0;
        s.spacing.scroll.floating_width = 5.0;
        s.spacing.scroll.bar_width = 7.0;
        s.spacing.scroll.handle_min_length = 28.0;
        s.spacing.scroll.dormant_handle_opacity = 0.36;
        s.spacing.scroll.active_handle_opacity = 0.72;
        s.text_styles
            .insert(egui::TextStyle::Body, FontId::proportional(15.0));
        s.text_styles
            .insert(egui::TextStyle::Button, FontId::proportional(14.5));
        s.text_styles
            .insert(egui::TextStyle::Small, FontId::proportional(12.0));
        s.text_styles.insert(
            egui::TextStyle::Heading,
            FontId::new(20.0, FontFamily::Name(FAM_DISPLAY.into())),
        );
    });
}

/// Шрифт заголовков (Segoe UI Bold, как в LLM Launcher).
pub fn display(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(FAM_DISPLAY.into()))
}

fn read_face(path: &str) -> Option<Arc<FontData>> {
    // Подгонка под пиксельную сетку, без субпиксельных сдвигов: буквы выглядят «напечатанными».
    let tweak = egui::FontTweak {
        hinting: Some(true),
        subpixel_binning: Some(false),
        ..Default::default()
    };
    std::fs::read(path)
        .ok()
        .map(|b| Arc::new(FontData::from_owned(b).tweak(tweak)))
}

/// Системные шрифты Windows/Android с полным покрытием символов и эмодзи во всех семействах
/// (убирает квадраты `□` в именах контактов, секретных чатах, кнопках и заголовках).
fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let has_segoe = if let Some(f) = read_face("C:\\Windows\\Fonts\\segoeui.ttf") {
        fonts.font_data.insert("segoe".into(), f);
        fonts
            .families
            .entry(FontFamily::Proportional)
            .or_default()
            .insert(0, "segoe".into());
        true
    } else {
        false
    };
    let bold = read_face("C:\\Windows\\Fonts\\segoeuib.ttf");
    let display = fonts
        .families
        .entry(FontFamily::Name(FAM_DISPLAY.into()))
        .or_default();
    if let Some(f) = bold {
        fonts.font_data.insert("segoe-bold".into(), f);
        display.push("segoe-bold".into());
    }
    if has_segoe {
        display.push("segoe".into());
    } else {
        display.push("Ubuntu-Light".into());
    }
    if let Some(f) = read_face("C:\\Windows\\Fonts\\consola.ttf") {
        fonts.font_data.insert("consola".into(), f);
        fonts
            .families
            .entry(FontFamily::Monospace)
            .or_default()
            .insert(0, "consola".into());
    } else if has_segoe {
        fonts
            .families
            .entry(FontFamily::Monospace)
            .or_default()
            .push("segoe".into());
    }
    if cfg!(target_os = "android")
        && let Ok(bytes) = std::fs::read("/system/fonts/Roboto-Regular.ttf")
    {
        fonts
            .font_data
            .insert("android-roboto".to_owned(), Arc::new(FontData::from_owned(bytes)));
        for family in [
            FontFamily::Proportional,
            FontFamily::Name(FAM_DISPLAY.into()),
        ] {
            fonts.families.entry(family).or_default().insert(0, "android-roboto".to_owned());
        }
    }
    // Фоллбэки Unicode/символов во ВСЕ три семейства — без 10-МБ NotoColorEmoji (эмодзи рисует атлас).
    let fallback_paths = [
        ("segoe-symbols", "C:\\Windows\\Fonts\\seguisym.ttf"),
        ("noto-symbols2", "/system/fonts/NotoSansSymbols2-Regular.ttf"),
        ("noto-symbols", "/system/fonts/NotoSansSymbols-Regular-Subsetted.ttf"),
        ("dejavu-sans", "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"),
    ];
    for (name, path) in fallback_paths {
        if let Ok(bytes) = std::fs::read(path) {
            fonts
                .font_data
                .insert(name.to_owned(), Arc::new(FontData::from_owned(bytes)));
            for family in [
                FontFamily::Proportional,
                FontFamily::Monospace,
                FontFamily::Name(FAM_DISPLAY.into()),
            ] {
                fonts
                    .families
                    .entry(family)
                    .or_default()
                    .push(name.to_owned());
            }
        }
    }
    ctx.set_fonts(fonts);
}

/// Серый оттенок аватарки по идентификатору: чёрно-белая палитра, ступени яркости.
pub fn avatar_color(id: i64) -> Color32 {
    const SHADES: [u8; 7] = [58, 74, 90, 104, 66, 82, 98];
    let g = SHADES[(id.unsigned_abs() % SHADES.len() as u64) as usize];
    Color32::from_rgb(g, g, g + 4)
}

/// Аватарка с первой буквой имени (круг).
pub fn avatar(ui: &mut egui::Ui, name: &str, id: i64, size: f32, tex: Option<&egui::TextureHandle>, corners: i32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    draw_avatar(ui, rect.center(), name, id, size, tex, corners);
}

/// Аватарка: фото, если оно загружено, иначе серый круг с буквой. `corners` — скругление
/// из настроек AyuGram (0..=23 для размера 48; 23 и больше — круг).
pub fn draw_avatar(
    ui: &egui::Ui,
    center: egui::Pos2,
    name: &str,
    id: i64,
    size: f32,
    tex: Option<&egui::TextureHandle>,
    corners: i32,
) {
    let rect = egui::Rect::from_center_size(center, Vec2::splat(size));
    let radius = (corners.clamp(0, 23) as f32 * size / 46.0).min(size / 2.0);
    let cr = CornerRadius::same(radius.round() as u8);
    if let Some(tex) = tex {
        egui::Image::new(tex)
            .corner_radius(cr)
            .fit_to_exact_size(Vec2::splat(size))
            .paint_at(ui, rect);
        return;
    }
    let painter = ui.painter();
    painter.rect_filled(rect, cr, avatar_color(id));
    let initial = name
        .chars()
        .find(|c| c.is_alphanumeric())
        .map(|c| c.to_uppercase().to_string())
        .unwrap_or_else(|| "?".to_owned());
    painter.text(
        center,
        egui::Align2::CENTER_CENTER,
        initial,
        FontId::proportional(size * 0.42),
        Color32::WHITE,
    );
}

const MONTHS: [&str; 12] = [
    "января", "февраля", "марта", "апреля", "мая", "июня", "июля", "августа", "сентября",
    "октября", "ноября", "декабря",
];
const WEEKDAYS: [&str; 7] = ["Пн", "Вт", "Ср", "Чт", "Пт", "Сб", "Вс"];

fn zoned(ts: i64) -> Option<jiff::Zoned> {
    let tz = LOCAL_TZ.get_or_init(jiff::tz::TimeZone::system).clone();
    jiff::Timestamp::from_second(ts).ok().map(|t| t.to_zoned(tz))
}

/// «ЧЧ:ММ» по местному времени.
pub fn hhmm(ts: i64) -> String {
    zoned(ts)
        .map(|z| z.strftime("%H:%M").to_string())
        .unwrap_or_default()
}

/// «ЧЧ:ММ:СС» по местному времени.
pub fn hhmmss(ts: i64) -> String {
    zoned(ts)
        .map(|z| z.strftime("%H:%M:%S").to_string())
        .unwrap_or_default()
}

/// «дд.мм.гггг ЧЧ:ММ:СС» по местному времени.
pub fn full_date(ts: i64) -> String {
    zoned(ts)
        .map(|z| z.strftime("%d.%m.%Y %H:%M:%S").to_string())
        .unwrap_or_default()
}

/// Дата для списка чатов: сегодня — время, в пределах недели — день недели, иначе «дд.мм.гг».
pub fn dialog_date(ts: i64, now: i64) -> String {
    let (Some(z), Some(n)) = (zoned(ts), zoned(now)) else {
        return String::new();
    };
    if z.date() == n.date() {
        return z.strftime("%H:%M").to_string();
    }
    let days = (n.date() - z.date()).get_days();
    if (0..7).contains(&days) {
        return WEEKDAYS[(z.weekday().to_monday_zero_offset()) as usize].to_owned();
    }
    z.strftime("%d.%m.%y").to_string()
}

/// Подпись разделителя дней в ленте: «21 ноября» (с годом для прошлых лет).
pub fn day_label(ts: i64, now: i64) -> String {
    let (Some(z), Some(n)) = (zoned(ts), zoned(now)) else {
        return String::new();
    };
    let month = MONTHS[(z.month() - 1) as usize];
    if z.year() == n.year() {
        format!("{} {month}", z.day())
    } else {
        format!("{} {month} {}", z.day(), z.year())
    }
}

/// Маскирует номер телефона, @имя или ID в режиме стримера (`ayu.streamer_mode`).
pub fn mask_streamer(value: &str, active: bool) -> String {
    if !active || value.is_empty() {
        return value.to_owned();
    }
    if value.starts_with('+') {
        "+• ••• ••• •• ••".to_owned()
    } else if value.starts_with('@') {
        "@••••••".to_owned()
    } else {
        "••••••".to_owned()
    }
}

/// Тот же ли календарный день (для разделителей).
pub fn same_day(a: i64, b: i64) -> bool {
    matches!((zoned(a), zoned(b)), (Some(x), Some(y)) if x.date() == y.date())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palette_and_orange_buttons() {
        for c in [ACCENT, BUTTON, UNREAD, DIALOG_ACTIVE, MSG_OUT_BG, OUT_TICK, WINDOW_BG, CHAT_BG] {
            assert!(c.r().abs_diff(c.b()) <= 8 && c.g().abs_diff(c.b()) <= 8, "{c:?}");
        }
        assert!(ORANGE.r() > ORANGE.g() && ORANGE.g() > ORANGE.b());
        let g = navy(0x5288c1);
        assert!(g.r() == g.g() && g.g() == g.b());
    }

    #[test]
    fn opacity_scales_backgrounds_and_buttons() {
        set_opacity(100);
        assert_eq!(t(WINDOW_BG).a(), 255);
        assert_eq!(t_btn(ORANGE).a(), 255);
        set_opacity(50);
        assert_eq!(t(WINDOW_BG).a(), 127);
        assert_eq!(t_btn(ORANGE).a(), 127);
        assert!(t2(WINDOW_BG).a() < 200);
        set_opacity(200);
        assert_eq!(t(WINDOW_BG).a(), 255);
        set_opacity(DEFAULT_OPACITY_PCT);
    }

    #[test]
    fn avatar_colors_are_stable_greys() {
        assert_eq!(avatar_color(5), avatar_color(5));
        assert_eq!(avatar_color(-5), avatar_color(5));
        for id in 0..7 {
            let c = avatar_color(id);
            assert!(c.b().abs_diff(c.r()) <= 4, "{c:?}");
        }
    }

    #[test]
    fn time_format_is_hh_mm() {
        let s = hhmm(1_700_000_000);
        assert_eq!(s.len(), 5);
        assert_eq!(&s[2..3], ":");
    }

    #[test]
    fn dates_in_chat_list() {
        let now = 1_763_700_000;
        assert_eq!(dialog_date(now - 60, now).len(), 5); // время
        assert_eq!(dialog_date(now - 3 * 86_400, now).chars().count(), 2); // день недели
        assert_eq!(dialog_date(now - 40 * 86_400, now).len(), 8); // дд.мм.гг
        assert!(same_day(now, now + 10));
        assert!(!same_day(now, now + 86_400 * 2));
        assert!(day_label(now, now).contains(' '));
        assert_eq!(mask_streamer("+79991234567", true), "+• ••• ••• •• ••");
        assert_eq!(mask_streamer("@durov", true), "@••••••");
        assert_eq!(mask_streamer("+79991234567", false), "+79991234567");
    }

    #[test]
    fn custom_theme_palette_updates_live_colors_and_resets() {
        set_opacity(100);
        let custom = rg_settings::CustomThemeSettings {
            accent_hex: "#3390EC".to_owned(),
            window_bg_hex: "#17212B".to_owned(),
            ..Default::default()
        };
        sync_custom_theme(&custom);
        assert_eq!(c_orange(), Color32::from_rgb(0x33, 0x90, 0xEC));
        assert_eq!(t(WINDOW_BG), Color32::from_rgb(0x17, 0x21, 0x2B));
        sync_custom_theme(&rg_settings::CustomThemeSettings::default());
        assert_eq!(c_orange(), ORANGE);
        assert_eq!(t(WINDOW_BG), WINDOW_BG);
        set_opacity(DEFAULT_OPACITY_PCT);
    }

    #[test]
    fn default_opacity_is_70_and_spring_curves_behave() {
        assert_eq!(DEFAULT_OPACITY_PCT, 70);
        assert_eq!(ios_spring_bounce(0.0), 0.0);
        assert_eq!(ios_spring_bounce(1.0), 1.0);
        assert!(ios_spring_bounce(0.5) > 0.9);
        assert!(spring_scale(1.0, 0.0) > 1.05);
        assert!(spring_scale(0.0, 1.0) < 1.0);
    }

    #[test]
    fn render_qr_image_produces_crisp_512_code_with_white_quiet_zone() {
        let img = render_qr_image("tg://login?token=test123", Color32::from_rgb(42, 171, 238))
            .expect("qr image must render");
        assert_eq!(img.size, [512, 512]);
        assert_eq!(img.pixels[0], Color32::WHITE);
        assert_eq!(img.pixels[512 * 511 + 511], Color32::WHITE);
        let center_px = img.pixels[256 * 512 + 256];
        assert_eq!(center_px, Color32::from_rgb(42, 171, 238));
    }
}
