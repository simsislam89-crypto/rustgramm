//! Максимально подробное локальное диагностическое логирование Rustgramm.
//! Пишет в `<RG_HOME>/logs/rustgramm.log` с авторотацией (4 МБ) и держит кольцевой
//! буфер последних записей в памяти для быстрого просмотра и копирования из Настроек.
//! Секреты (пароли, коды входа, api_hash, токены) никогда не записываются.

use std::collections::VecDeque;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

const MAX_LOG_BYTES: u64 = 4 * 1024 * 1024;
const RING_CAP: usize = 200;

struct LoggerState {
    path: PathBuf,
    ring: VecDeque<String>,
    writer: Option<std::io::BufWriter<fs::File>>,
    bytes_written: u64,
    pending_flush: u8,
}

static ENABLED: AtomicBool = AtomicBool::new(true);
static STATE: OnceLock<Mutex<LoggerState>> = OnceLock::new();
static PANIC_HOOK_ONCE: OnceLock<()> = OnceLock::new();

/// Регистрирует единый хук паник для синхронной записи в `rustgramm.log` и `crash.log` до завершения процесса.
pub fn install_panic_hook() {
    PANIC_HOOK_ONCE.get_or_init(|| {
        let prev = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let payload = if let Some(s) = info.payload().downcast_ref::<&str>() {
                (*s).to_owned()
            } else if let Some(s) = info.payload().downcast_ref::<String>() {
                s.clone()
            } else {
                "unknown panic".to_owned()
            };
            let loc = info
                .location()
                .map_or_else(|| "unknown".to_owned(), |l| format!("{}:{}:{}", l.file(), l.line(), l.column()));
            let thread = std::thread::current()
                .name()
                .unwrap_or("unnamed")
                .to_owned();
            let msg = format!("thread='{thread}' at {loc}: {payload}");
            error("PANIC", &msg);
            if let Some(p) = log_path()
                && let Some(dir) = p.parent()
            {
                let crash_path = dir.join("crash.log");
                if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(crash_path) {
                    let clean = sanitize_line(&msg);
                    let _ = writeln!(f, "[{}] [PANIC] {}", timestamp_now(), clean);
                    let _ = f.flush();
                }
            }
            prev(info);
        }));
    });
}

/// Включить или выключить запись диагностического лога.
pub fn set_enabled(on: bool) {
    ENABLED.store(on, Ordering::Relaxed);
}

pub fn is_enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

/// Инициализирует путь лога `<dir>/logs/rustgramm.log` и выполняет ротацию при превышении лимита.
pub fn init(home_dir: &Path) {
    let logs_dir = home_dir.join("logs");
    let _ = fs::create_dir_all(&logs_dir);
    let path = logs_dir.join("rustgramm.log");
    if let Ok(meta) = fs::metadata(&path)
        && meta.len() > MAX_LOG_BYTES
    {
        let backup = logs_dir.join("rustgramm.log.1");
        let _ = fs::remove_file(&backup);
        let _ = fs::rename(&path, &backup);
    }
    let init_bytes = fs::metadata(&path).map_or(0, |m| m.len());
    let writer = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .ok()
        .map(std::io::BufWriter::new);
    let state = STATE.get_or_init(|| {
        Mutex::new(LoggerState {
            path: path.clone(),
            ring: VecDeque::with_capacity(RING_CAP),
            writer: None,
            bytes_written: 0,
            pending_flush: 0,
        })
    });
    if let Ok(mut g) = state.lock() {
        g.path = path;
        g.writer = writer;
        g.bytes_written = init_bytes;
        g.pending_flush = 0;
    }
    info(
        "BOOT",
        &format!(
            "=== Rustgramm v{} ({}/{}) старт диагностики ===",
            env!("CARGO_PKG_VERSION"),
            std::env::consts::OS,
            std::env::consts::ARCH,
        ),
    );
}

/// Возвращает путь к текущему файлу лога, если инициализирован.
pub fn log_path() -> Option<PathBuf> {
    STATE.get()?.lock().ok().map(|g| g.path.clone())
}

/// Возвращает последние строки диагностического лога из памяти (для UI и копирования).
pub fn recent_lines() -> Vec<String> {
    STATE
        .get()
        .and_then(|s| s.lock().ok())
        .map(|g| g.ring.iter().cloned().collect())
        .unwrap_or_default()
}

