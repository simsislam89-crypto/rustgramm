//! Компоненты в стиле Apple iOS 26 Liquid Glass, 21st.dev и React Bits:
//! нейтральное обсидиановое стекло без синих пятен, гладкий Mesh-градиент и
//! радиальный отблик указателя, геометрия Lissé Squircle, точная пружинная физика,
//! macOS Magnification Dock, OtpInput и 3D-карточка TiltedCard.

use std::hash::Hash;

use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontId, Mesh, Pos2, Rect, Response, Sense, Shape,
    Stroke, StrokeKind, Ui, Vec2, epaint::Vertex,
};

use super::icons;
use super::theme::{
    self, DANGER, SUBTEXT, WHITE, c_orange, ios_spring, ios_spring_bounce, lerp_color,
    on_accent, tween,
};

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

    // Пресеты из core/spring.ts (#2 LiquidGlass):
    pub const PRESS: Self = Self::from_duration_bounce(0.24, 0.20);
    pub const RELEASE: Self = Self::from_duration_bounce(0.50, 0.42);
    pub const GLOW: Self = Self::from_duration_bounce(0.42, 0.0);
    pub const TRACK: Self = Self::from_duration_bounce(0.18, 0.0);
    #[allow(dead_code)]
    pub const LAYOUT: Self = Self::from_duration_bounce(0.40, 0.16);

    /// Пересчитывает `(duration, bounce)` из `core/spring.ts` в `(stiffness, damping)`.
    pub const fn from_duration_bounce(duration: f32, bounce: f32) -> Self {
        let d = if duration > 0.02 { duration } else { 0.02 };
        let omega = std::f32::consts::TAU / d;
        let stiffness = omega * omega;
        let damping = if bounce >= 0.0 {
            let b = if bounce < 0.95 { bounce } else { 0.95 };
            (2.0 * std::f32::consts::TAU * (1.0 - b)) / d
        } else {
            let b = if bounce > -0.95 { bounce } else { -0.95 };
            (2.0 * std::f32::consts::TAU) / (d * (1.0 + b))
        };
        Self { stiffness, damping, mass: 1.0 }
    }
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
    let mut state = ctx.data_mut(|d| {
        *d.get_temp_mut_or_insert_with(id, || SpringState { value: target, velocity: 0.0 })
    });
    let settled = (state.value - target).abs() < 0.0005 && state.velocity.abs() < 0.002;
    if !settled {
        state.step(target, dt, spec);
        ctx.data_mut(|d| d.insert_temp(id, state));
        ctx.request_repaint();
    }
    state.value
}

/// Геометрия скруглённого суперэллипса Lissé Squircle (`SMOOTHING = 0.8125`).
pub fn squircle_points(rect: Rect, radius: f32) -> Vec<Pos2> {
    const SMOOTHING: f32 = 0.8125;
    let w = rect.width();
    let h = rect.height();
    let hw = w * 0.5;
    let hh = h * 0.5;
    let budget = hw.min(hh);
    let r = radius.clamp(0.0, budget);
    if r < 1.0 || w < 4.0 || h < 4.0 {
        return vec![rect.left_top(), rect.right_top(), rect.right_bottom(), rect.left_bottom()];
    }

    let rad = |deg: f32| deg.to_radians();
    let mut p = (1.0 + SMOOTHING) * r;
    let o = 90.0 * (1.0 - SMOOTHING);
    let i = rad(o * 0.5).sin() * r * std::f32::consts::SQRT_2;
    let g = r * rad((90.0 - o) * 0.25).tan();
    let c = g * rad(45.0 * SMOOTHING).cos();
    let d = c * rad(45.0 * SMOOTHING).tan();
    let mut u = (p - i - c - d) / 3.0;
    let mut a = 2.0 * u;
    if p > budget {
        let rem = (budget - d - i - c).max(0.0);
        let m = rem / 6.0;
        let l = rem - m;
        u = u.min(l);
        a = rem - u;
        p = budget;
    }

    let mut tr = Vec::with_capacity(14);
    let x0 = hw - p;
    let y0 = -hh;
    tr.push((x0, y0));

    let cubic = |out: &mut Vec<(f32, f32)>, x0: f32, y0: f32, x1: f32, y1: f32, x2: f32, y2: f32, x3: f32, y3: f32, steps: usize| {
        for s in 1..=steps {
            let t = s as f32 / steps as f32;
            let w1 = 1.0 - t;
            out.push((
                x0 * w1 * w1 * w1 + 3.0 * w1 * w1 * t * x1 + 3.0 * w1 * t * t * x2 + t * t * t * x3,
                y0 * w1 * w1 * w1 + 3.0 * w1 * w1 * t * y1 + 3.0 * w1 * t * t * y2 + t * t * t * y3,
            ));
        }
    };

    cubic(&mut tr, x0, y0, x0 + a, y0, x0 + a + b_val(u), y0, x0 + a + u + c, y0 + d, 5);
    let ax = x0 + a + u + c;
    let ay = y0 + d;
    let chord = i * std::f32::consts::SQRT_2;
    let offset = (r * r - chord * chord * 0.25).max(0.0).sqrt();
    let cx = ax + i * 0.5 - (i / chord.max(1e-4)) * offset;
    let cy = ay + i * 0.5 + (i / chord.max(1e-4)) * offset;
    let angle = (ay - cy).atan2(ax - cx);
    let extent = 2.0 * (chord / (2.0 * r).max(1e-4)).min(1.0).asin();
    for s in 1..=3 {
        let th = angle + (extent * s as f32) / 3.0;
        tr.push((cx + r * th.cos(), cy + r * th.sin()));
    }
    let qx = ax + i;
    let qy = ay + i;
    cubic(&mut tr, qx, qy, qx + d, qy + c, qx + d, qy + u + c, qx + d, qy + a + u + c, 5);

    let center = rect.center();
    let mut pts = Vec::with_capacity(tr.len() * 4);
    for &(px, py) in &tr {
        pts.push(Pos2::new(center.x + px, center.y + py));
    }
    for &(px, py) in tr.iter().rev() {
        pts.push(Pos2::new(center.x + px, center.y - py));
    }
    for &(px, py) in &tr {
        pts.push(Pos2::new(center.x - px, center.y - py));
    }
    for &(px, py) in tr.iter().rev() {
        pts.push(Pos2::new(center.x - px, center.y + py));
    }
    pts
}

#[inline]
const fn b_val(u: f32) -> f32 {
    u
}

/// Материал поверхности Liquid Glass (нейтральная монохромная калибровка).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlassVariant {
    Regular,
    Clear,
    Tinted(Color32),
}

fn push_vertical_sheen_mesh(p: &egui::Painter, rect: Rect, top_c: Color32, mid_c: Color32, bot_c: Color32) {
    let mut mesh = Mesh::default();
    let mid_y = rect.top() + rect.height() * 0.45;
    let l = rect.left();
    let r = rect.right();
    let t = rect.top();
    let b = rect.bottom();
    let v = |x: f32, y: f32, color: Color32| Vertex {
        pos: Pos2::new(x, y),
        uv: egui::epaint::WHITE_UV,
        color,
    };
    mesh.vertices.extend_from_slice(&[
        v(l, t, top_c),
        v(r, t, top_c),
        v(l, mid_y, mid_c),
        v(r, mid_y, mid_c),
        v(l, b, bot_c),
        v(r, b, bot_c),
    ]);
    mesh.indices.extend_from_slice(&[0, 1, 2, 1, 3, 2, 2, 3, 4, 3, 5, 4]);
    p.add(Shape::mesh(mesh));
}

