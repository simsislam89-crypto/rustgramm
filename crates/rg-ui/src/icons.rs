//! Значки: настоящий набор Telegram Desktop (PNG-маски из `Telegram/Resources/icons`, GPLv3),
//! один атлас внутри программы. Маска красится в нужный цвет при рисовании, размер — как в
//! оригинале (логический размер = ширина `@2x` / 2). Нескольких значков AyuGram в наборе нет
//! (контакты, призрак, «Прочитать на сервере», три точки): они нарисованы линиями.

use std::collections::HashMap;
use std::sync::OnceLock;

use eframe::egui::{self, Color32, Context, Painter, Pos2, Rect, Shape, Stroke, Vec2};

const ATLAS_PNG: &[u8] = include_bytes!("../assets/icons.png");
const ATLAS_IDX: &str = include_str!("../assets/icons.idx");

#[derive(Clone)]
struct Atlas {
    tex: egui::TextureHandle,
    size: [f32; 2],
}

fn index() -> &'static HashMap<&'static str, [f32; 4]> {
    static IDX: OnceLock<HashMap<&'static str, [f32; 4]>> = OnceLock::new();
    IDX.get_or_init(|| {
        ATLAS_IDX
            .lines()
            .filter_map(|l| {
                let mut it = l.split(' ');
                let name = it.next()?;
                let mut n = || it.next()?.parse::<f32>().ok();
                Some((name, [n()?, n()?, n()?, n()?]))
            })
            .collect()
    })
}

fn atlas(ctx: &Context) -> Option<Atlas> {
    if let Some(a) = ctx.data(|d| d.get_temp::<Atlas>(egui::Id::NULL)) {
        return Some(a);
    }
    let img = image::load_from_memory(ATLAS_PNG).ok()?.to_luma8();
    let size = [img.width() as usize, img.height() as usize];
    let px: Vec<Color32> = img.as_raw().iter().map(|&l| Color32::from_white_alpha(l)).collect();
    let color = egui::ColorImage { size, source_size: Vec2::new(size[0] as f32, size[1] as f32), pixels: px };
    let tex = ctx.load_texture("tg-icons", color, egui::TextureOptions::LINEAR);
    let a = Atlas { tex, size: [size[0] as f32, size[1] as f32] };
    ctx.data_mut(|d| d.insert_temp(egui::Id::NULL, a.clone()));
    Some(a)
}

/// Сохраняет значок как белый PNG с прозрачностью (для страницы плеера). `true`, если записано.
pub fn export_png(name: &str, path: &std::path::Path) -> bool {
    static GRAY: OnceLock<Option<image::GrayImage>> = OnceLock::new();
    let Some(r) = index().get(name) else { return false };
    let Some(atlas) = GRAY.get_or_init(|| image::load_from_memory(ATLAS_PNG).ok().map(|i| i.to_luma8())) else {
        return false;
    };
    let (x, y, w, h) = (r[0] as u32, r[1] as u32, r[2] as u32, r[3] as u32);
    let crop = image::imageops::crop_imm(atlas, x, y, w, h).to_image();
    let mut out = image::RgbaImage::new(w, h);
    for (px, l) in out.pixels_mut().zip(crop.pixels()) {
        *px = image::Rgba([255, 255, 255, l.0[0]]);
    }
    out.save_with_format(path, image::ImageFormat::Png).is_ok()
}

/// Логический размер значка Telegram (px), `None`, если такого нет в наборе.
pub fn native_size(name: &str) -> Option<Vec2> {
    index().get(name).map(|r| Vec2::new(r[2], r[3]) / 2.0)
}

/// Рисует значок Telegram `name` (например `"menu/profile"`) по центру `c`, в родном размере.
pub fn tg(p: &Painter, name: &str, c: Pos2, color: Color32) {
    if let Some(sz) = native_size(name) {
        tg_sized(p, name, c, sz, color);
    }
}

/// То же, но с заданным размером (масштаб картинки).
pub fn tg_sized(p: &Painter, name: &str, c: Pos2, size: Vec2, color: Color32) {
    let (Some(r), Some(a)) = (index().get(name), atlas(p.ctx())) else {
        return;
    };
    let ppp = p.ctx().pixels_per_point();
    let snap = |v: f32| (v * ppp).round() / ppp;
    let min = Pos2::new(snap(c.x - size.x / 2.0), snap(c.y - size.y / 2.0));
    let uv = Rect::from_min_max(
        Pos2::new(r[0] / a.size[0], r[1] / a.size[1]),
        Pos2::new((r[0] + r[2]) / a.size[0], (r[1] + r[3]) / a.size[1]),
    );
    p.image(a.tex.id(), Rect::from_min_size(min, size), uv, color);
}

