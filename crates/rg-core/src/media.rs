//! Медиа: описание вложений, безопасные имена файлов и кэш на диске с лимитом размера.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use grammers_client::media::Media;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum MediaKind {
    Photo,
    /// Статичный стикер WebP, который можно показать.
    Sticker,
    /// Анимированный (TGS) или видео-стикер (WebM) с миниатюрой и воспроизведением.
    AnimatedSticker,
    Video,
    /// Короткая зацикленная анимация.
    Gif,
    /// Видеокружок.
    Round,
    Voice,
    Audio,
    File,
    /// Превью веб-ссылки (адрес, сайт, заголовок, описание).
    WebPage {
        url: String,
        site: String,
        title: String,
        desc: String,
    },
    /// Опрос или викторина (вопрос, варианты `(текст, процент, выбран)`, байты вариантов для голосования).
    Poll {
        question: String,
        options: Vec<(String, u8, bool)>,
        option_bytes: Vec<Vec<u8>>,
        total: i32,
        closed: bool,
        quiz: bool,
    },
    Other(String),
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct MediaInfo {
    pub kind: MediaKind,
    pub name: String,
    pub size: u64,
    /// Длительность видео и аудио, секунды.
    pub duration: u32,
    /// Размеры картинки или видео, px.
    pub dims: (i32, i32),
    /// Волна голосового: значения 0..=31.
    pub waveform: Vec<u8>,
    /// Есть ли миниатюра, которую можно скачать для предпросмотра.
    pub has_thumb: bool,
    pub mime: String,
    /// Скрыто под спойлером перед просмотром.
    #[serde(default)]
    pub spoiler: bool,
    /// Таймер самоуничтожения медиа, секунды (`Some(N)` — исчезающее медиа).
    #[serde(default)]
    pub ttl_seconds: Option<i32>,
}

impl MediaInfo {
    pub fn basic(kind: MediaKind, name: impl Into<String>, size: u64) -> Self {
        Self {
            kind,
            name: name.into(),
            size,
            duration: 0,
            dims: (0, 0),
            waveform: Vec::new(),
            has_thumb: false,
            mime: String::new(),
            spoiler: false,
            ttl_seconds: None,
        }
    }

    /// Можно ли получить картинку для предпросмотра.
    pub fn has_preview(&self) -> bool {
        match self.kind {
            MediaKind::Photo | MediaKind::Sticker => true,
            MediaKind::AnimatedSticker | MediaKind::Video | MediaKind::Gif | MediaKind::Round => self.has_thumb,
            _ => false,
        }
    }

    /// Сохраняется ли вложение как файл («Скачать»).
    pub fn is_downloadable(&self) -> bool {
        !matches!(
            self.kind,
            MediaKind::Other(_) | MediaKind::WebPage { .. } | MediaKind::Poll { .. }
        )
    }
}

/// Распаковывает волну голосового сообщения: пятибитные значения, подряд, младшие биты первыми.
pub fn decode_waveform(bytes: &[u8]) -> Vec<u8> {
    let total = bytes.len() * 8 / 5;
    (0..total)
        .map(|n| {
            let bit = n * 5;
            let (byte, off) = (bit / 8, bit % 8);
            let lo = u16::from(bytes[byte]);
            let hi = u16::from(*bytes.get(byte + 1).unwrap_or(&0));
            (((lo | (hi << 8)) >> off) & 0x1f) as u8
        })
        .collect()
}

/// Упаковывает массив пиков `0..=31` в битовый поток волны голосового сообщения Telegram (5 бит на столбик).
pub fn encode_waveform(peaks: &[u8]) -> Vec<u8> {
    let total_bits = peaks.len() * 5;
    let mut out = vec![0u8; total_bits.div_ceil(8)];
    for (i, &val) in peaks.iter().enumerate() {
        let v = u16::from(val.min(31));
        let bit = i * 5;
        let (byte, off) = (bit / 8, bit % 8);
        out[byte] |= ((v << off) & 0xff) as u8;
        if off > 3 && byte + 1 < out.len() {
            out[byte + 1] |= ((v >> (8 - off)) & 0xff) as u8;
        }
    }
    out
}

/// Расширение файла по MIME-типу, когда у документа нет имени (отбрасывает параметры `; codecs=...`).
pub fn ext_for_mime(mime: &str) -> &'static str {
    let base = mime.split(';').next().unwrap_or(mime).trim().to_ascii_lowercase();
    match base.as_str() {
        "audio/ogg" | "audio/opus" => "ogg",
        "audio/mpeg" | "audio/mp3" => "mp3",
        "audio/mp4" | "audio/m4a" | "audio/aac" => "m4a",
        "audio/wav" | "audio/x-wav" | "audio/wave" => "wav",
        "audio/flac" => "flac",
        "video/mp4" => "mp4",
        "video/webm" => "webm",
        "video/quicktime" => "mov",
        "image/gif" => "gif",
        "image/jpeg" => "jpg",
        "image/png" => "png",
        "image/webp" => "webp",
        "application/pdf" => "pdf",
        _ => "bin",
    }
}