fn push_radial_glow_mesh(p: &egui::Painter, center: Pos2, radius: f32, inner: Color32) {
    if radius <= 1.0 || inner.a() == 0 {
        return;
    }
    const SEGMENTS: usize = 18;
    let mut mesh = Mesh::default();
    mesh.vertices.push(Vertex { pos: center, uv: egui::epaint::WHITE_UV, color: inner });
    for i in 0..=SEGMENTS {
        let a = (i as f32 / SEGMENTS as f32) * std::f32::consts::TAU;
        mesh.vertices.push(Vertex {
            pos: center + Vec2::new(a.cos(), a.sin()) * radius,
            uv: egui::epaint::WHITE_UV,
            color: Color32::TRANSPARENT,
        });
    }
    for i in 1..=SEGMENTS as u32 {
        mesh.indices.extend_from_slice(&[0, i, i + 1]);
    }
    p.add(Shape::mesh(mesh));
}

/// Рисует матовую стеклянную поверхность с гладким радиальным Mesh-отбликом под указателем.
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
        && rect.expand(24.0).contains(pt)
    {
        let clipped = p.with_clip_rect(rect);
        let max_rad = rect.width().max(rect.height()).clamp(36.0, 150.0) * 0.55;
        let boost = 0.55 + 0.65 * hover_k.clamp(0.0, 1.0);
        push_radial_glow_mesh(&clipped, pt, max_rad, Color32::from_white_alpha((32.0 * boost).clamp(0.0, 72.0) as u8));
    }
}

/// Рисует нейтральную монохромную поверхность iOS 26 Liquid Glass:
/// 3-слойная мягкая тень (`--lg-shadow`), плавный Mesh-градиент без ступенек,
/// скругление Lissé Squircle и двойной скошенный спекулярный обод.
pub fn paint_surface(p: &egui::Painter, rect: Rect, radius: f32, variant: GlassVariant, hover_k: f32) {
    if rect.width() <= 1.0 || rect.height() <= 1.0 {
        return;
    }
    let op = theme::opacity_f32().clamp(0.45, 1.0);
    let hk = hover_k.clamp(0.0, 1.0);
    let max_r = (rect.width().min(rect.height()) * 0.5).max(0.0);
    let r = radius.clamp(0.0, max_r);
    let cr = CornerRadius::same(r.round() as u8);

    // 3-слойная мягкая тень (--lg-shadow).
    p.rect_filled(rect.translate(Vec2::new(0.0, 2.0)).expand(1.0), cr, Color32::from_black_alpha((32.0 * op) as u8));
    p.rect_filled(rect.translate(Vec2::new(0.0, 6.0)).expand(3.5), cr, Color32::from_black_alpha((16.0 * op) as u8));
    p.rect_filled(rect.translate(Vec2::new(0.0, 11.0)).expand(6.5), cr, Color32::from_black_alpha((7.0 * op) as u8));

    let (fill, rim_col, inner_a, top_sheen_a, bot_sheen_a) = match variant {
        GlassVariant::Regular => (
            Color32::from_rgba_unmultiplied(18, 18, 22, ((202.0 + 18.0 * hk) * op) as u8),
            Color32::from_white_alpha((40.0 + 30.0 * hk) as u8),
            (16.0 + 14.0 * hk) as u8,
            (20.0 + 10.0 * hk) as u8,
            (7.0 + 5.0 * hk) as u8,
        ),
        GlassVariant::Clear => (
            Color32::from_rgba_unmultiplied(14, 14, 17, ((150.0 + 26.0 * hk) * op) as u8),
            Color32::from_white_alpha((32.0 + 32.0 * hk) as u8),
            (13.0 + 14.0 * hk) as u8,
            (16.0 + 10.0 * hk) as u8,
            (6.0 + 4.0 * hk) as u8,
        ),
        GlassVariant::Tinted(col) => {
            let lum = 0.299 * f32::from(col.r()) + 0.587 * f32::from(col.g()) + 0.114 * f32::from(col.b());
            if lum >= 195.0 {
                // Кристаллическая светящаяся монохромная линза (как в 21st.dev на тёмном стекле).
                (
                    Color32::from_rgba_unmultiplied(36, 36, 42, ((198.0 + 24.0 * hk) * op) as u8),
                    Color32::from_white_alpha((118.0 + 58.0 * hk) as u8),
                    (34.0 + 22.0 * hk) as u8,
                    (40.0 + 22.0 * hk) as u8,
                    (16.0 + 10.0 * hk) as u8,
                )
            } else {
                let mix = lerp_color(Color32::from_rgb(20, 20, 24), col, 0.36 + 0.14 * hk);
                (
                    Color32::from_rgba_unmultiplied(mix.r(), mix.g(), mix.b(), (212.0 * op) as u8),
                    Color32::from_rgba_unmultiplied(col.r(), col.g(), col.b(), (110.0 + 40.0 * hk) as u8),
                    (22.0 + 16.0 * hk) as u8,
                    (24.0 + 12.0 * hk) as u8,
                    (8.0 + 6.0 * hk) as u8,
                )
            }
        }
    };

    p.rect_filled(rect, cr, fill);

    // Плавный вертикальный стеклянный блеск (без резких ступенек!).
    if rect.height() > 10.0 {
        let clipped = p.with_clip_rect(rect.shrink(1.0));
        push_vertical_sheen_mesh(
            &clipped,
            rect.shrink(1.0),
            Color32::from_white_alpha(top_sheen_a),
            Color32::TRANSPARENT,
            Color32::from_white_alpha(bot_sheen_a),
        );
    }

    // Двойной спекулярный обод: если радиус >= 8 — контур Lissé Squircle.
    if r >= 8.0 && rect.width() >= 20.0 && rect.height() >= 20.0 {
        let inner_pts = squircle_points(rect.shrink(1.0), (r - 1.0).max(2.0));
        p.add(Shape::closed_line(inner_pts, Stroke::new(0.85, Color32::from_white_alpha(inner_a))));
        let outer_pts = squircle_points(rect, r);
        p.add(Shape::closed_line(outer_pts, Stroke::new(1.0, rim_col)));
    } else {
        p.rect_stroke(rect.shrink(1.0), cr, Stroke::new(0.85, Color32::from_white_alpha(inner_a)), StrokeKind::Inside);
        p.rect_stroke(rect, cr, Stroke::new(1.0, rim_col), StrokeKind::Inside);
    }

    // Верхняя направленная световая грань (COS_SPREAD specular) + нижняя кромка глубины.
    if rect.width() > r * 2.0 + 8.0 {
        let inset = r.max(4.0);
        let top_a = (86.0 + 48.0 * hk).clamp(0.0, 210.0) as u8;
        p.line_segment(
            [Pos2::new(rect.left() + inset, rect.top() + 0.8), Pos2::new(rect.right() - inset, rect.top() + 0.8)],
            Stroke::new(1.1, Color32::from_white_alpha(top_a)),
        );
        p.line_segment(
            [Pos2::new(rect.left() + inset, rect.bottom() - 0.8), Pos2::new(rect.right() - inset, rect.bottom() - 0.8)],
            Stroke::new(1.0, Color32::from_black_alpha(86)),
        );
    }
}