/// Очищает файл лога и буфер в памяти.
pub fn clear() {
    if let Some(st) = STATE.get()
        && let Ok(mut g) = st.lock()
    {
        g.ring.clear();
        g.writer = None;
        let _ = fs::write(&g.path, b"");
        g.bytes_written = 0;
        g.pending_flush = 0;
        g.writer = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&g.path)
            .ok()
            .map(std::io::BufWriter::new);
    }
}

fn timestamp_now() -> String {
    let dur = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let total_secs = dur.as_secs();
    let ms = dur.subsec_millis();
    let days = (total_secs / 86_400) as i64;
    let sod = total_secs % 86_400;
    let h = sod / 3600;
    let m = (sod % 3600) / 60;
    let s = sod % 60;
    // Алгоритм Hinnant (civil_from_days) без внешних зависимостей.
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mo = if mp < 10 { mp + 3 } else { mp - 9 };
    let yr = y + i64::from(mo <= 2);
    format!("{yr:04}-{mo:02}-{d:02} {h:02}:{m:02}:{s:02}.{ms:03}")
}

/// Простая защита от случайной утечки токенов/ключей в строку лога.
pub fn sanitize_line(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for word in raw.split_inclusive(char::is_whitespace) {
        let trimmed = word.trim();
        let is_hex32 = trimmed.len() == 32 && trimmed.bytes().all(|b| b.is_ascii_hexdigit());
        if is_hex32 || trimmed.to_ascii_lowercase().starts_with("password=") {
            out.push_str("[REDACTED]");
            if let Some(ws) = word.chars().last().filter(|c| c.is_whitespace()) {
                out.push(ws);
            }
        } else {
            out.push_str(word);
        }
    }
    out.replace(['\r', '\n'], " ")
}

fn write_entry(level: &str, scope: &str, msg: &str) {
    if !ENABLED.load(Ordering::Relaxed) {
        return;
    }
    let clean = sanitize_line(msg);
    let line = format!("[{}] [{}] [{}] {}", timestamp_now(), level, scope, clean);
    let Some(st) = STATE.get() else {
        return;
    };
    let Ok(mut g) = st.lock() else {
        return;
    };
    if g.ring.len() >= RING_CAP {
        g.ring.pop_front();
    }
    g.ring.push_back(line.clone());
    if g.bytes_written > MAX_LOG_BYTES {
        g.writer = None;
        let backup = g.path.with_extension("log.1");
        let _ = fs::remove_file(&backup);
        let _ = fs::rename(&g.path, &backup);
        g.bytes_written = 0;
    }
    if g.writer.is_none() {
        g.writer = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&g.path)
            .ok()
            .map(std::io::BufWriter::new);
    }
    let urgent = level != "INFO";
    let should_flush = if let Some(w) = g.writer.as_mut() {
        let _ = writeln!(w, "{line}");
        true
    } else {
        false
    };
    if should_flush {
        g.bytes_written += line.len() as u64 + 1;
        g.pending_flush = g.pending_flush.saturating_add(1);
        if (urgent || g.pending_flush >= 8)
            && let Some(w) = g.writer.as_mut()
        {
            let _ = w.flush();
            g.pending_flush = 0;
        }
    }
}

pub fn info(scope: &str, msg: &str) {
    write_entry("INFO", scope, msg);
}

pub fn warn(scope: &str, msg: &str) {
    write_entry("WARN", scope, msg);
}

pub fn error(scope: &str, msg: &str) {
    write_entry("ERR ", scope, msg);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_redacts_hex32_and_newlines() {
        let s = sanitize_line("hash 0123456789abcdef0123456789abcdef\nnext");
        assert!(s.contains("[REDACTED]"));
        assert!(!s.contains('\n'));
    }

    #[test]
    fn logger_writes_and_reads_ring() {
        let dir = std::env::temp_dir().join("rg-diag-test");
        let _ = fs::remove_dir_all(&dir);
        init(&dir);
        info("TEST", "diag hello");
        let lines = recent_lines();
        assert!(lines.iter().any(|l| l.contains("diag hello")));
        assert!(log_path().is_some_and(|p| p.exists()));
        let _ = fs::remove_dir_all(&dir);
    }
}
