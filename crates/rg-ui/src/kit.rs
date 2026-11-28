//! Элементы настроек Telegram Desktop: заголовок раздела, строка-переключатель, пояснение,
//! строка со значением, разделитель. Размеры сняты с окна AyuGram (строка 40, отступ 20).

use std::sync::atomic::{AtomicBool, Ordering};

use eframe::egui::{self, Color32, CornerRadius, FontId, Pos2, Rect, Sense, Vec2};

use super::theme::*;

static MD3_SWITCHES: AtomicBool = AtomicBool::new(true);
static HIDE_DIVIDERS: AtomicBool = AtomicBool::new(false);

/// Синхронизирует глобальный стиль переключателей и разделителей настроек.
pub fn sync_prefs(md3: bool, disable_dividers: bool) {
    MD3_SWITCHES.store(md3, Ordering::Relaxed);
    HIDE_DIVIDERS.store(disable_dividers, Ordering::Relaxed);
}

pub const ROW_H: f32 = 40.0;
pub const PAD: f32 = 20.0;
/// Фон поясняющего текста (чуть светлее окна).
pub const DESC_BG: Color32 = navy(0x1c2733);
const TRACK_OFF: Color32 = navy(0x4b5866);

/// Полоса-разделитель между группами (как `AddSkip` + `AddDivider`).
pub fn divider(ui: &mut egui::Ui) {
    if HIDE_DIVIDERS.load(Ordering::Relaxed) {
        ui.add_space(2.0);
        return;
    }
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 8.0), Sense::hover());
    ui.painter().rect_filled(rect, 0.0, t(SIDEBAR_BG));
}

/// Синий заголовок раздела.
pub fn section(ui: &mut egui::Ui, title: &str) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 34.0), Sense::hover());
    let clip = Rect::from_min_max(rect.min, Pos2::new((rect.right() - PAD).max(rect.left() + 10.0), rect.bottom()));
    ui.painter().with_clip_rect(clip).text(
        Pos2::new(rect.left() + PAD, rect.bottom() - 8.0),
        egui::Align2::LEFT_BOTTOM,
        title,
        display(13.0),
        DIALOG_DATE,
    );
}

/// Серый поясняющий текст на светлой полосе.
pub fn desc(ui: &mut egui::Ui, text: &str) {
    let w = ui.available_width();
    let galley = ui.painter().layout(
        text.to_owned(),
        FontId::proportional(13.0),
        SUBTEXT,
        w - 2.0 * PAD,
    );
    let (rect, _) = ui.allocate_exact_size(Vec2::new(w, galley.size().y + 20.0), Sense::hover());
    ui.painter().rect_filled(rect, 0.0, t(DESC_BG));
    ui.painter()
        .galley(Pos2::new(rect.left() + PAD, rect.top() + 10.0), galley, SUBTEXT);
}

fn row_bg(ui: &egui::Ui, rect: Rect, resp: &egui::Response) {
    if resp.hovered() {
        ui.painter().rect_filled(rect, 0.0, t(WINDOW_OVER));
    }
}

/// Переключатель Telegram / iOS Liquid Glass (`material_switches`, эластичная капля при сдвиге).
fn switch(ui: &egui::Ui, center: Pos2, on: bool, key: &str) {
    let lin = tween(ui.ctx(), ("sw", key, center.y as i32), f32::from(u8::from(on)), 0.16);
    let k = if on { ios_spring(lin) } else { 1.0 - ios_spring(1.0 - lin) };
    let stretch = (1.0 - (2.0 * k - 1.0).abs()).max(0.0);
    if MD3_SWITCHES.load(Ordering::Relaxed) {
        let track = Rect::from_center_size(center, Vec2::new(38.0, 20.0));
        let off_fill = t_btn(TRACK_OFF).gamma_multiply(0.45);
        let fill = lerp_color(off_fill, t_btn(ORANGE), lin);
        ui.painter().rect_filled(track, CornerRadius::same(10), fill);
        if lin < 0.99 {
            ui.painter().rect_stroke(
                track,
                CornerRadius::same(10),
                egui::Stroke::new(1.5, t_btn(TRACK_OFF).gamma_multiply(1.0 - lin)),
                egui::StrokeKind::Inside,
            );
        }
        let radius = 5.5 + 1.5 * k;
        let knob_x = track.left() + 10.0 + k * (track.width() - 20.0);
        let kw = radius * 2.0 + 4.8 * stretch;
        let kh = radius * 2.0 - 0.8 * stretch;
        let knob_r = Rect::from_center_size(Pos2::new(knob_x, center.y), Vec2::new(kw, kh));
        ui.painter()
            .rect_filled(knob_r, CornerRadius::same((kh * 0.5).round() as u8), Color32::WHITE);
    } else {
        let track = Rect::from_center_size(center, Vec2::new(34.0, 14.0));
        let fill = lerp_color(t_btn(TRACK_OFF), t_btn(ORANGE), lin);
        ui.painter().rect_filled(track, CornerRadius::same(7), fill);
        let knob_x = track.left() + 7.0 + k * (track.width() - 14.0);
        let kw = 12.0 + 3.6 * stretch;
        let knob_r = Rect::from_center_size(Pos2::new(knob_x, center.y), Vec2::new(kw, 12.0));
        ui.painter().rect_filled(knob_r, CornerRadius::same(6), Color32::WHITE);
    }
}

