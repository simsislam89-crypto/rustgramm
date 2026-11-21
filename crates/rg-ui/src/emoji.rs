//! Цветные эмодзи: Twemoji (CC-BY 4.0, Twitter/jdecked), один атлас 36 px, встроенный в программу.
//! egui цветных эмодзи не рисует, поэтому текст режется на куски «текст / эмодзи», а эмодзи
//! выводятся картинками из атласа.

use std::collections::HashMap;

use eframe::egui::{self, Color32, RichText, Sense, Vec2, load::SizedTexture};

const ATLAS_PNG: &[u8] = include_bytes!("../assets/emoji.png");
const ATLAS_IDX: &str = include_str!("../assets/emoji.idx");
const CELL: f32 = 36.0;
const COLS: usize = 32;

/// Кусок текста: обычные символы или один эмодзи.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Seg {
    Text(String),
    Emoji(char),
}

pub struct Emoji {
    tex: egui::TextureHandle,
    index: HashMap<char, usize>,
    list: Vec<char>,
    size: [usize; 2],
}

/// Модификаторы, которые отдельно не рисуем: селектор вида, соединитель, оттенки кожи.
fn is_modifier(c: char) -> bool {
    matches!(c as u32, 0xFE0F | 0xFE0E | 0x200D | 0x20E3 | 0x1F3FB..=0x1F3FF)
}

/// Разделы панели, как в Telegram: подпись и эмодзи-значок на нижней полосе.
pub const CATEGORIES: [(&str, char); 8] = [
    ("Недавние", '\u{23f0}'),
    ("Смайлы и люди", '\u{1f600}'),
    ("Животные и природа", '\u{1f43b}'),
    ("Еда и напитки", '\u{1f354}'),
    ("Путешествия", '\u{1f697}'),
    ("Активности", '\u{26bd}'),
    ("Объекты", '\u{1f4a1}'),
    ("Символы", '\u{2764}'),
];

/// Раздел эмодзи (индекс в `CATEGORIES`, кроме «Недавние»).
pub fn category(c: char) -> usize {
    match c as u32 {
        0x1F600..=0x1F64F | 0x1F910..=0x1F92F | 0x1F970..=0x1F97A | 0x1F440..=0x1F450
        | 0x1F466..=0x1F487 | 0x1F9D0..=0x1F9FF | 0x270A..=0x270D
        | 0x261D | 0x1F90C..=0x1F90F | 0x1F930..=0x1F93A => 1,
        0x1F400..=0x1F43F | 0x1F980..=0x1F9AE | 0x1F331..=0x1F344 | 0x1F490..=0x1F4A0
        | 0x1F54A | 0x1F300..=0x1F330 | 0x2600..=0x2604 | 0x26C4..=0x26C5 => 2,
        0x1F345..=0x1F37F | 0x1F950..=0x1F96F | 0x1F9C0..=0x1F9CB | 0x2615 => 3,
        0x1F680..=0x1F6FF | 0x1F3E0..=0x1F3F0 | 0x2708 | 0x26F5 | 0x26FD | 0x26FA => 4,
        0x1F380..=0x1F3DF | 0x1F3F8..=0x1F3FA | 0x26BD..=0x26BE | 0x1F93B..=0x1F94F => 5,
        0x1F4A1..=0x1F4FF | 0x1F500..=0x1F53D | 0x1F550..=0x1F567 | 0x1FA70..=0x1FAFF
        | 0x231A..=0x231B | 0x2328 | 0x23F0..=0x23F3 | 0x2702 | 0x2709 | 0x270F..=0x2712 => 6,
        _ => 7,
    }
}

/// Порядок внутри раздела: сначала лица, затем жесты и люди.
fn intra(c: char) -> u8 {
    match c as u32 {
        0x1F600..=0x1F64F => 0,
        0x1F910..=0x1F92F | 0x1F970..=0x1F97A => 1,
        _ => 2,
    }
}

