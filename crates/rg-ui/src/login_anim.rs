//! Анимированные векторные талисманы экрана входа в стиле Telegram (чистое полотно без фона и кругов, 60 FPS HiDPI).

use eframe::egui::{self, Color32, Pos2, Sense, Stroke, Vec2, epaint::PathShape};

use super::theme::{self, c_orange, ios_spring_bounce, lerp_color};

/// Плавная квадратичная кривая Безье по набору семплов.
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

/// Плавный сглаженный контур обезьяньей головы (без резких стыков кругов).
fn sample_closed_contour(center: Pos2, s: f32, pts: &[(f32, f32)]) -> Vec<Pos2> {
    pts.iter().map(|&(dx, dy)| center + Vec2::new(dx * s, dy * s)).collect()
}

/// Поворачивает вектор вокруг нуля на угол `rad`.
fn rot(v: Vec2, rad: f32) -> Vec2 {
    let (s, c) = rad.sin_cos();
    Vec2::new(v.x * c - v.y * s, v.x * s + v.y * c)
}

/// Логотип Rustgramm для стартового экрана и ввода телефона: чистый парящий бумажный самолётик без круга и пульсации.
pub fn paint_plane_mascot(ui: &mut egui::Ui, _busy: bool, enter_k: f32) {
    let side = if ui.available_width() < 360.0 { 78.0 } else { 104.0 };
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(side), Sense::hover());
    let c = rect.center();
    let p = ui.painter();
    let hk = theme::tween(ui.ctx(), "lg-plane-h", f32::from(u8::from(resp.hovered())), 0.18);
    let spring_k = ios_spring_bounce(enter_k.clamp(0.0, 1.0));
    let scale = (0.86 + 0.14 * spring_k) * (1.0 + 0.05 * hk);
    let k = (side * 0.5) * scale / 54.0;
    let orange = c_orange();
    let fold_mid = lerp_color(orange, Color32::from_rgb(176, 84, 18), 0.36);
    let fold_deep = lerp_color(orange, Color32::from_rgb(112, 46, 10), 0.58);

    let float_y = (1.0 - spring_k) * 8.0 - hk * 2.5;
    let float_x = -(1.0 - spring_k) * 5.0;
    let pc = c + Vec2::new(float_x, float_y);
    let tilt = hk * 0.045;
    let pt = |vx: f32, vy: f32| pc + rot(Vec2::new(vx, vy) * k, tilt);

    let p_left = pt(-34.0, 3.0);
    let p_tip = pt(33.0, -25.0);
    let p_bot = pt(17.5, 28.0);
    let p_fold_top = pt(-9.5, 9.0);
    let p_fold_mid = pt(-5.5, 21.5);
    let p_notch = pt(2.5, 15.5);

    // Мягкая тень под левым крылом (без фонового круга или свечения).
    let shadow_off = Vec2::new(0.0, 4.0 * k);
    p.add(egui::Shape::convex_polygon(
        vec![p_left + shadow_off, p_tip + shadow_off, p_bot + shadow_off],
        Color32::from_black_alpha(44),
        Stroke::NONE,
    ));

    // Верхнее левое крыло с мягким бликом передней кромки.
    p.add(egui::Shape::convex_polygon(
        vec![p_left, p_tip, p_fold_top],
        Color32::from_rgb(248, 249, 252),
        Stroke::NONE,
    ));
    p.line_segment(
        [p_left, p_tip],
        Stroke::new((1.4 * k).max(1.0), Color32::WHITE),
    );

    // Правое оранжевое крыло и объёмная внутренняя складка.
    p.add(egui::Shape::convex_polygon(
        vec![p_fold_top, p_tip, p_bot],
        orange,
        Stroke::NONE,
    ));
    p.line_segment(
        [p_tip, p_bot],
        Stroke::new((1.1 * k).max(0.8), lerp_color(orange, Color32::WHITE, 0.28)),
    );
    p.add(egui::Shape::convex_polygon(
        vec![p_fold_top, p_tip, p_fold_mid],
        fold_mid,
        Stroke::NONE,
    ));
    p.add(egui::Shape::convex_polygon(
        vec![p_fold_mid, p_tip, p_notch],
        fold_deep,
        Stroke::NONE,
    ));
}

