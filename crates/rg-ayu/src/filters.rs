//! Фильтры сообщений AyuGram: скрывают сообщения, чей текст подходит под правило.
//! Правило — подстрока или регулярное выражение; можно исключить отдельные чаты.

use regex::{Regex, RegexBuilder};
pub use rg_settings::Filter;

/// Список правил в виде, готовом для проверки (регулярные выражения собраны один раз).
#[derive(Debug, Default)]
pub struct FilterSet {
    rules: Vec<(Filter, Option<Regex>)>,
}

impl FilterSet {
    /// Некорректные регулярные выражения пропускаются, но возвращаются в списке ошибок.
    pub fn build(filters: &[Filter]) -> (Self, Vec<String>) {
        let mut rules = Vec::new();
        let mut bad = Vec::new();
        for f in filters.iter().filter(|f| f.enabled && !f.text.is_empty()) {
            if f.is_regex {
                match RegexBuilder::new(&f.text)
                    .case_insensitive(!f.case_sensitive)
                    .size_limit(1 << 20)
                    .build()
                {
                    Ok(r) => rules.push((f.clone(), Some(r))),
                    Err(e) => bad.push(format!("{}: {e}", f.text)),
                }
            } else {
                rules.push((f.clone(), None));
            }
        }
        (Self { rules }, bad)
    }

    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    /// Нужно ли скрыть сообщение с таким текстом в этом чате.
    pub fn hides(&self, chat: i64, text: &str) -> bool {
        self.rules.iter().any(|(f, re)| {
            if f.excluded_chats.contains(&chat) {
                return false;
            }
            match re {
                Some(r) => r.is_match(text),
                None if f.case_sensitive => text.contains(&f.text),
                None => text.to_lowercase().contains(&f.text.to_lowercase()),
            }
        })
    }
}

/// Убирает «зальго» — нагромождение комбинируемых диакритических знаков (U+0300..U+036F и
/// родственные блоки): оставляет не больше двух подряд.
pub fn strip_zalgo(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut run = 0;
    for c in text.chars() {
        let combining = matches!(c as u32, 0x0300..=0x036F | 0x1AB0..=0x1AFF | 0x1DC0..=0x1DFF | 0x20D0..=0x20FF | 0xFE20..=0xFE2F);
        if combining {
            run += 1;
            if run > 2 {
                continue;
            }
        } else {
            run = 0;
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(text: &str, is_regex: bool) -> Filter {
        Filter {
            text: text.to_owned(),
            is_regex,
            ..Filter::default()
        }
    }

    #[test]
    fn substring_is_case_insensitive_by_default() {
        let (set, bad) = FilterSet::build(&[f("Реклама", false)]);
        assert!(bad.is_empty());
        assert!(set.hides(1, "Это РЕКЛАМА канала"));
        assert!(!set.hides(1, "обычное сообщение"));
    }

    #[test]
    fn regex_and_case_sensitivity() {
        let mut r = f(r"^\d{3}$", true);
        r.case_sensitive = true;
        let (set, _) = FilterSet::build(&[r]);
        assert!(set.hides(1, "123"));
        assert!(!set.hides(1, "1234"));
    }

    #[test]
    fn excluded_chat_is_not_filtered() {
        let mut x = f("спам", false);
        x.excluded_chats.push(7);
        let (set, _) = FilterSet::build(&[x]);
        assert!(set.hides(1, "спам"));
        assert!(!set.hides(7, "спам"));
    }

    #[test]
    fn bad_regex_reported_not_fatal() {
        let (set, bad) = FilterSet::build(&[f("(", true), f("ok", false)]);
        assert_eq!(bad.len(), 1);
        assert!(set.hides(1, "ok"));
    }

    #[test]
    fn disabled_and_empty_rules_are_ignored() {
        let mut off = f("x", false);
        off.enabled = false;
        let (set, _) = FilterSet::build(&[off, f("", false)]);
        assert!(set.is_empty());
    }

    #[test]
    fn zalgo_is_trimmed() {
        let z = "a\u{0300}\u{0301}\u{0302}\u{0303}\u{0304}b";
        assert_eq!(strip_zalgo(z), "a\u{0300}\u{0301}b");
        assert_eq!(strip_zalgo("привет"), "привет");
    }
}
