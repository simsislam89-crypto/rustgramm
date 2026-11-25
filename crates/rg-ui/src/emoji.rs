//! Цветные эмодзи: Twemoji (CC-BY 4.0, Twitter/jdecked), один атлас 36 px, встроенный в программу.
//! egui цветных эмодзи не рисует, поэтому текст режется на куски «текст / эмодзи», а эмодзи
//! выводятся картинками из атласа.

use std::collections::HashMap;

use eframe::egui::{self, Color32, RichText, Sense, Vec2, load::SizedTexture};

const ATLAS_BYTES: &[u8] = include_bytes!("../assets/emoji.webp");
const ATLAS_IDX: &str = include_str!("../assets/emoji.idx");
const CELL: f32 = 36.0;
const COLS: usize = 32;
const ATLAS_W: u32 = 1152;
const ATLAS_H: u32 = 1548;

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
    by_cat: [Vec<char>; 8],
    keywords: Vec<(String, Vec<char>)>,
    size: [usize; 2],
}

/// Встроенные ключевые слова (рус + eng) для мгновенного поиска эмодзи до ответа Telegram.
const BUILTIN_KEYWORDS: &[(&str, &str)] = &[
    ("улыбка смех радость haha lol smile happy grin", "😀😃😄😁😆😅😂🤣😊🙂😉"),
    ("любовь сердце поцелуй love heart kiss", "❤🧡💛💚💙💜🖤🤍💖💘💝😍🥰😘"),
    ("огонь жара топ fire hot lit", "🔥💥✨⚡🌟💫"),
    ("палец класс ок лайк ok thumbs up clap", "👍👎👌👏🙌🤝✌🤞💪🙏"),
    ("плач грусть слёзы cry sad tear", "😢😭😞😔😟😕🙁☹🥺"),
    ("злость гнев мат angry mad rage", "😠😡🤬😤👿💀"),
    ("шок удивление оу wow shock omg", "😮😯😲😳😱🤯😨😰"),
    ("думать вопрос think hmm", "🤔🤨🧐🙄😐😑😶"),
    ("круто очки cool sunglasses", "😎🤓🤩🥳🤠"),
    ("сон спать устал sleep tired yawn", "😴🥱😪😵🥴🤒🤕"),
    ("еда кофе чай пицца food coffee pizza beer", "☕🍵🍕🍔🍟🌮🍣🍰🎂🍺🍷🍎🍌🍓"),
    ("животные кот собака cat dog animal", "🐱🐶🐭🐹🐰🦊🐻🐼🐨🐯🦁🐸🐵🐧"),
    ("природа цветы солнце луна sun moon flower tree", "☀🌙⭐🌈☁❄🌸🌹🌻🌲🌴🌊"),
    ("работа деньги время clock money work computer", "💻📱⌨💰💵💎⏰⌛📅📌📎✏📝"),
    ("музыка игра спорт music game sport", "🎵🎶🎧🎸🎮⚽🏀🎾🏆🎯🎲"),
    ("путешествие машина самолёт car plane travel", "🚗🚕🚌✈🚀🚆🚢🏠🏢🌍"),
    ("праздник подарок торт party gift celebration", "🎉🎊🎈🎁🎂🥂🎇🎆"),
    ("галочка крест предупреждение check cross warn", "✅❌⚠❗❓💯🔴🟠🟡🟢🔵⚪⚫"),
];

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

