//! Движок плагинов Rustgramm (порт системы `.plugin` из AyuGram Android / exteraGram на чистый Rust).
//! Работает полностью локально и детерминированно (без сетевых вызовов и без Python-интерпретатора):
//! разбирает заголовки `.plugin` (`__id__`, `__name__`, `__version__`, `__author__`, `__description__`),
//! команды `/cmd`, текстовые макросы, хуки исходящих/входящих сообщений и безопасный калькулятор выражений.
//! В полной сборке (`feature = "plugins"`) включён по умолчанию; в облегчённой (`--no-default-features`)
//! может отключаться на уровне компиляции.

use std::path::Path;

/// Метаданные и правила одного плагина.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plugin {
    pub id: String,
    pub name: String,
    pub version: String,
    pub author: String,
    pub description: String,
    pub enabled: bool,
    pub builtin: bool,
    pub rules: Vec<PluginRule>,
}

/// Правило обработки сообщений плагином.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginRule {
    /// Команда в начале сообщения (`/cmd аргументы` → шаблон с `{args}`).
    Command {
        trigger: String,
        template: String,
    },
    /// Встроенная функция преобразования по команде (`/calc`, `/upper`, `/lower`, `/reverse`, `/spaced`).
    BuiltinCmd {
        trigger: String,
        kind: BuiltinOp,
    },
    /// Автозамена подстроки в исходящем сообщении.
    Replace {
        from: String,
        to: String,
    },
    /// Очистка невидимых мусорных символов во входящих и исходящих сообщениях.
    StripZeroWidth,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuiltinOp {
    Calc,
    Upper,
    Lower,
    Reverse,
    Spaced,
    Code,
    Quote,
    Boxed,
}

/// Итог обработки исходящего текста цепочкой активных плагинов.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HookOutcome {
    /// Текст не изменился.
    Unchanged,
    /// Текст преобразован хотя бы одним плагином.
    Modified(String),
}

impl HookOutcome {
    pub fn into_text(self, original: &str) -> String {
        match self {
            Self::Unchanged => original.to_owned(),
            Self::Modified(s) => s,
        }
    }
}

/// Реестр плагинов (встроенные + пользовательские `.plugin` из каталога `plugins/`).
#[derive(Debug, Clone, Default)]
pub struct PluginRegistry {
    pub plugins: Vec<Plugin>,
}

