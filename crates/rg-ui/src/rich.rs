//! Форматированный текст сообщений: жирный, курсив, подчёркнутый, зачёркнутый, код, спойлер,
//! ссылки и упоминания. Текст режется на куски по границам сущностей Telegram.

use std::collections::{HashMap, HashSet};

use eframe::egui::{self, Color32, FontFamily, FontId, RichText, Vec2};
use rg_core::engine::{Entity, EntityKind};

use super::emoji::{Emoji, Seg};
use super::theme::FAM_DISPLAY;

/// Кусок текста с одинаковым оформлением.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Run {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    pub code: bool,
    pub pre: bool,
    pub quote: bool,
    pub spoiler: bool,
    /// Куда ведёт ссылка (только http/https).
    pub url: Option<String>,
    /// Хэштег (`#rust` -> глобальный поиск по публичным каналам).
    pub hashtag: Option<String>,
    /// Пользовательский Premium-эмодзи (идентификатор документа Telegram).
    pub custom_emoji: Option<i64>,
}

/// Оборачивает выделение (или конец строки, если ничего не выделено) парными тегами форматирования.
pub fn wrap_selection(text: &mut String, sel_chars: Option<(usize, usize)>, open: &str, close: &str) {
    let chars: Vec<char> = text.chars().collect();
    let total = chars.len();
    let (a, b) = sel_chars.unwrap_or((total, total));
    let (lo, hi) = (a.min(b).min(total), a.max(b).min(total));
    if lo == hi {
        text.push_str(open);
        text.push_str(close);
        return;
    }
    let before: String = chars[..lo].iter().collect();
    let mid: String = chars[lo..hi].iter().collect();
    let after: String = chars[hi..].iter().collect();
    *text = format!("{before}{open}{mid}{close}{after}");
}

/// Разбирает разметку Telegram-стиля (`**жирный**`, `__курсив__`, `~~зачёркнутый~~`,
/// `` `код` ``, ```` ```блок``` ````, `||спойлер||`, `[текст](url)`, цитаты `> ...`),
/// возвращая чистый текст и `Vec<Entity>`, сохраняя точные индексы `custom_emojis`.
pub fn parse_formatted(raw: &str, custom_emojis: &[(usize, usize, i64)]) -> (String, Vec<Entity>) {
    let chars: Vec<char> = raw.chars().collect();
    let n = chars.len();
    let mut clean = String::with_capacity(raw.len());
    let mut char_map = vec![0usize; n + 1];
    let mut entities: Vec<Entity> = Vec::new();
    let mut i = 0usize;
    let mut out_len = 0usize;

    let starts_with = |idx: usize, pat: &[char]| -> bool {
        idx + pat.len() <= n && &chars[idx..idx + pat.len()] == pat
    };
    let find_close = |from: usize, pat: &[char]| -> Option<usize> {
        let mut j = from;
        while j + pat.len() <= n {
            if &chars[j..j + pat.len()] == pat {
                return Some(j);
            }
            j += 1;
        }
        None
    };

    while i < n {
        char_map[i] = out_len;
        // Цитата в начале строки: `> текст`
        let at_line_start = i == 0 || chars[i - 1] == '\n';
        if at_line_start && starts_with(i, &['>', ' ']) {
            let mut end_line = i + 2;
            while end_line < n && chars[end_line] != '\n' {
                end_line += 1;
            }
            if end_line > i + 2 {
                let start_c = out_len;
                for idx in (i + 2)..end_line {
                    char_map[idx] = out_len;
                    clean.push(chars[idx]);
                    out_len += 1;
                }
                char_map[end_line] = out_len;
                entities.push(Entity { kind: EntityKind::Quote, start: start_c, end: out_len, url: None });
                i = end_line;
                continue;
            }
        }

        // Блок кода: ```...```
        if starts_with(i, &['`', '`', '`'])
            && let Some(end) = find_close(i + 3, &['`', '`', '`'])
            && end > i + 3
        {
            let mut body_start = i + 3;
            if chars[body_start] == '\n' {
                body_start += 1;
            }
            if end > body_start {
                let start_c = out_len;
                for idx in body_start..end {
                    char_map[idx] = out_len;
                    clean.push(chars[idx]);
                    out_len += 1;
                }
                entities.push(Entity { kind: EntityKind::Pre, start: start_c, end: out_len, url: None });
                i = end + 3;
                continue;
            }
        }

        // Инлайн-маркеры: **, __, ~~, ||, `
        let mut matched_delim = false;
        for (pat, kind) in [
            (&['*', '*'][..], EntityKind::Bold),
            (&['_', '_'][..], EntityKind::Italic),
            (&['~', '~'][..], EntityKind::Strike),
            (&['|', '|'][..], EntityKind::Spoiler),
            (&['`'][..], EntityKind::Code),
        ] {
            if starts_with(i, pat)
                && let Some(end) = find_close(i + pat.len(), pat)
                && end > i + pat.len()
            {
                // Запрещаем случайный перенос через двойные звёздочки в математике без пары рядом, но разрешаем многословность внутри строки.
                let body = &chars[i + pat.len()..end];
                if !(pat.len() == 1 && body.contains(&'\n')) {
                    let start_c = out_len;
                    for idx in (i + pat.len())..end {
                        char_map[idx] = out_len;
                        clean.push(chars[idx]);
                        out_len += 1;
                    }
                    entities.push(Entity { kind, start: start_c, end: out_len, url: None });
                    i = end + pat.len();
                    matched_delim = true;
                    break;
                }
            }
        }
        if matched_delim {
            continue;
        }

        // Ссылка: [label](url)
        if chars[i] == '['
            && let Some(mid) = find_close(i + 1, &[']', '('])
            && let Some(end) = find_close(mid + 2, &[')'])
            && mid > i + 1
            && end > mid + 2
        {
            let url_s: String = chars[(mid + 2)..end].iter().collect();
            if is_safe_url(&url_s) {
                let start_c = out_len;
                for idx in (i + 1)..mid {
                    char_map[idx] = out_len;
                    clean.push(chars[idx]);
                    out_len += 1;
                }
                entities.push(Entity {
                    kind: EntityKind::Link,
                    start: start_c,
                    end: out_len,
                    url: Some(url_s),
                });
                i = end + 1;
                continue;
            }
        }

        clean.push(chars[i]);
        out_len += 1;
        i += 1;
    }
    char_map[n] = out_len;

    for &(st, ln, doc_id) in custom_emojis {
        if st < n && st + ln <= n {
            let cs = char_map[st];
            let ce = char_map[st + ln].max(cs + ln);
            if ce > cs {
                entities.push(Entity {
                    kind: EntityKind::CustomEmoji(doc_id),
                    start: cs,
                    end: ce,
                    url: None,
                });
            }
        }
    }

    (clean, entities)
}