/// Эластичная кнопка Liquid Glass (`#1 GlassButton` + `#2 LiquidGlassButton`):
/// при наведении мягко расширяется и светится, при зажатии смещается за указателем через `rubber_band`.
pub fn liquid_glass_button(
    ui: &mut Ui,
    id_salt: impl Hash + std::fmt::Debug,
    size: Vec2,
    icon: Option<&str>,
    label: &str,
    solid_accent: bool,
) -> Response {
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click_and_drag());
    let down = resp.is_pointer_button_down_on();
    let hov = resp.hovered();
    let hk = spring_anim(ui.ctx(), ("lgb2-h", &id_salt), f32::from(u8::from(hov)), SpringSpec::GLOW).clamp(0.0, 1.2);
    let pk = spring_anim(
        ui.ctx(),
        ("lgb2-p", &id_salt),
        f32::from(u8::from(down)),
        if down { SpringSpec::PRESS } else { SpringSpec::RELEASE },
    )
    .clamp(-0.15, 1.2);

    let ptr = ui.ctx().input(|i| i.pointer.hover_pos());
    let (tx_t, ty_t) = if down && let Some(pt) = ptr {
        let d = pt - rect.center();
        let lim = (rect.width().max(rect.height()) * 0.06).clamp(3.0, 8.0);
        (rubber_band(d.x, lim, 0.55), rubber_band(d.y, lim, 0.55))
    } else {
        (0.0, 0.0)
    };
    let tx = spring_anim(ui.ctx(), ("lgb2-tx", &id_salt), tx_t, if down { SpringSpec::TRACK } else { SpringSpec::RELEASE });
    let ty = spring_anim(ui.ctx(), ("lgb2-ty", &id_salt), ty_t, if down { SpringSpec::TRACK } else { SpringSpec::RELEASE });

    let grow_x = 1.6 * hk - 1.2 * pk;
    let grow_y = 0.8 * hk - 1.0 * pk;
    let draw_rect = rect.translate(Vec2::new(tx, ty)).expand2(Vec2::new(grow_x, grow_y));
    let rad = draw_rect.height() * 0.5;

    if solid_accent {
        let fill = lerp_color(c_orange(), Color32::WHITE, hk.clamp(0.0, 1.0));
        ui.painter().rect_filled(draw_rect, CornerRadius::same(rad.round() as u8), theme::t_btn(fill));
        ui.painter().rect_stroke(draw_rect, CornerRadius::same(rad.round() as u8), Stroke::new(1.0, Color32::from_white_alpha(150)), StrokeKind::Inside);
    } else {
        let var = if hov { GlassVariant::Tinted(c_orange()) } else { GlassVariant::Regular };
        paint_surface_interactive(ui.painter(), draw_rect, rad, var, (hk * 0.55 + pk * 0.45).clamp(0.0, 1.0), ptr);
    }

    let col = if solid_accent { on_accent() } else { lerp_color(SUBTEXT, Color32::WHITE, hk.clamp(0.0, 1.0)) };
    if let Some(ico) = icon {
        if label.is_empty() {
            icons::tg_sized(ui.painter(), ico, draw_rect.center(), Vec2::splat(16.0), col);
        } else {
            icons::tg_sized(ui.painter(), ico, Pos2::new(draw_rect.left() + 15.0, draw_rect.center().y), Vec2::splat(15.0), col);
            ui.painter().text(Pos2::new(draw_rect.left() + 29.0, draw_rect.center().y), Align2::LEFT_CENTER, label, FontId::proportional(13.0), col);
        }
    } else if !label.is_empty() {
        ui.painter().text(draw_rect.center(), Align2::CENTER_CENTER, label, FontId::proportional(13.0), col);
    }
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Кнопка-капсула Liquid Glass (`active = true` — чистый высокий контраст с `on_accent()`).
#[allow(dead_code)]
pub fn pill_button(
    ui: &mut Ui,
    id_salt: impl Hash + std::fmt::Debug,
    size: Vec2,
    icon: Option<&str>,
    label: &str,
    active: bool,
) -> Response {
    liquid_glass_button(ui, id_salt, size, icon, label, active)
}

/// Сегментированный переключатель Liquid Glass со скользящей монохромной пилюлей.
pub fn segmented_tabs(ui: &mut Ui, id_salt: &str, labels: &[&str], active: usize) -> Option<usize> {
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
    let stretch = ((anim_x - target_x).abs() * 0.34).min(0.42);
    let pill_w = slot_w * (1.0 + stretch);
    let pill_x = track.left() + pad + anim_x * slot_w - (pill_w - slot_w) * 0.5;
    let pill = Rect::from_min_size(
        Pos2::new(pill_x.clamp(track.left() + pad, track.right() - pad - pill_w), track.top() + pad),
        Vec2::new(pill_w, slot_h),
    );
    let accent = c_orange();
    ui.painter().rect_filled(pill, CornerRadius::same((slot_h * 0.5).round() as u8), theme::t_btn(accent));
    ui.painter().rect_stroke(
        pill,
        CornerRadius::same((slot_h * 0.5).round() as u8),
        Stroke::new(0.8, Color32::from_white_alpha(135)),
        StrokeKind::Inside,
    );

    let mut picked = None;
    let active_fg = on_accent();
    for (idx, &lbl) in labels.iter().enumerate() {
        let r = Rect::from_min_size(
            Pos2::new(track.left() + pad + idx as f32 * slot_w, track.top() + pad),
            Vec2::new(slot_w, slot_h),
        );
        let resp = ui.interact(r, ui.make_persistent_id((id_salt, idx)), Sense::click());
        let hk = tween(ui.ctx(), ("seg-tab-h", id_salt, idx), f32::from(u8::from(resp.hovered() || idx == active)), 0.12);
        let fg = if idx == active { active_fg } else { lerp_color(SUBTEXT, WHITE, hk) };
        ui.painter().text(r.center(), Align2::CENTER_CENTER, lbl, FontId::proportional(13.0), fg);
        if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            picked = Some(idx);
        }
    }
    picked
}

/// Статус поля подтверждения кода `OtpInput`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OtpStatus {
    Idle,
    Error,
    #[allow(dead_code)]
    Success,
}

