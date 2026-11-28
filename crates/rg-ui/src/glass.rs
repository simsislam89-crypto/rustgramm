//! Компоненты в стиле Apple iOS 26 Liquid Glass и 21st.dev:
//! полупрозрачное матовое стекло без цветных бликов фона, двухуровневая
//! спекулярная кромка, внутренний верхний блик и плавные пружинные переходы.

use std::hash::Hash;

use eframe::egui::{self, Color32, CornerRadius, FontId, Pos2, Rect, Response, Sense, Stroke, StrokeKind, Ui, Vec2};

use super::icons;
use super::theme::{self, DANGER, SUBTEXT, WHITE, c_orange, ios_spring, ios_spring_bounce, lerp_color, on_accent, tween};

/// Параметры физического демпфированного осциллятора (spring animation).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpringSpec {
    pub stiffness: f32,
    pub damping: f32,
    pub mass: f32,
}

impl SpringSpec {
    pub const SNAPPY: Self = Self { stiffness: 320.0, damping: 28.0, mass: 1.0 };
    pub const BOUNCY: Self = Self { stiffness: 360.0, damping: 20.0, mass: 1.0 };
    #[allow(dead_code)]
    pub const GENTLE: Self = Self { stiffness: 190.0, damping: 24.0, mass: 1.0 };
}

/// Состояние пружины в пространстве `(value, velocity)`.
#[derive(Debug, Clone, Copy, Default)]
pub struct SpringState {
    pub value: f32,
    pub velocity: f32,
}

impl SpringState {
    /// Шаг полу-неявного Эйлера (`sub-stepped` при большом `dt`) к цели `target`.
    pub fn step(&mut self, target: f32, dt: f32, spec: SpringSpec) {
        let clamped_dt = dt.clamp(0.0, 0.05);
        let sub_steps = if clamped_dt > 0.016 { 2 } else { 1 };
        let h = clamped_dt / sub_steps as f32;
        let inv_m = 1.0 / spec.mass.max(0.1);
        for _ in 0..sub_steps {
            let force = -spec.stiffness * (self.value - target) - spec.damping * self.velocity;
            self.velocity += force * inv_m * h;
            self.value += self.velocity * h;
        }
        if (self.value - target).abs() < 0.0005 && self.velocity.abs() < 0.002 {
            self.value = target;
            self.velocity = 0.0;
        }
    }
}

/// Асимптотическое резиновое сопротивление iOS UIScrollView при перетяжке за край.
#[allow(dead_code)]
pub fn rubber_band(offset: f32, limit: f32, constant: f32) -> f32 {
    if limit <= 0.0 || offset == 0.0 {
        return 0.0;
    }
    let abs = offset.abs();
    let damped = (abs * constant * limit) / (limit + abs * constant);
    offset.signum() * damped
}

/// Живая пружинная анимация по `id` поверх `egui::Context` temp-data.
pub fn spring_anim(ctx: &egui::Context, id_salt: impl Hash + std::fmt::Debug, target: f32, spec: SpringSpec) -> f32 {
    if theme::anim_scale() == 0.0 {
        return target;
    }
    let id = egui::Id::NULL.with(id_salt);
    let dt = ctx.input(|i| i.stable_dt).clamp(0.001, 0.05) / theme::anim_scale().max(0.25);
    let mut state = ctx.data_mut(|d| *d.get_temp_mut_or_insert_with(id, || SpringState { value: target, velocity: 0.0 }));
    let settled = (state.value - target).abs() < 0.0005 && state.velocity.abs() < 0.002;
    if !settled {
        state.step(target, dt, spec);
        ctx.data_mut(|d| d.insert_temp(id, state));
        ctx.request_repaint();
    }
    state.value
}

/// Материал поверхности Liquid Glass (по спецификации iOS 26 / 21st.dev).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlassVariant {
    Regular,
    Clear,
    Tinted(Color32),
}