/// Строка с переключателем; возвращает `true`, если значение изменилось.
pub fn toggle(ui: &mut egui::Ui, label: &str, value: &mut bool) -> bool {
    toggle_ex(ui, label, None, false, value)
}

/// Строка с переключателем и линейным значком слева (пункты левой панели).
pub fn toggle_icon(ui: &mut egui::Ui, icon: super::icons::Ico, label: &str, value: &mut bool) -> bool {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), ROW_H), Sense::click());
    row_bg(ui, rect, &resp);
    let p = ui.painter();
    super::icons::ico(p, Pos2::new(rect.left() + 30.0, rect.center().y), icon, DIALOG_DATE);
    let text_clip = Rect::from_min_max(
        Pos2::new(rect.left() + 60.0, rect.top()),
        Pos2::new((rect.right() - PAD - 44.0).max(rect.left() + 64.0), rect.bottom()),
    );
    p.with_clip_rect(text_clip).text(
        Pos2::new(rect.left() + 64.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        FontId::proportional(14.0),
        WHITE,
    );
    switch(ui, Pos2::new(rect.right() - PAD - 17.0, rect.center().y), *value, label);
    if resp.clicked() {
        *value = !*value;
        return true;
    }
    false
}

/// Строка с переключателем, бейджем BETA и счётчиком «n/m».
pub fn toggle_ex(
    ui: &mut egui::Ui,
    label: &str,
    counter: Option<(usize, usize, bool)>,
    beta: bool,
    value: &mut bool,
) -> bool {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), ROW_H), Sense::click());
    row_bg(ui, rect, &resp);
    let p = ui.painter();
    let max_x = (rect.right() - PAD - 44.0).max(rect.left() + PAD + 20.0);
    let clip = Rect::from_min_max(Pos2::new(rect.left() + PAD, rect.top()), Pos2::new(max_x, rect.bottom()));
    let cp = p.with_clip_rect(clip);
    let galley = cp.layout_no_wrap(label.to_owned(), FontId::proportional(14.0), WHITE);
    let mut x = rect.left() + PAD;
    cp.galley(Pos2::new(x, rect.center().y - galley.size().y / 2.0), galley.clone(), WHITE);
    x += galley.size().x + 6.0;
    if beta {
        badge(&cp, Pos2::new(x, rect.center().y), "BETA");
        x += 38.0;
    }
    if let Some((n, m, open)) = counter {
        let label_w = cp
            .layout_no_wrap(format!("{n}/{m}"), FontId::proportional(13.0), WHITE)
            .size()
            .x;
        cp.text(
            Pos2::new(x + 2.0, rect.center().y),
            egui::Align2::LEFT_CENTER,
            format!("{n}/{m}"),
            FontId::proportional(13.0),
            WHITE,
        );
        super::icons::chevron(&cp, Pos2::new(x + label_w + 14.0, rect.center().y), open, WHITE);
    }
    switch(ui, Pos2::new(rect.right() - PAD - 17.0, rect.center().y), *value, label);
    if resp.clicked() {
        *value = !*value;
        return true;
    }
    false
}