/// Расширенный `OtpInput` (порт React-референса с группировкой, pop-in, курсором,
/// стрелками/Delete/Backspace/Paste, встряской и статусной строкой).
pub fn otp_input_ex(
    ui: &mut Ui,
    id_salt: &str,
    code: &mut String,
    len: usize,
    group_every: usize,
    status: OtpStatus,
    status_msg: &str,
) -> bool {
    let digits_len = len.clamp(4, 8);
    let mut chars: Vec<char> = code.chars().filter(char::is_ascii_alphanumeric).take(digits_len).collect();
    let avail = ui.available_width();
    let gap = 8.0;
    let group_gaps = (digits_len.saturating_sub(1)).checked_div(group_every).unwrap_or(0);
    let extra_gap = 12.0;
    let raw_cell_w = (avail - gap * (digits_len.saturating_sub(1) as f32) - extra_gap * group_gaps as f32) / digits_len as f32;
    let cell_w = raw_cell_w.clamp(34.0, 44.0);
    let cell_h = 50.0;
    let total_w = cell_w * digits_len as f32 + gap * (digits_len.saturating_sub(1) as f32) + extra_gap * group_gaps as f32;

    let is_error = status == OtpStatus::Error;
    let is_success = status == OtpStatus::Success;
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
        let elapsed = ((now - t0) / 0.32).clamp(0.0, 1.0) as f32;
        if elapsed < 1.0 {
            // [0, -5, 4, -3, 0]
            shake_x = (elapsed * std::f32::consts::TAU * 2.0).sin() * 5.0 * (1.0 - elapsed);
            ui.ctx().request_repaint();
        }
    }

    let has_status = !status_msg.trim().is_empty();
    let total_h = cell_h + 6.0 + if has_status { 20.0 } else { 0.0 };
    let (row_r, row_resp) = ui.allocate_exact_size(Vec2::new(avail, total_h), Sense::click());
    let group_id = ui.make_persistent_id(("otp-group", id_salt));
    let mounted_k = egui::Id::NULL.with(("otp-mounted", id_salt));
    if !ui.ctx().data(|d| d.get_temp::<bool>(mounted_k).unwrap_or(false)) {
        ui.ctx().data_mut(|d| d.insert_temp(mounted_k, true));
        ui.memory_mut(|m| m.request_focus(group_id));
    }
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
                    let incoming: Vec<char> = pasted.chars().filter(char::is_ascii_alphanumeric).collect();
                    if incoming.len() >= digits_len {
                        chars.clear();
                        active_idx = 0;
                    }
                    for c in incoming {
                        if active_idx < chars.len() {
                            chars[active_idx] = c;
                        } else if chars.len() < digits_len {
                            chars.push(c);
                        }
                        if active_idx + 1 < digits_len {
                            active_idx += 1;
                        }
                    }
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
                    if active_idx < chars.len() {
                        chars.remove(active_idx);
                        active_idx = active_idx.min(chars.len()).min(digits_len.saturating_sub(1));
                        changed = true;
                    } else if !chars.is_empty() {
                        chars.pop();
                        active_idx = chars.len().min(digits_len.saturating_sub(1));
                        changed = true;
                    }
                }
                egui::Event::Key { key: egui::Key::Delete, pressed: true, .. } => {
                    if active_idx < chars.len() {
                        chars.remove(active_idx);
                        active_idx = active_idx.min(chars.len()).min(digits_len.saturating_sub(1));
                        changed = true;
                    }
                }
                egui::Event::Key { key: egui::Key::ArrowLeft, pressed: true, .. } => {
                    active_idx = active_idx.saturating_sub(1);
                }
                egui::Event::Key { key: egui::Key::ArrowRight, pressed: true, .. } => {
                    active_idx = (active_idx + 1).min(chars.len()).min(digits_len.saturating_sub(1));
                }
                egui::Event::Key { key: egui::Key::Home, pressed: true, .. } => {
                    active_idx = 0;
                }
                egui::Event::Key { key: egui::Key::End, pressed: true, .. } => {
                    active_idx = chars.len().min(digits_len.saturating_sub(1));
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

    let mut x_cur = row_r.center().x - total_w * 0.5 + shake_x;
    let top_y = row_r.top() + 3.0;
    let ptr = ui.ctx().input(|i| i.pointer.hover_pos());
    for i in 0..digits_len {
        if group_every > 0 && i > 0 && i % group_every == 0 {
            x_cur += extra_gap;
        }
        let cell = Rect::from_min_size(Pos2::new(x_cur, top_y), Vec2::new(cell_w, cell_h));
        x_cur += cell_w + gap;

        let cresp = ui.interact(cell, ui.make_persistent_id((id_salt, "cell", i)), Sense::click());
        if cresp.clicked() {
            ui.memory_mut(|m| m.request_focus(group_id));
            ui.ctx().data_mut(|d| d.insert_temp(active_idx_id, i.min(chars.len())));
        }
        let ch = chars.get(i).copied();
        let is_cur = has_focus && i == active_idx;

        // Per-cell pop-in при вводе/смене символа.
        let cell_anim_k = egui::Id::NULL.with(("otp-cell-anim", id_salt, i));
        let (prev_ch, changed_at) = ui.ctx().data_mut(|d| {
            let entry = d.get_temp::<(Option<char>, f64)>(cell_anim_k).unwrap_or((ch, now - 1.0));
            if entry.0 != ch {
                let next = (ch, now);
                d.insert_temp(cell_anim_k, next);
                next
            } else {
                entry
            }
        });
        let _ = prev_ch;
        let pop_prog = ((now - changed_at) / 0.22).clamp(0.0, 1.0) as f32;
        if pop_prog < 1.0 {
            ui.ctx().request_repaint();
        }
        let enter_k = ios_spring(pop_prog);

        // 5 состояний: Empty (утопленная), Filled, Active, Error, Success.
        let (bg, border_c, border_w) = if is_error {
            (Color32::from_rgba_unmultiplied(32, 16, 18, 220), DANGER, 2.0)
        } else if is_success {
            (Color32::from_rgba_unmultiplied(16, 30, 24, 220), Color32::from_rgb(52, 211, 153), 2.0)
        } else if is_cur {
            (Color32::from_rgba_unmultiplied(28, 28, 33, 232), Color32::from_rgb(244, 244, 248), 2.0)
        } else if ch.is_some() {
            (Color32::from_rgba_unmultiplied(24, 24, 28, 224), Color32::from_white_alpha(86), 1.5)
        } else {
            (Color32::from_rgba_unmultiplied(13, 13, 16, 200), Color32::from_white_alpha(26), 1.5)
        };
        let cr = CornerRadius::same(10);
        ui.painter().rect_filled(cell, cr, bg);
        if ch.is_none() && !is_cur && !is_error && !is_success {
            // Утопленная внутренняя тень пустой ячейки.
            ui.painter().line_segment(
                [Pos2::new(cell.left() + 6.0, cell.top() + 1.5), Pos2::new(cell.right() - 6.0, cell.top() + 1.5)],
                Stroke::new(1.5, Color32::from_black_alpha(110)),
            );
        }
        if (is_cur || cresp.hovered())
            && let Some(pt) = ptr
            && cell.expand(12.0).contains(pt)
        {
            push_radial_glow_mesh(&ui.painter().with_clip_rect(cell), pt, 34.0, Color32::from_white_alpha(18));
        }
        ui.painter().rect_stroke(cell, cr, Stroke::new(border_w, border_c), StrokeKind::Inside);

        if let Some(digit) = ch {
            let y_off = (1.0 - enter_k) * 8.0;
            let sc = 0.95 + 0.05 * enter_k;
            let txt_col = Color32::from_rgba_unmultiplied(244, 244, 248, (255.0 * enter_k.clamp(0.15, 1.0)) as u8);
            ui.painter().text(
                cell.center() + Vec2::new(0.0, y_off),
                Align2::CENTER_CENTER,
                digit.to_string(),
                FontId::monospace(18.0 * sc),
                txt_col,
            );
        } else if is_cur {
            // Плавный мигающий вертикальный курсор 1.5×17 px (1.06 с).
            let phase = (now / 1.06).fract();
            let visible = phase < 0.52;
            if visible {
                ui.painter().line_segment(
                    [Pos2::new(cell.center().x, cell.center().y - 8.5), Pos2::new(cell.center().x, cell.center().y + 8.5)],
                    Stroke::new(1.5, Color32::from_rgb(240, 240, 246)),
                );
            }
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(80));
        }
    }

    if has_status {
        let msg_col = if is_error {
            DANGER
        } else if is_success {
            Color32::from_rgb(52, 211, 153)
        } else {
            SUBTEXT
        };
        ui.painter().text(
            Pos2::new(row_r.center().x, top_y + cell_h + 12.0),
            Align2::CENTER_CENTER,
            status_msg,
            FontId::proportional(11.5),
            msg_col,
        );
    }

    if changed && chars.len() == digits_len {
        enter_submit = true;
    }
    enter_submit
}

/// Индивидуальные стеклянные ячейки OTP-кода.
#[allow(dead_code)]
pub fn otp_input(ui: &mut Ui, id_salt: &str, code: &mut String, len: usize, is_error: bool) -> bool {
    let group_every = if len == 6 { 3 } else { 0 };
    let status = if is_error { OtpStatus::Error } else { OtpStatus::Idle };
    otp_input_ex(ui, id_salt, code, len, group_every, status, "")
}