/// Три полосы: главное меню.
pub fn menu(p: &Painter, c: Pos2, color: Color32) {
    tg(p, "dialogs/dialogs_menu", c, color);
}

/// Стрелка «назад».
pub fn back(p: &Painter, c: Pos2, color: Color32) {
    tg(p, "info/info_back", c, color);
}

/// Лупа.
pub fn search(p: &Painter, c: Pos2, color: Color32) {
    tg(p, "top_bar_search", c, color);
}

/// Три вертикальные точки: меню чата.
pub fn dots(p: &Painter, c: Pos2, color: Color32) {
    for dy in [-5.0, 0.0, 5.0] {
        p.circle_filled(c + Vec2::new(0.0, dy), 1.6, color);
    }
}

/// Шеврон вниз (или вверх при `up`): раскрытие списков.
pub fn chevron(p: &Painter, c: Pos2, up: bool, color: Color32) {
    let d = if up { -1.0 } else { 1.0 };
    p.add(Shape::line(
        vec![c + Vec2::new(-4.0, -1.5 * d), c + Vec2::new(0.0, 2.5 * d), c + Vec2::new(4.0, -1.5 * d)],
        Stroke::new(1.5, color),
    ));
}

pub fn chevron_down(p: &Painter, c: Pos2, color: Color32) {
    chevron(p, c, false, color);
}

/// Бумажный самолётик: кнопка отправки.
pub fn send(p: &Painter, c: Pos2, color: Color32) {
    tg(p, "chat/input_send", c, color);
}

/// Скрепка (вложения).
pub fn attach(p: &Painter, c: Pos2, color: Color32) {
    tg(p, "chat/input_attach", c, color);
}

/// Галочка «отправлено»; со второй — «прочитано».
pub fn tick(p: &Painter, c: Pos2, color: Color32, double: bool) {
    tg(p, if double { "history_received" } else { "history_sent" }, c, color);
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

/// Значок по имени: набор Telegram или нарисованный линиями (`heart`, `grid`, `globe`,
/// `contacts`, `read`, `ghost`).
pub fn any(p: &Painter, name: &str, c: Pos2, color: Color32) {
    if native_size(name).is_some() {
        tg(p, name, c, color);
        return;
    }
    let s = Stroke::new(1.5, color);
    match name {
        "heart" => {
            let pts: Vec<Pos2> = (0..=40)
                .map(|i| {
                    let t = i as f32 / 40.0 * std::f32::consts::TAU;
                    let x = 16.0 * t.sin().powi(3);
                    let y = 13.0 * t.cos() - 5.0 * (2.0 * t).cos() - 2.0 * (3.0 * t).cos() - (4.0 * t).cos();
                    c + Vec2::new(x, -y + 1.0) * 0.52
                })
                .collect();
            p.add(Shape::closed_line(pts, s));
        }
        "grid" => {
            for (dx, dy) in [(-4.5, -4.5), (4.5, -4.5), (-4.5, 4.5), (4.5, 4.5)] {
                p.rect_stroke(
                    Rect::from_center_size(c + Vec2::new(dx, dy), Vec2::splat(6.5)),
                    egui::CornerRadius::same(2),
                    s,
                    egui::StrokeKind::Middle,
                );
            }
        }
        "globe" => {
            p.circle_stroke(c, 8.5, s);
            let ell: Vec<Pos2> = (0..=24)
                .map(|i| {
                    let t = i as f32 / 24.0 * std::f32::consts::TAU;
                    c + Vec2::new(4.0 * t.cos(), 8.5 * t.sin())
                })
                .collect();
            p.add(Shape::closed_line(ell, s));
            p.line_segment([c + Vec2::new(-8.5, 0.0), c + Vec2::new(8.5, 0.0)], s);
        }
        "contacts" => ico(p, c, Ico::Contacts, color),
        "read" => ico(p, c, Ico::Read, color),
        "ghost" => ico(p, c, Ico::Ghost, color),
        _ => {}
    }
}

/// Область значка 36×36 вокруг центра.
pub fn hit_rect(c: Pos2) -> Rect {
    Rect::from_center_size(c, Vec2::splat(36.0))
}

/// Значки списка и меню (главное меню, строки настроек).
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

/// Имя значка в наборе Telegram; `None` — рисуется линиями.
fn atlas_name(ico: Ico) -> Option<&'static str> {
    Some(match ico {
        Ico::Profile => "menu/profile",
        Ico::Group => "menu/groups",
        Ico::Channel => "menu/channel",
        Ico::Calls => "menu/phone",
        Ico::Saved => "menu/saved_messages",
        Ico::Settings => "menu/settings",
        Ico::Moon => "menu/night_mode",
        Ico::Pin => "menu/pin",
        Ico::Mute => "menu/mute",
        Ico::Archive => "menu/archive",
        Ico::Folder => "settings/folders",
        Ico::Check => "menu/select",
        Ico::Sticker => "menu/stickers",
        Ico::Gif => "menu/gif",
        Ico::Contacts | Ico::Read | Ico::Ghost => return None,
    })
}

