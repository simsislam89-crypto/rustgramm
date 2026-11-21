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

/// Значки списка и меню, нарисованные линиями на сетке 24×24 (как в Telegram Desktop).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ico {
    Profile,
    Group,
    Channel,
    Contacts,
    Calls,
    Saved,
    Read,
    Settings,
    Moon,
    Ghost,
    Pin,
    Mute,
    Archive,
    Folder,
    Check,
    Sticker,
    Gif,
}

/// Точки дуги окружности: центр, радиус, углы в градусах.
fn arc(c: Pos2, r: f32, from: f32, to: f32, n: usize) -> Vec<Pos2> {
    (0..=n)
        .map(|i| {
            let a = (from + (to - from) * i as f32 / n as f32).to_radians();
            c + Vec2::new(a.cos(), a.sin()) * r
        })
        .collect()
}

/// Рисует значок `ico` с центром в `c`, стороной `size` px.
pub fn ico(p: &Painter, c: Pos2, ico: Ico, size: f32, color: Color32) {
    let k = size / 24.0;
    let s = Stroke::new((1.6 * k).max(1.2), color);
    let pt = |x: f32, y: f32| c + Vec2::new(x, y) * k;
    let poly = |pts: Vec<Pos2>, closed: bool| {
        if closed {
            p.add(Shape::closed_line(pts, s));
        } else {
            p.add(Shape::line(pts, s));
        }
    };
    let person = |dx: f32, dy: f32, sc: f32| {
        p.circle_stroke(pt(dx, -5.0 * sc + dy), 3.6 * sc * k, s);
        let mut body = arc(pt(dx, 11.0 * sc + dy), 7.5 * sc * k, 200.0, 340.0, 10);
        body.insert(0, pt(dx - 7.0 * sc, 9.0 * sc + dy));
        poly(arc(pt(dx, 12.0 * sc + dy), 7.5 * sc * k, 195.0, 345.0, 12), false);
        let _ = body;
    };
    match ico {
        Ico::Profile | Ico::Contacts => {
            person(0.0, 0.0, 1.0);
        }
        Ico::Group => {
            person(-4.0, 1.0, 0.82);
            person(5.0, 2.0, 0.7);
        }
        Ico::Channel => {
            poly(vec![pt(-8.0, -2.0), pt(2.0, -7.0), pt(2.0, 7.0), pt(-8.0, 2.0)], true);
            poly(vec![pt(-6.0, 2.0), pt(-5.0, 8.0), pt(-2.5, 8.0), pt(-2.5, 5.5)], false);
            poly(vec![pt(6.0, -3.0), pt(6.0, 3.0)], false);
        }
        Ico::Calls => {
            let mut v = arc(pt(-1.0, 1.0), 8.0, 200.0, 290.0, 10);
            v.extend(arc(pt(1.0, -1.0), 3.0, 290.0, 400.0, 6));
            poly(v, false);
            poly(arc(pt(1.0, -1.0), 8.0, 100.0, 20.0, 8), false);
        }
        Ico::Saved => {
            poly(vec![pt(-5.0, -8.0), pt(5.0, -8.0), pt(5.0, 8.0), pt(0.0, 3.5), pt(-5.0, 8.0)], true);
        }
        Ico::Read => {
            p.rect_stroke(
                Rect::from_center_size(c, Vec2::new(17.0, 12.0) * k),
                egui_radius(2),
                s,
                eframe::egui::StrokeKind::Middle,
            );
            poly(vec![pt(-8.5, -5.0), pt(0.0, 1.5), pt(8.5, -5.0)], false);
        }
        Ico::Settings => {
            p.circle_stroke(c, 3.2 * k, s);
            p.circle_stroke(c, 7.2 * k, s);
            for i in 0..8 {
                let a = (i as f32 * 45.0).to_radians();
                let d = Vec2::new(a.cos(), a.sin());
                line(p, c + d * 7.2 * k, c + d * 10.0 * k, s);
            }
        }
        Ico::Moon => {
            let mut v = arc(pt(0.0, 0.0), 8.5, 60.0, 300.0, 18);
            v.extend(arc(pt(3.5, -1.5), 6.5, 270.0, 90.0 - 360.0 + 360.0, 12).into_iter().rev());
            poly(v, false);
        }
        Ico::Ghost => {
            let mut v = arc(pt(0.0, -2.0), 7.0, 180.0, 360.0, 14);
            v.extend([
                pt(7.0, 8.0),
                pt(4.7, 6.0),
                pt(2.3, 8.0),
                pt(0.0, 6.0),
                pt(-2.3, 8.0),
                pt(-4.7, 6.0),
                pt(-7.0, 8.0),
            ]);
            poly(v, true);
            p.circle_filled(pt(-2.6, -2.0), 1.1 * k, color);
            p.circle_filled(pt(2.6, -2.0), 1.1 * k, color);
        }
        Ico::Pin => {
            poly(vec![pt(-4.0, -8.0), pt(4.0, -8.0), pt(3.0, -1.0), pt(6.0, 2.0), pt(-6.0, 2.0), pt(-3.0, -1.0)], true);
            line(p, pt(0.0, 2.0), pt(0.0, 9.0), s);
        }
        Ico::Mute => {
            poly(vec![pt(-8.0, -3.0), pt(-4.0, -3.0), pt(1.0, -7.0), pt(1.0, 7.0), pt(-4.0, 3.0), pt(-8.0, 3.0)], true);
            line(p, pt(4.0, -4.0), pt(9.0, 4.0), s);
            line(p, pt(9.0, -4.0), pt(4.0, 4.0), s);
        }
        Ico::Archive => {
            p.rect_stroke(
                Rect::from_center_size(pt(0.0, -4.0), Vec2::new(18.0, 4.5) * k),
                egui_radius(1),
                s,
                eframe::egui::StrokeKind::Middle,
            );
            poly(vec![pt(-7.5, -1.5), pt(-7.5, 8.0), pt(7.5, 8.0), pt(7.5, -1.5)], false);
            line(p, pt(-3.0, 2.0), pt(3.0, 2.0), s);
        }
        Ico::Folder => {
            poly(vec![pt(-9.0, -6.0), pt(-3.0, -6.0), pt(-1.0, -3.5), pt(9.0, -3.5), pt(9.0, 7.0), pt(-9.0, 7.0)], true);
        }
        Ico::Check => {
            poly(vec![pt(-6.0, 0.5), pt(-2.0, 5.0), pt(7.0, -5.0)], false);
        }
        Ico::Sticker => {
            poly(arc(pt(0.0, 0.0), 9.0, 0.0, 360.0, 24), true);
            poly(arc(pt(0.0, 1.0), 5.0, 20.0, 160.0, 8), false);
            p.circle_filled(pt(-3.0, -3.0), 1.1 * k, color);
            p.circle_filled(pt(3.0, -3.0), 1.1 * k, color);
        }
        Ico::Gif => {
            p.rect_stroke(
                Rect::from_center_size(c, Vec2::new(19.0, 13.0) * k),
                egui_radius(3),
                s,
                eframe::egui::StrokeKind::Middle,
            );
            p.text(
                c,
                eframe::egui::Align2::CENTER_CENTER,
                "GIF",
                eframe::egui::FontId::proportional(8.0 * k.max(0.8)),
                color,
            );
        }
    }
}

fn egui_radius(r: u8) -> eframe::egui::CornerRadius {
    eframe::egui::CornerRadius::same(r)
}
