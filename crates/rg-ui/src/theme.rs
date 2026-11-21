//! Оформление как у Telegram Desktop (ночная тема): фоны тёмно-синие (`navy`), а все синие
//! акценты — кнопки, выделение, бейджи, исходящие пузыри, ссылки — оранжевые (`tg`).
//! Цвета взяты из `night.tdesktop-theme` AyuGram v7.2.9; акцент меняется перестановкой каналов R↔B:
//! оттенок поворачивается, яркость сохраняется.

use std::sync::Arc;

use eframe::egui::{
    self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Sense, Stroke, Vec2,
};

/// Цвет из палитры Telegram `0xRRGGBB` как есть: тёмно-синие фоны и серо-голубой текст ночной темы.
pub const fn navy(hex: u32) -> Color32 {
    Color32::from_rgb(((hex >> 16) & 0xff) as u8, ((hex >> 8) & 0xff) as u8, (hex & 0xff) as u8)
}

/// Акцентный цвет из палитры Telegram `0xRRGGBB` с синим → оранжевым.
pub const fn tg(hex: u32) -> Color32 {
    let r = ((hex >> 16) & 0xff) as u8;
    let g = ((hex >> 8) & 0xff) as u8;
    let b = (hex & 0xff) as u8;
    Color32::from_rgb(b, g, r)
}

// windowBg, windowBgOver, windowSubTextFg
pub const WINDOW_BG: Color32 = navy(0x17212b);
pub const WINDOW_OVER: Color32 = navy(0x232e3c);
pub const SUBTEXT: Color32 = navy(0x708499);
// windowBgActive (акцент), activeButtonBg
pub const ACCENT: Color32 = tg(0x5288c1);
pub const BUTTON: Color32 = tg(0x2f6ea5);
pub const BUTTON_OVER: Color32 = tg(0x3476ab);
pub const WHITE: Color32 = Color32::from_rgb(245, 245, 245);
// chat list
pub const DIALOG_OVER: Color32 = navy(0x202b36);
pub const DIALOG_ACTIVE: Color32 = tg(0x2b5278);
pub const DIALOG_DATE: Color32 = navy(0x8696a8);
pub const DIALOG_TEXT: Color32 = navy(0x7f91a4);
pub const UNREAD: Color32 = tg(0x4082bc);
pub const UNREAD_MUTED: Color32 = navy(0x3e546a);
pub const FILTER_INPUT: Color32 = navy(0x242f3d);
pub const TITLE_BG: Color32 = navy(0x242f3d);
pub const SIDEBAR_BG: Color32 = navy(0x0e1621);
// history
pub const CHAT_BG: Color32 = navy(0x0e1621);
pub const MSG_IN_BG: Color32 = navy(0x182533);
pub const MSG_OUT_BG: Color32 = tg(0x2b5278);
pub const MSG_IN_DATE: Color32 = navy(0x6d7f8f);
pub const MSG_OUT_DATE: Color32 = tg(0x7da8d3);
pub const MSG_OUT_TEXT: Color32 = tg(0xe4ecf2);
pub const OUT_TICK: Color32 = tg(0x6bbfff);
pub const SERVICE_BG: Color32 = Color32::from_rgba_premultiplied(28, 40, 53, 0xd5);
pub const COMPOSE_BG: Color32 = navy(0x17212b);
pub const DANGER: Color32 = Color32::from_rgb(230, 80, 70);
pub const ACCENT_TEXT: Color32 = Color32::WHITE;

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

pub fn apply(ctx: &egui::Context) {
    install_fonts(ctx);
    let mut v = egui::Visuals::dark();
    v.panel_fill = WINDOW_BG;
    v.window_fill = WINDOW_BG;
    v.extreme_bg_color = FILTER_INPUT;
    v.faint_bg_color = WINDOW_OVER;
    v.hyperlink_color = ACCENT;
    v.selection.bg_fill = DIALOG_ACTIVE;
    v.selection.stroke = Stroke::new(1.0, WHITE);
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, WHITE);
    v.widgets.inactive.fg_stroke = Stroke::new(1.0, WHITE);
    v.widgets.inactive.weak_bg_fill = FILTER_INPUT;
    v.widgets.hovered.weak_bg_fill = WINDOW_OVER;
    v.widgets.hovered.bg_fill = WINDOW_OVER;
    v.widgets.hovered.fg_stroke = Stroke::new(1.0, WHITE);
    v.widgets.active.bg_fill = BUTTON;
    v.widgets.active.weak_bg_fill = BUTTON;
    v.widgets.active.fg_stroke = Stroke::new(1.0, Color32::WHITE);
    let r = CornerRadius::same(10);
    v.widgets.inactive.corner_radius = r;
    v.widgets.hovered.corner_radius = r;
    v.widgets.active.corner_radius = r;
    v.widgets.noninteractive.corner_radius = r;
    v.window_stroke = Stroke::new(1.0, SIDEBAR_BG);
    ctx.set_visuals(v);
    ctx.global_style_mut(|s| {
        s.spacing.item_spacing = Vec2::new(8.0, 4.0);
        s.spacing.button_padding = Vec2::new(14.0, 7.0);
        s.text_styles
            .insert(egui::TextStyle::Body, FontId::proportional(15.0));
        s.text_styles
            .insert(egui::TextStyle::Button, FontId::proportional(15.0));
        s.text_styles
            .insert(egui::TextStyle::Small, FontId::proportional(12.0));
    });
}

/// Системные шрифты: цветных эмодзи egui не рисует, но символы и эмодзи Segoe дают больше значков.
fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
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

/// Цвет аватарки по идентификатору (палитра Telegram без синего).
pub fn avatar_color(id: i64) -> Color32 {
    const PALETTE: [(u8, u8, u8); 7] = [
        (229, 115, 90),
        (242, 160, 60),
        (196, 140, 96),
        (95, 190, 110),
        (222, 110, 160),
        (80, 190, 160),
        (214, 176, 70),
    ];
    let (r, g, b) = PALETTE[(id.unsigned_abs() % PALETTE.len() as u64) as usize];
    Color32::from_rgb(r, g, b)
}

/// Круглая аватарка с первой буквой имени.
pub fn avatar(ui: &mut egui::Ui, name: &str, id: i64, size: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    draw_avatar(ui.painter(), rect.center(), name, id, size);
}

pub fn draw_avatar(painter: &egui::Painter, center: egui::Pos2, name: &str, id: i64, size: f32) {
    painter.circle_filled(center, size / 2.0, avatar_color(id));
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
    fn palette_is_orange_not_blue() {
        // Все акценты тёплые: красного больше, чем синего.
        for c in [ACCENT, BUTTON, UNREAD, DIALOG_ACTIVE, MSG_OUT_BG, OUT_TICK] {
            assert!(c.r() > c.b(), "{c:?}");
        }
        assert_eq!(tg(0x5288c1), Color32::from_rgb(0xc1, 0x88, 0x52));
    }

    #[test]
    fn avatar_colors_are_stable_and_not_blue() {
        assert_eq!(avatar_color(5), avatar_color(5));
        assert_eq!(avatar_color(-5), avatar_color(5));
        for id in 0..7 {
            let c = avatar_color(id);
            assert!(c.b() <= c.r().max(c.g()), "{c:?}");
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
