//! Элементы настроек Telegram Desktop: заголовок раздела, строка-переключатель, пояснение,
//! строка со значением, разделитель. Размеры сняты с окна AyuGram (строка 40, отступ 20).

use eframe::egui::{self, Color32, CornerRadius, FontId, Pos2, Rect, Sense, Vec2};

use super::theme::*;

pub const ROW_H: f32 = 40.0;
pub const PAD: f32 = 20.0;
/// Фон поясняющего текста (чуть светлее окна).
pub const DESC_BG: Color32 = navy(0x1c2733);
const TRACK_OFF: Color32 = navy(0x4b5866);

/// Полоса-разделитель между группами (как `AddSkip` + `AddDivider`).
pub fn divider(ui: &mut egui::Ui) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 8.0), Sense::hover());
    ui.painter().rect_filled(rect, 0.0, SIDEBAR_BG);
}

/// Синий заголовок раздела.
pub fn section(ui: &mut egui::Ui, title: &str) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 34.0), Sense::hover());
    ui.painter().text(
        Pos2::new(rect.left() + PAD, rect.bottom() - 8.0),
        egui::Align2::LEFT_BOTTOM,
        title,
        FontId::proportional(14.0),
        ACCENT,
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
    ui.painter().rect_filled(rect, 0.0, DESC_BG);
    ui.painter()
        .galley(Pos2::new(rect.left() + PAD, rect.top() + 10.0), galley, SUBTEXT);
}

fn row_bg(ui: &egui::Ui, rect: Rect, resp: &egui::Response) {
    if resp.hovered() {
        ui.painter().rect_filled(rect, 0.0, WINDOW_OVER);
    }
}

/// Переключатель Telegram: узкая «таблетка» и круглая ручка.
fn switch(ui: &egui::Ui, center: Pos2, on: bool) {
    let track = Rect::from_center_size(center, Vec2::new(34.0, 14.0));
    let (fill, knob_x) = if on {
        (ACCENT, track.right() - 1.0)
    } else {
        (TRACK_OFF, track.left() + 1.0)
    };
    ui.painter().rect_filled(track, CornerRadius::same(7), fill);
    let knob = Pos2::new(
        if on { knob_x - 6.0 } else { knob_x + 6.0 },
        center.y,
    );
    ui.painter().circle_filled(knob, 6.0, WINDOW_BG);
}

/// Строка с переключателем; возвращает `true`, если значение изменилось.
pub fn toggle(ui: &mut egui::Ui, label: &str, value: &mut bool) -> bool {
    toggle_ex(ui, label, None, false, value)
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
    let galley = p.layout_no_wrap(label.to_owned(), FontId::proportional(14.0), WHITE);
    let mut x = rect.left() + PAD;
    p.galley(Pos2::new(x, rect.center().y - galley.size().y / 2.0), galley.clone(), WHITE);
    x += galley.size().x + 6.0;
    if beta {
        badge(p, Pos2::new(x, rect.center().y), "BETA");
        x += 38.0;
    }
    if let Some((n, m, open)) = counter {
        p.text(
            Pos2::new(x + 2.0, rect.center().y),
            egui::Align2::LEFT_CENTER,
            format!("{n}/{m} {}", if open { "⌃" } else { "⌄" }),
            FontId::proportional(13.0),
            WHITE,
        );
    }
    switch(ui, Pos2::new(rect.right() - PAD - 17.0, rect.center().y), *value);
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
    let g = p.layout_no_wrap(label.to_owned(), FontId::proportional(14.0), WHITE);
    p.galley(
        Pos2::new(rect.left() + PAD, rect.center().y - g.size().y / 2.0),
        g.clone(),
        WHITE,
    );
    if beta {
        badge(p, Pos2::new(rect.left() + PAD + g.size().x + 6.0, rect.center().y), "BETA");
    }
    p.text(
        Pos2::new(rect.right() - PAD, rect.center().y),
        egui::Align2::RIGHT_CENTER,
        value,
        FontId::proportional(14.0),
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
        p.text(
            Pos2::new(x + 8.0, rect.center().y),
            egui::Align2::CENTER_CENTER,
            icon,
            FontId::proportional(16.0),
            SUBTEXT,
        );
        x += 38.0;
    }
    p.text(
        Pos2::new(x, rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        FontId::proportional(14.0),
        WHITE,
    );
    if !value.is_empty() {
        p.text(
            Pos2::new(rect.right() - PAD, rect.center().y),
            egui::Align2::RIGHT_CENTER,
            value,
            FontId::proportional(14.0),
            ACCENT,
        );
    }
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
    p.rect_filled(track, CornerRadius::same(2), TRACK_OFF);
    p.rect_filled(
        Rect::from_min_max(track.min, Pos2::new(knob_x, track.max.y)),
        CornerRadius::same(2),
        ACCENT,
    );
    p.circle_filled(Pos2::new(knob_x, track.center().y), 7.0, ACCENT);
    p.text(
        Pos2::new(rect.right() - PAD, rect.center().y),
        egui::Align2::RIGHT_CENTER,
        shown,
        FontId::proportional(13.0),
        ACCENT,
    );
    changed
}

/// Подэлемент раскрывающегося списка (отступ вправо).
pub fn sub_toggle(ui: &mut egui::Ui, label: &str, value: &mut bool) -> bool {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 34.0), Sense::click());
    row_bg(ui, rect, &resp);
    let p = ui.painter();
    p.text(
        Pos2::new(rect.left() + PAD + 14.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        FontId::proportional(13.0),
        DIALOG_TEXT,
    );
    switch(ui, Pos2::new(rect.right() - PAD - 17.0, rect.center().y), *value);
    if resp.clicked() {
        *value = !*value;
        return true;
    }
    false
}
