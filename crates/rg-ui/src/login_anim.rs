//! Анимированные векторные талисманы экрана входа в стиле Telegram (чистое полотно без фона и кругов, 60 FPS HiDPI).

use eframe::egui::{self, Color32, Pos2, Sense, Stroke, Vec2};

use super::theme::{self, c_orange, ios_spring_bounce, lerp_color};

/// Плавная кривая Безье (кубическая) по набору семплов для гладких линий и ушек.
fn quad_arc(p0: Pos2, p1: Pos2, p2: Pos2, steps: usize) -> Vec<Pos2> {
    (0..=steps)
        .map(|i| {
            let t = i as f32 / steps as f32;
            let u = 1.0 - t;
            Pos2::new(
                u * u * p0.x + 2.0 * u * t * p1.x + t * t * p2.x,
                u * u * p0.y + 2.0 * u * t * p1.y + t * t * p2.y,
            )
        })
        .collect()
}

/// Поворачивает вектор вокруг нуля на угол `rad`.
fn rot(v: Vec2, rad: f32) -> Vec2 {
    let (s, c) = rad.sin_cos();
    Vec2::new(v.x * c - v.y * s, v.x * s + v.y * c)
}

/// Логотип Rustgramm для стартового экрана и ввода телефона: чистый парящий бумажный самолётик без круга и пульсации.
pub fn paint_plane_mascot(ui: &mut egui::Ui, _busy: bool, enter_k: f32) {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(108.0), Sense::hover());
    let c = rect.center();
    let p = ui.painter();
    let hk = theme::tween(ui.ctx(), "lg-plane-h", f32::from(u8::from(resp.hovered())), 0.18);
    let spring_k = ios_spring_bounce(enter_k.clamp(0.0, 1.0));
    let scale = (0.86 + 0.14 * spring_k) * (1.0 + 0.05 * hk);
    let r = 54.0 * scale;
    let orange = c_orange();
    let fold_mid = lerp_color(orange, Color32::from_rgb(175, 82, 16), 0.35);
    let fold_deep = lerp_color(orange, Color32::from_rgb(120, 48, 8), 0.55);

    let float_y = (1.0 - spring_k) * 10.0 - hk * 2.5;
    let float_x = -(1.0 - spring_k) * 6.0;
    let pc = c + Vec2::new(float_x, float_y);
    let k = r / 54.0;
    let tilt = hk * 0.04;
    let pt = |vx: f32, vy: f32| pc + rot(Vec2::new(vx, vy) * k, tilt);

    let p_left = pt(-34.0, 3.0);
    let p_tip = pt(32.0, -24.0);
    let p_bot = pt(17.0, 27.0);
    let p_fold_top = pt(-10.0, 9.0);
    let p_fold_mid = pt(-6.0, 21.0);

    p.add(egui::Shape::convex_polygon(
        vec![p_left, p_tip, p_fold_top],
        Color32::WHITE,
        Stroke::NONE,
    ));
    p.add(egui::Shape::convex_polygon(
        vec![p_fold_top, p_tip, p_bot],
        orange,
        Stroke::NONE,
    ));
    p.add(egui::Shape::convex_polygon(
        vec![p_fold_top, p_tip, p_fold_mid],
        fold_mid,
        Stroke::NONE,
    ));
    p.add(egui::Shape::convex_polygon(
        vec![p_fold_mid, p_tip, pt(2.0, 15.0)],
        fold_deep,
        Stroke::NONE,
    ));
}

