//! Оформление Rustgramm: чёрно-белое, в духе LLM Launcher — одна «бумага», один «шрифт» чернил
//! и ступени прозрачности. Цвета лишь по делу: красный для ошибок и удаления.
//! Крупные фоны проходят через `t()` и учитывают прозрачность окна (86% по умолчанию).

use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};

use eframe::egui::{
    self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Sense, Stroke, Vec2,
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

// ---- прозрачность окна
/// Непрозрачность фонов, 0..=255 (86% ≈ 219).
static ALPHA: AtomicU8 = AtomicU8::new(219);
/// Процент непрозрачности по умолчанию.
pub const DEFAULT_OPACITY_PCT: u8 = 86;

/// Задать непрозрачность фонов в процентах (15..=100).
pub fn set_opacity(pct: u8) {
    let pct = pct.clamp(15, 100);
    ALPHA.store((u16::from(pct) * 255 / 100) as u8, Ordering::Relaxed);
}

/// Единая подложка окна: рисуется один раз под всеми панелями, чтобы на стыках не было швов.
pub const BACKDROP: Color32 = Color32::from_rgb(10, 10, 12);

/// Лёгкий светлый налёт поверх подложки: панели списка, шапки, поле ввода.
pub fn tint_panel() -> Color32 {
    Color32::from_white_alpha(7)
}

/// Тёмный налёт ленты сообщений.
pub fn tint_chat() -> Color32 {
    Color32::from_black_alpha(70)
}

/// Налёт заголовка окна.
pub fn tint_title() -> Color32 {
    Color32::from_white_alpha(11)
}

/// Фон с учётом прозрачности окна.
pub fn t(c: Color32) -> Color32 {
    with_alpha(c, ALPHA.load(Ordering::Relaxed))
}

/// Фон окон и меню: чуть плотнее основного, чтобы текст читался при любой прозрачности.
pub fn t2(c: Color32) -> Color32 {
    with_alpha(c, ALPHA.load(Ordering::Relaxed).max(236))
}

fn with_alpha(c: Color32, a: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), a)
}

/// Плавный переход к `target` за `dur` секунд (мгновенно при «уменьшении движения»).
pub fn tween(ctx: &egui::Context, id: impl std::hash::Hash + std::fmt::Debug, target: f32, dur: f32) -> f32 {
    let id = egui::Id::NULL.with(id);
    ctx.animate_value_with_time(id, target, dur)
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

/// Системные шрифты Windows (Segoe UI, Consolas) плюс запасные символьные для значков.
fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    if let Some(f) = read_face("C:\\Windows\\Fonts\\segoeui.ttf") {
        fonts.font_data.insert("segoe".into(), f);
        fonts
            .families
            .entry(FontFamily::Proportional)
            .or_default()
            .insert(0, "segoe".into());
    }
    let bold = read_face("C:\\Windows\\Fonts\\segoeuib.ttf");
    let display = fonts
        .families
        .entry(FontFamily::Name(FAM_DISPLAY.into()))
        .or_default();
    if let Some(f) = bold {
        fonts.font_data.insert("segoe-bold".into(), f);
        display.push("segoe-bold".into());
    }
    display.push("Ubuntu-Light".into());
    if let Some(f) = read_face("C:\\Windows\\Fonts\\consola.ttf") {
        fonts.font_data.insert("consola".into(), f);
        fonts
            .families
            .entry(FontFamily::Monospace)
            .or_default()
            .insert(0, "consola".into());
    }
    for (name, path) in [
        ("segoe-emoji", "C:\\Windows\\Fonts\\seguiemj.ttf"),
        ("segoe-symbols", "C:\\Windows\\Fonts\\seguisym.ttf"),
        ("noto-symbols", "/system/fonts/NotoSansSymbols-Regular-Subsetted.ttf"),
    ] {
        if let Ok(bytes) = std::fs::read(path) {
            fonts
                .font_data
                .insert(name.to_owned(), Arc::new(FontData::from_owned(bytes)));
            for family in [FontFamily::Proportional, FontFamily::Monospace] {
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
    jiff::Timestamp::from_second(ts)
        .ok()
        .map(|t| t.to_zoned(jiff::tz::TimeZone::system()))
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

/// Тот же ли календарный день (для разделителей).
pub fn same_day(a: i64, b: i64) -> bool {
    matches!((zoned(a), zoned(b)), (Some(x), Some(y)) if x.date() == y.date())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palette_is_monochrome() {
        for c in [ACCENT, BUTTON, UNREAD, DIALOG_ACTIVE, MSG_OUT_BG, OUT_TICK, WINDOW_BG, CHAT_BG] {
            assert!(c.r().abs_diff(c.b()) <= 8 && c.g().abs_diff(c.b()) <= 8, "{c:?}");
        }
        let g = navy(0x5288c1);
        assert!(g.r() == g.g() && g.g() == g.b());
    }

    #[test]
    fn opacity_scales_backgrounds() {
        set_opacity(86);
        assert_eq!(t(WINDOW_BG).a(), 219);
        set_opacity(30);
        assert_eq!(t(WINDOW_BG).a(), 76);
        assert!(t2(WINDOW_BG).a() >= 236);
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
    }
}
