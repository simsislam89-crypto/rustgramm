//! Значки, нарисованные линиями: меню, отправка, скрепка, галочки. Шрифты и картинки не нужны.

use eframe::egui::{Color32, Painter, Pos2, Rect, Shape, Stroke, Vec2};

fn line(p: &Painter, a: Pos2, b: Pos2, stroke: Stroke) {
    p.line_segment([a, b], stroke);
}

/// Три полосы: главное меню.
pub fn menu(p: &Painter, c: Pos2, color: Color32) {
    let s = Stroke::new(2.0, color);
    for dy in [-6.0, 0.0, 6.0] {
        line(p, c + Vec2::new(-9.0, dy), c + Vec2::new(9.0, dy), s);
    }
}

/// Стрелка «назад».
pub fn back(p: &Painter, c: Pos2, color: Color32) {
    let s = Stroke::new(2.2, color);
    line(p, c + Vec2::new(7.0, 0.0), c + Vec2::new(-7.0, 0.0), s);
    line(p, c + Vec2::new(-7.0, 0.0), c + Vec2::new(-1.0, -6.0), s);
    line(p, c + Vec2::new(-7.0, 0.0), c + Vec2::new(-1.0, 6.0), s);
}

/// Лупа.
pub fn search(p: &Painter, c: Pos2, color: Color32) {
    let s = Stroke::new(1.8, color);
    p.circle_stroke(c + Vec2::new(-1.5, -1.5), 6.0, s);
    line(p, c + Vec2::new(3.0, 3.0), c + Vec2::new(8.0, 8.0), s);
}

/// Бумажный самолётик: кнопка отправки.
pub fn send(p: &Painter, c: Pos2, color: Color32) {
    p.add(Shape::convex_polygon(
        vec![
            c + Vec2::new(-9.0, -9.0),
            c + Vec2::new(11.0, 0.0),
            c + Vec2::new(-9.0, 9.0),
        ],
        color,
        Stroke::NONE,
    ));
    p.add(Shape::convex_polygon(
        vec![
            c + Vec2::new(-9.0, -9.0),
            c + Vec2::new(-4.0, 0.0),
            c + Vec2::new(-9.0, 9.0),
        ],
        Color32::from_black_alpha(70),
        Stroke::NONE,
    ));
}

/// Скрепка (вложения).
pub fn attach(p: &Painter, c: Pos2, color: Color32) {
    let s = Stroke::new(1.8, color);
    let pts = [
        c + Vec2::new(6.0, -4.0),
        c + Vec2::new(-4.0, 6.0),
        c + Vec2::new(-8.0, 2.0),
        c + Vec2::new(2.0, -8.0),
        c + Vec2::new(6.0, -5.0),
        c + Vec2::new(-2.0, 3.0),
    ];
    p.add(Shape::line(pts.to_vec(), s));
}

/// Галочка «отправлено»; со второй — «прочитано».
pub fn tick(p: &Painter, c: Pos2, color: Color32, double: bool) {
    let s = Stroke::new(1.5, color);
    let one = |dx: f32| {
        vec![
            c + Vec2::new(dx - 4.0, 0.5),
            c + Vec2::new(dx - 1.5, 3.0),
            c + Vec2::new(dx + 3.5, -3.0),
        ]
    };
    p.add(Shape::line(one(if double { -2.5 } else { 0.0 }), s));
    if double {
        p.add(Shape::line(one(2.5), s));
    }
}

/// Круглый логотип: самолётик на круге.
pub fn logo(p: &Painter, c: Pos2, r: f32, bg: Color32) {
    p.circle_filled(c, r, bg);
    let k = r / 22.0;
    let pts = vec![
        c + Vec2::new(-10.0, 0.5) * k,
        c + Vec2::new(10.0, -8.0) * k,
        c + Vec2::new(6.0, 9.0) * k,
        c + Vec2::new(0.0, 4.0) * k,
    ];
    p.add(Shape::convex_polygon(pts, Color32::WHITE, Stroke::NONE));
}

/// Область значка 36×36 вокруг центра.
pub fn hit_rect(c: Pos2) -> Rect {
    Rect::from_center_size(c, Vec2::splat(36.0))
}
