//! Собственная рамка окна: шапка с названием и кнопками, невидимые края для изменения размера.
//! Системная рамка отключена ради прозрачности (как в LLM Launcher).

use eframe::egui::{
    self, Align2, Color32, CornerRadius, CursorIcon, Pos2, Rect, ResizeDirection, Sense, Stroke, Vec2,
};

use super::{App, theme::*};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Glyph {
    Close,
    Minimize,
    Maximize,
    Restore,
}

/// Рисует значок кнопки окна линиями с учётом пружинного масштаба.
fn paint_glyph(p: &egui::Painter, c: Pos2, g: Glyph, color: Color32, scale: f32) {
    let s = Stroke::new(1.4, color);
    let h = 5.5 * scale;
    match g {
        Glyph::Close => {
            p.line_segment([c + Vec2::new(-h, -h), c + Vec2::new(h, h)], s);
            p.line_segment([c + Vec2::new(-h, h), c + Vec2::new(h, -h)], s);
        }
        Glyph::Minimize => {
            p.line_segment([c + Vec2::new(-h, 0.0), c + Vec2::new(h, 0.0)], s);
        }
        Glyph::Maximize => {
            p.rect_stroke(
                Rect::from_center_size(c, Vec2::splat(h * 2.0 - 1.0)),
                CornerRadius::same(1),
                s,
                egui::StrokeKind::Middle,
            );
        }
        Glyph::Restore => {
            let a = Rect::from_center_size(c + Vec2::new(-1.0, 1.0), Vec2::splat(h * 2.0 - 3.0));
            p.rect_stroke(a, CornerRadius::same(1), s, egui::StrokeKind::Middle);
            p.line_segment([a.right_top() + Vec2::new(2.0, -2.0), a.right_top() + Vec2::new(2.0, -2.0 + 0.1)], s);
            p.line_segment(
                [Pos2::new(a.left() + 2.0, a.top() - 2.0), Pos2::new(a.right() + 2.0, a.top() - 2.0)],
                s,
            );
            p.line_segment(
                [Pos2::new(a.right() + 2.0, a.top() - 2.0), Pos2::new(a.right() + 2.0, a.bottom() - 2.0)],
                s,
            );
        }
    }
}

impl App {
    /// Шапка окна: перетаскивание, двойной клик — развернуть, кнопки справа.
    pub(crate) fn titlebar(&mut self, ui: &mut egui::Ui) {
        let rect = ui.max_rect();
        ui.set_clip_rect(rect);
        let cy = rect.center().y;
        let ctx = ui.ctx().clone();

        let drag = Rect::from_min_max(rect.min, Pos2::new(rect.right() - 138.0, rect.bottom()));
        let r = ui.interact(drag, ui.make_persistent_id("title-drag"), Sense::click_and_drag());
        if r.drag_started() {
            ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }
        let maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
        if r.double_clicked() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
        }