fn badge(p: &egui::Painter, left_center: Pos2, text: &str) {
    let r = Rect::from_min_size(left_center - Vec2::new(0.0, 7.0), Vec2::new(32.0, 14.0));
    p.rect_filled(r, CornerRadius::same(3), ACCENT.gamma_multiply(0.55));
    p.text(
        r.center(),
        egui::Align2::CENTER_CENTER,
        text,
        FontId::proportional(9.0),
        WHITE,
    );
}

/// Строка «название — значение (синим)»; клик возвращает `true`.
pub fn value_row(ui: &mut egui::Ui, label: &str, value: &str, beta: bool) -> bool {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), ROW_H), Sense::click());
    row_bg(ui, rect, &resp);
    let p = ui.painter();
    let vg = p.layout_no_wrap(value.to_owned(), FontId::proportional(14.0), ACCENT);
    let right_limit = (rect.right() - PAD - vg.size().x - 10.0).max(rect.left() + PAD + 20.0);
    let clip = Rect::from_min_max(Pos2::new(rect.left() + PAD, rect.top()), Pos2::new(right_limit, rect.bottom()));
    let cp = p.with_clip_rect(clip);
    let g = cp.layout_no_wrap(label.to_owned(), FontId::proportional(14.0), WHITE);
    cp.galley(
        Pos2::new(rect.left() + PAD, rect.center().y - g.size().y / 2.0),
        g.clone(),
        WHITE,
    );
    if beta {
        badge(&cp, Pos2::new(rect.left() + PAD + g.size().x + 6.0, rect.center().y), "BETA");
    }
    p.galley(
        Pos2::new(rect.right() - PAD - vg.size().x, rect.center().y - vg.size().y / 2.0),
        vg,
        ACCENT,
    );
    resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
}

/// Строка перехода: значок, название, при необходимости значение справа.
pub fn nav_row(ui: &mut egui::Ui, icon: &str, label: &str, value: &str) -> bool {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), ROW_H), Sense::click());
    row_bg(ui, rect, &resp);
    let p = ui.painter();
    let mut x = rect.left() + PAD;
    if !icon.is_empty() {
        super::icons::any(p, icon, Pos2::new(x + 12.0, rect.center().y), SUBTEXT);
        x += 44.0;
    }
    let val_w = if value.is_empty() {
        0.0
    } else {
        let vg = p.layout_no_wrap(value.to_owned(), FontId::proportional(14.0), ACCENT);
        let w = vg.size().x;
        p.galley(
            Pos2::new(rect.right() - PAD - w, rect.center().y - vg.size().y / 2.0),
            vg,
            ACCENT,
        );
        w + 10.0
    };
    let right_limit = (rect.right() - PAD - val_w).max(x + 16.0);
    let clip = Rect::from_min_max(Pos2::new(x, rect.top()), Pos2::new(right_limit, rect.bottom()));
    p.with_clip_rect(clip).text(
        Pos2::new(x, rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        FontId::proportional(14.0),
        WHITE,
    );
    resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
}

/// Слайдер Telegram: тонкая линия, круглая ручка, значение справа.
pub fn slider(ui: &mut egui::Ui, label: &str, value: &mut i32, range: (i32, i32), shown: String) -> bool {
    ui.add_space(6.0);
    let (lrect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 22.0), Sense::hover());
    ui.painter().text(
        Pos2::new(lrect.left() + PAD, lrect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        FontId::proportional(14.0),
        WHITE,
    );
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 30.0), Sense::click_and_drag());
    let track = Rect::from_min_max(
        Pos2::new(rect.left() + PAD, rect.center().y - 1.5),
        Pos2::new(rect.right() - PAD - 40.0, rect.center().y + 1.5),
    );
    let mut changed = false;
    if let Some(pos) = resp.interact_pointer_pos()
        && (resp.dragged() || resp.clicked())
    {
        let t = ((pos.x - track.left()) / track.width()).clamp(0.0, 1.0);
        let v = range.0 + (t * (range.1 - range.0) as f32).round() as i32;
        if v != *value {
            *value = v;
            changed = true;
        }
    }
    let t = (*value - range.0) as f32 / (range.1 - range.0).max(1) as f32;
    let knob_x = track.left() + t * track.width();
    let p = ui.painter();
    p.rect_filled(track, CornerRadius::same(2), t_btn(TRACK_OFF));
    p.rect_filled(
        Rect::from_min_max(track.min, Pos2::new(knob_x, track.max.y)),
        CornerRadius::same(2),
        t_btn(ORANGE),
    );
    p.circle_filled(Pos2::new(knob_x, track.center().y), 7.0, t_btn(ORANGE));
    p.text(
        Pos2::new(rect.right() - PAD, rect.center().y),
        egui::Align2::RIGHT_CENTER,
        shown,
        FontId::proportional(13.0),
        ORANGE,
    );
    changed
}