/// Разрешён ли адрес для открытия в браузере: только http и https.
pub fn is_safe_url(url: &str) -> bool {
    let l = url.trim().to_ascii_lowercase();
    let Some((scheme, rest)) = l.split_once(':') else { return false };
    matches!(scheme, "http" | "https") && rest.starts_with("//") && !l.contains(char::is_whitespace)
}

/// Приводит адрес из текста к виду с протоколом (по умолчанию защищённый).
fn normalize_url(raw: &str) -> String {
    let raw = raw.trim();
    let has_scheme = raw.split_once(':').is_some_and(|(s, r)| !s.is_empty() && r.starts_with("//"));
    if has_scheme {
        raw.to_owned()
    } else {
        ["https", ":", "//", raw].concat()
    }
}

/// Режет `text` на куски по границам `ents` (индексы в символах).
pub fn build_runs(text: &str, ents: &[Entity]) -> Vec<Run> {
    let chars: Vec<char> = text.chars().collect();
    let mut cuts: Vec<usize> = vec![0, chars.len()];
    for e in ents {
        cuts.push(e.start.min(chars.len()));
        cuts.push(e.end.min(chars.len()));
    }
    cuts.sort_unstable();
    cuts.dedup();
    let mut runs: Vec<Run> = Vec::new();
    for w in cuts.windows(2) {
        let (a, b) = (w[0], w[1]);
        if a == b {
            continue;
        }
        let slice: String = chars[a..b].iter().collect();
        let mut r = Run { text: slice.clone(), ..Run::default() };
        for e in ents.iter().filter(|e| e.start <= a && e.end >= b) {
            match e.kind {
                EntityKind::Bold => r.bold = true,
                EntityKind::Italic => r.italic = true,
                EntityKind::Underline => r.underline = true,
                EntityKind::Strike => r.strike = true,
                EntityKind::Code => r.code = true,
                EntityKind::Pre => {
                    r.code = true;
                    r.pre = true;
                }
                EntityKind::Spoiler => r.spoiler = true,
                EntityKind::Quote => r.quote = true,
                EntityKind::Link => {
                    let url = e.url.clone().unwrap_or_else(|| normalize_url(&slice));
                    if is_safe_url(&url) {
                        r.url = Some(url);
                    }
                }
                EntityKind::Mention => {
                    let name = slice.trim_start_matches('@');
                    if !name.is_empty() {
                        r.url = Some(format!("https://t.me/{name}"));
                    }
                }
                EntityKind::Hashtag => {
                    let tag = slice.trim_start_matches('#').trim();
                    if !tag.is_empty() {
                        r.hashtag = Some(tag.to_owned());
                    }
                }
                EntityKind::Command => {}
                EntityKind::CustomEmoji(id) => r.custom_emoji = Some(id),
            }
        }
        runs.push(r);
    }
    runs
}

