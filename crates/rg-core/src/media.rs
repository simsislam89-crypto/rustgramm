//! Медиа: описание вложений, безопасные имена файлов и кэш на диске с лимитом размера.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use grammers_client::media::Media;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MediaKind {
    Photo,
    /// Статичный стикер WebP, который можно показать.
    Sticker,
    /// Анимированный или видео-стикер: показываем подпись.
    AnimatedSticker,
    File,
    Other(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaInfo {
    pub kind: MediaKind,
    pub name: String,
    pub size: u64,
}

impl MediaInfo {
    /// Можно ли получить картинку для предпросмотра.
    pub fn has_preview(&self) -> bool {
        matches!(self.kind, MediaKind::Photo | MediaKind::Sticker)
    }
}

pub fn describe(media: &Media) -> MediaInfo {
    match media {
        Media::Photo(p) => MediaInfo {
            kind: MediaKind::Photo,
            name: "Фото".to_owned(),
            size: p.size().map_or(0, |s| s as u64),
        },
        Media::Sticker(s) => {
            let mime = s.document.mime_type().unwrap_or_default();
            MediaInfo {
                kind: if mime == "image/webp" {
                    MediaKind::Sticker
                } else {
                    MediaKind::AnimatedSticker
                },
                name: "Стикер".to_owned(),
                size: s.document.size().map_or(0, |v| v as u64),
            }
        }
        Media::Document(d) => MediaInfo {
            kind: MediaKind::File,
            name: d.name().map_or_else(|| "Файл".to_owned(), safe_file_name),
            size: d.size().map_or(0, |v| v as u64),
        },
        Media::Contact(_) => other("Контакт"),
        Media::Poll(_) => other("Опрос"),
        Media::Geo(_) | Media::Venue(_) | Media::GeoLive(_) => other("Место"),
        Media::Dice(_) => other("Кубик"),
        Media::WebPage(_) => other("Ссылка"),
        _ => other("Вложение"),
    }
}

fn other(label: &str) -> MediaInfo {
    MediaInfo {
        kind: MediaKind::Other(label.to_owned()),
        name: label.to_owned(),
        size: 0,
    }
}

/// Имя файла без пути и опасных символов (защита от `..\..\x`, зарезервированных имён Windows).
pub fn safe_file_name(name: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or("");
    let mut out: String = base
        .chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '|' | '?' | '*' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    out = out.trim_matches(|c: char| c == '.' || c == ' ').to_owned();
    let stem = out.split('.').next().unwrap_or("").to_ascii_uppercase();
    let reserved = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (stem.len() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.ends_with(|c: char| c.is_ascii_digit()));
    if out.is_empty() || reserved {
        out = format!("file_{out}");
    }
    out.chars().take(120).collect()
}

/// Читаемый размер: «1,5 МБ».
pub fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["Б", "КБ", "МБ", "ГБ"];
    let mut v = bytes as f64;
    let mut i = 0;
    while v >= 1024.0 && i < UNITS.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{bytes} Б")
    } else {
        format!("{v:.1} {}", UNITS[i]).replace('.', ",")
    }
}

/// Удаляет самые старые файлы кэша, пока общий размер больше `max_bytes`.
/// Возвращает, сколько файлов удалено.
pub fn prune_cache(dir: &Path, max_bytes: u64) -> usize {
    let Ok(rd) = fs::read_dir(dir) else { return 0 };
    let mut files: Vec<(SystemTime, u64, PathBuf)> = rd
        .flatten()
        .filter_map(|e| {
            let md = e.metadata().ok()?;
            md.is_file()
                .then(|| (md.modified().unwrap_or(SystemTime::UNIX_EPOCH), md.len(), e.path()))
        })
        .collect();
    let mut total: u64 = files.iter().map(|f| f.1).sum();
    files.sort_by_key(|f| f.0);
    let mut removed = 0;
    for (_, len, path) in files {
        if total <= max_bytes {
            break;
        }
        if fs::remove_file(&path).is_ok() {
            total = total.saturating_sub(len);
            removed += 1;
        }
    }
    removed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_names_are_safe() {
        assert_eq!(safe_file_name("..\\..\\Windows\\evil.exe"), "evil.exe");
        assert_eq!(safe_file_name("../../etc/passwd"), "passwd");
        assert_eq!(safe_file_name("a<b>:c?.txt"), "a_b__c_.txt");
        assert_eq!(safe_file_name("CON"), "file_CON");
        assert_eq!(safe_file_name("nul.txt"), "file_nul.txt");
        assert_eq!(safe_file_name("COM1"), "file_COM1");
        assert_eq!(safe_file_name(""), "file_");
        assert_eq!(safe_file_name("..."), "file_");
        assert_eq!(safe_file_name(&"я".repeat(500)).chars().count(), 120);
    }

    #[test]
    fn sizes_are_human() {
        assert_eq!(human_size(10), "10 Б");
        assert_eq!(human_size(1536), "1,5 КБ");
        assert_eq!(human_size(5 * 1024 * 1024), "5,0 МБ");
    }

    #[test]
    fn cache_prunes_oldest_first() {
        let dir = std::env::temp_dir().join(format!("rg-cache-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        for (i, name) in ["a", "b", "c"].iter().enumerate() {
            let p = dir.join(name);
            fs::write(&p, vec![0u8; 100]).unwrap();
            let t = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1000 + i as u64);
            fs::File::options().write(true).open(&p).unwrap().set_modified(t).unwrap();
        }
        assert_eq!(prune_cache(&dir, 150), 2);
        assert!(!dir.join("a").exists() && !dir.join("b").exists());
        assert!(dir.join("c").exists());
        assert_eq!(prune_cache(&dir, 1000), 0);
        let _ = fs::remove_dir_all(&dir);
    }
}