/// Выразительная векторная обезьянка в стиле анимированных стикеров Telegram (БЕЗ кругового фона и бликов).
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
    let fur_deep = Color32::from_rgb(92, 54, 34);
    let fur_dark = Color32::from_rgb(118, 72, 46);
    let fur_mid = Color32::from_rgb(146, 92, 58);
    let fur_hi = Color32::from_rgb(168, 110, 72);
    let ear_inner = Color32::from_rgb(234, 182, 142);
    let ear_shadow = Color32::from_rgb(196, 140, 102);
    let face_skin = Color32::from_rgb(246, 206, 170);
    let face_shade = Color32::from_rgb(232, 182, 142);
    let blush = Color32::from_rgba_unmultiplied(245, 118, 96, 82);
    let line_col = Color32::from_rgb(70, 38, 22);

    // Уши с внутренней раковиной и тенью глубины.
    for dir in [-1.0_f32, 1.0_f32] {
        let ec = c + Vec2::new(dir * 35.5 * s, -4.5 * s);
        p.circle_filled(ec, 14.5 * s, fur_dark);
        p.circle_stroke(ec, 14.5 * s, Stroke::new(1.0 * s, fur_deep));
        let inner_c = ec + Vec2::new(-dir * 1.4 * s, 0.4 * s);
        p.circle_filled(inner_c, 9.2 * s, ear_inner);
        p.circle_filled(inner_c + Vec2::new(-dir * 1.2 * s, 1.0 * s), 5.2 * s, ear_shadow);
    }

    // Скульптурный силуэт головы (многоточечный плавный полигон вместо плоского круга).
    let head_pts = sample_closed_contour(
        c,
        s,
        &[
            (0.0, -37.5),
            (17.0, -35.5),
            (30.0, -25.0),
            (36.0, -9.0),
            (34.5, 9.0),
            (25.5, 25.0),
            (12.0, 34.5),
            (0.0, 36.5),
            (-12.0, 34.5),
            (-25.5, 25.0),
            (-34.5, 9.0),
            (-36.0, -9.0),
            (-30.0, -25.0),
            (-17.0, -35.5),
        ],
    );
    p.add(PathShape::convex_polygon(head_pts, fur_mid, Stroke::new(1.2 * s, fur_deep)));

    // Мягкий объёмный блик на макушке + пышный чубчик.
    p.circle_filled(c + Vec2::new(0.0, -23.0 * s), 16.0 * s, fur_hi.gamma_multiply(0.32));
    for (ox, oy, rad, col) in [
        (-6.0, -36.5, 7.2, fur_dark),
        (5.5, -37.0, 8.0, fur_mid),
        (-0.5, -40.5, 7.6, fur_dark),
    ] {
        p.circle_filled(c + Vec2::new(ox * s, oy * s), rad * s, col);
    }

    // Скульптурная светлая маска глаз + подбородок.
    for dir in [-1.0_f32, 1.0_f32] {
        p.circle_filled(c + Vec2::new(dir * 12.2 * s, -7.2 * s), 16.2 * s, face_skin);
    }
    let muzzle_pts = sample_closed_contour(
        c,
        s,
        &[
            (-24.0, 2.0),
            (-14.0, -2.5),
            (0.0, -1.5),
            (14.0, -2.5),
            (24.0, 2.0),
            (27.5, 13.5),
            (21.0, 25.5),
            (10.0, 31.5),
            (0.0, 33.0),
            (-10.0, 31.5),
            (-21.0, 25.5),
            (-27.5, 13.5),
        ],
    );
    p.add(PathShape::convex_polygon(muzzle_pts, face_skin, Stroke::NONE));
    p.circle_filled(c + Vec2::new(0.0, 22.0 * s), 15.0 * s, face_shade.gamma_multiply(0.28));

    // Лёгкий румянец на щёчках.
    p.circle_filled(c + Vec2::new(-20.5 * s, 9.0 * s), 5.6 * s, blush);
    p.circle_filled(c + Vec2::new(20.5 * s, 9.0 * s), 5.6 * s, blush);

    // Брови: при ошибке или подглядывании приподнимаются вверх.
    let brow_lift = if surprised { -3.2 } else if peek_right_only.is_some() { -1.8 } else { 0.0 };
    for dir in [-1.0_f32, 1.0_f32] {
        let bc = c + Vec2::new(dir * 12.5 * s, (-18.5 + brow_lift) * s);
        let brow = quad_arc(
            bc + Vec2::new(-5.5 * s * dir, 1.4 * s),
            bc + Vec2::new(0.0, -1.6 * s),
            bc + Vec2::new(5.5 * s * dir, 1.2 * s),
            8,
        );
        p.add(egui::Shape::line(brow, Stroke::new(2.0 * s, fur_deep)));
    }

    // Глаза: глубокая радужка, тёмный зрачок, двойной блик или плавное моргание.
    for (idx, dir) in [-1.0_f32, 1.0_f32].iter().enumerate() {
        let eye_c = c + Vec2::new(*dir * 12.0 * s, -6.5 * s);
        let winking = peek_right_only.is_some_and(|pk| idx == 1 && pk > 0.25);
        if blink_k > 0.65 && !winking {
            let arc = quad_arc(
                eye_c + Vec2::new(-6.4 * s, 0.2 * s),
                eye_c + Vec2::new(0.0, 4.2 * s),
                eye_c + Vec2::new(6.4 * s, 0.2 * s),
                10,
            );
            p.add(egui::Shape::line(arc, Stroke::new(2.3 * s, line_col)));
        } else {
            p.circle_filled(eye_c, 7.8 * s, Color32::WHITE);
            p.circle_stroke(eye_c, 7.8 * s, Stroke::new(1.1 * s, Color32::from_rgb(198, 155, 120)));
            let iris_c = eye_c + Vec2::new(gaze_x * 2.8 * s, gaze_y * 2.3 * s);
            p.circle_filled(iris_c, 4.8 * s, Color32::from_rgb(84, 52, 34));
            p.circle_filled(iris_c, 3.1 * s, Color32::from_rgb(24, 18, 16));
            p.circle_filled(iris_c + Vec2::new(-1.5 * s, -1.6 * s), 1.55 * s, Color32::WHITE);
            p.circle_filled(iris_c + Vec2::new(1.4 * s, 1.3 * s), 0.75 * s, Color32::from_white_alpha(200));
        }
    }

    // Ноздри + лёгкая переносица.
    let bridge = quad_arc(
        c + Vec2::new(-2.4 * s, 2.5 * s),
        c + Vec2::new(0.0, 4.0 * s),
        c + Vec2::new(2.4 * s, 2.5 * s),
        6,
    );
    p.add(egui::Shape::line(bridge, Stroke::new(1.3 * s, ear_shadow)));
    p.circle_filled(c + Vec2::new(-3.6 * s, 8.2 * s), 2.1 * s, line_col);
    p.circle_filled(c + Vec2::new(3.6 * s, 8.2 * s), 2.1 * s, line_col);

    // Выразительный рот.
    if surprised {
        let mouth_c = c + Vec2::new(0.0, 20.5 * s);
        p.circle_filled(mouth_c, 4.8 * s, Color32::from_rgb(132, 52, 48));
        p.circle_stroke(mouth_c, 4.8 * s, Stroke::new(1.2 * s, line_col));
    } else {
        let smile = quad_arc(
            c + Vec2::new(-10.0 * s, 17.0 * s),
            c + Vec2::new(0.0, 25.0 * s),
            c + Vec2::new(10.0 * s, 17.0 * s),
            12,
        );
        p.add(egui::Shape::line(smile, Stroke::new(2.4 * s, line_col)));
    }
}