        let p = ui.painter();
        p.text(
            Pos2::new(rect.left() + 16.0, cy),
            Align2::LEFT_CENTER,
            "RUSTGRAMM",
            display(13.0),
            WHITE,
        );
        p.line_segment(
            [Pos2::new(rect.left() + 108.0, cy - 6.0), Pos2::new(rect.left() + 108.0, cy + 6.0)],
            Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 40)),
        );
        p.text(
            Pos2::new(rect.left() + 118.0, cy),
            Align2::LEFT_CENTER,
            format!("v{}", env!("CARGO_PKG_VERSION")),
            egui::FontId::monospace(10.5),
            SUBTEXT,
        );

        let w = 44.0;
        let close = Rect::from_min_max(Pos2::new(rect.right() - w, rect.top()), rect.right_bottom());
        let maxi = close.translate(Vec2::new(-w, 0.0));
        let mini = maxi.translate(Vec2::new(-w, 0.0));
        self.window_button(ui, close, Glyph::Close);
        self.window_button(ui, maxi, if maximized { Glyph::Restore } else { Glyph::Maximize });
        self.window_button(ui, mini, Glyph::Minimize);
    }

    fn window_button(&mut self, ui: &mut egui::Ui, rect: Rect, g: Glyph) {
        let id = ui.make_persistent_id(("winbtn", g as u8));
        let resp = ui.interact(rect, id, Sense::click());
        let hover = tween(ui.ctx(), ("wb", g as u8), f32::from(u8::from(resp.hovered())), 0.12);
        if hover > 0.01 {
            let op = opacity_f32();
            let fill = if g == Glyph::Close {
                Color32::from_rgba_unmultiplied(242, 108, 108, (220.0 * hover * op) as u8)
            } else {
                Color32::from_rgba_unmultiplied(255, 255, 255, (28.0 * hover * op) as u8)
            };
            ui.painter().rect_filled(rect.shrink(4.0), CornerRadius::same(7), fill);
        }
        let color = if g == Glyph::Close && hover > 0.5 {
            Color32::WHITE
        } else {
            lerp_color(DIALOG_DATE, WHITE, hover)
        };
        let active = tween(ui.ctx(), ("wba", g as u8), f32::from(u8::from(resp.is_pointer_button_down_on())), 0.08);
        paint_glyph(ui.painter(), rect.center(), g, color, spring_scale(hover, active));
        if resp.clicked() {
            let ctx = ui.ctx();
            match g {
                Glyph::Close => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
                Glyph::Minimize => {
                    if self.settings.work_mode == rg_settings::WorkMode::TrayOnly {
                        self.hide_window_to_tray();
                    } else {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                    }
                }
                _ => {
                    let maxed = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maxed));
                }
            }
        }
    }

    /// Без системной рамки края окна невидимы: ловим их сами и просим систему менять размер.
    pub(crate) fn resize_edges(&mut self, ui: &mut egui::Ui) {
        let rect = ui.ctx().content_rect();
        if ui.ctx().input(|i| i.viewport().maximized.unwrap_or(false)) {
            return;
        }
        const T: f32 = 6.0;
        let zones: [(ResizeDirection, Rect, CursorIcon); 8] = [
            (ResizeDirection::North, Rect::from_min_size(rect.left_top() + Vec2::new(T * 2.0, 0.0), Vec2::new(rect.width() - T * 4.0, T)), CursorIcon::ResizeNorth),
            (ResizeDirection::South, Rect::from_min_size(rect.left_bottom() + Vec2::new(T * 2.0, -T), Vec2::new(rect.width() - T * 4.0, T)), CursorIcon::ResizeSouth),
            (ResizeDirection::West, Rect::from_min_size(rect.left_top() + Vec2::new(0.0, T * 2.0), Vec2::new(T, rect.height() - T * 4.0)), CursorIcon::ResizeWest),
            (ResizeDirection::East, Rect::from_min_size(rect.right_top() + Vec2::new(-T, T * 2.0), Vec2::new(T, rect.height() - T * 4.0)), CursorIcon::ResizeEast),
            (ResizeDirection::NorthWest, Rect::from_min_size(rect.left_top(), Vec2::splat(T * 2.0)), CursorIcon::ResizeNorthWest),
            (ResizeDirection::NorthEast, Rect::from_min_size(rect.right_top() + Vec2::new(-T * 2.0, 0.0), Vec2::splat(T * 2.0)), CursorIcon::ResizeNorthEast),
            (ResizeDirection::SouthWest, Rect::from_min_size(rect.left_bottom() + Vec2::new(0.0, -T * 2.0), Vec2::splat(T * 2.0)), CursorIcon::ResizeSouthWest),
            (ResizeDirection::SouthEast, Rect::from_min_size(rect.right_bottom() + Vec2::new(-T * 2.0, -T * 2.0), Vec2::splat(T * 2.0)), CursorIcon::ResizeSouthEast),
        ];
        for (dir, zone, icon) in zones {
            let resp = ui.interact(zone, ui.make_persistent_id(("resize", dir as u8)), Sense::click_and_drag());
            if resp.hovered() {
                ui.ctx().set_cursor_icon(icon);
            }
            if resp.drag_started() {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::BeginResize(dir));
            }
        }
    }
}