impl PluginRegistry {
    /// Создаёт реестр со стандартным набором плагинов для телефона и ПК.
    pub fn with_builtins() -> Self {
        Self {
            plugins: vec![
                Plugin {
                    id: "extera.macros".to_owned(),
                    name: "Быстрые команды и эмоции".to_owned(),
                    version: "1.2.0".to_owned(),
                    author: "Rustgramm".to_owned(),
                    description: "Команды /shrug, /flip, /unflip, /lenny и макросы :arrow:, :check:, :star:".to_owned(),
                    enabled: true,
                    builtin: true,
                    rules: vec![
                        PluginRule::Command {
                            trigger: "/shrug".to_owned(),
                            template: "{args} ¯\\_(ツ)_/¯".to_owned(),
                        },
                        PluginRule::Command {
                            trigger: "/flip".to_owned(),
                            template: "{args} (╯°□°）╯︵ ┻━┻".to_owned(),
                        },
                        PluginRule::Command {
                            trigger: "/unflip".to_owned(),
                            template: "{args} ┬─┬ノ( º _ ºノ)".to_owned(),
                        },
                        PluginRule::Command {
                            trigger: "/lenny".to_owned(),
                            template: "{args} ( ͡° ͜ʖ ͡°)".to_owned(),
                        },
                        PluginRule::Replace {
                            from: ":arrow:".to_owned(),
                            to: "→".to_owned(),
                        },
                        PluginRule::Replace {
                            from: ":check:".to_owned(),
                            to: "✓".to_owned(),
                        },
                        PluginRule::Replace {
                            from: ":star:".to_owned(),
                            to: "★".to_owned(),
                        },
                    ],
                },
                Plugin {
                    id: "extera.calc".to_owned(),
                    name: "Калькулятор в чате".to_owned(),
                    version: "1.1.0".to_owned(),
                    author: "Rustgramm".to_owned(),
                    description: "Вычисляет арифметические выражения прямо при отправке: /calc (2 + 3) * 4".to_owned(),
                    enabled: true,
                    builtin: true,
                    rules: vec![PluginRule::BuiltinCmd {
                        trigger: "/calc".to_owned(),
                        kind: BuiltinOp::Calc,
                    }],
                },
                Plugin {
                    id: "extera.typography".to_owned(),
                    name: "Чистая типографика".to_owned(),
                    version: "1.0.0".to_owned(),
                    author: "Rustgramm".to_owned(),
                    description: "Автозамена -- на длинное тире —, << >> на кавычки « » и +- на ±".to_owned(),
                    enabled: false,
                    builtin: true,
                    rules: vec![
                        PluginRule::Replace {
                            from: " -- ".to_owned(),
                            to: " — ".to_owned(),
                        },
                        PluginRule::Replace {
                            from: "<<".to_owned(),
                            to: "«".to_owned(),
                        },
                        PluginRule::Replace {
                            from: ">>".to_owned(),
                            to: "»".to_owned(),
                        },
                        PluginRule::Replace {
                            from: "+-".to_owned(),
                            to: "±".to_owned(),
                        },
                    ],
                },
                Plugin {
                    id: "extera.formatter".to_owned(),
                    name: "Текстовые трансформеры".to_owned(),
                    version: "1.1.0".to_owned(),
                    author: "Rustgramm".to_owned(),
                    description: "Команды /upper, /lower, /reverse, /spaced, /code, /quote, /box".to_owned(),
                    enabled: true,
                    builtin: true,
                    rules: vec![
                        PluginRule::BuiltinCmd {
                            trigger: "/upper".to_owned(),
                            kind: BuiltinOp::Upper,
                        },
                        PluginRule::BuiltinCmd {
                            trigger: "/lower".to_owned(),
                            kind: BuiltinOp::Lower,
                        },
                        PluginRule::BuiltinCmd {
                            trigger: "/reverse".to_owned(),
                            kind: BuiltinOp::Reverse,
                        },
                        PluginRule::BuiltinCmd {
                            trigger: "/spaced".to_owned(),
                            kind: BuiltinOp::Spaced,
                        },
                        PluginRule::BuiltinCmd {
                            trigger: "/code".to_owned(),
                            kind: BuiltinOp::Code,
                        },
                        PluginRule::BuiltinCmd {
                            trigger: "/quote".to_owned(),
                            kind: BuiltinOp::Quote,
                        },
                        PluginRule::BuiltinCmd {
                            trigger: "/box".to_owned(),
                            kind: BuiltinOp::Boxed,
                        },
                    ],
                },
                Plugin {
                    id: "extera.clean_zwsp".to_owned(),
                    name: "Санитар невидимых символов".to_owned(),
                    version: "1.0.0".to_owned(),
                    author: "Rustgramm".to_owned(),
                    description: "Удаляет скрытые Zero-Width символы слежки (U+200B..U+200D, U+FEFF) из текста".to_owned(),
                    enabled: true,
                    builtin: true,
                    rules: vec![PluginRule::StripZeroWidth],
                },
            ],
        }
    }

    /// Синхронизирует список отключённых идентификаторов из настроек (`disabled_ids`).
    pub fn sync_disabled(&mut self, disabled_ids: &[String]) {
        for p in &mut self.plugins {
            p.enabled = !disabled_ids.iter().any(|d| d == &p.id);
        }
    }

    /// Возвращает список отключённых идентификаторов для сохранения в `Settings`.
    pub fn disabled_ids(&self) -> Vec<String> {
        self.plugins
            .iter()
            .filter(|p| !p.enabled)
            .map(|p| p.id.clone())
            .collect()
    }