/// Анимированная обезьянка для экрана кода подтверждения: следит глазами за вводом цифр (БЕЗ фона).
pub fn paint_code_monkey(ui: &mut egui::Ui, code_len: usize, has_error: bool) {
    let side = if ui.available_width() < 360.0 { 88.0 } else { 110.0 };
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(side), Sense::hover());
    let t = ui.input(|i| i.time);
    let bob = (t * 2.5).sin() as f32 * 2.0;
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
    let s = (side / 110.0) * 1.12;
    paint_monkey_head(ui.painter(), c, s, gaze_x, 0.45, blink_k, has_error, None);
    ui.ctx().request_repaint_after(std::time::Duration::from_millis(33));
}

/// Обезьянка 2FA для облачного пароля: закрывает глаза лапками и подглядывает при показе пароля (БЕЗ фона).
pub fn paint_password_monkey(ui: &mut egui::Ui, show_password: bool, pass_len: usize) {
    let side = if ui.available_width() < 360.0 { 88.0 } else { 110.0 };
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(side), Sense::hover());
    let t = ui.input(|i| i.time);
    let bob = (t * 2.3).sin() as f32 * 1.8;
    let s = (side / 110.0) * 1.12;
    let c = rect.center() + Vec2::new(0.0, bob);

    let hide_target = if pass_len > 0 || !show_password { 1.0 } else { 0.45 };
    let cover_left = theme::tween(ui.ctx(), "lg-2fa-l", hide_target, 0.22);
    let cover_right = theme::tween(
        ui.ctx(),
        "lg-2fa-r",
        if show_password { 0.34 } else { hide_target },
        0.22,
    );
    let peek_amt = if show_password { Some(1.0 - cover_right) } else { None };
    let cyc = t % 4.1;
    let blink_k = if cyc > 3.92 { 1.0 } else { 0.0 };

    let p = ui.painter();
    paint_monkey_head(p, c, s, 0.35, 0.65, blink_k, false, peek_amt);

    let paw_deep = Color32::from_rgb(92, 54, 34);
    let paw_fur = Color32::from_rgb(130, 80, 50);
    let paw_pad = Color32::from_rgb(236, 188, 150);
    for (dir, cov) in [(-1.0_f32, cover_left), (1.0_f32, cover_right)] {
        let rest = c + Vec2::new(dir * 26.0 * s, 28.0 * s);
        let eye_pos = c + Vec2::new(dir * 13.0 * s, -6.5 * s);
        let paw_c = Pos2::new(
            rest.x + (eye_pos.x - rest.x) * cov,
            rest.y + (eye_pos.y - rest.y) * cov,
        );
        let arm_base = c + Vec2::new(dir * 32.0 * s, 40.0 * s);
        p.line_segment([arm_base, paw_c], Stroke::new(12.0 * s, paw_fur));
        p.circle_filled(paw_c, 12.2 * s, paw_fur);
        p.circle_stroke(paw_c, 12.2 * s, Stroke::new(1.0 * s, paw_deep));
        p.circle_filled(paw_c + Vec2::new(0.0, 1.5 * s), 6.2 * s, paw_pad);
        for f in [-5.2_f32, -1.7, 1.7, 5.2] {
            let fc = paw_c + Vec2::new(f * s, -9.8 * s);
            p.circle_filled(fc, 3.0 * s, paw_fur);
            p.circle_stroke(fc, 3.0 * s, Stroke::new(0.8 * s, paw_deep));
        }
    }
    ui.ctx().request_repaint_after(std::time::Duration::from_millis(33));
}

/// Статичный оранжевый векторный ключ без бликов и фоновой пульсации (для экрана ввода API-ключей).
pub fn paint_keys_mascot(ui: &mut egui::Ui) {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(92.0), Sense::hover());
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