pub fn describe(media: &Media) -> MediaInfo {
    use grammers_client::tl;
    match media {
        Media::Photo(p) => {
            let mut info = MediaInfo::basic(MediaKind::Photo, "Фото", p.size().map_or(0, |s| s as u64));
            info.spoiler = p.raw.spoiler;
            info.ttl_seconds = p.raw.ttl_seconds;
            info
        }
        Media::Sticker(s) => {
            let mime = s.document.mime_type().unwrap_or_default().to_owned();
            let emoji = s.raw_attrs.alt.clone();
            let label = if emoji.is_empty() { "Стикер".to_owned() } else { format!("Стикер {emoji}") };
            let mut info = MediaInfo::basic(
                if mime == "image/webp" { MediaKind::Sticker } else { MediaKind::AnimatedSticker },
                label,
                s.document.size().map_or(0, |v| v as u64),
            );
            info.has_thumb = !s.document.thumbs().is_empty();
            info.mime = mime;
            info
        }
        Media::Document(d) => {
            let mime = d.mime_type().unwrap_or_default().to_owned();
            let (mut video, mut round, mut animated, mut audio, mut voice) = (false, false, false, false, false);
            let (mut duration, mut dims, mut waveform) = (0u32, (0, 0), Vec::new());
            if let Some(tl::enums::Document::Document(doc)) = d.raw.document.as_ref() {
                for a in &doc.attributes {
                    match a {
                        tl::enums::DocumentAttribute::Video(v) => {
                            video = true;
                            round = v.round_message;
                            duration = v.duration.max(0.0) as u32;
                            dims = (v.w, v.h);
                        }
                        tl::enums::DocumentAttribute::Audio(au) => {
                            audio = true;
                            voice = au.voice;
                            duration = au.duration.max(0) as u32;
                            if let Some(w) = &au.waveform {
                                waveform = decode_waveform(w);
                            }
                        }
                        tl::enums::DocumentAttribute::Animated => animated = true,
                        _ => {}
                    }
                }
            }
            let kind = if voice {
                MediaKind::Voice
            } else if round {
                MediaKind::Round
            } else if video && animated || mime == "image/gif" {
                MediaKind::Gif
            } else if video || mime.starts_with("video/") {
                MediaKind::Video
            } else if audio || mime.starts_with("audio/") {
                MediaKind::Audio
            } else {
                MediaKind::File
            };
            let name = match &kind {
                MediaKind::Voice => "Голосовое сообщение".to_owned(),
                MediaKind::Round => "Видеосообщение".to_owned(),
                MediaKind::Gif => "GIF".to_owned(),
                _ => d
                    .audio_title()
                    .map(str::to_owned)
                    .or_else(|| d.name().map(safe_file_name))
                    .unwrap_or_else(|| match kind {
                        MediaKind::Video => "Видео".to_owned(),
                        MediaKind::Audio => "Аудио".to_owned(),
                        _ => "Файл".to_owned(),
                    }),
            };
            let mut info = MediaInfo::basic(kind, name, d.size().map_or(0, |v| v as u64));
            info.duration = duration;
            info.dims = dims;
            info.waveform = waveform;
            info.has_thumb = !d.thumbs().is_empty();
            info.mime = mime;
            info.spoiler = d.raw.spoiler;
            info.ttl_seconds = d.raw.ttl_seconds;
            info
        }
        Media::Contact(c) => other(&format!(
            "Контакт: {} {} (+{})",
            c.raw.first_name, c.raw.last_name, c.raw.phone_number
        )),
        Media::Poll(p) => {
            let tl::enums::TextWithEntities::Entities(q) = &p.raw.question;
            let total = p.raw_results.total_voters.unwrap_or(0).max(0);
            let voters = p.raw_results.results.as_deref().unwrap_or(&[]);
            let mut options = Vec::with_capacity(p.raw.answers.len());
            let mut option_bytes = Vec::with_capacity(p.raw.answers.len());
            for ans in &p.raw.answers {
                let tl::enums::PollAnswer::Answer(a) = ans else { continue };
                let tl::enums::TextWithEntities::Entities(t) = &a.text;
                let (cnt, chosen) = voters
                    .iter()
                    .find_map(|v| {
                        let tl::enums::PollAnswerVoters::Voters(v) = v;
                        (v.option == a.option).then_some((v.voters.unwrap_or(0).max(0), v.chosen))
                    })
                    .unwrap_or((0, false));
                let pct = if total > 0 {
                    ((i64::from(cnt) * 100 / i64::from(total)).clamp(0, 100)) as u8
                } else {
                    0
                };
                options.push((t.text.clone(), pct, chosen));
                option_bytes.push(a.option.clone());
            }
            MediaInfo::basic(
                MediaKind::Poll {
                    question: q.text.clone(),
                    options,
                    option_bytes,
                    total,
                    closed: p.raw.closed,
                    quiz: p.raw.quiz,
                },
                "Опрос",
                0,
            )
        }
        Media::Geo(_) | Media::Venue(_) | Media::GeoLive(_) => other("Место"),
        Media::Dice(d) => other(&format!("{} ({})", d.raw.emoticon, d.raw.value)),
        Media::WebPage(w) => match &w.raw.webpage {
            tl::enums::WebPage::Page(wp) => MediaInfo::basic(
                MediaKind::WebPage {
                    url: wp.url.clone(),
                    site: wp.site_name.clone().unwrap_or_else(|| wp.display_url.clone()),
                    title: wp.title.clone().unwrap_or_default(),
                    desc: wp.description.clone().unwrap_or_default(),
                },
                "Ссылка",
                0,
            ),
            _ => other("Ссылка"),
        },
        _ => other("Вложение"),
    }
}