/// macOS Magnification Dock (#10): динамическая ширина каждого слота в зависимости
/// от горизонтального расстояния до курсора, подъём и всплывающий `DockLabel`.
pub fn glass_dock(
    ui: &mut Ui,
    id_salt: &str,
    items: &[(&str, &str, i32)],
    active: Option<usize>,
    compact_labels: bool,
) -> Option<usize> {
    if items.is_empty() {
        return None;
    }
    let avail_w = ui.available_width();
    let ptr = ui.ctx().input(|i| i.pointer.hover_pos());
    let panel_h = if compact_labels { 56.0 } else { 52.0 };
    let base_slot = if compact_labels { ((avail_w - 40.0) / items.len() as f32).clamp(52.0, 86.0) } else { 44.0 };
    let max_slot = if compact_labels { (base_slot + 18.0).min(106.0) } else { 70.0 };
    let distance = 128.0_f32;
    let pad_x = 10.0;
    let gap = 6.0;

    // Измеряем целевые ширины каждого слота по прошлой позиции дока.
    let bar_center_id = egui::Id::NULL.with(("dock-center-x", id_salt));
    let prev_cx = ui.ctx().data(|d| d.get_temp::<f32>(bar_center_id)).unwrap_or(avail_w * 0.5);
    let est_total_w = items.len() as f32 * base_slot + (items.len().saturating_sub(1) as f32) * gap + pad_x * 2.0;
    let est_left = prev_cx - est_total_w * 0.5 + pad_x;
    let bar_hovered = ptr.is_some_and(|p| (p.x - prev_cx).abs() <= est_total_w * 0.6 && p.y >= ui.cursor().top() - 18.0 && p.y <= ui.cursor().top() + panel_h + 20.0);

    let mut slot_widths = Vec::with_capacity(items.len());
    let mut sum_w = pad_x * 2.0 + (items.len().saturating_sub(1) as f32) * gap;
    for idx in 0..items.len() {
        let est_slot_cx = est_left + idx as f32 * (base_slot + gap) + base_slot * 0.5;
        let target_w = if bar_hovered && let Some(pt) = ptr {
            let d = (pt.x - est_slot_cx).abs();
            let k = (1.0 - d / distance).clamp(0.0, 1.0);
            let smooth = k * k * (3.0 - 2.0 * k);
            base_slot + (max_slot - base_slot) * smooth
        } else {
            base_slot
        };
        let w = spring_anim(ui.ctx(), ("dock-w", id_salt, idx), target_w, SpringSpec::BOUNCY).clamp(base_slot * 0.92, max_slot * 1.08);
        sum_w += w;
        slot_widths.push(w);
    }

    let bar_w = sum_w.min((avail_w - 16.0).max(160.0));
    let (outer_r, _) = ui.allocate_exact_size(Vec2::new(avail_w, panel_h + 12.0), Sense::hover());
    let bar_rect = Rect::from_center_size(
        Pos2::new(outer_r.center().x, outer_r.top() + panel_h * 0.5 + 4.0),
        Vec2::new(bar_w, panel_h),
    );
    ui.ctx().data_mut(|d| d.insert_temp(bar_center_id, bar_rect.center().x));
    paint_surface_interactive(ui.painter(), bar_rect, panel_h * 0.5, GlassVariant::Regular, if bar_hovered { 0.22 } else { 0.08 }, ptr);

    let slot_h = bar_rect.height() - 10.0;
    let mut x_cur = bar_rect.left() + pad_x;
    let mut slot_rects = Vec::with_capacity(items.len());
    for w in &slot_widths {
        let sr = Rect::from_min_size(Pos2::new(x_cur, bar_rect.top() + 5.0), Vec2::new(*w, slot_h));
        slot_rects.push(sr);
        x_cur += *w + gap;
    }

    if let Some(act_idx) = active
        && let Some(target_r) = slot_rects.get(act_idx.min(slot_rects.len() - 1))
    {
        let pill_x = spring_anim(ui.ctx(), ("dock-act-x", id_salt), target_r.left(), SpringSpec::BOUNCY);
        let pill_w = spring_anim(ui.ctx(), ("dock-act-w", id_salt), target_r.width(), SpringSpec::BOUNCY);
        let pill_r = Rect::from_min_size(Pos2::new(pill_x, target_r.top()), Vec2::new(pill_w.max(24.0), slot_h));
        paint_surface_interactive(ui.painter(), pill_r, slot_h * 0.5, GlassVariant::Tinted(c_orange()), 0.45, ptr);
    }

    let mut picked = None;
    let mut hovered_tip: Option<(Pos2, &str, f32)> = None;
    for (idx, &(icon_name, label, badge)) in items.iter().enumerate() {
        let srect = slot_rects[idx];
        let resp = ui.interact(srect, ui.make_persistent_id((id_salt, idx)), Sense::click());
        let is_act = active == Some(idx);
        let mag = ((slot_widths[idx] - base_slot) / (max_slot - base_slot).max(1.0)).clamp(0.0, 1.0);
        let hk = tween(ui.ctx(), ("glass-tab-h", id_salt, idx), f32::from(u8::from(resp.hovered() || is_act)), 0.14);
        let scale = 1.0 + 0.08 * ios_spring_bounce(hk) + 0.34 * mag;
        let lift_y = -6.0 * mag;
        let col = if is_act { Color32::WHITE } else { lerp_color(SUBTEXT, WHITE, hk.max(mag)) };
        let ico_y = if compact_labels { srect.top() + 15.0 + lift_y * 0.5 } else { srect.center().y + lift_y };
        let ico_c = Pos2::new(srect.center().x, ico_y);
        icons::tg_sized(ui.painter(), icon_name, ico_c, Vec2::splat(17.5 * scale), col);
        if compact_labels {
            ui.painter().text(Pos2::new(srect.center().x, srect.bottom() - 9.0), Align2::CENTER_CENTER, label, FontId::proportional(10.5), col);
        }
        if is_act && !compact_labels {
            ui.painter().circle_filled(Pos2::new(srect.center().x, bar_rect.bottom() - 4.0), 2.0, Color32::WHITE);
        }
        if badge > 0 {
            let b_txt = if badge > 99 { "99+".to_owned() } else { badge.to_string() };
            let bw = (b_txt.chars().count() as f32 * 5.8 + 8.0).max(16.0);
            let br = Rect::from_center_size(Pos2::new(ico_c.x + 14.0, ico_c.y - 6.0), Vec2::new(bw, 14.0));
            ui.painter().rect_filled(br, CornerRadius::same(7), c_orange());
            ui.painter().text(br.center(), Align2::CENTER_CENTER, b_txt, FontId::proportional(9.5), on_accent());
        }
        if resp.hovered() {
            let tip_k = tween(ui.ctx(), ("dock-tip", id_salt, idx), 1.0, 0.14);
            hovered_tip = Some((Pos2::new(srect.center().x, bar_rect.top() - 10.0 - 8.0 * tip_k), label, tip_k));
        }
        if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            picked = Some(idx);
        }
    }
    if let Some((tip_c, tip_lbl, tip_k)) = hovered_tip
        && tip_k > 0.05
    {
        paint_tooltip_card(ui.painter(), tip_c, tip_lbl);
    }
    picked
}

/// Плавающая нижняя iOS 26 Liquid Glass панель вкладок / macOS Dock.
pub fn floating_tab_bar(ui: &mut Ui, id_salt: &str, items: &[(&str, &str, i32)], active: usize) -> Option<usize> {
    glass_dock(ui, id_salt, items, Some(active), true)
}

