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

/// Порядок групп в панели выбора.
fn rank(c: char) -> u8 {
    match c as u32 {
        0x1F600..=0x1F64F => 0,
        0x1F910..=0x1F9FF => 1,
        0x1F440..=0x1F4FF => 2,
        0x1F300..=0x1F43F => 3,
        0x1F500..=0x1F6FF | 0x1FA70..=0x1FAFF => 4,
        0x2600..=0x27BF | 0x2B50..=0x2B55 => 5,
        _ => 6,
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
        // В панели сначала смайлики и жесты, затем предметы, природа, транспорт и символы.
        list.sort_by_key(|c| (rank(*c), *c as u32));
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

    /// Сетка выбора эмодзи; возвращает выбранный символ.
    pub fn picker(&self, ui: &mut egui::Ui) -> Option<char> {
        let mut chosen = None;
        let cell = 30.0;
        egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
            ui.spacing_mut().item_spacing = Vec2::splat(2.0);
            ui.horizontal_wrapped(|ui| {
                for &c in &self.list {
                    let Some(img) = self.image(c, 24.0) else { continue };
                    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(cell), Sense::click());
                    if resp.hovered() {
                        ui.painter().rect_filled(
                            rect,
                            egui::CornerRadius::same(6),
                            Color32::from_rgba_unmultiplied(255, 255, 255, 24),
                        );
                    }
                    img.paint_at(ui, egui::Rect::from_center_size(rect.center(), Vec2::splat(24.0)));
                    if resp.clicked() {
                        chosen = Some(c);
                    }
                }
            });
        });
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
