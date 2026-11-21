//! Форматированный текст сообщений: жирный, курсив, подчёркнутый, зачёркнутый, код, спойлер,
//! ссылки и упоминания. Текст режется на куски по границам сущностей Telegram.

use std::collections::HashSet;

use eframe::egui::{self, Color32, FontFamily, FontId, RichText};
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
    pub quote: bool,
    pub spoiler: bool,
    /// Куда ведёт ссылка (только http/https).
    pub url: Option<String>,
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
                EntityKind::Code | EntityKind::Pre => r.code = true,
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
                EntityKind::Hashtag | EntityKind::Command => {}
            }
        }
        runs.push(r);
    }
    runs
}

/// Нажимаемая область текста: ссылка или спойлер (номер куска).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Hit {
    Link(String),
    Spoiler(usize),
}

/// Рисует куски в потоке с переносом и возвращает нажимаемые области. `open` — раскрытые спойлеры.
#[allow(clippy::too_many_arguments)]
pub fn render(
    ui: &mut egui::Ui,
    emoji: Option<&Emoji>,
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
                        if run.italic || run.quote {
                            rt = rt.italics();
                        }
                        if run.underline {
                            rt = rt.underline();
                        }
                        if run.strike {
                            rt = rt.strikethrough();
                        }
                        if run.code {
                            rt = rt
                                .monospace()
                                .background_color(Color32::from_rgba_unmultiplied(255, 255, 255, 24));
                        }
                        if run.url.is_some() {
                            rt = rt.color(link).underline();
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
}