    /// Загружает все пользовательские `.plugin` файлы из каталога `dir`.
    pub fn load_dir(&mut self, dir: &Path) -> usize {
        let Ok(rd) = std::fs::read_dir(dir) else {
            return 0;
        };
        let mut count = 0usize;
        for entry in rd.flatten() {
            let p = entry.path();
            if p.extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| matches!(e, "plugin" | "rgplugin"))
                && let Ok(src) = std::fs::read_to_string(&p)
                && let Ok(parsed) = parse_plugin(&src)
            {
                self.upsert(parsed);
                count += 1;
            }
        }
        count
    }

    /// Добавляет или обновляет плагин по его `id`.
    pub fn upsert(&mut self, plugin: Plugin) {
        if let Some(existing) = self.plugins.iter_mut().find(|p| p.id == plugin.id) {
            let was_enabled = existing.enabled;
            *existing = plugin;
            existing.enabled = was_enabled;
        } else {
            self.plugins.push(plugin);
        }
    }

    /// Удаляет пользовательский плагин по `id` (встроенные не удаляются, только выключаются).
    pub fn remove(&mut self, id: &str) -> bool {
        let before = self.plugins.len();
        self.plugins.retain(|p| p.builtin || p.id != id);
        self.plugins.len() < before
    }

    /// Применяет все включённые плагины к исходящему сообщению перед отправкой.
    pub fn apply_outgoing(&self, input: &str) -> HookOutcome {
        let mut current = input.to_owned();
        let mut changed = false;
        for plugin in self.plugins.iter().filter(|p| p.enabled) {
            for rule in &plugin.rules {
                if let Some(next) = apply_rule(rule, &current)
                    && next != current
                {
                    current = next;
                    changed = true;
                }
            }
        }
        if changed {
            HookOutcome::Modified(current)
        } else {
            HookOutcome::Unchanged
        }
    }

    /// Применяет правила очистки ко входящему сообщению.
    pub fn apply_incoming(&self, input: &str) -> String {
        let mut current = input.to_owned();
        for plugin in self.plugins.iter().filter(|p| p.enabled) {
            for rule in &plugin.rules {
                if matches!(rule, PluginRule::StripZeroWidth) {
                    current = strip_zero_width(&current);
                }
            }
        }
        current
    }
}

fn strip_zero_width(s: &str) -> String {
    s.chars()
        .filter(|&c| !matches!(c as u32, 0x200B..=0x200F | 0x202A..=0x202E | 0x2060..=0x2064 | 0xFEFF))
        .collect()
}

fn apply_rule(rule: &PluginRule, text: &str) -> Option<String> {
    match rule {
        PluginRule::Command { trigger, template } => {
            let trimmed = text.trim();
            if trimmed == trigger {
                Some(template.replace("{args}", "").trim().to_owned())
            } else {
                let rest = trimmed.strip_prefix(trigger)?;
                if !rest.starts_with(char::is_whitespace) {
                    return None;
                }
                let args = rest.trim();
                Some(template.replace("{args}", args).trim().to_owned())
            }
        }
        PluginRule::BuiltinCmd { trigger, kind } => {
            let trimmed = text.trim();
            let rest = trimmed.strip_prefix(trigger)?;
            if !rest.is_empty() && !rest.starts_with(char::is_whitespace) {
                return None;
            }
            let args = rest.trim();
            if args.is_empty() {
                return None;
            }
            match kind {
                BuiltinOp::Calc => {
                    let val = eval_expr(args).ok()?;
                    let formatted = if (val - val.round()).abs() < 1e-9 {
                        format!("{}", val.round() as i64)
                    } else {
                        format!("{val:.4}")
                            .trim_end_matches('0')
                            .trim_end_matches('.')
                            .to_owned()
                    };
                    Some(format!("{args} = {formatted}"))
                }
                BuiltinOp::Upper => Some(args.to_uppercase()),
                BuiltinOp::Lower => Some(args.to_lowercase()),
                BuiltinOp::Reverse => Some(args.chars().rev().collect()),
                BuiltinOp::Spaced => {
                    let out: Vec<String> = args.chars().map(|c| c.to_string()).collect();
                    Some(out.join(" "))
                }
                BuiltinOp::Code => Some(format!("`{args}`")),
                BuiltinOp::Quote => Some(format!("> {args}")),
                BuiltinOp::Boxed => Some(format!("[ {args} ]")),
            }
        }
        PluginRule::Replace { from, to } => {
            if !from.is_empty() && text.contains(from.as_str()) {
                Some(text.replace(from.as_str(), to.as_str()))
            } else {
                None
            }
        }
        PluginRule::StripZeroWidth => {
            let cleaned = strip_zero_width(text);
            (cleaned != text).then_some(cleaned)
        }
    }
}