/// 8 живых монохромных полос голоса из `#9 AIComposer`.
pub fn paint_voice_bars(p: &egui::Painter, ctx: &egui::Context, origin_center: Pos2) {
    const BARS: [f32; 8] = [0.5, 0.9, 0.6, 1.0, 0.7, 0.45, 0.85, 0.55];
    let t = ctx.input(|i| i.time) as f32;
    ctx.request_repaint_after(std::time::Duration::from_millis(32));
    let total_w = BARS.len() as f32 * 3.0 + (BARS.len() - 1) as f32 * 3.0;
    let start_x = origin_center.x - total_w * 0.5;
    for (i, &peak) in BARS.iter().enumerate() {
        let phase = t * (5.2 + (i % 4) as f32 * 0.9) + i as f32 * 0.55;
        let k = (phase.sin() * 0.5 + 0.5) * peak;
        let h = 4.0 + 16.0 * k;
        let bar_r = Rect::from_center_size(Pos2::new(start_x + i as f32 * 6.0 + 1.5, origin_center.y), Vec2::new(3.0, h));
        p.rect_filled(bar_r, CornerRadius::same(2), Color32::from_rgb(242, 242, 246));
    }
}

/// Настройки 3D-карточки `TiltedCard` (#16 React Bits).
#[derive(Debug, Clone)]
pub struct TiltedCardConfig {
    pub size: Vec2,
    pub radius: f32,
    pub rotate_amplitude: f32,
    pub scale_on_hover: f32,
    pub perspective: f32,
    pub smoothness: f32,
    pub glare: f32,
    pub shadow: f32,
    pub parallax: f32,
    pub floating: bool,
    pub has_back: bool,
    pub caption: String,
}

impl Default for TiltedCardConfig {
    fn default() -> Self {
        Self {
            size: Vec2::new(200.0, 140.0),
            radius: 15.0,
            rotate_amplitude: 12.0,
            scale_on_hover: 1.05,
            perspective: 800.0,
            smoothness: 0.6,
            glare: 0.5,
            shadow: 0.5,
            parallax: 1.0,
            floating: false,
            has_back: false,
            caption: String::new(),
        }
    }
}

/// Результат рендера `TiltedCard`: позиция наклонённой карточки, активная сторона и хелпер параллакса.
pub struct TiltedCardOutput {
    pub response: Response,
    pub rect: Rect,
    pub is_back: bool,
    pub hover_k: f32,
    tilt_x: f32,
    tilt_y: f32,
    parallax: f32,
    caption: String,
    caption_pos: Option<Pos2>,
    caption_alpha: f32,
}

impl TiltedCardOutput {
    /// Смещение слоя `TiltedCardLayer` на заданной глубине `depth` (`px`).
    pub fn parallax_offset(&self, depth: f32) -> Vec2 {
        let factor = depth * 0.035 * self.parallax;
        Vec2::new(self.tilt_y * factor, -self.tilt_x * factor)
    }

    /// Рисует парящий у курсора белый бейдж `captionText` поверх содержимого карточки.
    pub fn paint_caption_badge(&self, p: &egui::Painter) {
        if self.caption.trim().is_empty() || self.caption_alpha <= 0.04 {
            return;
        }
        let Some(pos) = self.caption_pos else { return };
        let a = (self.caption_alpha * 255.0).clamp(0.0, 255.0) as u8;
        let bg = Color32::from_rgba_unmultiplied(245, 245, 248, a);
        let fg = Color32::from_rgba_unmultiplied(12, 12, 14, a);
        let g = p.layout_no_wrap(self.caption.clone(), FontId::proportional(11.0), fg);
        let badge = Rect::from_min_size(pos + Vec2::new(12.0, -14.0), g.size() + Vec2::new(16.0, 8.0));
        p.rect_filled(badge.translate(Vec2::new(0.0, 3.0)), CornerRadius::same(6), Color32::from_black_alpha((70.0 * self.caption_alpha) as u8));
        p.rect_filled(badge, CornerRadius::same(6), bg);
        p.galley(badge.min + Vec2::new(8.0, 4.0), g, fg);
    }
}

/// Проецирует 2D-смещение `(lx, ly)` от центра карточки с углом наклона `(rx_deg, ry_deg)` и перспективой `d`.
pub fn project_tilted_point(center: Pos2, lx: f32, ly: f32, rx_deg: f32, ry_deg: f32, perspective: f32) -> Pos2 {
    let rx = rx_deg.to_radians();
    let ry = ry_deg.to_radians();
    let x1 = lx * ry.cos();
    let z1 = -lx * ry.sin();
    let y2 = ly * rx.cos() - z1 * rx.sin();
    let z2 = ly * rx.sin() + z1 * rx.cos();
    let persp = perspective.max(180.0);
    let scale = (persp / (persp - z2).max(40.0)).clamp(0.65, 1.5);
    Pos2::new(center.x + x1 * scale, center.y + y2 * scale)
}