/// Рисует матовую стеклянную поверхность с мягкой тенью, внутренним верхним бликом,
/// двуслойной спекулярной кромкой и опциональным интерактивным свечением под указателем.
pub fn paint_surface_interactive(
    p: &egui::Painter,
    rect: Rect,
    radius: f32,
    variant: GlassVariant,
    hover_k: f32,
    pointer_pos: Option<Pos2>,
) {
    paint_surface(p, rect, radius, variant, hover_k);
    if let Some(pt) = pointer_pos
        && rect.expand(16.0).contains(pt)
    {
        let r = radius.clamp(0.0, (rect.width().min(rect.height()) * 0.5).max(0.0));
        let clipped = p.with_clip_rect(rect);
        let rad = rect.width().max(rect.height()).clamp(28.0, 96.0) * 0.42;
        clipped.circle_filled(pt, rad, Color32::from_white_alpha((12.0 + 10.0 * hover_k) as u8));
        clipped.circle_filled(pt, rad * 0.48, Color32::from_white_alpha((10.0 + 10.0 * hover_k) as u8));
        let _ = r;
    }
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

    // Внутренний верхний блик (двухслойное матовое преломление без цветных пятен фона).
    let sheen_h = (rect.height() * 0.44).clamp(4.0, 20.0);
    let sheen_rect = Rect::from_min_max(
        Pos2::new(rect.left() + 1.0, rect.top() + 1.0),
        Pos2::new(rect.right() - 1.0, (rect.top() + sheen_h).min(rect.bottom() - 1.0)),
    );
    let sheen_r = (r - 1.0).max(0.0).round() as u8;
    p.rect_filled(
        sheen_rect,
        CornerRadius { nw: sheen_r, ne: sheen_r, sw: 3, se: 3 },
        Color32::from_white_alpha((12.0 + 10.0 * hk) as u8),
    );
    if rect.height() > 14.0 {
        let micro_h = (rect.height() * 0.16).clamp(2.0, 6.0);
        let micro_rect = Rect::from_min_max(
            Pos2::new(rect.left() + 2.0, rect.top() + 1.0),
            Pos2::new(rect.right() - 2.0, (rect.top() + micro_h).min(rect.bottom() - 2.0)),
        );
        p.rect_filled(
            micro_rect,
            CornerRadius { nw: sheen_r, ne: sheen_r, sw: 2, se: 2 },
            Color32::from_white_alpha((8.0 + 6.0 * hk) as u8),
        );
    }
    p.rect_stroke(
        rect.shrink(1.0),
        CornerRadius::same(sheen_r),
        Stroke::new(0.8, Color32::from_white_alpha((12.0 + 12.0 * hk) as u8)),
        StrokeKind::Inside,
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

/// Кнопка-капсула Liquid Glass (по мотивам 21st.dev *Liquid Glass Button* / *Elastic Pill*).
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
    let hk = spring_anim(ui.ctx(), ("lgb-h", &id_salt), f32::from(u8::from(resp.hovered())), SpringSpec::SNAPPY).clamp(0.0, 1.25);
    let pk = spring_anim(ui.ctx(), ("lgb-p", &id_salt), f32::from(u8::from(resp.is_pointer_button_down_on())), SpringSpec::BOUNCY).clamp(0.0, 1.2);
    let draw_rect = rect.shrink((1.4 * pk).clamp(-0.6, 2.4));
    let rad = draw_rect.height() * 0.5;
    let var = if active {
        GlassVariant::Tinted(c_orange())
    } else {
        GlassVariant::Regular
    };
    let ptr = ui.ctx().input(|i| i.pointer.hover_pos());
    paint_surface_interactive(ui.painter(), draw_rect, rad, var, hk.clamp(0.0, 1.0), ptr);
    let col = if active { Color32::WHITE } else { lerp_color(SUBTEXT, WHITE, hk.clamp(0.0, 1.0)) };
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

/// Сегментированный переключатель Liquid Glass со скользящей монохромной пилюлей.
pub fn segmented_tabs(
    ui: &mut Ui,
    id_salt: &str,
    labels: &[&str],
    active: usize,
) -> Option<usize> {
    if labels.is_empty() {
        return None;
    }
    let w = ui.available_width();
    let h = 34.0;
    let (track, _) = ui.allocate_exact_size(Vec2::new(w, h), Sense::hover());
    paint_surface(ui.painter(), track, h * 0.5, GlassVariant::Clear, 0.04);

    let pad = 3.0;
    let slot_w = ((track.width() - pad * 2.0) / labels.len() as f32).max(28.0);
    let slot_h = track.height() - pad * 2.0;
    let target_x = active.min(labels.len() - 1) as f32;
    let anim_x = spring_anim(ui.ctx(), ("seg-tab-x", id_salt), target_x, SpringSpec::BOUNCY)
        .clamp(-0.15, labels.len() as f32 - 0.85);
    let pill = Rect::from_min_size(
        Pos2::new(track.left() + pad + anim_x * slot_w, track.top() + pad),
        Vec2::new(slot_w, slot_h),
    );
    let ptr = ui.ctx().input(|i| i.pointer.hover_pos());
    paint_surface_interactive(ui.painter(), pill, slot_h * 0.5, GlassVariant::Tinted(c_orange()), 0.35, ptr);

    let mut picked = None;
    for (idx, &lbl) in labels.iter().enumerate() {
        let r = Rect::from_min_size(
            Pos2::new(track.left() + pad + idx as f32 * slot_w, track.top() + pad),
            Vec2::new(slot_w, slot_h),
        );
        let resp = ui.interact(r, ui.make_persistent_id((id_salt, idx)), Sense::click());
        let hk = tween(ui.ctx(), ("seg-tab-h", id_salt, idx), f32::from(u8::from(resp.hovered() || idx == active)), 0.12);
        let fg = if idx == active { Color32::WHITE } else { lerp_color(SUBTEXT, WHITE, hk) };
        ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, lbl, FontId::proportional(13.0), fg);
        if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            picked = Some(idx);
        }
    }
    picked
}