/// Нажимаемая область текста: ссылка, хэштег или спойлер (номер куска).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Hit {
    Link(String),
    Hashtag(String),
    Spoiler(usize),
}

/// Рисует куски в потоке с переносом и возвращает нажимаемые области. `open` — раскрытые спойлеры.
#[allow(clippy::too_many_arguments)]
pub fn render(
    ui: &mut egui::Ui,
    emoji: Option<&Emoji>,
    custom_tex: &HashMap<i64, egui::TextureHandle>,
    runs: &[Run],
    size: f32,
    color: Color32,
    link: Color32,
    msg_id: i32,
    open: &HashSet<(i32, usize)>,
) -> Vec<(egui::Rect, Hit)> {
    let mut hot = Vec::new();
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        for (idx, run) in runs.iter().enumerate() {
            let shown = !run.spoiler || open.contains(&(msg_id, idx));
            if shown
                && let Some(doc_id) = run.custom_emoji
                && let Some(tex) = custom_tex.get(&doc_id)
            {
                let img = egui::Image::new(tex)
                    .fit_to_exact_size(Vec2::splat(size * 1.35))
                    .corner_radius(3);
                ui.add(img);
                continue;
            }
            let segs = match emoji {
                Some(e) if shown => e.segments(&run.text),
                _ => vec![Seg::Text(run.text.clone())],
            };
            for seg in segs {
                match seg {
                    Seg::Emoji(c) => {
                        if let Some(img) = emoji.and_then(|e| e.image(c, size * 1.25)) {
                            ui.add(img);
                        }
                    }
                    Seg::Text(s) => {
                        let mut rt = RichText::new(s).size(size).color(color);
                        if run.bold {
                            rt = rt.font(FontId::new(size, FontFamily::Name(FAM_DISPLAY.into())));
                        }
                        if run.italic {
                            rt = rt.italics();
                        }
                        if run.quote {
                            rt = rt
                                .italics()
                                .background_color(Color32::from_rgba_unmultiplied(245, 130, 32, 26));
                        }
                        if run.underline {
                            rt = rt.underline();
                        }
                        if run.strike {
                            rt = rt.strikethrough();
                        }
                        if run.code || run.pre {
                            rt = rt
                                .monospace()
                                .background_color(Color32::from_rgba_unmultiplied(
                                    255,
                                    255,
                                    255,
                                    if run.pre { 32 } else { 22 },
                                ));
                        }
                        if run.url.is_some() {
                            rt = rt.color(link).underline();
                        } else if run.hashtag.is_some() {
                            rt = rt.color(link);
                        }
                        if !shown {
                            // Скрытый спойлер: текст сливается с плашкой.
                            let veil = Color32::from_rgb(88, 88, 96);
                            rt = rt.color(veil).background_color(veil);
                        }
                        let resp = ui.add(egui::Label::new(rt).selectable(false));
                        if run.spoiler && !shown {
                            hot.push((resp.rect, Hit::Spoiler(idx)));
                        } else if let Some(u) = &run.url {
                            hot.push((resp.rect, Hit::Link(u.clone())));
                        } else if let Some(tag) = &run.hashtag {
                            hot.push((resp.rect, Hit::Hashtag(tag.clone())));
                        } else if run.spoiler {
                            // раскрытый спойлер: клик снова скрывает
                            hot.push((resp.rect, Hit::Spoiler(idx)));
                        }
                    }
                }
            }
        }
    });
    hot
}

