//! Компоненты в стиле Apple iOS 26 Liquid Glass и 21st.dev:
//! полупрозрачное матовое стекло без цветных бликов фона, двухуровневая
//! спекулярная кромка, внутренний верхний блик и плавные пружинные переходы.

use eframe::egui::{self, Color32, CornerRadius, FontId, Pos2, Rect, Response, Sense, Stroke, StrokeKind, Ui, Vec2};

use super::icons;
use super::theme::{self, SUBTEXT, WHITE, c_orange, ios_spring, ios_spring_bounce, lerp_color, tween};

/// Материал поверхности Liquid Glass (по спецификации iOS 26 / 21st.dev).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlassVariant {
    Regular,
    Clear,
    Tinted(Color32),
}

/// Рисует матовую стеклянную поверхность с мягкой тенью, внутренним верхним бликом
/// и двуслойной спекулярной кромкой (сверху ярче, снизу глубже).
pub fn paint_surface(p: &egui::Painter, rect: Rect, radius: f32, variant: GlassVariant, hover_k: f32) {
    if rect.width() <= 1.0 || rect.height() <= 1.0 {
        return;
    }
    let op = theme::opacity_f32().clamp(0.45, 1.0);
    let hk = hover_k.clamp(0.0, 1.0);
    let max_r = (rect.width().min(rect.height()) * 0.5).max(0.0);
    let r = radius.clamp(0.0, max_r);
    let cr = CornerRadius::same(r.round() as u8);

    // Мягкая двухслойная тень для отделения плавающих панелей и капсул от ленты.
    p.rect_filled(
        rect.translate(Vec2::new(0.0, 2.5)).expand(1.5),
        cr,
        Color32::from_black_alpha((32.0 * op) as u8),
    );
    p.rect_filled(
        rect.translate(Vec2::new(0.0, 6.0)).expand(2.5),
        cr,
        Color32::from_black_alpha((14.0 * op) as u8),
    );

    let (fill, rim_a, top_spec_a, accent_tint) = match variant {
        GlassVariant::Regular => (
            Color32::from_rgba_unmultiplied(22, 24, 32, ((186.0 + 18.0 * hk) * op) as u8),
            (36.0 + 28.0 * hk) as u8,
            (74.0 + 36.0 * hk) as u8,
            None,
        ),
        GlassVariant::Clear => (
            Color32::from_rgba_unmultiplied(18, 20, 28, ((138.0 + 26.0 * hk) * op) as u8),
            (30.0 + 30.0 * hk) as u8,
            (62.0 + 36.0 * hk) as u8,
            None,
        ),
        GlassVariant::Tinted(col) => {
            let mix = lerp_color(Color32::from_rgb(24, 24, 30), col, 0.34 + 0.14 * hk);
            (
                Color32::from_rgba_unmultiplied(mix.r(), mix.g(), mix.b(), (208.0 * op) as u8),
                (70.0 + 40.0 * hk) as u8,
                (92.0 + 40.0 * hk) as u8,
                Some(col),
            )
        }
    };

    p.rect_filled(rect, cr, fill);

    // Внутренний верхний блик (имитация линзы / отражения света на верхней грани стекла).
    let sheen_h = (rect.height() * 0.42).clamp(4.0, 18.0);
    let sheen_rect = Rect::from_min_max(
        Pos2::new(rect.left() + 1.0, rect.top() + 1.0),
        Pos2::new(rect.right() - 1.0, (rect.top() + sheen_h).min(rect.bottom() - 1.0)),
    );
    let sheen_r = (r - 1.0).max(0.0).round() as u8;
    p.rect_filled(
        sheen_rect,
        CornerRadius { nw: sheen_r, ne: sheen_r, sw: 2, se: 2 },
        Color32::from_white_alpha((10.0 + 9.0 * hk) as u8),
    );

    // Внешний спекулярный контур + подчёркнутая верхняя грань + нижняя глубина.
    let rim_col = match accent_tint {
        Some(c) => Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), rim_a.max(115)),
        None => Color32::from_white_alpha(rim_a),
    };
    p.rect_stroke(rect, cr, Stroke::new(1.0, rim_col), StrokeKind::Inside);

    if rect.width() > r * 2.0 + 6.0 {
        let inset = r.max(4.0);
        p.line_segment(
            [
                Pos2::new(rect.left() + inset, rect.top() + 0.8),
                Pos2::new(rect.right() - inset, rect.top() + 0.8),
            ],
            Stroke::new(1.0, Color32::from_white_alpha(top_spec_a)),
        );
        p.line_segment(
            [
                Pos2::new(rect.left() + inset, rect.bottom() - 0.8),
                Pos2::new(rect.right() - inset, rect.bottom() - 0.8),
            ],
            Stroke::new(1.0, Color32::from_black_alpha(78)),
        );
    }
}