/// Индивидуальные стеклянные ячейки OTP-кода (ввод цифр, Backspace/Arrows, Ctrl+V, pop-in, мигающий курсор и встряска при ошибке).
pub fn otp_input(
    ui: &mut Ui,
    id_salt: &str,
    code: &mut String,
    len: usize,
    is_error: bool,
) -> bool {
    let digits_len = len.clamp(4, 8);
    let mut chars: Vec<char> = code.chars().filter(char::is_ascii_alphanumeric).take(digits_len).collect();
    let avail = ui.available_width();
    let gap = 8.0;
    let cell_w = ((avail - gap * (digits_len.saturating_sub(1) as f32)) / digits_len as f32).clamp(38.0, 50.0);
    let cell_h = 54.0;
    let total_w = cell_w * digits_len as f32 + gap * (digits_len.saturating_sub(1) as f32);

    // Пружинная горизонтальная встряска при появлении ошибки.
    let now = ui.input(|i| i.time);
    let err_t = ui.ctx().data_mut(|d| {
        let k = egui::Id::NULL.with(("otp-err-at", id_salt));
        let prev_k = egui::Id::NULL.with(("otp-err-prev", id_salt));
        let was_err = d.get_temp::<bool>(prev_k).unwrap_or(false);
        if is_error && !was_err {
            d.insert_temp(k, now);
        }
        d.insert_temp(prev_k, is_error);
        d.get_temp::<f64>(k)
    });
    let mut shake_x = 0.0_f32;
    if is_error && let Some(t0) = err_t {
        let elapsed = (ui.input(|i| i.time) - t0) as f32;
        if elapsed < 0.45 {
            shake_x = (elapsed * 38.0).sin() * 6.0 * (1.0 - elapsed / 0.45);
            ui.ctx().request_repaint();
        }
    }

    let (row_r, row_resp) = ui.allocate_exact_size(Vec2::new(avail, cell_h + 6.0), Sense::click());
    let group_id = ui.make_persistent_id(("otp-group", id_salt));
    if row_resp.clicked() {
        ui.memory_mut(|m| m.request_focus(group_id));
    }
    let has_focus = ui.memory(|m| m.has_focus(group_id));
    let active_idx_id = egui::Id::NULL.with(("otp-idx", id_salt));
    let mut active_idx = ui
        .ctx()
        .data(|d| d.get_temp::<usize>(active_idx_id))
        .unwrap_or(chars.len())
        .min(digits_len.saturating_sub(1));

    let mut enter_submit = false;
    let mut changed = false;
    if has_focus {
        let events = ui.input(|i| i.events.clone());
        for ev in events {
            match ev {
                egui::Event::Paste(pasted) => {
                    for c in pasted.chars().filter(char::is_ascii_alphanumeric) {
                        if chars.len() < digits_len {
                            chars.push(c);
                        }
                    }
                    active_idx = chars.len().min(digits_len.saturating_sub(1));
                    changed = true;
                }
                egui::Event::Text(txt) => {
                    for c in txt.chars().filter(char::is_ascii_alphanumeric) {
                        if active_idx < chars.len() {
                            chars[active_idx] = c;
                        } else if chars.len() < digits_len {
                            chars.push(c);
                        }
                        active_idx = chars.len().min(digits_len.saturating_sub(1));
                        changed = true;
                    }
                }
                egui::Event::Key { key: egui::Key::Backspace, pressed: true, .. } => {
                    if !chars.is_empty() {
                        chars.pop();
                        active_idx = chars.len().min(digits_len.saturating_sub(1));
                        changed = true;
                    }
                }
                egui::Event::Key { key: egui::Key::ArrowLeft, pressed: true, .. } => {
                    active_idx = active_idx.saturating_sub(1);
                }
                egui::Event::Key { key: egui::Key::ArrowRight, pressed: true, .. } => {
                    active_idx = (active_idx + 1).min(chars.len()).min(digits_len.saturating_sub(1));
                }
                egui::Event::Key { key: egui::Key::Enter, pressed: true, .. } => {
                    enter_submit = true;
                }
                _ => {}
            }
        }
    }
    if changed {
        *code = chars.iter().collect();
    }
    ui.ctx().data_mut(|d| d.insert_temp(active_idx_id, active_idx));

    let start_x = row_r.center().x - total_w * 0.5 + shake_x;
    let top_y = row_r.top() + 3.0;
    let now = ui.input(|i| i.time);
    for i in 0..digits_len {
        let cell = Rect::from_min_size(Pos2::new(start_x + i as f32 * (cell_w + gap), top_y), Vec2::new(cell_w, cell_h));
        let cresp = ui.interact(cell, ui.make_persistent_id((id_salt, "cell", i)), Sense::click());
        if cresp.clicked() {
            ui.memory_mut(|m| m.request_focus(group_id));
            ui.ctx().data_mut(|d| d.insert_temp(active_idx_id, i.min(chars.len())));
        }
        let ch = chars.get(i).copied();
        let is_cur = has_focus && i == active_idx;
        let pop_k = spring_anim(ui.ctx(), ("otp-pop", id_salt, i), f32::from(u8::from(ch.is_some())), SpringSpec::BOUNCY);
        let draw_cell = if ch.is_some() { cell.expand((pop_k - 1.0).clamp(-1.0, 2.0) * 1.6) } else { cell };
        let variant = if is_error {
            GlassVariant::Tinted(DANGER)
        } else if is_cur || ch.is_some() {
            GlassVariant::Tinted(c_orange())
        } else {
            GlassVariant::Regular
        };
        paint_surface_interactive(
            ui.painter(),
            draw_cell,
            13.0,
            variant,
            if is_cur { 0.48 } else if cresp.hovered() { 0.25 } else { 0.08 },
            ui.ctx().input(|inp| inp.pointer.hover_pos()),
        );
        if let Some(digit) = ch {
            let sz = 22.0 + 3.0 * (pop_k - 1.0).clamp(0.0, 0.25);
            ui.painter().text(
                draw_cell.center(),
                egui::Align2::CENTER_CENTER,
                digit.to_string(),
                FontId::proportional(sz),
                Color32::WHITE,
            );
        } else if is_cur {
            let blink = ((now * 2.4).sin() * 0.5 + 0.5) as f32;
            let caret_h = 22.0;
            ui.painter().line_segment(
                [
                    Pos2::new(draw_cell.center().x, draw_cell.center().y - caret_h * 0.5),
                    Pos2::new(draw_cell.center().x, draw_cell.center().y + caret_h * 0.5),
                ],
                Stroke::new(2.0, c_orange().gamma_multiply(0.35 + 0.65 * blink)),
            );
            ui.ctx().request_repaint();
        } else {
            ui.painter().circle_filled(draw_cell.center(), 3.0, Color32::from_white_alpha(38));
        }
    }
    if changed && chars.len() == digits_len {
        enter_submit = true;
    }
    enter_submit
}