/// React Bits #16 `<TiltedCard />`: 3D перспективный наклон за курсором, смещающаяся тень,
/// направленный `glare`-блик, переворот на 180° при клике (`has_back`) и плавающий `caption`.
pub fn tilted_card(ui: &mut Ui, id_salt: impl Hash + std::fmt::Debug, cfg: &TiltedCardConfig) -> TiltedCardOutput {
    let (base_rect, resp) = ui.allocate_exact_size(cfg.size, Sense::click());
    let hovered = resp.hovered();
    let ptr = ui.ctx().input(|i| i.pointer.hover_pos());

    let flip_id = egui::Id::NULL.with(("tc-flip-state", &id_salt));
    let mut flipped = ui.ctx().data(|d| d.get_temp::<bool>(flip_id).unwrap_or(false));
    if cfg.has_back && resp.clicked() {
        flipped = !flipped;
        ui.ctx().data_mut(|d| d.insert_temp(flip_id, flipped));
    }

    let t_now = ui.input(|i| i.time) as f32;
    let idle_rx = if cfg.floating && !hovered { (t_now * 1.3).sin() * 2.5 } else { 0.0 };
    let idle_ry = if cfg.floating && !hovered { (t_now * 1.05 + 0.8).cos() * 3.0 } else { 0.0 };
    if cfg.floating {
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(32));
    }

    let (target_rx, target_ry) = if hovered && let Some(pt) = ptr {
        let nx = ((pt.x - base_rect.center().x) / (base_rect.width() * 0.5).max(1.0)).clamp(-1.0, 1.0);
        let ny = ((pt.y - base_rect.center().y) / (base_rect.height() * 0.5).max(1.0)).clamp(-1.0, 1.0);
        (-ny * cfg.rotate_amplitude, nx * cfg.rotate_amplitude)
    } else {
        (idle_rx, idle_ry)
    };

    let dur = (0.14 + cfg.smoothness.clamp(0.0, 1.0) * 0.34).clamp(0.14, 0.55);
    let spec = SpringSpec::from_duration_bounce(dur, 0.22 * cfg.smoothness.clamp(0.0, 1.0));
    let rx = spring_anim(ui.ctx(), ("tc-rx", &id_salt), target_rx, spec);
    let ry = spring_anim(ui.ctx(), ("tc-ry", &id_salt), target_ry, spec);
    let scale = spring_anim(
        ui.ctx(),
        ("tc-sc", &id_salt),
        if hovered { cfg.scale_on_hover } else { 1.0 },
        SpringSpec::SNAPPY,
    );
    let flip_deg = spring_anim(
        ui.ctx(),
        ("tc-flip", &id_salt),
        if flipped { 180.0 } else { 0.0 },
        SpringSpec::from_duration_bounce(0.55, 0.18),
    );
    let hk = tween(ui.ctx(), ("tc-h", &id_salt), f32::from(u8::from(hovered)), 0.14);

    let flip_rad = flip_deg.to_radians();
    let flip_cos = flip_rad.cos();
    let is_back = flip_cos < 0.0;
    let flip_scale_x = flip_cos.abs().max(0.05);

    let scaled_w = base_rect.width() * scale * flip_scale_x;
    let scaled_h = base_rect.height() * scale;
    let draw_rect = Rect::from_center_size(base_rect.center(), Vec2::new(scaled_w, scaled_h));
    let r = (cfg.radius * scale).clamp(4.0, draw_rect.width().min(draw_rect.height()) * 0.5);

    // Сдвигающаяся мягкая тень под 3D-карточкой.
    if cfg.shadow > 0.01 {
        let sh_off = Vec2::new(-ry * 0.85, 6.0 + rx * 0.85);
        let sh_a = (46.0 * cfg.shadow * (0.7 + 0.45 * hk)).clamp(0.0, 96.0) as u8;
        ui.painter().rect_filled(draw_rect.translate(sh_off).expand(3.0), CornerRadius::same(r.round() as u8), Color32::from_black_alpha(sh_a));
    }

    // Перспективно проецированный контур карточки.
    let center = draw_rect.center();
    let sq = squircle_points(draw_rect, r);
    let proj_pts: Vec<Pos2> = sq
        .into_iter()
        .map(|pt| project_tilted_point(center, pt.x - center.x, pt.y - center.y, rx, ry, cfg.perspective))
        .collect();
    let bg = if is_back { Color32::from_rgba_unmultiplied(24, 24, 29, 232) } else { Color32::from_rgba_unmultiplied(18, 18, 22, 226) };
    ui.painter().add(Shape::convex_polygon(proj_pts.clone(), bg, Stroke::NONE));

    // Направленный блик (glare) к курсору + световая кромка со стороны наклона.
    if cfg.glare > 0.01 {
        let glare_pt = if hovered && let Some(pt) = ptr {
            pt
        } else {
            Pos2::new(draw_rect.left() + draw_rect.width() * 0.25, draw_rect.top() + draw_rect.height() * 0.2)
        };
        let glare_r = draw_rect.width().max(draw_rect.height()) * 0.65;
        let flip_flash = (1.0 - flip_cos.abs()) * 0.5;
        let glare_a = ((22.0 + 38.0 * hk + 45.0 * flip_flash) * cfg.glare).clamp(0.0, 92.0) as u8;
        push_radial_glow_mesh(&ui.painter().with_clip_rect(draw_rect), glare_pt, glare_r, Color32::from_white_alpha(glare_a));
    }
    let rim_a = (46.0 + 68.0 * hk).clamp(0.0, 165.0) as u8;
    ui.painter().add(Shape::closed_line(proj_pts, Stroke::new(1.1, Color32::from_white_alpha(rim_a))));

    let resp = if cfg.has_back { resp.on_hover_cursor(egui::CursorIcon::PointingHand) } else { resp };
    TiltedCardOutput {
        response: resp,
        rect: draw_rect,
        is_back,
        hover_k: hk,
        tilt_x: rx,
        tilt_y: ry,
        parallax: cfg.parallax,
        caption: cfg.caption.clone(),
        caption_pos: ptr.filter(|_| hovered),
        caption_alpha: hk,
    }
}

/// #1 Liquid Glass Modal Dialog Frame: пружинная стеклянная карточка модального окна.
pub fn paint_modal_card(p: &egui::Painter, rect: Rect, radius: f32, appear_k: f32) {
    let k = appear_k.clamp(0.0, 1.0);
    if k <= 0.01 {
        return;
    }
    paint_surface(p, rect, radius, GlassVariant::Regular, 0.14 * k);
}

/// #3 Liquid Glass Tooltip Card: компактная стеклянная подсказка с верхним бликом.
pub fn paint_tooltip_card(p: &egui::Painter, anchor_center: Pos2, text: &str) {
    if text.trim().is_empty() {
        return;
    }
    let g = p.layout_no_wrap(text.to_owned(), FontId::proportional(11.0), WHITE);
    let tip_r = Rect::from_center_size(anchor_center, Vec2::new(g.size().x + 16.0, 22.0));
    paint_surface(p, tip_r, 11.0, GlassVariant::Regular, 0.28);
    p.galley(tip_r.center() - g.size() * 0.5, g, WHITE);
}

/// #8 Counter (Animated Digits): плавно пересчитываемый цифровой счётчик на пружине.
pub fn animated_counter(ui: &mut Ui, id_salt: impl Hash + std::fmt::Debug, value: i64, font: FontId, color: Color32) {
    let cur = spring_anim(ui.ctx(), ("anim-cnt", &id_salt), value as f32, SpringSpec::SNAPPY);
    let shown = cur.round() as i64;
    let tilt = ((value as f32 - cur).clamp(-1.0, 1.0)) * 2.5;
    let txt = shown.to_string();
    let g = ui.painter().layout_no_wrap(txt, font, color);
    let (r, _) = ui.allocate_exact_size(g.size() + Vec2::new(2.0, 0.0), Sense::hover());
    ui.painter().galley(Pos2::new(r.left() + 1.0, r.center().y - g.size().y * 0.5 + tilt), g, color);
}

/// #9 Magnet Button: мягкое притяжение иконки/FAB к указателю в радиусе `radius`.
pub fn magnet_offset(
    ctx: &egui::Context,
    id_salt: impl Hash + std::fmt::Debug,
    center: Pos2,
    radius: f32,
    max_pull: f32,
) -> Vec2 {
    let target = ctx
        .input(|i| i.pointer.hover_pos())
        .and_then(|pt| {
            let delta = pt - center;
            let dist = delta.length();
            if dist > 0.5 && dist < radius {
                let strength = (1.0 - dist / radius).powi(2) * max_pull;
                Some(delta / dist * strength)
            } else {
                None
            }
        })
        .unwrap_or(Vec2::ZERO);
    Vec2::new(
        spring_anim(ctx, ("mag-x", &id_salt), target.x, SpringSpec::BOUNCY),
        spring_anim(ctx, ("mag-y", &id_salt), target.y, SpringSpec::BOUNCY),
    )
}

/// #10 Star Border: монохромный бегущий спекулярный контур вокруг карточек Stars и премиум-пилюль.
pub fn paint_star_border(p: &egui::Painter, ctx: &egui::Context, rect: Rect, radius: f32, active: bool) {
    let phase = (ctx.input(|i| i.time) * if active { 2.4 } else { 1.3 }) as f32;
    if active {
        ctx.request_repaint_after(std::time::Duration::from_millis(32));
    }
    let cr = CornerRadius::same(radius.clamp(0.0, rect.width().min(rect.height()) * 0.5).round() as u8);
    p.rect_stroke(rect, cr, Stroke::new(1.1, Color32::from_white_alpha(if active { 74 } else { 36 })), StrokeKind::Inside);
    let w = (rect.width() - radius * 1.4).max(8.0);
    let t0 = phase.sin() * 0.5 + 0.5;
    let t1 = 1.0 - t0;
    let span = (w * 0.24).clamp(10.0, 34.0);
    let top_x = rect.left() + radius * 0.7 + (w - span) * t0;
    let bot_x = rect.left() + radius * 0.7 + (w - span) * t1;
    p.line_segment(
        [Pos2::new(top_x, rect.top() + 0.8), Pos2::new(top_x + span, rect.top() + 0.8)],
        Stroke::new(1.8, Color32::from_white_alpha(if active { 210 } else { 140 })),
    );
    p.line_segment(
        [Pos2::new(bot_x, rect.bottom() - 0.8), Pos2::new(bot_x + span, rect.bottom() - 0.8)],
        Stroke::new(1.5, Color32::from_white_alpha(if active { 140 } else { 80 })),
    );
}