/// Общая векторная мордочка обезьянки Telegram без фона и без подложек.
#[allow(clippy::too_many_arguments)]
fn paint_monkey_head(
    p: &egui::Painter,
    c: Pos2,
    scale: f32,
    gaze_x: f32,
    gaze_y: f32,
    blink_k: f32,
    surprised: bool,
    peek_right_only: Option<f32>,
) {
    let s = scale;
    let fur_dark = Color32::from_rgb(112, 68, 42);
    let fur_mid = Color32::from_rgb(138, 86, 54);
    let ear_inner = Color32::from_rgb(232, 180, 140);
    let face_skin = Color32::from_rgb(244, 200, 162);
    let blush = Color32::from_rgba_unmultiplied(245, 118, 96, 90);
    let line_col = Color32::from_rgb(76, 44, 26);

    // Уши (без фонового круга — чисто на холсте).
    for dir in [-1.0_f32, 1.0_f32] {
        let ec = c + Vec2::new(dir * 38.0 * s, -4.0 * s);
        p.circle_filled(ec, 14.5 * s, fur_dark);
        p.circle_filled(ec + Vec2::new(-dir * 1.5 * s, 0.5 * s), 9.0 * s, ear_inner);
    }

    // Чубчик сверху.
    let tuft = c + Vec2::new(0.0, -40.0 * s);
    p.circle_filled(tuft + Vec2::new(-5.0 * s, 2.0 * s), 7.5 * s, fur_dark);
    p.circle_filled(tuft + Vec2::new(4.0 * s, 0.0), 8.5 * s, fur_dark);

    // Основная голова.
    p.circle_filled(c + Vec2::new(0.0, -2.0 * s), 37.5 * s, fur_mid);

    // Светлая маска глаз и мордочка.
    p.circle_filled(c + Vec2::new(-12.5 * s, -6.5 * s), 16.5 * s, face_skin);
    p.circle_filled(c + Vec2::new(12.5 * s, -6.5 * s), 16.5 * s, face_skin);
    // Мордочка снизу (овал из двух перекрывающихся кругов + центр).
    p.circle_filled(c + Vec2::new(-9.0 * s, 14.0 * s), 17.0 * s, face_skin);
    p.circle_filled(c + Vec2::new(9.0 * s, 14.0 * s), 17.0 * s, face_skin);
    p.circle_filled(c + Vec2::new(0.0, 15.0 * s), 18.5 * s, face_skin);

    // Лёгкий румянец.
    p.circle_filled(c + Vec2::new(-21.0 * s, 8.0 * s), 5.5 * s, blush);
    p.circle_filled(c + Vec2::new(21.0 * s, 8.0 * s), 5.5 * s, blush);

    // Глаза: белки, зрачки (следят за вводом!) и блики; при моргании — дуги.
    for (idx, dir) in [-1.0_f32, 1.0_f32].iter().enumerate() {
        let eye_c = c + Vec2::new(*dir * 12.0 * s, -6.5 * s);
        let winking = peek_right_only.is_some_and(|pk| idx == 1 && pk > 0.25);
        if blink_k > 0.65 && !winking {
            let arc = quad_arc(
                eye_c + Vec2::new(-6.0 * s, 0.0),
                eye_c + Vec2::new(0.0, 3.8 * s),
                eye_c + Vec2::new(6.0 * s, 0.0),
                8,
            );
            p.add(egui::Shape::line(arc, Stroke::new(2.2 * s, line_col)));
        } else {
            p.circle_filled(eye_c, 7.6 * s, Color32::WHITE);
            p.circle_stroke(eye_c, 7.6 * s, Stroke::new(1.0 * s, Color32::from_rgb(205, 165, 132)));
            let pupil_c = eye_c + Vec2::new(gaze_x * 3.0 * s, gaze_y * 2.5 * s);
            p.circle_filled(pupil_c, 4.3 * s, Color32::from_rgb(36, 26, 22));
            p.circle_filled(pupil_c + Vec2::new(-1.4 * s, -1.5 * s), 1.5 * s, Color32::WHITE);
        }
    }

    // Ноздри.
    p.circle_filled(c + Vec2::new(-3.4 * s, 8.5 * s), 2.0 * s, line_col);
    p.circle_filled(c + Vec2::new(3.4 * s, 8.5 * s), 2.0 * s, line_col);

    // Рот: удивлённый кружок или милая улыбка.
    if surprised {
        p.circle_filled(c + Vec2::new(0.0, 20.5 * s), 4.6 * s, Color32::from_rgb(138, 62, 56));
    } else {
        let smile = quad_arc(
            c + Vec2::new(-9.5 * s, 17.5 * s),
            c + Vec2::new(0.0, 24.5 * s),
            c + Vec2::new(9.5 * s, 17.5 * s),
            12,
        );
        p.add(egui::Shape::line(smile, Stroke::new(2.3 * s, line_col)));
    }
}

/// Анимированная обезьянка для экрана кода подтверждения: следит глазами за вводом цифр (БЕЗ фона).
pub fn paint_code_monkey(ui: &mut egui::Ui, code_len: usize, has_error: bool) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(112.0), Sense::hover());
    let t = ui.input(|i| i.time);
    let bob = (t * 2.6).sin() as f32 * 2.2;
    let err_k = theme::tween(ui.ctx(), "lg-mk-err", f32::from(u8::from(has_error)), 0.18);
    let wobble = if has_error {
        (t * 24.0).sin() as f32 * 4.5 * err_k
    } else {
        0.0
    };
    let gaze_target = ((code_len.min(5) as f32 / 5.0) * 2.0 - 1.0).clamp(-1.0, 1.0);
    let gaze_x = theme::tween(ui.ctx(), "lg-mk-gaze", gaze_target, 0.14);
    let cyc = t % 3.6;
    let blink_k = if cyc > 3.42 {
        1.0 - ((cyc - 3.51).abs() / 0.09).clamp(0.0, 1.0) as f32
    } else {
        0.0
    };
    let c = rect.center() + Vec2::new(wobble, bob);
    paint_monkey_head(ui.painter(), c, 1.12, gaze_x, 0.45, blink_k, has_error, None);
    ui.ctx().request_repaint_after(std::time::Duration::from_millis(33));
}