/// Кнопка-капсула Liquid Glass (по мотивам 21st.dev *Liquid Glass Button*).
#[allow(dead_code)]
pub fn pill_button(
    ui: &mut Ui,
    id_salt: impl std::hash::Hash + std::fmt::Debug,
    size: Vec2,
    icon: Option<&str>,
    label: &str,
    active: bool,
) -> Response {
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let hk = tween(ui.ctx(), ("lgb-h", &id_salt), f32::from(u8::from(resp.hovered())), 0.14);
    let pk = tween(ui.ctx(), ("lgb-p", &id_salt), f32::from(u8::from(resp.is_pointer_button_down_on())), 0.08);
    let draw_rect = rect.shrink((1.2 * pk).clamp(0.0, 2.0));
    let rad = draw_rect.height() * 0.5;
    let var = if active {
        GlassVariant::Tinted(c_orange())
    } else {
        GlassVariant::Regular
    };
    paint_surface(ui.painter(), draw_rect, rad, var, hk);
    let col = if active { Color32::WHITE } else { lerp_color(SUBTEXT, WHITE, hk) };
    if let Some(ico) = icon {
        if label.is_empty() {
            icons::tg_sized(ui.painter(), ico, draw_rect.center(), Vec2::splat(16.0), col);
        } else {
            icons::tg_sized(
                ui.painter(),
                ico,
                Pos2::new(draw_rect.left() + 16.0, draw_rect.center().y),
                Vec2::splat(15.0),
                col,
            );
            ui.painter().text(
                Pos2::new(draw_rect.left() + 30.0, draw_rect.center().y),
                egui::Align2::LEFT_CENTER,
                label,
                FontId::proportional(13.0),
                col,
            );
        }
    } else if !label.is_empty() {
        ui.painter().text(
            draw_rect.center(),
            egui::Align2::CENTER_CENTER,
            label,
            FontId::proportional(13.0),
            col,
        );
    }
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Плавающая нижняя iOS 26 Liquid Glass панель вкладок (по мотивам 21st.dev *Glass Tab Bar*):
/// стеклянная капсула со скользящей оранжевой линзой активной вкладки.
pub fn floating_tab_bar(
    ui: &mut Ui,
    id_salt: &str,
    items: &[(&str, &str, i32)],
    active: usize,
) -> Option<usize> {
    if items.is_empty() {
        return None;
    }
    let avail_w = ui.available_width();
    let bar_w = (avail_w - 24.0).clamp(240.0, 420.0);
    let bar_h = 56.0;
    let (outer_r, _) = ui.allocate_exact_size(Vec2::new(avail_w, bar_h + 12.0), Sense::hover());
    let bar_rect = Rect::from_center_size(
        Pos2::new(outer_r.center().x, outer_r.top() + bar_h * 0.5 + 4.0),
        Vec2::new(bar_w, bar_h),
    );
    let p = ui.painter();
    paint_surface(p, bar_rect, bar_h * 0.5, GlassVariant::Regular, 0.08);

    let pad_x = 8.0;
    let pad_y = 6.0;
    let slot_w = (bar_rect.width() - pad_x * 2.0) / items.len() as f32;
    let slot_h = bar_rect.height() - pad_y * 2.0;

    // Плавный слайд активной стеклянной линзы между вкладками (не ограничиваем [0..1]!).
    let anim_idx = tween(
        ui.ctx(),
        ("glass-tab-x", id_salt),
        active.min(items.len() - 1) as f32,
        0.22,
    );
    let pill_rect = Rect::from_min_size(
        Pos2::new(bar_rect.left() + pad_x + anim_idx * slot_w, bar_rect.top() + pad_y),
        Vec2::new(slot_w, slot_h),
    );
    paint_surface(
        ui.painter(),
        pill_rect.shrink2(Vec2::new(2.0, 0.0)),
        slot_h * 0.5,
        GlassVariant::Tinted(c_orange()),
        0.35,
    );

    let mut picked = None;
    for (idx, &(icon_name, label, badge)) in items.iter().enumerate() {
        let srect = Rect::from_min_size(
            Pos2::new(bar_rect.left() + pad_x + idx as f32 * slot_w, bar_rect.top() + pad_y),
            Vec2::new(slot_w, slot_h),
        );
        let resp = ui.interact(srect, ui.make_persistent_id((id_salt, idx)), Sense::click());
        let hk = tween(
            ui.ctx(),
            ("glass-tab-h", id_salt, idx),
            f32::from(u8::from(resp.hovered() || idx == active)),
            0.14,
        );
        let scale = 1.0 + 0.08 * ios_spring_bounce(hk);
        let col = if idx == active {
            Color32::WHITE
        } else {
            lerp_color(SUBTEXT, WHITE, hk * 0.75)
        };
        let ico_c = Pos2::new(srect.center().x, srect.top() + 15.0);
        icons::tg_sized(ui.painter(), icon_name, ico_c, Vec2::splat(17.5 * scale), col);
        ui.painter().text(
            Pos2::new(srect.center().x, srect.bottom() - 10.5),
            egui::Align2::CENTER_CENTER,
            label,
            FontId::proportional(10.5),
            col,
        );
        if badge > 0 {
            let b_txt = if badge > 99 { "99+".to_owned() } else { badge.to_string() };
            let bw = (b_txt.chars().count() as f32 * 5.8 + 8.0).max(16.0);
            let br = Rect::from_center_size(
                Pos2::new(ico_c.x + 14.0, ico_c.y - 6.0),
                Vec2::new(bw, 14.0),
            );
            ui.painter().rect_filled(br, CornerRadius::same(7), c_orange());
            ui.painter().text(
                br.center(),
                egui::Align2::CENTER_CENTER,
                b_txt,
                FontId::proportional(9.5),
                Color32::WHITE,
            );
        }
        if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            picked = Some(idx);
        }
    }
    picked
}