/// #11 Spotlight Card: матовая стеклянная карточка с живым фонариком-подсветкой под курсором.
pub fn paint_spotlight_card(p: &egui::Painter, rect: Rect, radius: f32, pointer_pos: Option<Pos2>, hover_k: f32) {
    paint_surface_interactive(p, rect, radius, GlassVariant::Regular, hover_k, pointer_pos);
}

/// #13 Gradual Blur: многослойное матовое затухание у верхней и нижней кромки прокручиваемой ленты.
pub fn paint_gradual_blur_edges(p: &egui::Painter, rect: Rect, top_h: f32, bot_h: f32) {
    if rect.height() < 60.0 || rect.width() < 20.0 {
        return;
    }
    let bg = theme::c_window_bg();
    let op = theme::opacity_f32();
    let steps = 6usize;
    if top_h > 2.0 {
        let step_h = top_h / steps as f32;
        for i in 0..steps {
            let k = 1.0 - i as f32 / steps as f32;
            let a = (135.0 * k * k * op).clamp(0.0, 210.0) as u8;
            let strip = Rect::from_min_size(Pos2::new(rect.left(), rect.top() + i as f32 * step_h), Vec2::new(rect.width(), step_h + 0.5));
            p.rect_filled(strip, 0.0, Color32::from_rgba_unmultiplied(bg.r(), bg.g(), bg.b(), a));
        }
    }
    if bot_h > 2.0 {
        let step_h = bot_h / steps as f32;
        for i in 0..steps {
            let k = 1.0 - i as f32 / steps as f32;
            let a = (145.0 * k * k * op).clamp(0.0, 220.0) as u8;
            let y = rect.bottom() - (i as f32 + 1.0) * step_h;
            let strip = Rect::from_min_size(Pos2::new(rect.left(), y), Vec2::new(rect.width(), step_h + 0.5));
            p.rect_filled(strip, 0.0, Color32::from_rgba_unmultiplied(bg.r(), bg.g(), bg.b(), a));
        }
    }
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
    let pos = Pos2::new((screen.center().x - w * 0.5).max(12.0), (screen.bottom() - h - 72.0 + y_shift).max(48.0));
    egui::Area::new(egui::Id::new("rg-glass-toast"))
        .order(egui::Order::Foreground)
        .interactable(false)
        .fixed_pos(pos)
        .show(ctx, |ui| {
            ui.set_opacity(k);
            let (rect, _) = ui.allocate_exact_size(Vec2::new(w, h), Sense::hover());
            let tint = if is_error { GlassVariant::Tinted(Color32::from_rgb(210, 62, 62)) } else { GlassVariant::Regular };
            paint_surface(ui.painter(), rect, h * 0.5, tint, 0.2);
            let dot_c = Pos2::new(rect.left() + 14.0, rect.center().y);
            ui.painter().circle_filled(dot_c, 3.5, if is_error { Color32::from_rgb(255, 130, 130) } else { c_orange() });
            ui.painter().galley(Pos2::new(rect.left() + 24.0, rect.center().y - galley.size().y * 0.5), galley, WHITE);
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spring_duration_bounce_and_squircle_geometry_work() {
        let press = SpringSpec::PRESS;
        assert!(press.stiffness > 500.0 && press.damping > 20.0);
        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(200.0, 64.0));
        let pts = squircle_points(rect, 20.0);
        assert!(pts.len() >= 40);
        assert!(pts.iter().all(|p| p.x >= -0.1 && p.x <= 200.1 && p.y >= -0.1 && p.y <= 64.1));
    }

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
        fn collect(s: &Shape, buf: &mut String) {
            match s {
                Shape::Text(t) => {
                    buf.push_str(t.galley.text());
                    buf.push('\n');
                }
                Shape::Vec(v) => v.iter().for_each(|x| collect(x, buf)),
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
                    let _ = otp_input_ex(ui, "otp-test", &mut code, 6, 3, OtpStatus::Error, "Неверный код");
                });
            },
        );
        out.textures_delta.clear();
        assert!(tab_pick.is_none());
        let mut txt = String::new();
        fn collect_t(s: &Shape, buf: &mut String) {
            match s {
                Shape::Text(t) => {
                    buf.push_str(t.galley.text());
                    buf.push(' ');
                }
                Shape::Vec(v) => v.iter().for_each(|x| collect_t(x, buf)),
                _ => {}
            }
        }
        out.shapes.iter().for_each(|cs| collect_t(&cs.shape, &mut txt));
        assert!(txt.contains("Эмодзи") && txt.contains("Стикеры") && txt.contains("GIF"));
        assert!(txt.contains('1') && txt.contains('2') && txt.contains("Неверный код"));
    }

    #[test]
    fn tilted_card_and_voice_bars_render_and_project_cleanly() {
        let p0 = project_tilted_point(Pos2::new(100.0, 100.0), -50.0, -40.0, 10.0, 10.0, 800.0);
        let p1 = project_tilted_point(Pos2::new(100.0, 100.0), 50.0, -40.0, 10.0, 10.0, 800.0);
        assert!(p0.x < p1.x);

        let ctx = egui::Context::default();
        let mut out = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(420.0, 480.0))),
                ..Default::default()
            },
            |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    let card = Rect::from_min_size(Pos2::new(20.0, 20.0), Vec2::new(300.0, 120.0));
                    paint_modal_card(ui.painter(), card, 16.0, 1.0);
                    paint_spotlight_card(ui.painter(), card, 16.0, Some(Pos2::new(80.0, 60.0)), 0.7);
                    paint_star_border(ui.painter(), ui.ctx(), card, 16.0, false);
                    paint_tooltip_card(ui.painter(), Pos2::new(140.0, 16.0), "Подсказка");
                    animated_counter(ui, "cnt-test", 42, FontId::proportional(14.0), WHITE);
                    let off = magnet_offset(ui.ctx(), "mag-test", card.center(), 48.0, 4.0);
                    assert!(off.length() <= 5.0);
                    paint_gradual_blur_edges(ui.painter(), card, 18.0, 18.0);
                    paint_voice_bars(ui.painter(), ui.ctx(), Pos2::new(100.0, 200.0));
                    let tc = tilted_card(
                        ui,
                        "tc-test",
                        &TiltedCardConfig {
                            size: Vec2::new(180.0, 120.0),
                            has_back: true,
                            caption: "Подарок".to_owned(),
                            ..Default::default()
                        },
                    );
                    assert!(!tc.is_back);
                    assert_eq!(tc.parallax_offset(30.0), Vec2::ZERO);
                    tc.paint_caption_badge(ui.painter());
                });
            },
        );
        out.textures_delta.clear();
        let mut txt = String::new();
        fn collect_all(s: &Shape, buf: &mut String) {
            match s {
                Shape::Text(t) => {
                    buf.push_str(t.galley.text());
                    buf.push(' ');
                }
                Shape::Vec(v) => v.iter().for_each(|x| collect_all(x, buf)),
                _ => {}
            }
        }
        out.shapes.iter().for_each(|cs| collect_all(&cs.shape, &mut txt));
        assert!(txt.contains("Подсказка") && txt.contains("42"));
    }
}