/// Плавающая нижняя iOS 26 Liquid Glass панель вкладок / macOS Dock с proximity magnification и всплывающим DockLabel.
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
    let ptr = ui.ctx().input(|i| i.pointer.hover_pos());
    paint_surface_interactive(ui.painter(), bar_rect, bar_h * 0.5, GlassVariant::Regular, 0.08, ptr);

    let pad_x = 8.0;
    let pad_y = 6.0;
    let slot_w = (bar_rect.width() - pad_x * 2.0) / items.len() as f32;
    let slot_h = bar_rect.height() - pad_y * 2.0;

    let anim_idx = spring_anim(
        ui.ctx(),
        ("glass-tab-x", id_salt),
        active.min(items.len() - 1) as f32,
        SpringSpec::BOUNCY,
    )
    .clamp(-0.15, items.len() as f32 - 0.85);
    let pill_rect = Rect::from_min_size(
        Pos2::new(bar_rect.left() + pad_x + anim_idx * slot_w, bar_rect.top() + pad_y),
        Vec2::new(slot_w, slot_h),
    );
    paint_surface_interactive(
        ui.painter(),
        pill_rect.shrink2(Vec2::new(2.0, 0.0)),
        slot_h * 0.5,
        GlassVariant::Tinted(c_orange()),
        0.35,
        ptr,
    );

    let mut picked = None;
    let mut hovered_tip: Option<(Pos2, &str)> = None;
    for (idx, &(icon_name, label, badge)) in items.iter().enumerate() {
        let srect = Rect::from_min_size(
            Pos2::new(bar_rect.left() + pad_x + idx as f32 * slot_w, bar_rect.top() + pad_y),
            Vec2::new(slot_w, slot_h),
        );
        let resp = ui.interact(srect, ui.make_persistent_id((id_salt, idx)), Sense::click());
        let prox = ptr
            .filter(|p| bar_rect.expand2(Vec2::new(12.0, 20.0)).contains(*p))
            .map_or(0.0, |p| (1.0 - (p.x - srect.center().x).abs() / (slot_w * 1.45)).clamp(0.0, 1.0));
        let hk = tween(
            ui.ctx(),
            ("glass-tab-h", id_salt, idx),
            f32::from(u8::from(resp.hovered() || idx == active)),
            0.14,
        );
        let scale = 1.0 + 0.08 * ios_spring_bounce(hk) + 0.16 * prox;
        let lift_y = -3.5 * prox;
        let col = if idx == active {
            Color32::WHITE
        } else {
            lerp_color(SUBTEXT, WHITE, hk.max(prox) * 0.85)
        };
        let ico_c = Pos2::new(srect.center().x, srect.top() + 15.0 + lift_y);
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
                on_accent(),
            );
        }
        if resp.hovered() {
            hovered_tip = Some((Pos2::new(srect.center().x, bar_rect.top() - 10.0), label));
        }
        if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            picked = Some(idx);
        }
    }
    if let Some((tip_c, tip_lbl)) = hovered_tip {
        let g = ui.painter().layout_no_wrap(tip_lbl.to_owned(), FontId::proportional(11.0), WHITE);
        let tip_r = Rect::from_center_size(tip_c - Vec2::new(0.0, 8.0), Vec2::new(g.size().x + 16.0, 22.0));
        paint_surface(ui.painter(), tip_r, 11.0, GlassVariant::Regular, 0.25);
        ui.painter().galley(tip_r.center() - g.size() * 0.5, g, WHITE);
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

    #[test]
    fn spring_physics_rubber_band_segmented_and_otp_work() {
        let mut s = SpringState::default();
        for _ in 0..60 {
            s.step(1.0, 0.016, SpringSpec::BOUNCY);
        }
        assert!((s.value - 1.0).abs() < 0.05);
        assert!(rubber_band(200.0, 100.0, 0.55) < 100.0);

        let ctx = egui::Context::default();
        let mut code = "12".to_owned();
        let mut tab_pick = None;
        let mut out = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(400.0, 400.0))),
                ..Default::default()
            },
            |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    tab_pick = segmented_tabs(ui, "seg-test", &["Эмодзи", "Стикеры", "GIF"], 1);
                    let _ = otp_input(ui, "otp-test", &mut code, 5, false);
                });
            },
        );
        out.textures_delta.clear();
        assert!(tab_pick.is_none());
        let mut txt = String::new();
        fn collect_t(s: &egui::Shape, buf: &mut String) {
            match s {
                egui::Shape::Text(t) => {
                    buf.push_str(t.galley.text());
                    buf.push(' ');
                }
                egui::Shape::Vec(v) => v.iter().for_each(|x| collect_t(x, buf)),
                _ => {}
            }
        }
        out.shapes.iter().for_each(|cs| collect_t(&cs.shape, &mut txt));
        assert!(txt.contains("Эмодзи") && txt.contains("Стикеры") && txt.contains("GIF"));
        assert!(txt.contains('1') && txt.contains('2'));
    }
}