/// Разбирает файл `.plugin` (поддерживает заголовки `__id__ = "..."` из exteraGram и правила `cmd:` / `replace:`).
pub fn parse_plugin(src: &str) -> Result<Plugin, String> {
    let mut id = String::new();
    let mut name = String::new();
    let mut version = "1.0.0".to_owned();
    let mut author = "User".to_owned();
    let mut description = String::new();
    let mut rules = Vec::new();

    for raw_line in src.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with("//") {
            continue;
        }
        let clean = line.trim_start_matches('#').trim();
        if let Some((k, v)) = clean.split_once('=') {
            let key = k.trim().trim_matches('_').to_ascii_lowercase();
            let val = unquote(v.trim());
            match key.as_str() {
                "id" => id = val,
                "name" | "title" => name = val,
                "version" => version = val,
                "author" => author = val,
                "description" | "desc" => description = val,
                _ => {}
            }
        }
        if let Some(rest) = clean
            .strip_prefix("cmd:")
            .or_else(|| clean.strip_prefix("command "))
            && let Some((trig, tmpl)) = rest.split_once("=>")
        {
            let t = trig.trim();
            if t.starts_with('/') {
                rules.push(PluginRule::Command {
                    trigger: t.to_owned(),
                    template: unquote(tmpl.trim()),
                });
            }
        } else if let Some(rest) = clean
            .strip_prefix("replace:")
            .or_else(|| clean.strip_prefix("replace "))
            && let Some((from, to)) = rest.split_once("=>")
        {
            let f = unquote(from.trim());
            if !f.is_empty() {
                rules.push(PluginRule::Replace {
                    from: f,
                    to: unquote(to.trim()),
                });
            }
        } else if clean == "rule:strip_zwsp" {
            rules.push(PluginRule::StripZeroWidth);
        }
    }

    if id.trim().is_empty() {
        return Err("В плагине не указан __id__".to_owned());
    }
    if name.trim().is_empty() {
        name.clone_from(&id);
    }
    Ok(Plugin {
        id,
        name,
        version,
        author,
        description,
        enabled: true,
        builtin: false,
        rules,
    })
}

fn unquote(s: &str) -> String {
    let s = s.trim();
    if ((s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\''))) && s.len() >= 2 {
        s[1..s.len() - 1].to_owned()
    } else {
        s.to_owned()
    }
}

/// Безопасный рекурсивный парсер арифметических выражений (`+`, `-`, `*`, `/`, `%`, скобки, унарный минус).
pub fn eval_expr(expr: &str) -> Result<f64, String> {
    let tokens = tokenize_math(expr)?;
    if tokens.is_empty() {
        return Err("Пустое выражение".to_owned());
    }
    let mut pos = 0usize;
    let val = parse_add_sub(&tokens, &mut pos)?;
    if pos < tokens.len() {
        return Err("Лишний токен в выражении".to_owned());
    }
    if !val.is_finite() {
        return Err("Деление на ноль или переполнение".to_owned());
    }
    Ok(val)
}

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Num(f64),
    Op(char),
    LParen,
    RParen,
}

fn tokenize_math(s: &str) -> Result<Vec<Tok>, String> {
    let mut out = Vec::new();
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0usize;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
        } else if c == '(' {
            out.push(Tok::LParen);
            i += 1;
        } else if c == ')' {
            out.push(Tok::RParen);
            i += 1;
        } else if matches!(c, '+' | '-' | '*' | '/' | '%') {
            out.push(Tok::Op(c));
            i += 1;
        } else if c.is_ascii_digit() || c == '.' {
            let start = i;
            let mut dots = 0usize;
            while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                if chars[i] == '.' {
                    dots += 1;
                }
                i += 1;
            }
            if dots > 1 {
                return Err("Некорректное число".to_owned());
            }
            let num_str: String = chars[start..i].iter().collect();
            let v = num_str.parse::<f64>().map_err(|_| "Некорректное число".to_owned())?;
            out.push(Tok::Num(v));
        } else {
            return Err(format!("Неизвестный символ: {c}"));
        }
    }
    Ok(out)
}