/// Подэлемент раскрывающегося списка (отступ вправо).
pub fn sub_toggle(ui: &mut egui::Ui, label: &str, value: &mut bool) -> bool {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 34.0), Sense::click());
    row_bg(ui, rect, &resp);
    let p = ui.painter();
    let clip = Rect::from_min_max(
        Pos2::new(rect.left() + PAD + 14.0, rect.top()),
        Pos2::new((rect.right() - PAD - 44.0).max(rect.left() + PAD + 24.0), rect.bottom()),
    );
    p.with_clip_rect(clip).text(
        Pos2::new(rect.left() + PAD + 14.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        FontId::proportional(13.0),
        DIALOG_TEXT,
    );
    switch(ui, Pos2::new(rect.right() - PAD - 17.0, rect.center().y), *value, label);
    if resp.clicked() {
        *value = !*value;
        return true;
    }
    false
}

pub const MENU_ITEM_H: f32 = 28.0;
pub const MENU_MIN_W: f32 = 176.0;

/// Рамка компактного всплывающего меню Telegram.
pub fn compact_popup_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(t2(WINDOW_BG))
        .stroke(egui::Stroke::new(1.0, Color32::from_white_alpha(24)))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(egui::Margin::symmetric(3, 4))
}

/// Компактный разделитель внутри контекстного меню.
pub fn menu_separator(ui: &mut egui::Ui) {
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(Vec2::new(w, 5.0), Sense::hover());
    ui.painter().line_segment(
        [
            Pos2::new(rect.left() + 8.0, rect.center().y),
            Pos2::new((rect.right() - 8.0).max(rect.left() + 8.0), rect.center().y),
        ],
        egui::Stroke::new(1.0, Color32::from_white_alpha(20)),
    );
}

/// Строка контекстного меню Telegram: значок слева, подпись, компактная высота 28 px, красная для опасных.
pub fn menu_item(ui: &mut egui::Ui, icon: &str, label: &str, danger: bool) -> bool {
    let w = ui.available_width().max(MENU_MIN_W);
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, MENU_ITEM_H), Sense::click());
    let p = ui.painter();
    if resp.hovered() {
        p.rect_filled(rect.shrink2(Vec2::new(4.0, 1.0)), CornerRadius::same(6), t2(WINDOW_OVER));
    }
    let (icon_c, text_c) = if danger { (DANGER, DANGER) } else { (DIALOG_DATE, WHITE) };
    super::icons::tg_sized(p, icon, Pos2::new(rect.left() + 18.0, rect.center().y), Vec2::splat(15.0), icon_c);
    let clip = Rect::from_min_max(
        Pos2::new(rect.left() + 36.0, rect.top()),
        Pos2::new((rect.right() - 10.0).max(rect.left() + 44.0), rect.bottom()),
    );
    p.with_clip_rect(clip).text(
        Pos2::new(rect.left() + 36.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        FontId::proportional(13.0),
        text_c,
    );
    resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sync_prefs_updates_switch_and_divider_flags() {
        sync_prefs(true, true);
        assert!(MD3_SWITCHES.load(Ordering::Relaxed));
        assert!(HIDE_DIVIDERS.load(Ordering::Relaxed));
        sync_prefs(false, false);
        assert!(!MD3_SWITCHES.load(Ordering::Relaxed));
        assert!(!HIDE_DIVIDERS.load(Ordering::Relaxed));
    }

    #[test]
    fn compact_menu_constants_and_frame_stay_tight() {
        const {
            assert!(MENU_ITEM_H <= 28.0);
            assert!(MENU_MIN_W <= 180.0);
        }
        let frame = compact_popup_frame();
        assert_eq!(frame.inner_margin, egui::Margin::symmetric(3, 4));
        assert_eq!(frame.corner_radius, CornerRadius::same(10));
        assert_eq!(frame.stroke.width, 1.0);
    }
}