fn other(label: &str) -> MediaInfo {
    MediaInfo::basic(MediaKind::Other(label.to_owned()), label, 0)
}

/// Расширения, которые безопасно открывать программой по умолчанию. Всё остальное
/// (в первую очередь исполняемые файлы) только показывается в папке.
pub fn is_safe_to_open(path: &Path) -> bool {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    matches!(
        ext.as_str(),
        "mp4" | "mkv" | "webm" | "mov" | "avi" | "m4v" | "mp3" | "ogg" | "oga" | "opus" | "m4a" | "wav"
            | "flac" | "jpg" | "jpeg" | "png" | "gif" | "webp" | "bmp" | "pdf" | "txt"
    )
}

/// Открывает скачанный файл: безопасные типы — программой по умолчанию, прочие — показывает
/// в проводнике. Без окон консоли.
pub fn open_downloaded(path: &Path) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let mut cmd = if is_safe_to_open(path) {
            let mut c = std::process::Command::new("rundll32");
            c.arg("url.dll,FileProtocolHandler").arg(path);
            c
        } else {
            let mut c = std::process::Command::new("explorer");
            c.arg(format!("/select,{}", path.display()));
            c
        };
        let _ = cmd.creation_flags(0x0800_0000).spawn();
    }
    #[cfg(all(unix, not(target_os = "android")))]
    {
        if is_safe_to_open(path) {
            let opener = if cfg!(target_os = "macos") { "open" } else { "xdg-open" };
            let _ = std::process::Command::new(opener).arg(path).spawn();
        }
    }
    #[cfg(target_os = "android")]
    let _ = path;
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

    #[test]
    fn waveform_unpacks_five_bit_values() {
        // 0b00001_00010 | ... : два значения 1 и 2 в трёх байтах.
        let bytes = [0b0100_0001, 0b0000_0000, 0];
        let w = decode_waveform(&bytes);
        assert_eq!(w[0], 1);
        assert_eq!(w[1], 2);
        assert_eq!(w.len(), 4);
    }

    #[test]
    fn only_media_types_open_directly() {
        assert!(is_safe_to_open(Path::new("a.MP4")));
        assert!(is_safe_to_open(Path::new("voice.ogg")));
        assert!(!is_safe_to_open(Path::new("setup.exe")));
        assert!(!is_safe_to_open(Path::new("run.bat")));
        assert!(!is_safe_to_open(Path::new("noext")));
    }

    #[test]
    fn mime_gives_extension() {
        assert_eq!(ext_for_mime("audio/ogg"), "ogg");
        assert_eq!(ext_for_mime("audio/ogg; codecs=opus"), "ogg");
        assert_eq!(ext_for_mime("audio/mp4; codecs=mp4a"), "m4a");
        assert_eq!(ext_for_mime("weird/thing"), "bin");
    }

    #[test]
    fn waveform_encode_decode_roundtrip() {
        let peaks: Vec<u8> = (0..63).map(|i| ((i * 7) % 32) as u8).collect();
        let packed = encode_waveform(&peaks);
        let decoded = decode_waveform(&packed);
        assert_eq!(&decoded[..peaks.len()], peaks.as_slice());
    }

    #[test]
    fn animated_sticker_webpage_and_poll_flags() {
        let mut st = MediaInfo::basic(MediaKind::AnimatedSticker, "Стикер 🔥", 12_000);
        assert!(!st.has_preview());
        st.has_thumb = true;
        assert!(st.has_preview());
        assert!(st.is_downloadable());

        let wp = MediaInfo::basic(
            MediaKind::WebPage {
                url: "https://telegram.org".into(),
                site: "Telegram".into(),
                title: "Telegram Messenger".into(),
                desc: "Fast and secure".into(),
            },
            "Ссылка",
            0,
        );
        assert!(!wp.is_downloadable());

        let poll = MediaInfo::basic(
            MediaKind::Poll {
                question: "Rust?".into(),
                options: vec![("Да".into(), 100, true)],
                option_bytes: vec![b"0".to_vec()],
                total: 1,
                closed: false,
                quiz: false,
            },
            "Опрос",
            0,
        );
        assert!(!poll.is_downloadable());
    }
}