fn parse_add_sub(tokens: &[Tok], pos: &mut usize) -> Result<f64, String> {
    let mut left = parse_mul_div(tokens, pos)?;
    while let Some(Tok::Op(op @ ('+' | '-'))) = tokens.get(*pos) {
        *pos += 1;
        let right = parse_mul_div(tokens, pos)?;
        if *op == '+' {
            left += right;
        } else {
            left -= right;
        }
    }
    Ok(left)
}

fn parse_mul_div(tokens: &[Tok], pos: &mut usize) -> Result<f64, String> {
    let mut left = parse_unary(tokens, pos)?;
    while let Some(Tok::Op(op @ ('*' | '/' | '%'))) = tokens.get(*pos) {
        *pos += 1;
        let right = parse_unary(tokens, pos)?;
        match *op {
            '*' => left *= right,
            '/' => {
                if right.abs() < 1e-15 {
                    return Err("Деление на ноль".to_owned());
                }
                left /= right;
            }
            '%' => {
                if right.abs() < 1e-15 {
                    return Err("Деление на ноль".to_owned());
                }
                left %= right;
            }
            _ => {}
        }
    }
    Ok(left)
}

fn parse_unary(tokens: &[Tok], pos: &mut usize) -> Result<f64, String> {
    match tokens.get(*pos) {
        Some(Tok::Op('-')) => {
            *pos += 1;
            Ok(-parse_unary(tokens, pos)?)
        }
        Some(Tok::Op('+')) => {
            *pos += 1;
            parse_unary(tokens, pos)
        }
        Some(Tok::LParen) => {
            *pos += 1;
            let v = parse_add_sub(tokens, pos)?;
            if !matches!(tokens.get(*pos), Some(Tok::RParen)) {
                return Err("Незакрытая скобка".to_owned());
            }
            *pos += 1;
            Ok(v)
        }
        Some(Tok::Num(n)) => {
            *pos += 1;
            Ok(*n)
        }
        _ => Err("Ожидалось число или скобка".to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtins_transform_commands_and_macros() {
        let reg = PluginRegistry::with_builtins();
        assert_eq!(
            reg.apply_outgoing("/shrug привет"),
            HookOutcome::Modified("привет ¯\\_(ツ)_/¯".to_owned())
        );
        assert_eq!(
            reg.apply_outgoing("/calc (2 + 3) * 4"),
            HookOutcome::Modified("(2 + 3) * 4 = 20".to_owned())
        );
        assert_eq!(
            reg.apply_outgoing("/upper rustgramm"),
            HookOutcome::Modified("RUSTGRAMM".to_owned())
        );
        assert_eq!(
            reg.apply_outgoing("/reverse abc"),
            HookOutcome::Modified("cba".to_owned())
        );
        assert_eq!(
            reg.apply_outgoing("готово :check: :arrow: :star:"),
            HookOutcome::Modified("готово ✓ → ★".to_owned())
        );
    }

    #[test]
    fn parses_exteragram_plugin_header_and_rules() {
        let src = r#"
__id__ = "custom.hello"
__name__ = "Приветствие"
__version__ = "2.0"
__author__ = "Tester"
__description__ = "Тестовый плагин"
# cmd: /hi => "Привет, {args}!"
# replace: ":fire:" => "🔥"
"#;
        let p = parse_plugin(src).unwrap();
        assert_eq!(p.id, "custom.hello");
        assert_eq!(p.name, "Приветствие");
        assert_eq!(p.version, "2.0");
        assert_eq!(p.rules.len(), 2);
        let mut reg = PluginRegistry::default();
        reg.upsert(p);
        assert_eq!(
            reg.apply_outgoing("/hi мир"),
            HookOutcome::Modified("Привет, мир!".to_owned())
        );
        assert_eq!(
            reg.apply_outgoing("топ :fire:"),
            HookOutcome::Modified("топ 🔥".to_owned())
        );
    }
}