impl Emoji {
    /// Распаковывает атлас в текстуру; `None`, если встроенная картинка повреждена.
    pub fn load(ctx: &egui::Context) -> Option<Self> {
        let img = image::load_from_memory(ATLAS_PNG).ok()?.to_rgba8();
        let size = [img.width() as usize, img.height() as usize];
        let color = egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw());
        let tex = ctx.load_texture("emoji-atlas", color, egui::TextureOptions::LINEAR);
        let mut index = HashMap::new();
        let mut list = Vec::new();
        for (n, line) in ATLAS_IDX.lines().enumerate() {
            if let Some(c) = u32::from_str_radix(line.trim(), 16).ok().and_then(char::from_u32) {
                index.insert(c, n);
                list.push(c);
            }
        }
        // Порядок в панели: по разделам, внутри — по коду.
        list.sort_by_key(|c| (category(*c), intra(*c), *c as u32));
        Some(Self { tex, index, list, size })
    }

    /// Все доступные эмодзи в порядке атласа.
    pub fn all(&self) -> &[char] {
        &self.list
    }

    pub fn has(&self, c: char) -> bool {
        self.index.contains_key(&c)
    }

    /// Картинка эмодзи нужного размера (в логических пикселях).
    pub fn image(&self, c: char, size: f32) -> Option<egui::Image<'static>> {
        let n = *self.index.get(&c)?;
        let (col, row) = ((n % COLS) as f32, (n / COLS) as f32);
        let (w, h) = (self.size[0] as f32, self.size[1] as f32);
        let uv = egui::Rect::from_min_max(
            egui::pos2(col * CELL / w, row * CELL / h),
            egui::pos2((col + 1.0) * CELL / w, (row + 1.0) * CELL / h),
        );
        Some(
            egui::Image::new(SizedTexture::new(self.tex.id(), Vec2::splat(size)))
                .uv(uv)
                .fit_to_exact_size(Vec2::splat(size)),
        )
    }

    /// Режет текст на куски; модификаторы после эмодзи отбрасываются.
    pub fn segments(&self, text: &str) -> Vec<Seg> {
        let mut out: Vec<Seg> = Vec::new();
        let mut buf = String::new();
        for c in text.chars() {
            if self.has(c) {
                if !buf.is_empty() {
                    out.push(Seg::Text(std::mem::take(&mut buf)));
                }
                out.push(Seg::Emoji(c));
            } else if is_modifier(c) && matches!(out.last(), Some(Seg::Emoji(_))) && buf.is_empty() {
                // оттенок кожи, ZWJ и селекторы вида съедаем
            } else {
                buf.push(c);
            }
        }
        if !buf.is_empty() {
            out.push(Seg::Text(buf));
        }
        out
    }

    /// Есть ли в тексте хотя бы один эмодзи из атласа.
    pub fn contains(&self, text: &str) -> bool {
        text.chars().any(|c| self.has(c))
    }

    /// Выводит текст с картинками вместо эмодзи (переносится по ширине контейнера).
    pub fn rich_label(&self, ui: &mut egui::Ui, text: &str, size: f32, color: Color32) {
        if !self.contains(text) {
            ui.label(RichText::new(text).size(size).color(color));
            return;
        }
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            for seg in self.segments(text) {
                match seg {
                    Seg::Text(s) => {
                        ui.label(RichText::new(s).size(size).color(color));
                    }
                    Seg::Emoji(c) => {
                        if let Some(img) = self.image(c, size * 1.25) {
                            ui.add(img);
                        }
                    }
                }
            }
        });
    }

    /// Рисует одну строку текста с эмодзи-картинками от точки `pos`, обрезая по `clip`.
    pub fn paint_line(
        &self,
        ui: &egui::Ui,
        clip: egui::Rect,
        pos: egui::Pos2,
        text: &str,
        size: f32,
        color: Color32,
    ) {
        let painter = ui.painter().with_clip_rect(clip);
        let font = egui::FontId::proportional(size);
        let mut x = pos.x;
        for seg in self.segments(text) {
            if x > clip.right() {
                break;
            }
            match seg {
                Seg::Text(s) => {
                    let galley = painter.layout_no_wrap(s, font.clone(), color);
                    let w = galley.size().x;
                    painter.galley(egui::pos2(x, pos.y), galley, color);
                    x += w;
                }
                Seg::Emoji(c) => {
                    let side = size * 1.15;
                    if let Some(img) = self.image(c, side) {
                        let r = egui::Rect::from_min_size(
                            egui::pos2(x, pos.y + (size * 1.2 - side) / 2.0),
                            Vec2::splat(side),
                        );
                        if r.intersects(clip) {
                            img.paint_at(ui, r);
                        }
                    }
                    x += side + 1.0;
                }
            }
        }
    }

    /// Панель выбора: разделы с заголовками и полоса значков внизу. Возвращает выбранный эмодзи.
    /// `jump` — раздел, к которому нужно прокрутить (сбрасывается).
    pub fn picker(&self, ui: &mut egui::Ui, recents: &str, jump: &mut Option<usize>) -> Option<char> {
        let mut chosen = None;
        let cell = 36.0;
        let cols = ((ui.available_width() / cell).floor() as usize).max(1);
        let recent: Vec<char> = recents.chars().filter(|c| self.has(*c)).collect();
        let want = jump.take();
        egui::ScrollArea::vertical()
            .max_height(ui.available_height() - 44.0)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                for (cat, (title, _)) in CATEGORIES.iter().enumerate() {
                    let items: Vec<char> = if cat == 0 {
                        recent.clone()
                    } else {
                        self.list.iter().copied().filter(|c| category(*c) == cat).collect()
                    };
                    if items.is_empty() {
                        continue;
                    }
                    let (head, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 28.0), Sense::hover());
                    ui.painter().text(
                        egui::pos2(head.left() + 8.0, head.bottom() - 6.0),
                        egui::Align2::LEFT_BOTTOM,
                        *title,
                        egui::FontId::proportional(12.5),
                        Color32::from_rgb(150, 150, 158),
                    );
                    if want == Some(cat) {
                        ui.scroll_to_rect(head, Some(egui::Align::TOP));
                    }
                    for row in items.chunks(cols) {
                        let (rect, _) = ui.allocate_exact_size(Vec2::new(cell * cols as f32, cell), Sense::hover());
                        for (k, &c) in row.iter().enumerate() {
                            let r = egui::Rect::from_min_size(
                                egui::pos2(rect.left() + k as f32 * cell, rect.top()),
                                Vec2::splat(cell),
                            );
                            let resp = ui.interact(r, ui.make_persistent_id(("em", cat, c as u32)), Sense::click());
                            let k_hover = ui.ctx().animate_bool_with_time(resp.id, resp.hovered(), 0.1);
                            if k_hover > 0.01 {
                                ui.painter().rect_filled(
                                    r.shrink(2.0),
                                    egui::CornerRadius::same(8),
                                    Color32::from_rgba_unmultiplied(255, 255, 255, (26.0 * k_hover) as u8),
                                );
                            }
                            // Лёгкое увеличение под курсором, как в Telegram.
                            let side = 26.0 + 4.0 * k_hover;
                            if let Some(img) = self.image(c, side) {
                                img.paint_at(ui, egui::Rect::from_center_size(r.center(), Vec2::splat(side)));
                            }
                            if resp.clicked() {
                                chosen = Some(c);
                            }
                        }
                    }
                }
            });
        // Нижняя полоса разделов.
        ui.add_space(4.0);
        let (bar, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 36.0), Sense::hover());
        ui.painter().line_segment(
            [bar.left_top(), bar.right_top()],
            egui::Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 18)),
        );
        let n = CATEGORIES.len() as f32;
        for (cat, (_, icon)) in CATEGORIES.iter().enumerate() {
            let c = egui::pos2(bar.left() + bar.width() * (cat as f32 + 0.5) / n, bar.center().y);
            let r = egui::Rect::from_center_size(c, Vec2::splat(30.0));
            let resp = ui.interact(r, ui.make_persistent_id(("emcat", cat)), Sense::click());
            let k = ui.ctx().animate_bool_with_time(resp.id, resp.hovered(), 0.1);
            let side = 18.0 + 3.0 * k;
            if let Some(img) = self.image(*icon, side) {
                // Не выбранные значки приглушены.
                img.tint(Color32::from_white_alpha((150.0 + 105.0 * k) as u8))
                    .paint_at(ui, egui::Rect::from_center_size(c, Vec2::splat(side)));
            }
            if resp.clicked() {
                *jump = Some(cat);
            }
        }
        chosen
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_is_unique_and_large() {
        let mut v: Vec<&str> = ATLAS_IDX.lines().collect();
        assert!(v.len() > 1000);
        v.sort_unstable();
        v.dedup();
        assert_eq!(v.len(), ATLAS_IDX.lines().count());
    }

    #[test]
    fn atlas_png_decodes_and_fits_index() {
        let img = image::load_from_memory(ATLAS_PNG).unwrap();
        let rows = ATLAS_IDX.lines().count().div_ceil(COLS);
        assert_eq!(img.width() as usize, COLS * CELL as usize);
        assert_eq!(img.height() as usize, rows * CELL as usize);
    }

    #[test]
    fn modifiers_are_recognised() {
        assert!(is_modifier('\u{fe0f}') && is_modifier('\u{200d}') && is_modifier('\u{1f3fd}'));
        assert!(!is_modifier('a'));
    }
}