/// Плавное всплывающее Liquid Glass уведомление (тост) снизу по центру без скачков раскладки.
#[allow(deprecated)]
pub fn paint_toast(ctx: &egui::Context, text: &str, is_error: bool, alpha_k: f32) {
    let k = alpha_k.clamp(0.0, 1.0);
    if k <= 0.01 || text.trim().is_empty() {
        return;
    }
    let screen = ctx.content_rect();
    let max_w = (screen.width() - 32.0).clamp(180.0, 460.0);
    let font = FontId::proportional(13.0);
    let galley = ctx.fonts_mut(|f| f.layout(text.to_owned(), font, WHITE, max_w - 36.0));
    let w = (galley.size().x + 38.0).clamp(140.0, max_w);
    let h = (galley.size().y + 18.0).max(36.0);
    let y_shift = (1.0 - ios_spring(k)) * 14.0;
    let pos = Pos2::new(
        (screen.center().x - w * 0.5).max(12.0),
        (screen.bottom() - h - 72.0 + y_shift).max(48.0),
    );
    egui::Area::new(egui::Id::new("rg-glass-toast"))
        .order(egui::Order::Foreground)
        .interactable(false)
        .fixed_pos(pos)
        .show(ctx, |ui| {
            ui.set_opacity(k);
            let (rect, _) = ui.allocate_exact_size(Vec2::new(w, h), Sense::hover());
            let tint = if is_error {
                GlassVariant::Tinted(Color32::from_rgb(210, 62, 62))
            } else {
                GlassVariant::Regular
            };
            paint_surface(ui.painter(), rect, h * 0.5, tint, 0.2);
            let dot_c = Pos2::new(rect.left() + 14.0, rect.center().y);
            ui.painter().circle_filled(
                dot_c,
                3.5,
                if is_error { Color32::from_rgb(255, 130, 130) } else { c_orange() },
            );
            ui.painter().galley(
                Pos2::new(rect.left() + 24.0, rect.center().y - galley.size().y * 0.5),
                galley,
                WHITE,
            );
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glass_surface_pill_button_tab_bar_and_toast_render_cleanly() {
        let ctx = egui::Context::default();
        theme::set_opacity(70);
        let mut clicked_tab = None;
        let mut out_shapes = Vec::new();
        for _ in 0..2 {
            let raw = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(390.0, 844.0))),
                ..Default::default()
            };
            let mut out = ctx.run_ui(raw, |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    let r = Rect::from_min_size(Pos2::new(20.0, 20.0), Vec2::new(220.0, 48.0));
                    paint_surface(ui.painter(), r, 24.0, GlassVariant::Clear, 0.4);
                    let _ = pill_button(ui, "test-btn", Vec2::new(140.0, 38.0), Some("menu/chats"), "Чаты", true);
                    clicked_tab = floating_tab_bar(
                        ui,
                        "test-tabs",
                        &[
                            ("menu/chats", "Чаты", 5),
                            ("menu/archive", "Архив", 0),
                            ("folders/folders_type_contacts", "Контакты", 0),
                            ("menu/settings", "Настройки", 0),
                        ],
                        0,
                    );
                });
                paint_toast(ui.ctx(), "Скопировано в буфер обмена", false, 1.0);
            });
            out.textures_delta.clear();
            out_shapes = out.shapes;
        }
        assert!(clicked_tab.is_none());
        let mut txt = String::new();
        fn collect(s: &egui::Shape, buf: &mut String) {
            match s {
                egui::Shape::Text(t) => {
                    buf.push_str(t.galley.text());
                    buf.push('\n');
                }
                egui::Shape::Vec(v) => v.iter().for_each(|x| collect(x, buf)),
                _ => {}
            }
        }
        out_shapes.iter().for_each(|cs| collect(&cs.shape, &mut txt));
        assert!(txt.contains("Чаты") && txt.contains("Архив") && txt.contains("Контакты") && txt.contains("Настройки"));
        assert!(txt.contains("Скопировано в буфер обмена"));
    }
}