/// Значки нижней полосы (набор Telegram), в порядке `CATEGORIES`.
pub const CATEGORY_ICONS: [&str; 8] = [
    "emoji/emoji_recent",
    "emoji/emoji_smile",
    "emoji/emoji_nature",
    "emoji/emoji_food",
    "emoji/emoji_travel",
    "emoji/emoji_activities",
    "emoji/emoji_objects",
    "emoji/emoji_love",
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
    /// При наличии валидного локального `emoji-apple.png` встроенный атлас не декодируется вовсе (экономия ОЗУ и старта).
    pub fn load(ctx: &egui::Context) -> Option<Self> {
        let local = std::fs::read(crate::worker::home().join("emoji-apple.png"))
            .ok()
            .and_then(|b| image::load_from_memory(&b).ok())
            .map(|i| i.to_rgba8())
            .filter(|i| i.dimensions() == (ATLAS_W, ATLAS_H));
        let img = match local {
            Some(l) => l,
            None => image::load_from_memory(ATLAS_BYTES).ok()?.to_rgba8(),
        };
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
        let mut by_cat: [Vec<char>; 8] = std::array::from_fn(|_| Vec::new());
        for &c in &list {
            let cat = category(c);
            if cat < by_cat.len() {
                by_cat[cat].push(c);
            }
        }
        let keywords: Vec<(String, Vec<char>)> = BUILTIN_KEYWORDS
            .iter()
            .map(|(words, chars)| {
                (
                    (*words).to_owned(),
                    chars.chars().filter(|c| index.contains_key(c)).collect(),
                )
            })
            .collect();
        Some(Self { tex, index, list, by_cat, keywords, size })
    }

    /// Дополняет индекс поиска ключевыми словами из Telegram (`messages.getEmojiKeywords`).
    pub fn merge_keywords(&mut self, pairs: Vec<(String, Vec<String>)>) {
        for (kw, ems) in pairs {
            let chars: Vec<char> = ems
                .into_iter()
                .filter_map(|s| s.chars().find(|c| self.index.contains_key(c)))
                .collect();
            if !chars.is_empty() && !kw.is_empty() {
                self.keywords.push((kw.to_lowercase(), chars));
            }
        }
    }

    /// Поиск эмодзи по русским и английским ключевым словам.
    pub fn search(&self, query: &str) -> Vec<char> {
        let q = query.trim().to_lowercase();
        if q.is_empty() {
            return Vec::new();
        }
        let mut out = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for (kw, chars) in &self.keywords {
            if kw.contains(&q) || kw.split_whitespace().any(|w| w.starts_with(&q)) {
                for &c in chars {
                    if seen.insert(c) {
                        out.push(c);
                    }
                }
            }
        }
        out
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

    /// Быстрая проверка, может ли символ находиться в атласе эмодзи (отсекает ASCII и кириллицу в O(1)).
    #[inline]
    pub fn might_be_atlas_char(c: char) -> bool {
        let u = c as u32;
        u >= 0x00A9 && !(0x0100..=0x2000).contains(&u)
    }

    /// Есть ли в тексте хотя бы один эмодзи из атласа (ASCII fast-path + пропуск кириллицы без хэша).
    #[inline]
    pub fn contains(&self, text: &str) -> bool {
        if text.is_ascii() {
            return false;
        }
        text.chars().any(|c| Self::might_be_atlas_char(c) && self.has(c))
    }

    /// Выводит текст с картинками вместо эмодзи (переносится по ширине контейнера).
    pub fn rich_label(&self, ui: &mut egui::Ui, text: &str, size: f32, color: Color32) {
        if !self.contains(text) {
            ui.label(RichText::new(text).size(size).color(color));
            return;
        }
        // Одно-три эмодзи без текста показываются крупно, как в Telegram.
        let segs = self.segments(text);
        let only_emoji = !segs.is_empty() && segs.iter().all(|s| matches!(s, Seg::Emoji(_)));
        let jumbo = match segs.len() {
            1 if only_emoji => Some(44.0),
            2..=3 if only_emoji => Some(34.0),
            _ => None,
        };
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = if jumbo.is_some() { 2.0 } else { 0.0 };
            for seg in segs {
                match seg {
                    Seg::Text(s) => {
                        ui.label(RichText::new(s).size(size).color(color));
                    }
                    Seg::Emoji(c) => {
                        if let Some(img) = self.image(c, jumbo.unwrap_or(size * 1.35)) {
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

    /// Панель выбора: разделы с заголовками, поиск и полоса значков внизу.
    /// `jump` — раздел, к которому нужно прокрутить (сбрасывается).
    pub fn picker(
        &self,
        ui: &mut egui::Ui,
        recents: &str,
        query: &str,
        jump: &mut Option<usize>,
    ) -> Option<char> {
        let mut chosen = None;
        let cell = 40.0;
        let cols = ((ui.available_width() / cell).floor() as usize).max(1);
        let q = query.trim();
        let want = jump.take();
        egui::ScrollArea::vertical()
            .max_height(ui.available_height())
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                if !q.is_empty() {
                    let hits = self.search(q);
                    if hits.is_empty() {
                        ui.add_space(28.0);
                        ui.vertical_centered(|ui| {
                            ui.label(RichText::new("Эмодзи не найдены").color(Color32::from_rgb(140, 140, 148)));
                        });
                        return;
                    }
                    self.paint_grid(ui, 99, &hits, cell, cols, &mut chosen);
                    return;
                }
                let recent: Vec<char> = recents.chars().filter(|c| self.has(*c)).collect();
                for (cat, (title, _)) in CATEGORIES.iter().enumerate() {
                    let items: &[char] = if cat == 0 { &recent } else { &self.by_cat[cat] };
                    if items.is_empty() {
                        continue;
                    }
                    let (head, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 28.0), Sense::hover());
                    if ui.is_rect_visible(head) {
                        ui.painter().text(
                            egui::pos2(head.left() + 8.0, head.bottom() - 6.0),
                            egui::Align2::LEFT_BOTTOM,
                            *title,
                            egui::FontId::proportional(12.5),
                            Color32::from_rgb(150, 150, 158),
                        );
                    }
                    if want == Some(cat) {
                        ui.scroll_to_rect(head, Some(egui::Align::TOP));
                    }
                    self.paint_grid(ui, cat, items, cell, cols, &mut chosen);
                }
            });
        chosen
    }

    fn paint_grid(
        &self,
        ui: &mut egui::Ui,
        cat: usize,
        items: &[char],
        cell: f32,
        cols: usize,
        chosen: &mut Option<char>,
    ) {
        for row in items.chunks(cols) {
            let (rect, _) = ui.allocate_exact_size(Vec2::new(cell * cols as f32, cell), Sense::hover());
            if !ui.is_rect_visible(rect) {
                continue;
            }
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
                let side = 30.0 + 4.0 * k_hover;
                if let Some(img) = self.image(c, side) {
                    img.paint_at(ui, egui::Rect::from_center_size(r.center(), Vec2::splat(side)));
                }
                if resp.clicked() {
                    *chosen = Some(c);
                }
            }
        }
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
        let img = image::load_from_memory(ATLAS_BYTES).unwrap();
        let rows = ATLAS_IDX.lines().count().div_ceil(COLS);
        assert_eq!(img.width() as usize, COLS * CELL as usize);
        assert_eq!(img.height() as usize, rows * CELL as usize);
    }

    #[test]
    fn modifiers_are_recognised() {
        assert!(is_modifier('\u{fe0f}') && is_modifier('\u{200d}') && is_modifier('\u{1f3fd}'));
        assert!(!is_modifier('a'));
    }

    #[test]
    fn builtin_keywords_cover_russian_and_english() {
        let has_fire = BUILTIN_KEYWORDS.iter().any(|(kw, em)| kw.contains("огонь") && kw.contains("fire") && em.contains('🔥'));
        let has_heart = BUILTIN_KEYWORDS.iter().any(|(kw, em)| kw.contains("сердце") && kw.contains("heart") && em.contains('❤'));
        assert!(has_fire && has_heart);
    }

    #[test]
    fn fast_emoji_precheck_matches_entire_atlas_equivalently() {
        let mut idx = HashMap::new();
        for (n, line) in ATLAS_IDX.lines().enumerate() {
            if let Some(c) = u32::from_str_radix(line.trim(), 16).ok().and_then(char::from_u32) {
                assert!(
                    Emoji::might_be_atlas_char(c),
                    "atlas char U+{:X} missed by fast-path filter",
                    c as u32
                );
                idx.insert(c, n);
            }
        }
        let samples = [
            "",
            "Plain ASCII message 12345!",
            "Обычное русское сообщение без эмодзи, но длинное и чатов много!",
            "Привет 🔥 как дела?",
            "❤",
            "café naïve résumé",
        ];
        let t0 = std::time::Instant::now();
        for _ in 0..1000 {
            for s in &samples {
                let ref_hit = s.chars().any(|c| idx.contains_key(&c));
                let fast_hit = !s.is_ascii()
                    && s.chars().any(|c| Emoji::might_be_atlas_char(c) && idx.contains_key(&c));
                assert_eq!(fast_hit, ref_hit, "mismatch on sample {s:?}");
            }
        }
        let _elapsed = t0.elapsed();
    }
}