/// Обезьянка 2FA для облачного пароля: закрывает глаза лапками и подглядывает при показе пароля (БЕЗ фона).
pub fn paint_password_monkey(ui: &mut egui::Ui, show_password: bool, pass_len: usize) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(112.0), Sense::hover());
    let t = ui.input(|i| i.time);
    let bob = (t * 2.3).sin() as f32 * 2.0;
    let s = 1.12_f32;
    let c = rect.center() + Vec2::new(0.0, bob);

    // Левая лапа закрывает глаз всегда при вводе/скрытом пароле; правая приоткрывается при `show_password`.
    let hide_target = if pass_len > 0 || !show_password { 1.0 } else { 0.45 };
    let cover_left = theme::tween(ui.ctx(), "lg-2fa-l", hide_target, 0.22);
    let cover_right = theme::tween(
        ui.ctx(),
        "lg-2fa-r",
        if show_password { 0.36 } else { hide_target },
        0.22,
    );
    let peek_amt = if show_password { Some(1.0 - cover_right) } else { None };
    let cyc = t % 4.1;
    let blink_k = if cyc > 3.92 { 1.0 } else { 0.0 };

    let p = ui.painter();
    paint_monkey_head(p, c, s, 0.35, 0.65, blink_k, false, peek_amt);

    let paw_fur = Color32::from_rgb(122, 74, 46);
    let paw_pad = Color32::from_rgb(232, 184, 146);
    for (dir, cov) in [(-1.0_f32, cover_left), (1.0_f32, cover_right)] {
        let rest = c + Vec2::new(dir * 26.0 * s, 28.0 * s);
        let eye_pos = c + Vec2::new(dir * 13.0 * s, -6.5 * s);
        let paw_c = Pos2::new(
            rest.x + (eye_pos.x - rest.x) * cov,
            rest.y + (eye_pos.y - rest.y) * cov,
        );
        // Предплечье к низу.
        let arm_base = c + Vec2::new(dir * 32.0 * s, 40.0 * s);
        p.line_segment([arm_base, paw_c], Stroke::new(11.5 * s, paw_fur));
        // Ладошка + пальчики.
        p.circle_filled(paw_c, 12.2 * s, paw_fur);
        p.circle_filled(paw_c + Vec2::new(0.0, 1.5 * s), 6.2 * s, paw_pad);
        for f in [-4.5_f32, 0.0, 4.5] {
            p.circle_filled(paw_c + Vec2::new(f * s, -9.5 * s), 3.4 * s, paw_fur);
        }
    }
    ui.ctx().request_repaint_after(std::time::Duration::from_millis(33));
}

/// Статичный оранжевый векторный ключ без бликов и фоновой пульсации (для экрана ввода API-ключей).
pub fn paint_keys_mascot(ui: &mut egui::Ui) {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(96.0), Sense::hover());
    let hk = theme::tween(ui.ctx(), "lg-keys-h", f32::from(u8::from(resp.hovered())), 0.16);
    let c = rect.center() + Vec2::new(0.0, -hk * 2.0);
    let orange = c_orange();
    let edge = lerp_color(orange, Color32::from_rgb(255, 200, 130), 0.25);
    let p = ui.painter();
    let pt = |x: f32, y: f32| c + Vec2::new(x, y);

    let bow = pt(-16.0, 0.0);
    p.circle_stroke(bow, 15.0, Stroke::new(6.0, orange));
    p.circle_stroke(bow, 15.0, Stroke::new(1.5, edge));
    p.line_segment([pt(-2.0, 0.0), pt(30.0, 0.0)], Stroke::new(6.0, orange));
    p.line_segment([pt(16.0, 0.0), pt(16.0, 11.0)], Stroke::new(5.5, orange));
    p.line_segment([pt(26.0, 0.0), pt(26.0, 14.0)], Stroke::new(5.5, orange));
}