/// Рисует значок `ico` с центром в `c`. Значки Telegram — в родном размере 24 px,
/// нарисованные линиями — на сетке 24×24 (штрих 1,5 px, как в наборе Telegram).
pub fn ico(p: &Painter, c: Pos2, ico: Ico, color: Color32) {
    if let Some(name) = atlas_name(ico) {
        tg(p, name, c, color);
        return;
    }
    let s = Stroke::new(1.5, color);
    let pt = |x: f32, y: f32| c + Vec2::new(x, y);
    match ico {
        Ico::Contacts => {
            p.circle_stroke(pt(0.0, -4.5), 3.8, s);
            p.add(Shape::line(arc(pt(0.0, 10.0), 7.5, 207.0, 333.0, 14), s));
        }
        Ico::Read => {
            p.rect_stroke(
                Rect::from_center_size(c, Vec2::new(18.0, 14.0)),
                egui::CornerRadius::same(3),
                s,
                egui::StrokeKind::Middle,
            );
            p.add(Shape::line(vec![pt(-8.0, -4.0), pt(0.0, 2.0), pt(8.0, -4.0)], s));
        }
        _ => {
            // Призрак AyuGram: купол, подол «зубцами», два глаза.
            let mut v = arc(pt(0.0, -2.0), 7.5, 180.0, 360.0, 16);
            v.extend([
                pt(7.5, 8.0),
                pt(5.0, 6.0),
                pt(2.5, 8.0),
                pt(0.0, 6.0),
                pt(-2.5, 8.0),
                pt(-5.0, 6.0),
                pt(-7.5, 8.0),
            ]);
            p.add(Shape::closed_line(v, s));
            p.circle_filled(pt(-2.6, -2.0), 1.1, color);
            p.circle_filled(pt(2.6, -2.0), 1.1, color);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atlas_decodes_and_every_name_fits() {
        let img = image::load_from_memory(ATLAS_PNG).expect("атлас читается").to_luma8();
        assert!(!index().is_empty());
        for (name, r) in index() {
            assert!(r[0] + r[2] <= img.width() as f32 && r[1] + r[3] <= img.height() as f32, "{name} вне атласа");
        }
    }

    #[test]
    fn used_icons_exist() {
        for n in [
            "menu/profile", "menu/groups", "menu/channel", "menu/phone", "menu/saved_messages",
            "menu/settings", "menu/night_mode", "menu/pin", "menu/mute", "menu/archive",
            "chat/input_attach", "chat/input_smile_face", "chat/input_send", "dialogs/dialogs_menu",
            "dialogs/dialogs_pinned", "dialogs/dialogs_mute", "top_bar_search", "info/info_close",
            "info/info_back", "history_pin", "history_views", "history_sent", "history_received",
            "archive_userpic", "menu/tag_filter", "menu/chat_bubble", "menu/palette", "menu/favorite",
            "menu/delete", "menu/reply", "menu/forward", "menu/copy", "menu/edit", "menu/unread",
        ] {
            assert!(native_size(n).is_some(), "нет значка {n}");
        }
        assert_eq!(native_size("menu/profile"), Some(Vec2::splat(24.0)));
    }
}