/// Открывает адрес в браузере по умолчанию (только http/https, без окна консоли).
pub fn open_url(url: &str) {
    if !is_safe_url(url) {
        return;
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let _ = std::process::Command::new("rundll32")
            .args(["url.dll,FileProtocolHandler", url])
            .creation_flags(0x0800_0000)
            .spawn();
    }
    #[cfg(all(unix, not(target_os = "android")))]
    {
        let opener = if cfg!(target_os = "macos") { "open" } else { "xdg-open" };
        let _ = std::process::Command::new(opener).arg(url).spawn();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ent(kind: EntityKind, start: usize, end: usize) -> Entity {
        Entity { kind, start, end, url: None }
    }

    #[test]
    fn plain_text_is_one_run() {
        let r = build_runs("привет", &[]);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].text, "привет");
    }

    #[test]
    fn bold_in_the_middle_splits_into_three() {
        let r = build_runs("а жирный б", &[ent(EntityKind::Bold, 2, 8)]);
        assert_eq!(r.iter().map(|x| x.text.as_str()).collect::<Vec<_>>(), ["а ", "жирный", " б"]);
        assert!(!r[0].bold && r[1].bold && !r[2].bold);
    }

    #[test]
    fn overlapping_entities_merge_flags() {
        let r = build_runs("abcd", &[ent(EntityKind::Bold, 0, 3), ent(EntityKind::Italic, 1, 4)]);
        assert_eq!(r.len(), 3);
        assert!(r[1].bold && r[1].italic);
        assert!(!r[2].bold && r[2].italic);
    }

    #[test]
    fn link_gets_scheme_and_unsafe_ones_are_dropped() {
        let r = build_runs("site.com", &[ent(EntityKind::Link, 0, 8)]);
        assert_eq!(r[0].url.as_deref(), Some("https://site.com"));
        let bad = Entity { kind: EntityKind::Link, start: 0, end: 3, url: Some("javascript:alert(1)".into()) };
        assert_eq!(build_runs("abc", &[bad])[0].url, None);
        assert!(!is_safe_url("file:///c:/x"));
        assert!(is_safe_url("https://t.me/x"));
    }

    #[test]
    fn mention_links_to_telegram() {
        let r = build_runs("@durov", &[ent(EntityKind::Mention, 0, 6)]);
        assert_eq!(r[0].url.as_deref(), Some("https://t.me/durov"));
    }

    #[test]
    fn custom_emoji_sets_doc_id_on_run() {
        let r = build_runs("hi 🔥!", &[ent(EntityKind::CustomEmoji(42), 3, 4)]);
        assert_eq!(r.len(), 3);
        assert_eq!(r[0].custom_emoji, None);
        assert_eq!(r[1].custom_emoji, Some(42));
        assert_eq!(r[2].custom_emoji, None);
    }

    #[test]
    fn parse_formatted_extracts_markdown_and_remaps_custom_emoji() {
        let raw = "**жирный** и `код` ||секрет|| 🔥";
        let fire_idx = raw.chars().position(|c| c == '🔥').unwrap();
        let (clean, ents) = parse_formatted(raw, &[(fire_idx, 1, 777)]);
        assert_eq!(clean, "жирный и код секрет 🔥");
        assert!(ents.iter().any(|e| e.kind == EntityKind::Bold && e.start == 0 && e.end == 6));
        assert!(ents.iter().any(|e| e.kind == EntityKind::Code && e.start == 9 && e.end == 12));
        assert!(ents.iter().any(|e| e.kind == EntityKind::Spoiler && e.start == 13 && e.end == 19));
        assert!(ents.iter().any(|e| e.kind == EntityKind::CustomEmoji(777) && e.start == 20 && e.end == 21));
    }

    #[test]
    fn parse_formatted_handles_quotes_and_links() {
        let (clean, ents) = parse_formatted("> цитата\n[сайт](https://t.me/test)", &[]);
        assert_eq!(clean, "цитата\nсайт");
        assert!(ents.iter().any(|e| e.kind == EntityKind::Quote));
        assert!(ents.iter().any(|e| e.kind == EntityKind::Link && e.url.as_deref() == Some("https://t.me/test")));
    }

    #[test]
    fn wrap_selection_and_pre_block_work() {
        let mut s = "привет мир".to_owned();
        wrap_selection(&mut s, Some((7, 10)), "**", "**");
        assert_eq!(s, "привет **мир**");
        let (clean, ents) = parse_formatted("```fn main() {}``` и __курсив__ ~~нет~~ [x](javascript:1)", &[]);
        assert_eq!(clean, "fn main() {} и курсив нет [x](javascript:1)");
        assert!(ents.iter().any(|e| e.kind == EntityKind::Pre));
        assert!(ents.iter().any(|e| e.kind == EntityKind::Italic));
        assert!(ents.iter().any(|e| e.kind == EntityKind::Strike));
        assert!(!ents.iter().any(|e| e.kind == EntityKind::Link));
    }
}
