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
            if let Some(target_dialog) = f.dialog_id
                && target_dialog != chat
            {
                return false;
            }
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

/// Экспорт фильтров в JSON (совместимый формат для буфера и файлов).
pub fn export_filters_json(filters: &[Filter]) -> String {
    serde_json::to_string_pretty(filters).unwrap_or_else(|_| "[]".to_owned())
}

#[derive(serde::Deserialize)]
#[serde(untagged)]
enum ImportPayload {
    List(Vec<ImportFilterItem>),
    Wrapped { filters: Vec<ImportFilterItem> },
}

#[derive(serde::Deserialize)]
struct ImportFilterItem {
    text: String,
    #[serde(default, alias = "isRegex")]
    is_regex: bool,
    #[serde(default, alias = "caseSensitive")]
    case_sensitive: bool,
    #[serde(default = "yes_default")]
    enabled: bool,
    #[serde(default, alias = "dialogId")]
    dialog_id: Option<i64>,
    #[serde(default, alias = "excludedChats")]
    excluded_chats: Vec<i64>,
}

fn yes_default() -> bool {
    true
}

/// Импорт фильтров из JSON (поддерживаются списки Rustgramm и AyuGram Desktop).
pub fn import_filters_json(raw: &str) -> Result<Vec<Filter>, String> {
    let payload: ImportPayload = serde_json::from_str(raw.trim()).map_err(|e| format!("JSON: {e}"))?;
    let items = match payload {
        ImportPayload::List(v) | ImportPayload::Wrapped { filters: v } => v,
    };
    Ok(items
        .into_iter()
        .filter(|x| !x.text.trim().is_empty())
        .map(|x| Filter {
            text: x.text,
            is_regex: x.is_regex,
            case_sensitive: x.case_sensitive,
            enabled: x.enabled,
            dialog_id: x.dialog_id,
            excluded_chats: x.excluded_chats,
        })
        .collect())
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

    #[test]
    fn dialog_specific_filter_applies_only_to_that_chat() {
        let mut d = f("секрет", false);
        d.dialog_id = Some(42);
        let (set, _) = FilterSet::build(&[d]);
        assert!(set.hides(42, "тут секрет"));
        assert!(!set.hides(43, "тут секрет"));
    }

    #[test]
    fn export_and_import_filters_roundtrip_and_ayu_camelcase() {
        let mut rule = f("акция", false);
        rule.dialog_id = Some(-100);
        rule.excluded_chats = vec![7];
        let exported = export_filters_json(&[rule.clone()]);
        let back = import_filters_json(&exported).unwrap();
        assert_eq!(back, vec![rule]);

        let ayu_json = r#"{"filters":[{"text":"^spam$","isRegex":true,"caseSensitive":true,"dialogId":5,"excludedChats":[9]},{"text":"   "}]}"#;
        let parsed = import_filters_json(ayu_json).unwrap();
        assert_eq!(parsed.len(), 1);
        assert!(parsed[0].is_regex && parsed[0].case_sensitive);
        assert_eq!(parsed[0].dialog_id, Some(5));
        assert_eq!(parsed[0].excluded_chats, vec![9]);
        assert!(import_filters_json("not-json").is_err());
    }
}
