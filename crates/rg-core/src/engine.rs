//! Движок: высокоуровневые операции клиента с режимом призрака и локальным хранилищем.
//! Любая служебная отправка («прочитано») проходит через политику `rg-ayu::ghost`.

use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use grammers_client::Client;
use grammers_client::update::Update;
use grammers_mtsender::UpdatesConfiguration;
use grammers_session::types::PeerRef;
use grammers_session::updates::UpdatesLike;
use rg_ayu::filters::FilterSet;
use rg_ayu::ghost::{self, Verdict};
use rg_ayu::store::MessageStore;
use rg_settings::Settings;

use crate::media::{self, MediaInfo, MediaKind};
use tokio::sync::mpsc;

type BoxErr = Box<dyn std::error::Error + Send + Sync>;

/// Строка списка чатов для интерфейса.
#[derive(Debug, Clone)]
pub struct DialogRow {
    pub peer: PeerRef,
    pub id: i64,
    pub name: String,
    pub last: String,
    /// Время последнего сообщения (unix), 0 если нет.
    pub date: i64,
    pub unread: i32,
    pub pinned: bool,
    pub muted: bool,
    /// Чат с самим собой (Избранное).
    pub saved: bool,
    /// Лежит в архиве.
    pub archived: bool,
    pub kind: DialogKind,
    /// Собеседник есть в контактах.
    pub contact: bool,
    /// Наибольший идентификатор своего сообщения, прочитанного собеседником (две галочки).
    pub read_out: i32,
    /// Пользовательский Premium-эмодзи статуса (`document_id`), если установлен.
    pub emoji_status: Option<i64>,
    /// Официальный подтверждённый аккаунт/канал.
    pub verified: bool,
    /// Наличие Telegram Premium.
    pub premium: bool,
}

/// Музыкальный трек (из профиля `saved_music` или аудиосообщения).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AudioTrack {
    pub id: i64,
    pub access_hash: i64,
    pub file_ref: Vec<u8>,
    pub title: String,
    pub performer: String,
    pub file_name: String,
    pub mime: String,
    pub duration: u32,
    pub size: u64,
}

impl AudioTrack {
    /// Человекочитаемое название трека для карточки профиля и мини-плеера.
    pub fn display_title(&self) -> String {
        match (self.performer.trim(), self.title.trim()) {
            ("", "") => {
                if !self.file_name.trim().is_empty() {
                    self.file_name.trim().to_owned()
                } else {
                    "Аудиозапись".to_owned()
                }
            }
            ("", t) => t.to_owned(),
            (p, "") => p.to_owned(),
            (p, t) => format!("{p} — {t}"),
        }
    }

    pub fn ext(&self) -> &str {
        if let Some((_, e)) = self.file_name.rsplit_once('.')
            && !e.is_empty()
            && e.len() <= 5
            && e.chars().all(|c| c.is_ascii_alphanumeric())
        {
            return e;
        }
        match self.mime.as_str() {
            "audio/mp4" | "audio/x-m4a" | "audio/m4a" => "m4a",
            "audio/ogg" | "audio/opus" => "ogg",
            "audio/flac" => "flac",
            "audio/wav" | "audio/x-wav" => "wav",
            _ => "mp3",
        }
    }
}

/// Сведения о собеседнике, группе или канале для правой панели.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PeerInfo {
    pub id: i64,
    pub name: String,
    pub username: String,
    pub about: String,
    pub phone: String,
    pub status: String,
    pub members: i32,
    pub blocked: bool,
    pub broadcast: bool,
    pub invite_link: String,
    pub emoji_status: Option<i64>,
    pub verified: bool,
    pub premium: bool,
    pub birthday: String,
    pub business_hours: String,
    /// Сохранённая музыка в профиле (`UserFull.saved_music` + `users.getSavedMusic`).
    pub saved_music: Vec<AudioTrack>,
}

/// Итог `Engine::fetch_media`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fetched {
    Ready(PathBuf),
    Cancelled,
    Missing,
}

/// Событие потока обновлений для открытого чата и списка.
#[derive(Debug, Clone)]
pub enum Live {
    /// Новое или изменённое сообщение чата (ключ в формате Bot API).
    Message(i64, Box<MessageRow>),
    /// Удалены сообщения (для каналов — ключ канала, иначе `None`).
    Deleted(Option<i64>, Vec<i32>),
    /// Собеседник прочитал свои сообщения до `max_id` включительно.
    ReadOutbox(i64, i32),
}

/// Тип чата для фильтров папок.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialogKind {
    User,
    Bot,
    Group,
    Channel,
}

/// Вид форматирования фрагмента текста.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum EntityKind {
    Bold,
    Italic,
    Underline,
    Strike,
    Code,
    Pre,
    Spoiler,
    Quote,
    /// Ссылка (в тексте или скрытая за словом).
    Link,
    Mention,
    Hashtag,
    Command,
    /// Пользовательский (Premium) эмодзи по идентификатору документа Telegram.
    CustomEmoji(i64),
}

/// Фрагмент форматирования в символах (не в UTF-16, как у Telegram).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Entity {
    pub kind: EntityKind,
    pub start: usize,
    pub end: usize,
    /// Адрес для `Link`, имя для `Mention`.
    pub url: Option<String>,
}

/// Переводит сущности Telegram (смещения в UTF-16) в символьные индексы.
pub fn convert_entities(text: &str, raw: &[grammers_client::tl::enums::MessageEntity]) -> Vec<Entity> {
    use grammers_client::tl::enums::MessageEntity as E;
    // Накопленное число UTF-16 единиц перед каждым символом.
    let mut at16 = Vec::with_capacity(text.chars().count() + 1);
    let mut n = 0usize;
    for c in text.chars() {
        at16.push(n);
        n += c.len_utf16();
    }
    at16.push(n);
    let to_char = |off: i32| at16.partition_point(|&x| x < off.max(0) as usize);
    let mut out = Vec::new();
    for e in raw {
        let (kind, off, len, url) = match e {
            E::Bold(x) => (EntityKind::Bold, x.offset, x.length, None),
            E::Italic(x) => (EntityKind::Italic, x.offset, x.length, None),
            E::Underline(x) => (EntityKind::Underline, x.offset, x.length, None),
            E::Strike(x) => (EntityKind::Strike, x.offset, x.length, None),
            E::Code(x) => (EntityKind::Code, x.offset, x.length, None),
            E::Pre(x) => (EntityKind::Pre, x.offset, x.length, None),
            E::Spoiler(x) => (EntityKind::Spoiler, x.offset, x.length, None),
            E::Blockquote(x) => (EntityKind::Quote, x.offset, x.length, None),
            E::Url(x) => (EntityKind::Link, x.offset, x.length, None),
            E::TextUrl(x) => (EntityKind::Link, x.offset, x.length, Some(x.url.clone())),
            E::Email(x) => (EntityKind::Link, x.offset, x.length, None),
            E::Mention(x) => (EntityKind::Mention, x.offset, x.length, None),
            E::Hashtag(x) => (EntityKind::Hashtag, x.offset, x.length, None),
            E::BotCommand(x) => (EntityKind::Command, x.offset, x.length, None),
            E::CustomEmoji(x) => (EntityKind::CustomEmoji(x.document_id), x.offset, x.length, None),
            _ => continue,
        };
        let (start, end) = (to_char(off), to_char(off + len));
        if end > start {
            out.push(Entity { kind, start, end, url });
        }
    }
    out
}

/// Переводит символьный диапазон `(char_start, char_len)` в UTF-16 `(offset, length)` для Telegram TL.
pub fn utf16_range(text: &str, char_start: usize, char_len: usize) -> (i32, i32) {
    let mut off = 0usize;
    let mut len = 0usize;
    for (idx, ch) in text.chars().enumerate() {
        let u = ch.len_utf16();
        if idx < char_start {
            off += u;
        } else if idx < char_start + char_len {
            len += u;
        } else {
            break;
        }
    }
    (off as i32, len as i32)
}

/// Сообщение для интерфейса.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct MessageRow {
    pub id: i32,
    pub text: String,
    pub outgoing: bool,
    pub date: i64,
    pub deleted: bool,
    pub edits: usize,
    pub media: Option<MediaInfo>,
    /// Имя отправителя (для групп и каналов).
    pub sender: String,
    /// Ответ на сообщение: идентификатор и начало его текста, если оно есть в выборке.
    pub reply_to: Option<(i32, String)>,
    /// Источник пересылки.
    pub forwarded_from: Option<String>,
    pub pinned: bool,
    /// Время последней правки (unix), если сообщение менялось.
    pub edit_date: Option<i64>,
    pub views: Option<i32>,
    /// Число реакций.
    pub reactions: i32,
    /// Отправитель (формат Bot API), 0 если неизвестен.
    pub sender_id: i64,
    /// Форматирование текста.
    pub entities: Vec<Entity>,
    /// Номер альбома: сообщения с одним номером показываются одним пузырём.
    pub grouped: Option<i64>,
}

impl MessageRow {
    /// Строка без служебных полей (для сохранённых удалённых сообщений и тестов).
    pub fn plain(id: i32, text: String, outgoing: bool, date: i64) -> Self {
        Self {
            id,
            text,
            outgoing,
            date,
            deleted: false,
            edits: 0,
            media: None,
            sender: String::new(),
            reply_to: None,
            forwarded_from: None,
            pinned: false,
            edit_date: None,
            views: None,
            reactions: 0,
            sender_id: 0,
            entities: Vec::new(),
            grouped: None,
        }
    }
}

#[derive(Clone)]
pub struct Engine {
    pub client: Client,
    pub settings: Arc<RwLock<Settings>>,
    store: Arc<MessageStore>,
    dirs: Arc<Dirs>,
}

/// Каталоги кэша предпросмотров и загрузок.
#[derive(Debug, Default)]
struct Dirs {
    cache: PathBuf,
    downloads: PathBuf,
}

/// Предел кэша предпросмотров.
const CACHE_LIMIT: u64 = 200 * 1024 * 1024;
/// Крупнее этого предпросмотр не качаем.
const PREVIEW_LIMIT: u64 = 20 * 1024 * 1024;

/// Идентификатор чата в виде, пригодном для хранения (формат Bot API).
fn peer_key(peer: &PeerRef) -> i64 {
    peer.id.bot_api_dialog_id_unchecked()
}

/// Извлекает `document_id` пользовательского Premium-эмодзи статуса.
fn emoji_status_id(s: Option<&grammers_client::tl::enums::EmojiStatus>) -> Option<i64> {
    use grammers_client::tl::enums::EmojiStatus as E;
    match s? {
        E::Status(x) => Some(x.document_id),
        E::Collectible(x) => Some(x.document_id),
        _ => None,
    }
}

/// Превращает документ Telegram в музыкальный трек (`saved_music` или аудио).
fn audio_track_of(doc: &grammers_client::tl::enums::Document) -> Option<AudioTrack> {
    use grammers_client::tl;
    let tl::enums::Document::Document(d) = doc else {
        return None;
    };
    let mut title = String::new();
    let mut performer = String::new();
    let mut file_name = String::new();
    let mut duration = 0u32;
    for a in &d.attributes {
        match a {
            tl::enums::DocumentAttribute::Audio(x) => {
                title = x.title.clone().unwrap_or_default();
                performer = x.performer.clone().unwrap_or_default();
                duration = x.duration.max(0) as u32;
            }
            tl::enums::DocumentAttribute::Filename(x) => {
                file_name = x.file_name.clone();
            }
            _ => {}
        }
    }
    Some(AudioTrack {
        id: d.id,
        access_hash: d.access_hash,
        file_ref: d.file_reference.clone(),
        title,
        performer,
        file_name,
        mime: d.mime_type.clone(),
        duration,
        size: d.size.max(0) as u64,
    })
}

fn fmt_birthday(b: &grammers_client::tl::enums::Birthday) -> String {
    const MONTHS: [&str; 12] = [
        "января", "февраля", "марта", "апреля", "мая", "июня",
        "июля", "августа", "сентября", "октября", "ноября", "декабря",
    ];
    let grammers_client::tl::enums::Birthday::Birthday(b) = b;
    let m = MONTHS.get(b.month.saturating_sub(1) as usize).copied().unwrap_or("");
    match b.year {
        Some(y) if y > 0 => format!("{} {m} {y}", b.day),
        _ => format!("{} {m}", b.day),
    }
}

/// Подпись последнего сообщения в списке чатов (если текст пуст — показываем тип вложения, как в AyuGram).
fn dialog_last_preview(m: &grammers_client::message::Message) -> String {
    let text = m.text().trim();
    if !text.is_empty() {
        return text.chars().take(80).collect();
    }
    match m.media().as_ref().map(media::describe) {
        Some(info) => match info.kind {
            MediaKind::Photo => "🖼 Фото".to_owned(),
            MediaKind::Video => "📹 Видео".to_owned(),
            MediaKind::Gif => "🎬 GIF".to_owned(),
            MediaKind::Round => "⭕ Видеосообщение".to_owned(),
            MediaKind::Voice => "🎤 Голосовое сообщение".to_owned(),
            MediaKind::Audio => format!("🎵 {}", info.name),
            MediaKind::Sticker | MediaKind::AnimatedSticker => {
                if info.name.is_empty() { "Стикер".to_owned() } else { info.name }
            }
            MediaKind::Poll { .. } => format!("📊 {}", info.name),
            MediaKind::WebPage { .. } => format!("🔗 {}", info.name),
            MediaKind::File => format!("📎 {}", info.name),
            MediaKind::Other(label) => label,
        },
        None => String::new(),
    }
}

fn raw_media_label(media: Option<&grammers_client::tl::enums::MessageMedia>) -> String {
    use grammers_client::tl::enums::MessageMedia as M;
    match media {
        Some(M::Photo(_)) => "🖼 Фото".to_owned(),
        Some(M::Document(_)) => "📎 Вложение".to_owned(),
        Some(M::Poll(_)) => "📊 Опрос".to_owned(),
        Some(M::Geo(_) | M::GeoLive(_) | M::Venue(_)) => "📍 Геопозиция".to_owned(),
        Some(M::Contact(_)) => "👤 Контакт".to_owned(),
        _ => String::new(),
    }
}

impl Engine {
    pub fn new(client: Client, settings: Settings, store: MessageStore) -> Self {
        Self {
            client,
            settings: Arc::new(RwLock::new(settings)),
            store: Arc::new(store),
            dirs: Arc::new(Dirs::default()),
        }
    }

    /// Задать каталоги кэша и загрузок (создаются при необходимости).
    pub fn with_dirs(mut self, cache: &Path, downloads: &Path) -> Self {
        let _ = std::fs::create_dir_all(cache);
        let _ = std::fs::create_dir_all(downloads);
        self.dirs = Arc::new(Dirs {
            cache: cache.to_owned(),
            downloads: downloads.to_owned(),
        });
        self
    }

    pub fn settings(&self) -> Settings {
        self.settings
            .read()
            .map(|s| s.clone())
            .unwrap_or_default()
    }

    pub fn update_settings(&self, f: impl FnOnce(&mut Settings)) {
        if let Ok(mut s) = self.settings.write() {
            f(&mut s);
        }
    }

    /// Отметить чат прочитанным, если режим призрака это позволяет. `Ok(false)` — запрос погашен.
    pub async fn mark_read(&self, peer: PeerRef) -> Result<bool, grammers_client::InvocationError> {
        let ghost_mode = self.settings().ghost;
        let id = grammers_tl_types_id::READ_HISTORY;
        if ghost::verdict(&ghost_mode, id) == Verdict::Drop {
            return Ok(false);
        }
        self.client.mark_as_read(peer).await?;
        Ok(true)
    }

    /// «Прочитать на сервере»: отметка о прочтении отправляется всегда, даже в режиме призрака
    /// (сбрасывает непрочитанные сообщения, упоминания `@`, реакции и метку «непрочитано»).
    pub async fn force_read(&self, peer: PeerRef) -> Result<(), grammers_client::InvocationError> {
        use grammers_client::tl;
        let in_peer = tl::enums::InputPeer::from(peer);
        let read_res = match &in_peer {
            tl::enums::InputPeer::Channel(c) => {
                self.client
                    .invoke(&tl::functions::channels::ReadHistory {
                        channel: tl::enums::InputChannel::Channel(tl::types::InputChannel {
                            channel_id: c.channel_id,
                            access_hash: c.access_hash,
                        }),
                        max_id: i32::MAX,
                    })
                    .await
                    .map(|_| ())
            }
            _ => {
                self.client
                    .invoke(&tl::functions::messages::ReadHistory {
                        peer: in_peer.clone(),
                        max_id: i32::MAX,
                    })
                    .await
                    .map(|_| ())
            }
        };
        let req_mentions = tl::functions::messages::ReadMentions {
            peer: in_peer.clone(),
            top_msg_id: None,
        };
        let req_reactions = tl::functions::messages::ReadReactions {
            peer: in_peer.clone(),
            top_msg_id: None,
            saved_peer_id: None,
        };
        let req_unmark = tl::functions::messages::MarkDialogUnread {
            unread: false,
            parent_peer: None,
            peer: tl::enums::InputDialogPeer::Peer(tl::types::InputDialogPeer { peer: in_peer }),
        };
        let _ = tokio::join!(
            self.client.invoke(&req_mentions),
            self.client.invoke(&req_reactions),
            self.client.invoke(&req_unmark),
        );
        read_res
    }

    /// Пакетно отметить прочитанными на сервере сразу несколько чатов (кнопка «Прочитать на сервере» в боковом меню).
    pub async fn force_read_many(&self, peers: &[PeerRef]) -> usize {
        let mut handles = Vec::with_capacity(peers.len());
        for &p in peers.iter().take(40) {
            let eng = self.clone();
            handles.push(tokio::spawn(async move { eng.force_read(p).await.is_ok() }));
        }
        let mut ok_count = 0usize;
        for h in handles {
            if h.await.unwrap_or(false) {
                ok_count += 1;
            }
        }
        ok_count
    }

    /// Чаты из папки «Архив» (folder_id = 1).
    pub async fn archived(&self, limit: usize) -> Result<Vec<DialogRow>, grammers_client::InvocationError> {
        use grammers_client::tl;
        let res = self
            .client
            .invoke(&tl::functions::messages::GetDialogs {
                exclude_pinned: false,
                folder_id: Some(1),
                offset_date: 0,
                offset_id: 0,
                offset_peer: tl::enums::InputPeer::Empty,
                limit: limit.min(100) as i32,
                hash: 0,
            })
            .await?;
        let (dialogs, messages, chats, users) = match res {
            tl::enums::messages::Dialogs::Dialogs(d) => (d.dialogs, d.messages, d.chats, d.users),
            tl::enums::messages::Dialogs::Slice(d) => (d.dialogs, d.messages, d.chats, d.users),
            tl::enums::messages::Dialogs::NotModified(_) => return Ok(Vec::new()),
        };
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64);
        let mut out = Vec::new();
        for d in dialogs {
            let tl::enums::Dialog::Dialog(d) = d else { continue };
            let pid = grammers_session::types::PeerId::from(&d.peer);
            let mut kind = DialogKind::Group;
            let mut contact = false;
            let mut emoji_status = None;
            let mut verified = false;
            let mut premium = false;
            let (name, peer) = match &d.peer {
                tl::enums::Peer::User(u) => {
                    let Some(user) = users.iter().find(|x| matches!(x, tl::enums::User::User(y) if y.id == u.user_id)) else { continue };
                    let name = match user {
                        tl::enums::User::User(y) => {
                            kind = if y.bot { DialogKind::Bot } else { DialogKind::User };
                            contact = y.contact;
                            emoji_status = emoji_status_id(y.emoji_status.as_ref());
                            verified = y.verified;
                            premium = y.premium;
                            format!("{} {}", y.first_name.clone().unwrap_or_default(), y.last_name.clone().unwrap_or_default())
                                .trim()
                                .to_owned()
                        }
                        tl::enums::User::Empty(_) => String::new(),
                    };
                    (name, grammers_session::types::PeerRef::from(user))
                }
                tl::enums::Peer::Chat(c) => {
                    let Some(chat) = chats.iter().find(|x| matches!(x, tl::enums::Chat::Chat(y) if y.id == c.chat_id)) else { continue };
                    let name = match chat {
                        tl::enums::Chat::Chat(y) => y.title.clone(),
                        _ => String::new(),
                    };
                    (name, grammers_session::types::PeerRef::from(chat))
                }
                tl::enums::Peer::Channel(c) => {
                    let Some(chat) = chats.iter().find(|x| matches!(x, tl::enums::Chat::Channel(y) if y.id == c.channel_id)) else { continue };
                    let name = match chat {
                        tl::enums::Chat::Channel(y) => {
                            if y.broadcast {
                                kind = DialogKind::Channel;
                            }
                            emoji_status = emoji_status_id(y.emoji_status.as_ref());
                            verified = y.verified;
                            y.title.clone()
                        }
                        _ => String::new(),
                    };
                    (name, grammers_session::types::PeerRef::from(chat))
                }
            };
            let (last, date) = messages
                .iter()
                .find_map(|m| match m {
                    tl::enums::Message::Message(m)
                        if m.id == d.top_message && grammers_session::types::PeerId::from(&m.peer_id) == pid =>
                    {
                        let preview = if m.message.trim().is_empty() {
                            raw_media_label(m.media.as_ref())
                        } else {
                            m.message.chars().take(80).collect()
                        };
                        Some((preview, i64::from(m.date)))
                    }
                    _ => None,
                })
                .unwrap_or_default();
            let muted = matches!(&d.notify_settings, tl::enums::PeerNotifySettings::Settings(s) if s.mute_until.is_some_and(|t| i64::from(t) > now));
            out.push(DialogRow {
                peer,
                id: pid.bot_api_dialog_id_unchecked(),
                name: if name.is_empty() { "Без имени".to_owned() } else { name },
                last,
                date,
                unread: d.unread_count.max(i32::from(d.unread_mark)),
                pinned: d.pinned,
                muted,
                saved: false,
                archived: true,
                kind,
                contact,
                read_out: d.read_outbox_max_id,
                emoji_status,
                verified,
                premium,
            });
        }
        Ok(out)
    }

    /// Переместить чат в архив или вернуть из него.
    pub async fn set_archived(&self, peer: PeerRef, archive: bool) -> Result<(), grammers_client::InvocationError> {
        use grammers_client::tl;
        self.client
            .invoke(&tl::functions::folders::EditPeerFolders {
                folder_peers: vec![tl::enums::InputFolderPeer::Peer(tl::types::InputFolderPeer {
                    peer: peer.into(),
                    folder_id: i32::from(archive),
                })],
            })
            .await?;
        Ok(())
    }

    /// Закрепить чат в списке или открепить.
    pub async fn set_pinned_dialog(&self, peer: PeerRef, pinned: bool) -> Result<(), grammers_client::InvocationError> {
        use grammers_client::tl;
        self.client
            .invoke(&tl::functions::messages::ToggleDialogPin {
                pinned,
                peer: tl::enums::InputDialogPeer::Peer(tl::types::InputDialogPeer { peer: peer.into() }),
            })
            .await?;
        Ok(())
    }

    /// Отключить звук чата навсегда или включить.
    pub async fn set_muted(&self, peer: PeerRef, muted: bool) -> Result<(), grammers_client::InvocationError> {
        use grammers_client::tl;
        self.client
            .invoke(&tl::functions::account::UpdateNotifySettings {
                peer: tl::enums::InputNotifyPeer::Peer(tl::types::InputNotifyPeer { peer: peer.into() }),
                settings: tl::enums::InputPeerNotifySettings::Settings(tl::types::InputPeerNotifySettings {
                    show_previews: None,
                    silent: None,
                    mute_until: Some(if muted { i32::MAX } else { 0 }),
                    sound: None,
                    stories_muted: None,
                    stories_hide_sender: None,
                    stories_sound: None,
                }),
            })
            .await?;
        Ok(())
    }

    pub async fn dialogs(&self, limit: usize) -> Result<Vec<DialogRow>, grammers_client::InvocationError> {
        let mut it = self.client.iter_dialogs();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64);
        let mut out = Vec::new();
        while out.len() < limit {
            let Some(d) = it.next().await? else { break };
            let archived = matches!(&d.raw, grammers_client::tl::enums::Dialog::Dialog(x) if x.folder_id == Some(1));
            let (unread, pinned, muted) = match &d.raw {
                grammers_client::tl::enums::Dialog::Dialog(x) => (
                    x.unread_count.max(i32::from(x.unread_mark)),
                    x.pinned,
                    matches!(&x.notify_settings, grammers_client::tl::enums::PeerNotifySettings::Settings(s) if s.mute_until.is_some_and(|t| i64::from(t) > now)),
                ),
                _ => (0, false, false),
            };
            let (emoji_status, verified, premium) = match d.peer() {
                grammers_client::peer::Peer::User(u) => match &u.raw {
                    grammers_client::tl::enums::User::User(y) => (
                        emoji_status_id(y.emoji_status.as_ref()),
                        y.verified,
                        y.premium,
                    ),
                    _ => (None, false, false),
                },
                grammers_client::peer::Peer::Channel(c) => (
                    emoji_status_id(c.raw.emoji_status.as_ref()),
                    c.raw.verified,
                    false,
                ),
                _ => (None, false, false),
            };
            out.push(DialogRow {
                peer: d.peer_ref(),
                id: d.peer_id().bot_api_dialog_id_unchecked(),
                name: d.peer().name().unwrap_or("Без имени").to_owned(),
                last: d
                    .last_message
                    .as_ref()
                    .map(dialog_last_preview)
                    .unwrap_or_default(),
                date: d.last_message.as_ref().map_or(0, |m| m.date().as_second()),
                unread,
                pinned,
                muted,
                saved: matches!(d.peer(), grammers_client::peer::Peer::User(u) if u.is_self()),
                archived,
                kind: match d.peer() {
                    grammers_client::peer::Peer::User(u) if u.is_bot() => DialogKind::Bot,
                    grammers_client::peer::Peer::User(_) => DialogKind::User,
                    grammers_client::peer::Peer::Channel(_) => DialogKind::Channel,
                    _ => DialogKind::Group,
                },
                contact: matches!(d.peer(), grammers_client::peer::Peer::User(u) if u.contact()),
                read_out: match &d.raw {
                    grammers_client::tl::enums::Dialog::Dialog(x) => x.read_outbox_max_id,
                    _ => 0,
                },
                emoji_status,
                verified,
                premium,
            });
        }
        Ok(out)
    }

    /// Последние сообщения чата (новые — в конце), с пометками об удалении и правках из хранилища.
    pub async fn history(
        &self,
        peer: PeerRef,
        limit: usize,
    ) -> Result<Vec<MessageRow>, grammers_client::InvocationError> {
        self.history_before(peer, 0, limit).await
    }

    /// Сведения о чате для правой панели (имя, @имя, о себе, телефон, статус, музыка профиля, число участников).
    pub async fn peer_info(&self, peer: PeerRef) -> Result<PeerInfo, grammers_client::InvocationError> {
        use grammers_client::tl;
        let mut info = PeerInfo {
            id: peer_key(&peer),
            ..PeerInfo::default()
        };
        match tl::enums::InputPeer::from(peer) {
            tl::enums::InputPeer::User(u) => {
                let input_user = tl::enums::InputUser::User(tl::types::InputUser {
                    user_id: u.user_id,
                    access_hash: u.access_hash,
                });
                let req_full = tl::functions::users::GetFullUser { id: input_user.clone() };
                let req_music = tl::functions::users::GetSavedMusic {
                    id: input_user,
                    offset: 0,
                    limit: 20,
                    hash: 0,
                };
                let (full_res, music_res) = tokio::join!(
                    self.client.invoke(&req_full),
                    self.client.invoke(&req_music),
                );
                let tl::enums::users::UserFull::Full(f) = full_res?;
                let tl::enums::UserFull::Full(full) = f.full_user;
                info.about = full.about.unwrap_or_default();
                info.blocked = full.blocked;
                if let Some(b) = &full.birthday {
                    info.birthday = fmt_birthday(b);
                }
                if full.business_work_hours.is_some() {
                    info.business_hours = "Открыто".to_owned();
                }
                if let Some(doc) = &full.saved_music
                    && let Some(tr) = audio_track_of(doc)
                {
                    info.saved_music.push(tr);
                }
                if let Ok(tl::enums::users::SavedMusic::Music(m)) = music_res {
                    for doc in &m.documents {
                        if let Some(tr) = audio_track_of(doc)
                            && !info.saved_music.iter().any(|x| x.id == tr.id)
                        {
                            info.saved_music.push(tr);
                        }
                    }
                }
                if let Some(tl::enums::User::User(x)) = f.users.iter().find(|x| matches!(x, tl::enums::User::User(y) if y.id == u.user_id)) {
                    info.name = [x.first_name.clone(), x.last_name.clone()]
                        .into_iter()
                        .flatten()
                        .collect::<Vec<_>>()
                        .join(" ");
                    info.username = x.username.clone().unwrap_or_default();
                    info.phone = x.phone.clone().unwrap_or_default();
                    info.emoji_status = emoji_status_id(x.emoji_status.as_ref());
                    info.verified = x.verified;
                    info.premium = x.premium;
                    if let Some(s) = &x.status {
                        info.status = status_text(s);
                    }
                }
            }
            tl::enums::InputPeer::Channel(c) => {
                let tl::enums::messages::ChatFull::Full(f) = self
                    .client
                    .invoke(&tl::functions::channels::GetFullChannel {
                        channel: tl::enums::InputChannel::Channel(tl::types::InputChannel {
                            channel_id: c.channel_id,
                            access_hash: c.access_hash,
                        }),
                    })
                    .await?;
                if let tl::enums::ChatFull::ChannelFull(full) = f.full_chat {
                    info.about = full.about;
                    info.members = full.participants_count.unwrap_or(0);
                    if let Some(tl::enums::ExportedChatInvite::ChatInviteExported(inv)) = full.exported_invite {
                        info.invite_link = inv.link;
                    }
                }
                if let Some(tl::enums::Chat::Channel(y)) = f.chats.iter().find(|x| matches!(x, tl::enums::Chat::Channel(y) if y.id == c.channel_id)) {
                    info.name = y.title.clone();
                    info.username = y.username.clone().unwrap_or_default();
                    info.broadcast = y.broadcast;
                    info.emoji_status = emoji_status_id(y.emoji_status.as_ref());
                    info.verified = y.verified;
                }
            }
            tl::enums::InputPeer::Chat(c) => {
                let tl::enums::messages::ChatFull::Full(f) = self
                    .client
                    .invoke(&tl::functions::messages::GetFullChat { chat_id: c.chat_id })
                    .await?;
                if let tl::enums::ChatFull::Full(full) = f.full_chat {
                    info.about = full.about;
                    if let Some(tl::enums::ChatParticipants::Participants(p)) = Some(full.participants) {
                        info.members = p.participants.len() as i32;
                    }
                }
                if let Some(tl::enums::Chat::Chat(y)) = f.chats.iter().find(|x| matches!(x, tl::enums::Chat::Chat(y) if y.id == c.chat_id)) {
                    info.name = y.title.clone();
                }
            }
            _ => {}
        }
        Ok(info)
    }

    /// Скачать музыкальный трек (из `saved_music`) в кэш `cache/audio/<id>.<ext>`.
    pub async fn download_track(&self, track: &AudioTrack) -> Result<PathBuf, BoxErr> {
        self.download_track_streaming(track, |_, _, _, _| {}).await
    }

    /// Потоковая загрузка музыкального трека кусками по 512 КБ с колбэком `(done, total, &part, &dest)`:
    /// позволяет начать воспроизведение через `stream.rs` уже после первых 128 КБ.
    pub async fn download_track_streaming(
        &self,
        track: &AudioTrack,
        mut progress: impl FnMut(u64, u64, &Path, &Path),
    ) -> Result<PathBuf, BoxErr> {
        use grammers_client::tl;
        use std::io::Write;
        let dir = self.dirs.cache.join("audio");
        std::fs::create_dir_all(&dir)?;
        let file = dir.join(format!("{}.{}", track.id, track.ext()));
        if file.exists() && file.metadata().is_ok_and(|m| m.len() > 0) {
            return Ok(file);
        }
        let part = file.with_extension("part");
        let loc = RawLocation(tl::enums::InputFileLocation::InputDocumentFileLocation(
            tl::types::InputDocumentFileLocation {
                id: track.id,
                access_hash: track.access_hash,
                file_reference: track.file_ref.clone(),
                thumb_size: String::new(),
            },
        ));
        let total = track.size.max(1);
        let mut dl = self.client.iter_download(&loc).chunk_size(512 * 1024);
        let mut out = std::fs::File::create(&part)?;
        let mut done = 0u64;
        while let Some(chunk) = dl.next().await? {
            out.write_all(&chunk)?;
            let _ = out.flush();
            done += chunk.len() as u64;
            progress(done, total, &part, &file);
        }
        drop(out);
        let _ = std::fs::rename(&part, &file);
        Ok(file)
    }

    /// Ссылка на отправителя сообщения (для загрузки его аватарки в группах).
    pub async fn sender_ref(&self, peer: PeerRef, msg_id: i32) -> Option<PeerRef> {
        let msgs = self.client.get_messages_by_id(peer, &[msg_id]).await.ok()?;
        let m = msgs.into_iter().next()??;
        m.sender()?.to_ref().await.ok()?
    }

    /// Сообщения старше `before_id` (0 — с самых новых). Для подгрузки при прокрутке вверх.
    pub async fn history_before(
        &self,
        peer: PeerRef,
        before_id: i32,
        limit: usize,
    ) -> Result<Vec<MessageRow>, grammers_client::InvocationError> {
        let key = peer_key(&peer);
        let mut it = self.client.iter_messages(peer).offset_id(before_id);
        let mut msgs = Vec::new();
        while msgs.len() < limit {
            let Some(m) = it.next().await? else { break };
            msgs.push(m);
        }
        // Правки одним запросом; запись в базу — в фоне пачкой, лента не ждёт диск.
        let counts = self.store.edit_counts(key).await.unwrap_or_default();
        if self.settings().ayu.save_deleted_messages {
            let batch: Vec<(i32, String, i64)> = msgs
                .iter()
                .map(|m| (m.id(), m.text().to_owned(), m.date().as_second()))
                .collect();
            let store = self.store.clone();
            tokio::spawn(async move {
                let _ = store.remember_many(key, &batch).await;
            });
        }
        let mut rows: Vec<MessageRow> = msgs
            .iter()
            .map(|m| Self::row(key, m, counts.get(&m.id()).copied().unwrap_or(0)))
            .collect();
        if before_id == 0 {
            // Удалённые, но сохранённые у нас, возвращаем на место по времени.
            for s in self.deleted(key).await {
                if !rows.iter().any(|r| r.id == s.msg_id) {
                    let mut r = MessageRow::plain(s.msg_id, s.text, false, s.date);
                    r.deleted = true;
                    r.edits = counts.get(&s.msg_id).copied().unwrap_or(0);
                    rows.push(r);
                }
            }
        }
        rows.sort_by_key(|r| (r.date, r.id));
        Ok(self.finish(key, rows))
    }

    /// Фильтры и «зальго» из настроек; подписи ответов по сообщениям выборки.
    fn finish(&self, key: i64, mut rows: Vec<MessageRow>) -> Vec<MessageRow> {
        let cfg = self.settings();
        if cfg.ayu.filters_enabled {
            let (set, _) = FilterSet::build(&cfg.filters);
            if !set.is_empty() {
                rows.retain(|r| !set.hides(key, &r.text));
            }
        }
        if !cfg.ayu.shadow_ban_ids.is_empty() {
            rows.retain(|r| !cfg.ayu.shadow_ban_ids.contains(&r.sender_id));
        }
        if cfg.ayu.filter_zalgo {
            for r in &mut rows {
                r.text = rg_ayu::filters::strip_zalgo(&r.text);
            }
        }
        let texts: std::collections::HashMap<i32, String> = rows
            .iter()
            .map(|r| (r.id, r.text.chars().take(80).collect()))
            .collect();
        for r in &mut rows {
            if let Some((id, snippet)) = &mut r.reply_to
                && let Some(t) = texts.get(id)
            {
                snippet.clone_from(t);
            }
        }
        rows
    }

    fn row(_key: i64, m: &grammers_client::message::Message, edits: usize) -> MessageRow {
        let reply_to = m.reply_to_message_id().map(|id| (id, String::new()));
        let forwarded_from = m.forward_header().map(|h| {
            let grammers_client::tl::enums::MessageFwdHeader::Header(h) = h;
            h.from_name
                .or(h.post_author)
                .unwrap_or_else(|| "скрытый отправитель".to_owned())
        });
        MessageRow {
            id: m.id(),
            text: m.text().to_owned(),
            outgoing: m.outgoing(),
            date: m.date().as_second(),
            deleted: false,
            edits,
            media: m.media().as_ref().map(media::describe),
            sender: m
                .sender()
                .and_then(|p| p.name())
                .unwrap_or_default()
                .to_owned(),
            reply_to,
            forwarded_from,
            pinned: m.pinned(),
            edit_date: m.edit_date().map(|d| d.as_second()),
            views: m.view_count(),
            reactions: m.reaction_count().unwrap_or(0),
            sender_id: m
                .sender_id()
                .map_or(0, |p| p.bot_api_dialog_id_unchecked()),
            entities: m
                .fmt_entities()
                .map(|e| convert_entities(m.text(), e))
                .unwrap_or_default(),
            grouped: m.grouped_id(),
        }
    }

    /// Поиск по тексту внутри чата (на сервере); результат — по возрастанию времени.
    pub async fn search(
        &self,
        peer: PeerRef,
        query: &str,
        limit: usize,
    ) -> Result<Vec<MessageRow>, grammers_client::InvocationError> {
        let key = peer_key(&peer);
        let mut it = self.client.search_messages(peer).query(query);
        let mut rows = Vec::new();
        while rows.len() < limit {
            let Some(m) = it.next().await? else { break };
            rows.push(Self::row(key, &m, 0));
        }
        rows.sort_by_key(|r| (r.date, r.id));
        Ok(rows)
    }

    /// Отправить текст, при `reply_to` — ответом на сообщение.
    pub async fn send_reply(
        &self,
        peer: PeerRef,
        text: &str,
        reply_to: Option<i32>,
    ) -> Result<(), grammers_client::InvocationError> {
        let ghost_mode = self.settings().ghost;
        let msg = grammers_client::message::InputMessage::new()
            .text(text)
            .reply_to(reply_to)
            .silent(ghost_mode.should_send_without_sound());
        self.client.send_message(peer, msg).await?;
        if ghost::should_mark_read_after_action(&ghost_mode) {
            self.client.mark_as_read(peer).await?;
        }
        Ok(())
    }

    /// Отправить файл (картинки уходят как фото, остальное — документом).
    pub async fn send_file(
        &self,
        peer: PeerRef,
        path: &Path,
        caption: &str,
        reply_to: Option<i32>,
    ) -> Result<(), BoxErr> {
        let up = self.client.upload_file(path).await?;
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_lowercase)
            .unwrap_or_default();
        let base = grammers_client::message::InputMessage::new()
            .text(caption)
            .reply_to(reply_to)
            .silent(self.settings().ghost.should_send_without_sound());
        let msg = if matches!(ext.as_str(), "jpg" | "jpeg" | "png" | "webp") {
            base.photo(up)
        } else {
            base.file(up)
        };
        self.client.send_message(peer, msg).await?;
        Ok(())
    }

    /// Изменить текст своего сообщения.
    pub async fn edit(
        &self,
        peer: PeerRef,
        id: i32,
        text: &str,
    ) -> Result<(), grammers_client::InvocationError> {
        let msg = grammers_client::message::InputMessage::new().text(text);
        self.client.edit_message(peer, id, msg).await
    }

    /// Удалить сообщения. Копия в локальном хранилище остаётся (функция AyuGram).
    pub async fn delete(
        &self,
        peer: PeerRef,
        ids: &[i32],
    ) -> Result<usize, grammers_client::InvocationError> {
        self.client.delete_messages(peer, ids).await
    }

    /// Переслать сообщения в другой чат.
    pub async fn forward(
        &self,
        from: PeerRef,
        to: PeerRef,
        ids: &[i32],
    ) -> Result<(), grammers_client::InvocationError> {
        self.client
            .forward_messages(to, ids, from)
            .await
            .map(|_| ())
    }

    /// Пересылка без автора (функция AyuGram): отправляет копию сообщения без плашки «Переслано от».
    pub async fn forward_no_author(
        &self,
        from: PeerRef,
        to: PeerRef,
        msg_id: i32,
    ) -> Result<(), grammers_client::InvocationError> {
        if let Some(m) = self.message_by_id(from, msg_id).await? {
            let text = m.text();
            if !text.is_empty() {
                self.send_reply(to, text, None).await?;
                return Ok(());
            }
        }
        self.client
            .forward_messages(to, &[msg_id], from)
            .await
            .map(|_| ())
    }

    /// Встроенный переводчик Telegram (`messages.translateText`, без сторонних серверов).
    pub async fn translate_text(
        &self,
        text: &str,
        to_lang: &str,
    ) -> Result<String, grammers_client::InvocationError> {
        use grammers_client::tl;
        let res = self
            .client
            .invoke(&tl::functions::messages::TranslateText {
                peer: None,
                id: None,
                text: Some(vec![tl::enums::TextWithEntities::Entities(
                    tl::types::TextWithEntities {
                        text: text.to_owned(),
                        entities: Vec::new(),
                    },
                )]),
                to_lang: to_lang.to_owned(),
                tone: None,
            })
            .await?;
        let tl::enums::messages::TranslatedText::TranslateResult(r) = res;
        let out = r
            .result
            .into_iter()
            .map(|t| {
                let tl::enums::TextWithEntities::Entities(e) = t;
                e.text
            })
            .collect::<Vec<_>>()
            .join("\n");
        Ok(out)
    }

    pub async fn pin(
        &self,
        peer: PeerRef,
        id: i32,
        pinned: bool,
    ) -> Result<(), grammers_client::InvocationError> {
        if pinned {
            self.client.pin_message(peer, id).await
        } else {
            self.client.unpin_message(peer, id).await
        }
    }

    /// Поставить реакцию-эмодзи; пустая строка снимает свою реакцию.
    pub async fn react(
        &self,
        peer: PeerRef,
        id: i32,
        emoji: &str,
    ) -> Result<(), grammers_client::InvocationError> {
        use grammers_client::message::InputReactions;
        let reactions = if emoji.is_empty() {
            InputReactions::remove()
        } else {
            InputReactions::from(emoji)
        };
        self.client.send_reactions(peer, id, reactions).await?;
        Ok(())
    }

    /// Фото профиля или чата в кэше (`cache/avatars/<id>.jpg`); `None`, если фото нет.
    pub async fn avatar(&self, peer: PeerRef) -> Result<Option<PathBuf>, BoxErr> {
        let dir = self.dirs.cache.join("avatars");
        std::fs::create_dir_all(&dir)?;
        let file = dir.join(format!("{}.jpg", peer_key(&peer)));
        if file.exists() {
            return Ok(Some(file));
        }
        let resolved = self.client.resolve_peer(peer).await?;
        let Some(photo) = resolved.photo(false).await? else {
            return Ok(None);
        };
        self.client.download_media(&photo, &file).await?;
        Ok(Some(file))
    }

    /// Картинка для предпросмотра (фото или статичный стикер) в кэше на диске.
    pub async fn preview_path(
        &self,
        peer: PeerRef,
        msg_id: i32,
    ) -> Result<Option<PathBuf>, BoxErr> {
        let file = self.dirs.cache.join(format!("{}_{msg_id}.img", peer_key(&peer)));
        if file.exists() {
            return Ok(Some(file));
        }
        let Some(m) = self.message_by_id(peer, msg_id).await? else {
            return Ok(None);
        };
        let Some(media) = m.media() else { return Ok(None) };
        let info = media::describe(&media);
        let is_thumb = matches!(
            info.kind,
            MediaKind::AnimatedSticker | MediaKind::Video | MediaKind::Gif | MediaKind::Round
        );
        if !info.has_preview() || (!is_thumb && info.size > PREVIEW_LIMIT) {
            return Ok(None);
        }
        match &media {
            // У видео, GIF, кружков и анимированных стикеров берём самую большую скачиваемую миниатюру.
            grammers_client::media::Media::Document(d) if is_thumb => {
                use grammers_client::media::Downloadable;
                let thumbs = d.thumbs();
                let Some(th) = thumbs.iter().rev().find(|x| x.to_raw_input_location().is_some()) else {
                    return Ok(None);
                };
                self.client.download_media(th, &file).await?;
            }
            grammers_client::media::Media::Sticker(s) if is_thumb => {
                use grammers_client::media::Downloadable;
                let thumbs = s.document.thumbs();
                let Some(th) = thumbs.iter().rev().find(|x| x.to_raw_input_location().is_some()) else {
                    return Ok(None);
                };
                self.client.download_media(th, &file).await?;
            }
            _ => {
                self.client.download_media(&media, &file).await?;
            }
        }
        media::prune_cache(&self.dirs.cache, CACHE_LIMIT);
        Ok(Some(file))
    }

    /// Скачать вложение в каталог загрузок под безопасным именем; существующие файлы не затираются.
    pub async fn download_file(&self, peer: PeerRef, msg_id: i32) -> Result<Option<PathBuf>, BoxErr> {
        let Some(m) = self.message_by_id(peer, msg_id).await? else {
            return Ok(None);
        };
        let Some(media) = m.media() else { return Ok(None) };
        let info = media::describe(&media);
        let ext = media::ext_for_mime(&info.mime);
        let name = match info.kind {
            MediaKind::Photo => format!("photo_{msg_id}.jpg"),
            MediaKind::Sticker => format!("sticker_{msg_id}.webp"),
            MediaKind::Voice => format!("voice_{msg_id}.{ext}"),
            MediaKind::Round => format!("round_{msg_id}.{ext}"),
            MediaKind::Gif => format!("gif_{msg_id}.{ext}"),
            _ => {
                let n = media::safe_file_name(&info.name);
                if n.contains('.') { n } else { format!("{n}.{ext}") }
            }
        };
        let mut dest = self.dirs.downloads.join(&name);
        if dest.exists() {
            dest = self.dirs.downloads.join(format!("{msg_id}_{name}"));
        }
        self.client.download_media(&media, &dest).await?;
        Ok(Some(dest))
    }

    /// Скачанный раньше ролик в кэше (`cache/media/<чат>_<сообщение>.<расширение>`), если он целый.
    pub fn cached_media(&self, peer: PeerRef, msg_id: i32) -> Option<PathBuf> {
        let prefix = format!("{}_{msg_id}.", peer_key(&peer));
        std::fs::read_dir(self.dirs.cache.join("media"))
            .ok()?
            .flatten()
            .map(|e| e.path())
            .find(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with(&prefix) && !n.ends_with(".part"))
            })
    }

    /// Скачивает вложение по кускам с отчётом о ходе загрузки `progress(скачано, всего)`.
    /// Кэшированный файл возвращается сразу. Загрузка пишется в `.part` и переименовывается
    /// по завершении; флаг `cancel` прерывает её и удаляет недокачанное.
    /// `to_downloads` — сохранить в каталог загрузок под именем файла, иначе — в кэш плеера.
    pub async fn fetch_media(
        &self,
        peer: PeerRef,
        msg_id: i32,
        to_downloads: bool,
        cancel: &std::sync::atomic::AtomicBool,
        progress: impl Fn(u64, u64),
    ) -> Result<Fetched, BoxErr> {
        self.fetch_media_stream(peer, msg_id, to_downloads, cancel, progress, |_, _, _, _| {})
            .await
    }

    /// То же, что `fetch_media`, но вызывает `on_stream(part, dest, total, ext)` сразу после
    /// создания `.part` и первых чанков (для потокового воспроизведения до окончания загрузки).
    pub async fn fetch_media_stream(
        &self,
        peer: PeerRef,
        msg_id: i32,
        to_downloads: bool,
        cancel: &std::sync::atomic::AtomicBool,
        progress: impl Fn(u64, u64),
        mut on_stream: impl FnMut(&Path, &Path, u64, &str),
    ) -> Result<Fetched, BoxErr> {
        use std::io::Write;
        use std::sync::atomic::Ordering;
        if !to_downloads && let Some(p) = self.cached_media(peer, msg_id) {
            return Ok(Fetched::Ready(p));
        }
        let Some(m) = self.message_by_id(peer, msg_id).await? else {
            return Ok(Fetched::Missing);
        };
        let Some(media) = m.media() else { return Ok(Fetched::Missing) };
        let info = media::describe(&media);
        let ext = media::ext_for_mime(&info.mime);
        let dest = if to_downloads {
            let n = media::safe_file_name(&info.name);
            let name = if n.contains('.') { n } else { format!("{n}.{ext}") };
            let mut d = self.dirs.downloads.join(&name);
            if d.exists() {
                d = self.dirs.downloads.join(format!("{msg_id}_{name}"));
            }
            d
        } else {
            let dir = self.dirs.cache.join("media");
            std::fs::create_dir_all(&dir)?;
            dir.join(format!("{}_{msg_id}.{ext}", peer_key(&peer)))
        };
        let part = dest.with_extension(format!("{ext}.part"));
        let total = info.size.max(1);
        let mut file = std::fs::File::create(&part)?;
        let mut done = 0u64;
        let mut streamed = false;
        let stream_at = (384 * 1024u64).min(total);
        let mut it = self.client.iter_download(&media);
        progress(0, total);
        loop {
            if cancel.load(Ordering::Relaxed) {
                drop(file);
                let _ = std::fs::remove_file(&part);
                return Ok(Fetched::Cancelled);
            }
            match it.next().await {
                Ok(Some(chunk)) => {
                    file.write_all(&chunk)?;
                    file.flush()?;
                    done += chunk.len() as u64;
                    progress(done.min(total), total);
                    if !to_downloads && !streamed && done >= stream_at && done < total {
                        streamed = true;
                        on_stream(&part, &dest, total, ext);
                    }
                }
                Ok(None) => break,
                Err(e) => {
                    drop(file);
                    let _ = std::fs::remove_file(&part);
                    return Err(Box::new(e));
                }
            }
        }
        file.flush()?;
        drop(file);
        std::fs::rename(&part, &dest)?;
        Ok(Fetched::Ready(dest))
    }

    async fn message_by_id(
        &self,
        peer: PeerRef,
        msg_id: i32,
    ) -> Result<Option<grammers_client::message::Message>, grammers_client::InvocationError> {
        Ok(self
            .client
            .get_messages_by_id(peer, &[msg_id])
            .await?
            .into_iter()
            .next()
            .flatten())
    }

    pub async fn send(&self, peer: PeerRef, text: &str) -> Result<(), grammers_client::InvocationError> {
        let ghost_mode = self.settings().ghost;
        let silent = ghost_mode.should_send_without_sound();
        let msg = grammers_client::message::InputMessage::new().text(text).silent(silent);
        self.client.send_message(peer, msg).await?;
        if ghost::should_mark_read_after_action(&ghost_mode) {
            // Пользователь ответил — отметка о прочтении ожидаема.
            self.client.mark_as_read(peer).await?;
        }
        Ok(())
    }

    async fn remember(&self, peer: i64, id: i32, text: &str, date: i64) {
        if self.settings().ayu.save_deleted_messages {
            let _ = self.store.remember(peer, id, text, date).await;
        }
    }

    async fn deleted(&self, peer: i64) -> Vec<rg_ayu::store::Saved> {
        self.store.deleted(peer).await.unwrap_or_default()
    }


    /// Прежние версии правок сообщения.
    pub async fn edit_history(&self, peer: &PeerRef, id: i32) -> Vec<String> {
        self.store
            .history(peer_key(peer), id)
            .await
            .unwrap_or_default()
    }

    /// Поток обновлений: сохраняет новые, изменённые и удалённые сообщения.
    /// Возвращается, когда поток закончился. Для каждого события зовёт `on_event`.
    pub async fn run_updates(
        &self,
        updates: mpsc::Receiver<UpdatesLike>,
        mut on_event: impl FnMut(&Update) + Send,
    ) -> Result<(), BoxErr> {
        let mut stream = self
            .client
            .stream_updates(updates, UpdatesConfiguration { catch_up: false })
            .await?;
        loop {
            let upd = stream.next().await?;
            self.apply(&upd).await;
            on_event(&upd);
        }
    }

    /// Превращает обновление в событие для интерфейса (без сетевых запросов).
    pub fn live(&self, upd: &Update) -> Option<Live> {
        match upd {
            Update::NewMessage(m) | Update::MessageEdited(m) => {
                let key = m.peer_id().bot_api_dialog_id_unchecked();
                let rows = self.finish(key, vec![Self::row(key, m, 0)]);
                rows.into_iter().next().map(|r| Live::Message(key, Box::new(r)))
            }
            Update::MessageDeleted(d) => Some(Live::Deleted(
                d.channel_id().map(|ch| -1_000_000_000_000 - ch),
                d.messages().to_vec(),
            )),
            Update::Raw(r) => {
                use grammers_client::tl::enums::{Peer, Update as U};
                let U::ReadHistoryOutbox(x) = &r.raw else { return None };
                let key = match &x.peer {
                    Peer::User(u) => u.user_id,
                    Peer::Chat(c) => -c.chat_id,
                    Peer::Channel(c) => -1_000_000_000_000 - c.channel_id,
                };
                Some(Live::ReadOutbox(key, x.max_id))
            }
            _ => None,
        }
    }

    async fn apply(&self, upd: &Update) {
        let cfg = self.settings();
        match upd {
            Update::NewMessage(m) if cfg.ayu.save_deleted_messages => {
                let key = m.peer_id().bot_api_dialog_id_unchecked();
                self.remember(key, m.id(), m.text(), m.date().as_second())
                    .await;
            }
            Update::MessageEdited(m) if cfg.ayu.save_messages_history => {
                let key = m.peer_id().bot_api_dialog_id_unchecked();
                let _ = self
                    .store
                    .record_edit(key, m.id(), m.text(), m.date().as_second())
                    .await;
            }
            Update::MessageDeleted(d) if cfg.ayu.save_deleted_messages => {
                let _ = match d.channel_id() {
                    // Каналы в формате Bot API: -100 + id.
                    Some(ch) => {
                        self.store
                            .mark_deleted(-1_000_000_000_000 - ch, d.messages())
                            .await
                    }
                    None => self.store.mark_deleted_private(d.messages()).await,
                };
            }
            _ => {}
        }
    }
}

/// Идентификатор TL-конструктора `messages.readHistory` для политики призрака.
mod grammers_tl_types_id {
    use grammers_client::tl::{Identifiable, functions};
    pub const READ_HISTORY: u32 = functions::messages::ReadHistory::CONSTRUCTOR_ID;
}

/// Папка чатов (фильтр диалогов).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderRow {
    pub id: i32,
    pub title: String,
    pub contacts: bool,
    pub non_contacts: bool,
    pub groups: bool,
    pub broadcasts: bool,
    pub bots: bool,
    pub exclude_muted: bool,
    pub exclude_read: bool,
    pub exclude_archived: bool,
    /// Явно добавленные и закреплённые чаты (формат Bot API).
    pub include: Vec<i64>,
    pub exclude: Vec<i64>,
}

/// Входит ли чат в папку (правила Telegram: явные включения, затем типы и исключения).
pub fn folder_matches(f: &FolderRow, d: &DialogRow) -> bool {
    if f.exclude.contains(&d.id) {
        return false;
    }
    if f.include.contains(&d.id) {
        return true;
    }
    if d.archived && f.exclude_archived {
        return false;
    }
    if f.exclude_muted && d.muted {
        return false;
    }
    if f.exclude_read && d.unread == 0 {
        return false;
    }
    match d.kind {
        DialogKind::User => {
            if d.contact {
                f.contacts
            } else {
                f.non_contacts
            }
        }
        DialogKind::Bot => f.bots,
        DialogKind::Group => f.groups,
        DialogKind::Channel => f.broadcasts,
    }
}

/// Строка списка контактов.
#[derive(Debug, Clone)]
pub struct ContactRow {
    pub peer: PeerRef,
    pub id: i64,
    pub name: String,
    /// «в сети», «был(а) недавно» и т. п.
    pub status: String,
}

/// Набор стикеров.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StickerSetRow {
    pub id: i64,
    pub access_hash: i64,
    pub title: String,
    pub count: i32,
    /// Версия значка набора (для загрузки его миниатюры).
    pub thumb_version: Option<i32>,
    /// Набор пользовательских эмодзи (Premium-эмодзи).
    pub emojis: bool,
    pub installed: bool,
    /// Документ-обложка (для рекомендуемых наборов), если известен.
    pub cover: Option<Box<StickerRow>>,
}

/// Один стикер набора.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StickerRow {
    pub id: i64,
    pub access_hash: i64,
    pub file_ref: Vec<u8>,
    pub mime: String,
    pub emoji: String,
    /// Тип лучшей миниатюры документа (для анимированных стикеров и эмодзи), пусто — нет.
    pub thumb: String,
}

impl StickerRow {
    /// Статичный WebP: файл целиком можно показать картинкой.
    pub fn is_static(&self) -> bool {
        self.mime == "image/webp"
    }

    /// Есть ли что показать: сам файл или миниатюра анимированного стикера.
    pub fn has_preview(&self) -> bool {
        self.is_static() || !self.thumb.is_empty()
    }
}

/// Лучшая миниатюра документа (самая крупная из обычных, без «stripped»).
fn best_thumb(d: &grammers_client::tl::types::Document) -> String {
    use grammers_client::tl::enums::PhotoSize as P;
    d.thumbs
        .iter()
        .flatten()
        .filter_map(|t| match t {
            P::Size(x) => Some((x.w * x.h, x.r#type.clone())),
            P::Progressive(x) => Some((x.w * x.h, x.r#type.clone())),
            _ => None,
        })
        .max_by_key(|(a, _)| *a)
        .map(|(_, t)| t)
        .unwrap_or_default()
}

fn sticker_row_of(d: &grammers_client::tl::types::Document) -> StickerRow {
    use grammers_client::tl;
    let emoji = d
        .attributes
        .iter()
        .find_map(|a| match a {
            tl::enums::DocumentAttribute::Sticker(x) => Some(x.alt.clone()),
            tl::enums::DocumentAttribute::CustomEmoji(x) => Some(x.alt.clone()),
            _ => None,
        })
        .unwrap_or_default();
    StickerRow {
        id: d.id,
        access_hash: d.access_hash,
        file_ref: d.file_reference.clone(),
        mime: d.mime_type.clone(),
        emoji,
        thumb: best_thumb(d),
    }
}

fn set_row_of(x: &grammers_client::tl::types::StickerSet, installed: bool) -> StickerSetRow {
    StickerSetRow {
        id: x.id,
        access_hash: x.access_hash,
        title: x.title.clone(),
        count: x.count,
        thumb_version: x.thumb_version,
        emojis: x.emojis,
        installed,
        cover: None,
    }
}

/// Пометка статуса пользователя для списка контактов.
fn status_text(s: &grammers_client::tl::enums::UserStatus) -> String {
    use grammers_client::tl::enums::UserStatus as U;
    match s {
        U::Online(_) => "в сети".to_owned(),
        U::Offline(o) => format!("был(а) в сети {}", fmt_ago(i64::from(o.was_online))),
        U::Recently(_) => "был(а) недавно".to_owned(),
        U::LastWeek(_) => "был(а) на этой неделе".to_owned(),
        U::LastMonth(_) => "был(а) в этом месяце".to_owned(),
        U::Empty => "был(а) давно".to_owned(),
    }
}

/// «сегодня в 14:05» / «вчера» / «3 дн. назад» по метке времени.
fn fmt_ago(ts: i64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    let d = (now - ts).max(0);
    match d {
        0..=59 => "только что".to_owned(),
        60..=3599 => format!("{} мин. назад", d / 60),
        3600..=86_399 => format!("{} ч. назад", d / 3600),
        86_400..=172_799 => "вчера".to_owned(),
        _ => format!("{} дн. назад", d / 86_400),
    }
}

/// Загрузка файла по «сырому» расположению (для стикеров из наборов).
struct RawLocation(grammers_client::tl::enums::InputFileLocation);

impl grammers_client::media::Downloadable for RawLocation {
    fn to_raw_input_location(&self) -> Option<grammers_client::tl::enums::InputFileLocation> {
        Some(self.0.clone())
    }
}

impl Engine {
    /// Папки чатов пользователя (без «Все чаты» и общих папок-ссылок).
    pub async fn folders(&self) -> Result<Vec<FolderRow>, grammers_client::InvocationError> {
        use grammers_client::tl;
        let tl::enums::messages::DialogFilters::Filters(f) = self
            .client
            .invoke(&tl::functions::messages::GetDialogFilters {})
            .await?;
        let ids = |peers: &[tl::enums::InputPeer]| -> Vec<i64> {
            peers
                .iter()
                .filter(|p| !matches!(p, tl::enums::InputPeer::Empty | tl::enums::InputPeer::PeerSelf))
                .map(|p| PeerRef::from(p).id.bot_api_dialog_id_unchecked())
                .collect()
        };
        let mut out = Vec::new();
        for x in &f.filters {
            if let tl::enums::DialogFilter::Filter(x) = x {
                let mut include = ids(&x.include_peers);
                include.extend(ids(&x.pinned_peers));
                out.push(FolderRow {
                    id: x.id,
                    title: {
                        let tl::enums::TextWithEntities::Entities(t) = &x.title;
                        t.text.clone()
                    },
                    contacts: x.contacts,
                    non_contacts: x.non_contacts,
                    groups: x.groups,
                    broadcasts: x.broadcasts,
                    bots: x.bots,
                    exclude_muted: x.exclude_muted,
                    exclude_read: x.exclude_read,
                    exclude_archived: x.exclude_archived,
                    include,
                    exclude: ids(&x.exclude_peers),
                });
            }
        }
        Ok(out)
    }

    /// Создать или обновить папку чатов (`messages.updateDialogFilter`).
    pub async fn save_folder(&self, f: &FolderRow) -> Result<(), grammers_client::InvocationError> {
        use grammers_client::tl;
        let filter = tl::enums::DialogFilter::Filter(tl::types::DialogFilter {
            contacts: f.contacts,
            non_contacts: f.non_contacts,
            groups: f.groups,
            broadcasts: f.broadcasts,
            bots: f.bots,
            exclude_muted: f.exclude_muted,
            exclude_read: f.exclude_read,
            exclude_archived: f.exclude_archived,
            title_noanimate: false,
            id: f.id,
            title: tl::enums::TextWithEntities::Entities(tl::types::TextWithEntities {
                text: f.title.clone(),
                entities: Vec::new(),
            }),
            emoticon: None,
            color: None,
            pinned_peers: Vec::new(),
            include_peers: Vec::new(),
            exclude_peers: Vec::new(),
        });
        self.client
            .invoke(&tl::functions::messages::UpdateDialogFilter {
                id: f.id,
                filter: Some(filter),
            })
            .await?;
        Ok(())
    }

    /// Удалить папку чатов по идентификатору (`messages.updateDialogFilter` с пустым фильтром).
    pub async fn delete_folder(&self, id: i32) -> Result<(), grammers_client::InvocationError> {
        use grammers_client::tl;
        self.client
            .invoke(&tl::functions::messages::UpdateDialogFilter { id, filter: None })
            .await?;
        Ok(())
    }

    /// Ключевые слова для поиска эмодзи из Telegram (`messages.getEmojiKeywords`).
    pub async fn emoji_keywords(&self, lang: &str) -> Result<Vec<(String, Vec<String>)>, grammers_client::InvocationError> {
        use grammers_client::tl;
        let tl::enums::EmojiKeywordsDifference::Difference(diff) = self
            .client
            .invoke(&tl::functions::messages::GetEmojiKeywords {
                lang_code: lang.to_owned(),
            })
            .await?;
        let mut out = Vec::with_capacity(diff.keywords.len());
        for kw in diff.keywords {
            if let tl::enums::EmojiKeyword::Keyword(k) = kw {
                out.push((k.keyword.to_lowercase(), k.emoticons));
            }
        }
        Ok(out)
    }

    /// Глобальный поиск пользователей, групп и каналов в Telegram (`contacts.search`).
    pub async fn global_search(&self, query: &str, limit: i32) -> Result<Vec<ContactRow>, grammers_client::InvocationError> {
        use grammers_client::tl;
        let q = query.trim();
        if q.len() < 3 {
            return Ok(Vec::new());
        }
        let tl::enums::contacts::Found::Found(found) = self
            .client
            .invoke(&tl::functions::contacts::Search {
                broadcasts: true,
                bots: true,
                q: q.to_owned(),
                limit: limit.clamp(1, 30),
            })
            .await?;
        let mut out = Vec::new();
        for u in &found.users {
            if let tl::enums::User::User(y) = u
                && !y.deleted
            {
                let name = format!(
                    "{} {}",
                    y.first_name.clone().unwrap_or_default(),
                    y.last_name.clone().unwrap_or_default()
                )
                .trim()
                .to_owned();
                let sub = y
                    .username
                    .as_ref()
                    .map(|un| format!("@{un}"))
                    .unwrap_or_else(|| y.status.as_ref().map(status_text).unwrap_or_default());
                out.push(ContactRow {
                    peer: PeerRef::from(u),
                    id: y.id,
                    name: if name.is_empty() { "Без имени".to_owned() } else { name },
                    status: sub,
                });
            }
        }
        for c in &found.chats {
            match c {
                tl::enums::Chat::Channel(ch) => {
                    let peer = PeerRef::from(c);
                    let sub = ch
                        .username
                        .as_ref()
                        .map(|un| format!("@{un}"))
                        .unwrap_or_else(|| {
                            if ch.broadcast {
                                "Канал".to_owned()
                            } else {
                                "Группа".to_owned()
                            }
                        });
                    out.push(ContactRow {
                        peer,
                        id: peer_key(&peer),
                        name: ch.title.clone(),
                        status: sub,
                    });
                }
                tl::enums::Chat::Chat(ch) => {
                    let peer = PeerRef::from(c);
                    out.push(ContactRow {
                        peer,
                        id: peer_key(&peer),
                        name: ch.title.clone(),
                        status: format!("{} участников", ch.participants_count),
                    });
                }
                _ => {}
            }
        }
        Ok(out)
    }

    /// Контакты пользователя, по алфавиту.
    pub async fn contacts(&self) -> Result<Vec<ContactRow>, grammers_client::InvocationError> {
        use grammers_client::tl;
        let res = self
            .client
            .invoke(&tl::functions::contacts::GetContacts { hash: 0 })
            .await?;
        let tl::enums::contacts::Contacts::Contacts(c) = res else {
            return Ok(Vec::new());
        };
        let mut out: Vec<ContactRow> = c
            .users
            .iter()
            .filter_map(|u| match u {
                tl::enums::User::User(y) if !y.deleted && !y.is_self => {
                    let name = format!(
                        "{} {}",
                        y.first_name.clone().unwrap_or_default(),
                        y.last_name.clone().unwrap_or_default()
                    )
                    .trim()
                    .to_owned();
                    Some(ContactRow {
                        peer: PeerRef::from(u),
                        id: y.id,
                        name: if name.is_empty() { "Без имени".to_owned() } else { name },
                        status: y.status.as_ref().map(status_text).unwrap_or_default(),
                    })
                }
                _ => None,
            })
            .collect();
        out.sort_by_key(|r| r.name.to_lowercase());
        Ok(out)
    }

    /// Установленные наборы стикеров (без масок и наборов эмодзи).
    pub async fn sticker_sets(&self) -> Result<Vec<StickerSetRow>, grammers_client::InvocationError> {
        use grammers_client::tl;
        let res = self
            .client
            .invoke(&tl::functions::messages::GetAllStickers { hash: 0 })
            .await?;
        let tl::enums::messages::AllStickers::Stickers(s) = res else {
            return Ok(Vec::new());
        };
        Ok(s.sets
            .iter()
            .filter_map(|x| {
                let tl::enums::StickerSet::Set(x) = x;
                (!x.masks && !x.emojis && !x.archived).then(|| set_row_of(x, true))
            })
            .collect())
    }

    /// Сохранённые GIF пользователя (документы с миниатюрами).
    pub async fn saved_gifs(&self) -> Result<Vec<StickerRow>, grammers_client::InvocationError> {
        use grammers_client::tl;
        let res = self.client.invoke(&tl::functions::messages::GetSavedGifs { hash: 0 }).await?;
        let tl::enums::messages::SavedGifs::Gifs(g) = res else {
            return Ok(Vec::new());
        };
        Ok(g.gifs
            .iter()
            .filter_map(|d| match d {
                tl::enums::Document::Document(d) => Some(sticker_row_of(d)),
                _ => None,
            })
            .collect())
    }

    /// Отправка сообщения с необязательным ответом и пользовательскими Premium-эмодзи.
    pub async fn send_rich(
        &self,
        peer: PeerRef,
        text: &str,
        custom_emojis: &[(usize, usize, i64)],
        reply_to: Option<i32>,
    ) -> Result<(), grammers_client::InvocationError> {
        use grammers_client::tl;
        let ghost_mode = self.settings().ghost;
        let silent = ghost_mode.should_send_without_sound();
        let mut msg = grammers_client::message::InputMessage::new()
            .text(text)
            .silent(silent);
        if let Some(id) = reply_to {
            msg = msg.reply_to(Some(id));
        }
        if !custom_emojis.is_empty() {
            let entities: Vec<tl::enums::MessageEntity> = custom_emojis
                .iter()
                .filter_map(|&(start, len, doc_id)| {
                    let (offset, length) = utf16_range(text, start, len);
                    (length > 0).then_some(tl::enums::MessageEntity::CustomEmoji(
                        tl::types::MessageEntityCustomEmoji {
                            offset,
                            length,
                            document_id: doc_id,
                        },
                    ))
                })
                .collect();
            if !entities.is_empty() {
                msg = msg.fmt_entities(entities);
            }
        }
        self.client.send_message(peer, msg).await?;
        if ghost::should_mark_read_after_action(&ghost_mode) {
            self.client.mark_as_read(peer).await?;
        }
        Ok(())
    }

    /// Создать канал (`broadcast`) или супергруппу с необязательным публичным `@username` и приглашением любых людей.
    pub async fn create_channel_full(
        &self,
        title: &str,
        about: &str,
        broadcast: bool,
        username: Option<&str>,
        members: &[PeerRef],
    ) -> Result<(), grammers_client::InvocationError> {
        use grammers_client::tl;
        let upd = self
            .client
            .invoke(&tl::functions::channels::CreateChannel {
                broadcast,
                megagroup: !broadcast,
                for_import: false,
                forum: false,
                title: title.to_owned(),
                about: about.to_owned(),
                geo_point: None,
                address: None,
                ttl_period: None,
            })
            .await?;
        let chats = match upd {
            tl::enums::Updates::Updates(u) => u.chats,
            tl::enums::Updates::Combined(u) => u.chats,
            _ => Vec::new(),
        };
        let created = chats.iter().find_map(|c| match c {
            tl::enums::Chat::Channel(ch) => Some(tl::enums::InputChannel::Channel(tl::types::InputChannel {
                channel_id: ch.id,
                access_hash: ch.access_hash.unwrap_or(0),
            })),
            _ => None,
        });
        if let Some(channel) = created {
            if let Some(un) = username.map(str::trim).filter(|s| !s.is_empty()) {
                let clean = un.trim_start_matches('@').to_owned();
                let _ = self
                    .client
                    .invoke(&tl::functions::channels::UpdateUsername {
                        channel: channel.clone(),
                        username: clean,
                    })
                    .await;
            }
            let users: Vec<tl::enums::InputUser> = members
                .iter()
                .filter_map(|p| match tl::enums::InputPeer::from(*p) {
                    tl::enums::InputPeer::User(u) => Some(tl::enums::InputUser::User(tl::types::InputUser {
                        user_id: u.user_id,
                        access_hash: u.access_hash,
                    })),
                    _ => None,
                })
                .collect();
            if !users.is_empty() {
                let _ = self
                    .client
                    .invoke(&tl::functions::channels::InviteToChannel { channel, users })
                    .await;
            }
        }
        Ok(())
    }

    /// Создать канал (`broadcast`) или супергруппу.
    pub async fn create_channel(&self, title: &str, about: &str, broadcast: bool) -> Result<(), grammers_client::InvocationError> {
        self.create_channel_full(title, about, broadcast, None, &[]).await
    }

    /// Пригласить выбранных пользователей в существующий канал, супергруппу или группу.
    pub async fn invite_members(
        &self,
        peer: PeerRef,
        members: &[PeerRef],
    ) -> Result<(), grammers_client::InvocationError> {
        use grammers_client::tl;
        let users: Vec<tl::enums::InputUser> = members
            .iter()
            .filter_map(|p| match tl::enums::InputPeer::from(*p) {
                tl::enums::InputPeer::User(u) => Some(tl::enums::InputUser::User(tl::types::InputUser {
                    user_id: u.user_id,
                    access_hash: u.access_hash,
                })),
                _ => None,
            })
            .collect();
        if users.is_empty() {
            return Ok(());
        }
        match tl::enums::InputPeer::from(peer) {
            tl::enums::InputPeer::Channel(c) => {
                self.client
                    .invoke(&tl::functions::channels::InviteToChannel {
                        channel: tl::enums::InputChannel::Channel(tl::types::InputChannel {
                            channel_id: c.channel_id,
                            access_hash: c.access_hash,
                        }),
                        users,
                    })
                    .await?;
            }
            tl::enums::InputPeer::Chat(c) => {
                for user_id in users {
                    let _ = self
                        .client
                        .invoke(&tl::functions::messages::AddChatUser {
                            chat_id: c.chat_id,
                            user_id,
                            fwd_limit: 50,
                        })
                        .await;
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// Список участников канала/супергруппы/группы для панели профиля.
    pub async fn channel_participants(
        &self,
        peer: PeerRef,
        limit: i32,
    ) -> Result<Vec<ContactRow>, grammers_client::InvocationError> {
        use grammers_client::tl;
        let users = match tl::enums::InputPeer::from(peer) {
            tl::enums::InputPeer::Channel(c) => {
                let res = self
                    .client
                    .invoke(&tl::functions::channels::GetParticipants {
                        channel: tl::enums::InputChannel::Channel(tl::types::InputChannel {
                            channel_id: c.channel_id,
                            access_hash: c.access_hash,
                        }),
                        filter: tl::enums::ChannelParticipantsFilter::ChannelParticipantsRecent,
                        offset: 0,
                        limit: limit.clamp(1, 100),
                        hash: 0,
                    })
                    .await?;
                match res {
                    tl::enums::channels::ChannelParticipants::Participants(p) => p.users,
                    tl::enums::channels::ChannelParticipants::NotModified => Vec::new(),
                }
            }
            tl::enums::InputPeer::Chat(c) => {
                let tl::enums::messages::ChatFull::Full(f) = self
                    .client
                    .invoke(&tl::functions::messages::GetFullChat { chat_id: c.chat_id })
                    .await?;
                f.users
            }
            _ => Vec::new(),
        };
        Ok(users
            .iter()
            .filter_map(|u| match u {
                tl::enums::User::User(y) if !y.deleted => {
                    let name = format!(
                        "{} {}",
                        y.first_name.clone().unwrap_or_default(),
                        y.last_name.clone().unwrap_or_default()
                    )
                    .trim()
                    .to_owned();
                    let status = y
                        .username
                        .as_ref()
                        .map(|un| format!("@{un}"))
                        .unwrap_or_else(|| y.status.as_ref().map(status_text).unwrap_or_default());
                    Some(ContactRow {
                        peer: PeerRef::from(u),
                        id: y.id,
                        name: if name.is_empty() { "Без имени".to_owned() } else { name },
                        status,
                    })
                }
                _ => None,
            })
            .collect())
    }

    /// Получить или создать ссылку-приглашение в группу/канал (`messages.exportChatInvite`).
    pub async fn export_invite_link(&self, peer: PeerRef) -> Result<String, grammers_client::InvocationError> {
        use grammers_client::tl;
        let res = self
            .client
            .invoke(&tl::functions::messages::ExportChatInvite {
                legacy_revoke_permanent: false,
                request_needed: false,
                peer: tl::enums::InputPeer::from(peer),
                expire_date: None,
                usage_limit: None,
                title: None,
                subscription_pricing: None,
            })
            .await?;
        Ok(match res {
            tl::enums::ExportedChatInvite::ChatInviteExported(inv) => inv.link,
            tl::enums::ExportedChatInvite::ChatInvitePublicJoinRequests => String::new(),
        })
    }

    /// Заблокировать или разблокировать пользователя (`contacts.block` / `contacts.unblock`).
    pub async fn set_blocked(&self, peer: PeerRef, block: bool) -> Result<(), grammers_client::InvocationError> {
        use grammers_client::tl;
        let id = tl::enums::InputPeer::from(peer);
        if block {
            self.client
                .invoke(&tl::functions::contacts::Block { my_stories_from: false, id })
                .await?;
        } else {
            self.client
                .invoke(&tl::functions::contacts::Unblock { my_stories_from: false, id })
                .await?;
        }
        Ok(())
    }

    /// Загружает документы пользовательских Premium-эмодзи по их `document_id`.
    pub async fn custom_emoji_docs(
        &self,
        ids: &[i64],
    ) -> Result<Vec<StickerRow>, grammers_client::InvocationError> {
        use grammers_client::tl;
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let chunk: Vec<i64> = ids.iter().copied().take(100).collect();
        let docs = self
            .client
            .invoke(&tl::functions::messages::GetCustomEmojiDocuments { document_id: chunk })
            .await?;
        Ok(docs
            .iter()
            .filter_map(|d| match d {
                tl::enums::Document::Document(doc) => Some(sticker_row_of(doc)),
                _ => None,
            })
            .collect())
    }

    /// Проголосовать в опросе (`messages.sendVote`).
    pub async fn vote_poll(
        &self,
        peer: PeerRef,
        msg_id: i32,
        option: Vec<u8>,
    ) -> Result<(), grammers_client::InvocationError> {
        use grammers_client::tl;
        self.client
            .invoke(&tl::functions::messages::SendVote {
                peer: tl::enums::InputPeer::from(peer),
                msg_id,
                options: vec![option],
            })
            .await?;
        Ok(())
    }

    /// Создать обычную группу с выбранными контактами.
    pub async fn create_group(&self, title: &str, members: &[PeerRef]) -> Result<(), grammers_client::InvocationError> {
        use grammers_client::tl;
        let users = members
            .iter()
            .filter_map(|p| match tl::enums::InputPeer::from(*p) {
                tl::enums::InputPeer::User(u) => Some(tl::enums::InputUser::User(tl::types::InputUser {
                    user_id: u.user_id,
                    access_hash: u.access_hash,
                })),
                _ => None,
            })
            .collect();
        self.client
            .invoke(&tl::functions::messages::CreateChat {
                users,
                title: title.to_owned(),
                ttl_period: None,
            })
            .await?;
        Ok(())
    }

    /// Установленные наборы пользовательских (Premium) эмодзи.
    pub async fn emoji_sets(&self) -> Result<Vec<StickerSetRow>, grammers_client::InvocationError> {
        use grammers_client::tl;
        let res = self
            .client
            .invoke(&tl::functions::messages::GetEmojiStickers { hash: 0 })
            .await?;
        let tl::enums::messages::AllStickers::Stickers(s) = res else {
            return Ok(Vec::new());
        };
        Ok(s.sets
            .iter()
            .filter_map(|x| {
                let tl::enums::StickerSet::Set(x) = x;
                (!x.archived).then(|| set_row_of(x, true))
            })
            .collect())
    }

    /// Рекомендуемые Telegram наборы эмодзи; `installed` — уже добавлены.
    pub async fn featured_emoji_sets(&self) -> Result<Vec<StickerSetRow>, grammers_client::InvocationError> {
        use grammers_client::tl;
        let res = self
            .client
            .invoke(&tl::functions::messages::GetFeaturedEmojiStickers { hash: 0 })
            .await?;
        let tl::enums::messages::FeaturedStickers::Stickers(s) = res else {
            return Ok(Vec::new());
        };
        use tl::enums::StickerSetCovered as C;
        Ok(s.sets
            .iter()
            .map(|c| {
                let (set, cover) = match c {
                    C::Covered(x) => (&x.set, Some(&x.cover)),
                    C::StickerSetMultiCovered(x) => (&x.set, x.covers.first()),
                    C::StickerSetFullCovered(x) => (&x.set, x.documents.first()),
                    C::StickerSetNoCovered(x) => (&x.set, None),
                };
                let tl::enums::StickerSet::Set(set) = set;
                let mut row = set_row_of(set, set.installed_date.is_some());
                row.cover = cover.and_then(|d| match d {
                    tl::enums::Document::Document(d) => Some(Box::new(sticker_row_of(d))),
                    _ => None,
                });
                row
            })
            .collect())
    }

    /// Добавить набор себе.
    pub async fn install_set(&self, set: &StickerSetRow) -> Result<(), grammers_client::InvocationError> {
        use grammers_client::tl;
        self.client
            .invoke(&tl::functions::messages::InstallStickerSet {
                stickerset: tl::enums::InputStickerSet::Id(tl::types::InputStickerSetId {
                    id: set.id,
                    access_hash: set.access_hash,
                }),
                archived: false,
            })
            .await?;
        Ok(())
    }

    /// Стикеры (эмодзи) набора.
    pub async fn sticker_set(&self, set: &StickerSetRow) -> Result<Vec<StickerRow>, grammers_client::InvocationError> {
        use grammers_client::tl;
        let res = self
            .client
            .invoke(&tl::functions::messages::GetStickerSet {
                stickerset: tl::enums::InputStickerSet::Id(tl::types::InputStickerSetId {
                    id: set.id,
                    access_hash: set.access_hash,
                }),
                hash: 0,
            })
            .await?;
        let tl::enums::messages::StickerSet::Set(s) = res else {
            return Ok(Vec::new());
        };
        Ok(s.documents
            .iter()
            .filter_map(|d| {
                let tl::enums::Document::Document(d) = d else { return None };
                Some(sticker_row_of(d))
            })
            .collect())
    }

    /// Файл стикера или его миниатюры в кэше (`cache/stickers/<id>.webp`, `<id>_t.webp`).
    pub async fn sticker_file(&self, st: &StickerRow) -> Result<Option<PathBuf>, BoxErr> {
        use grammers_client::tl;
        let (name, thumb) = if st.is_static() {
            (format!("{}.webp", st.id), String::new())
        } else if !st.thumb.is_empty() {
            (format!("{}_t.webp", st.id), st.thumb.clone())
        } else {
            return Ok(None);
        };
        let dir = self.dirs.cache.join("stickers");
        std::fs::create_dir_all(&dir)?;
        let file = dir.join(name);
        if file.exists() {
            return Ok(Some(file));
        }
        let loc = RawLocation(tl::enums::InputFileLocation::InputDocumentFileLocation(
            tl::types::InputDocumentFileLocation {
                id: st.id,
                access_hash: st.access_hash,
                file_reference: st.file_ref.clone(),
                thumb_size: thumb,
            },
        ));
        self.client.download_media(&loc, &file).await?;
        Ok(Some(file))
    }

    /// Миниатюра значка набора (`cache/stickers/set_<id>.webp`).
    pub async fn set_icon(&self, set: &StickerSetRow) -> Result<Option<PathBuf>, BoxErr> {
        use grammers_client::tl;
        let Some(version) = set.thumb_version else {
            return Ok(None);
        };
        let dir = self.dirs.cache.join("stickers");
        std::fs::create_dir_all(&dir)?;
        let file = dir.join(format!("set_{}.webp", set.id));
        if file.exists() {
            return Ok(Some(file));
        }
        let loc = RawLocation(tl::enums::InputFileLocation::InputStickerSetThumb(
            tl::types::InputStickerSetThumb {
                stickerset: tl::enums::InputStickerSet::Id(tl::types::InputStickerSetId {
                    id: set.id,
                    access_hash: set.access_hash,
                }),
                thumb_version: version,
            },
        ));
        self.client.download_media(&loc, &file).await?;
        Ok(Some(file))
    }

    /// Отправить стикер из набора.
    pub async fn send_sticker(
        &self,
        peer: PeerRef,
        st: &StickerRow,
        reply_to: Option<i32>,
    ) -> Result<(), grammers_client::InvocationError> {
        use grammers_client::tl;
        use std::sync::atomic::{AtomicI64, Ordering};
        static SEQ: AtomicI64 = AtomicI64::new(0);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos() as i64);
        let random_id = nanos ^ (SEQ.fetch_add(1, Ordering::Relaxed) << 20);
        let ghost = self.settings().ghost;
        self.client
            .invoke(&tl::functions::messages::SendMedia {
                silent: ghost.should_send_without_sound(),
                background: false,
                clear_draft: false,
                noforwards: false,
                update_stickersets_order: false,
                invert_media: false,
                allow_paid_floodskip: false,
                peer: peer.into(),
                reply_to: reply_to.map(|id| {
                    tl::enums::InputReplyTo::Message(tl::types::InputReplyToMessage {
                        reply_to_msg_id: id,
                        top_msg_id: None,
                        reply_to_peer_id: None,
                        quote_text: None,
                        quote_entities: None,
                        quote_offset: None,
                        monoforum_peer_id: None,
                        todo_item_id: None,
                        poll_option: None,
                        quote_rich: None,
                    })
                }),
                media: tl::enums::InputMedia::Document(tl::types::InputMediaDocument {
                    spoiler: false,
                    id: tl::enums::InputDocument::Document(tl::types::InputDocument {
                        id: st.id,
                        access_hash: st.access_hash,
                        file_reference: st.file_ref.clone(),
                    }),
                    video_cover: None,
                    video_timestamp: None,
                    ttl_seconds: None,
                    query: None,
                }),
                message: String::new(),
                random_id,
                reply_markup: None,
                entities: None,
                schedule_date: None,
                schedule_repeat_period: None,
                send_as: None,
                quick_reply_shortcut: None,
                effect: None,
                allow_paid_stars: None,
                suggested_post: None,
            })
            .await?;
        if ghost::should_mark_read_after_action(&ghost) {
            self.client.mark_as_read(peer).await?;
        }
        Ok(())
    }
}

/// Активный сеанс (устройство) аккаунта.
#[derive(Debug, Clone)]
pub struct SessionRow {
    pub hash: i64,
    pub current: bool,
    pub device: String,
    pub app: String,
    pub platform: String,
    pub place: String,
    pub ip: String,
    pub active: i64,
}

/// Все 12 правил конфиденциальности Telegram (как в AyuGram Desktop).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrivacyKeyKind {
    PhoneNumber,
    StatusTimestamp,
    ProfilePhoto,
    Forwards,
    PhoneCall,
    VoiceMessages,
    Messages,
    Birthday,
    Gifts,
    About,
    SavedMusic,
    ChatInvite,
}

impl PrivacyKeyKind {
    pub const ALL: [Self; 12] = [
        Self::PhoneNumber,
        Self::StatusTimestamp,
        Self::ProfilePhoto,
        Self::Forwards,
        Self::PhoneCall,
        Self::VoiceMessages,
        Self::Messages,
        Self::Birthday,
        Self::Gifts,
        Self::About,
        Self::SavedMusic,
        Self::ChatInvite,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Self::PhoneNumber => "Номер телефона",
            Self::StatusTimestamp => "Последняя активность",
            Self::ProfilePhoto => "Фото профиля",
            Self::Forwards => "Пересланные сообщения",
            Self::PhoneCall => "Звонки",
            Self::VoiceMessages => "Голосовые сообщения",
            Self::Messages => "Сообщения",
            Self::Birthday => "День рождения",
            Self::Gifts => "Подарки",
            Self::About => "О себе",
            Self::SavedMusic => "Музыка в профиле",
            Self::ChatInvite => "Группы и каналы",
        }
    }

    fn to_tl(self) -> grammers_client::tl::enums::InputPrivacyKey {
        use grammers_client::tl::enums::InputPrivacyKey as K;
        match self {
            Self::PhoneNumber => K::PhoneNumber,
            Self::StatusTimestamp => K::StatusTimestamp,
            Self::ProfilePhoto => K::ProfilePhoto,
            Self::Forwards => K::Forwards,
            Self::PhoneCall => K::PhoneCall,
            Self::VoiceMessages => K::VoiceMessages,
            Self::Messages => K::NoPaidMessages,
            Self::Birthday => K::Birthday,
            Self::Gifts => K::StarGiftsAutoSave,
            Self::About => K::About,
            Self::SavedMusic => K::SavedMusic,
            Self::ChatInvite => K::ChatInvite,
        }
    }
}

/// Базовый уровень доступа для правила приватности.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrivacyLevel {
    Everybody,
    Contacts,
    Nobody,
}

impl PrivacyLevel {
    pub fn label(self) -> &'static str {
        match self {
            Self::Everybody => "Все",
            Self::Contacts => "Мои контакты",
            Self::Nobody => "Никто",
        }
    }
}

/// Строка приватности: ключ, название, описание и базовый уровень.
#[derive(Debug, Clone)]
pub struct PrivacyRow {
    pub key: PrivacyKeyKind,
    pub title: &'static str,
    pub rule: String,
    pub level: PrivacyLevel,
}

/// Категория уведомлений Telegram.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotifyScope {
    Users,
    Chats,
    Broadcasts,
}

impl NotifyScope {
    pub fn title(self) -> &'static str {
        match self {
            Self::Users => "Личные чаты",
            Self::Chats => "Группы",
            Self::Broadcasts => "Каналы",
        }
    }

    fn to_tl(self) -> grammers_client::tl::enums::InputNotifyPeer {
        use grammers_client::tl::enums::InputNotifyPeer as P;
        match self {
            Self::Users => P::InputNotifyUsers,
            Self::Chats => P::InputNotifyChats,
            Self::Broadcasts => P::InputNotifyBroadcasts,
        }
    }
}

/// Уведомления Telegram для типа чатов: включены ли и показывается ли текст.
#[derive(Debug, Clone)]
pub struct NotifyRow {
    pub scope: NotifyScope,
    pub title: &'static str,
    pub enabled: bool,
    pub preview: bool,
}

/// Заблокированный пользователь с данными для разблокировки.
#[derive(Debug, Clone)]
pub struct BlockedUser {
    pub id: i64,
    pub access_hash: i64,
    pub name: String,
    pub username: String,
}

/// Всё, что нужно страницам «Настройки Telegram»: данные аккаунта, приватность, уведомления,
/// заблокированные, сеансы, 2FA, автоудаление и TTL аккаунта.
#[derive(Debug, Clone, Default)]
pub struct AccountOverview {
    pub me: PeerInfo,
    pub privacy: Vec<PrivacyRow>,
    pub notify: Vec<NotifyRow>,
    pub blocked: Vec<BlockedUser>,
    pub sessions: Vec<SessionRow>,
    pub session_ttl_days: i32,
    pub has_2fa: bool,
    pub login_email: String,
    pub default_history_ttl: i32,
    pub account_ttl_days: i32,
    pub contact_signup_notify: bool,
}

fn privacy_level(rules: &[grammers_client::tl::enums::PrivacyRule]) -> PrivacyLevel {
    use grammers_client::tl::enums::PrivacyRule as R;
    let mut base = PrivacyLevel::Nobody;
    for r in rules {
        match r {
            R::PrivacyValueAllowAll => base = PrivacyLevel::Everybody,
            R::PrivacyValueAllowContacts => base = PrivacyLevel::Contacts,
            R::PrivacyValueDisallowAll => base = PrivacyLevel::Nobody,
            _ => {}
        }
    }
    base
}

fn privacy_text(rules: &[grammers_client::tl::enums::PrivacyRule]) -> String {
    use grammers_client::tl::enums::PrivacyRule as R;
    let (mut base, mut allow, mut deny) = ("Никто", 0usize, 0usize);
    // Правила идут от частных к общим: последнее «разрешить всех / контакты / запретить всех» задаёт основу.
    for r in rules {
        match r {
            R::PrivacyValueAllowAll => base = "Все",
            R::PrivacyValueAllowContacts => base = "Мои контакты",
            R::PrivacyValueDisallowAll => base = "Никто",
            R::PrivacyValueAllowUsers(u) => allow += u.users.len(),
            R::PrivacyValueAllowChatParticipants(c) => allow += c.chats.len(),
            R::PrivacyValueDisallowUsers(u) => deny += u.users.len(),
            R::PrivacyValueDisallowChatParticipants(c) => deny += c.chats.len(),
            _ => {}
        }
    }
    let mut s = base.to_owned();
    if deny > 0 {
        s.push_str(&format!(" (−{deny})"));
    }
    if allow > 0 {
        s.push_str(&format!(" (+{allow})"));
    }
    s
}

impl Engine {
    /// Меняет имя, фамилию и «О себе» (`account.updateProfile`).
    pub async fn update_profile(&self, first: &str, last: &str, about: &str) -> Result<(), grammers_client::InvocationError> {
        use grammers_client::tl;
        self.client
            .invoke(&tl::functions::account::UpdateProfile {
                first_name: Some(first.to_owned()),
                last_name: Some(last.to_owned()),
                about: Some(about.to_owned()),
            })
            .await?;
        Ok(())
    }

    /// Меняет публичное имя пользователя (`account.updateUsername`).
    pub async fn update_username(&self, username: &str) -> Result<(), grammers_client::InvocationError> {
        use grammers_client::tl;
        self.client
            .invoke(&tl::functions::account::UpdateUsername {
                username: username.trim().trim_start_matches('@').to_owned(),
            })
            .await?;
        Ok(())
    }

    /// Изменяет правило приватности (`account.setPrivacy`).
    pub async fn set_privacy(
        &self,
        key: PrivacyKeyKind,
        level: PrivacyLevel,
    ) -> Result<(), grammers_client::InvocationError> {
        use grammers_client::tl;
        let rule = match level {
            PrivacyLevel::Everybody => tl::enums::InputPrivacyRule::InputPrivacyValueAllowAll,
            PrivacyLevel::Contacts => tl::enums::InputPrivacyRule::InputPrivacyValueAllowContacts,
            PrivacyLevel::Nobody => tl::enums::InputPrivacyRule::InputPrivacyValueDisallowAll,
        };
        self.client
            .invoke(&tl::functions::account::SetPrivacy {
                key: key.to_tl(),
                rules: vec![rule],
            })
            .await?;
        Ok(())
    }

    /// Изменяет серверные уведомления для категории чатов (`account.updateNotifySettings`).
    pub async fn set_notify_scope(
        &self,
        scope: NotifyScope,
        enabled: bool,
        preview: bool,
    ) -> Result<(), grammers_client::InvocationError> {
        use grammers_client::tl;
        self.client
            .invoke(&tl::functions::account::UpdateNotifySettings {
                peer: scope.to_tl(),
                settings: tl::enums::InputPeerNotifySettings::Settings(tl::types::InputPeerNotifySettings {
                    show_previews: Some(preview),
                    silent: Some(!enabled),
                    mute_until: Some(if enabled { 0 } else { i32::MAX }),
                    sound: None,
                    stories_muted: None,
                    stories_hide_sender: None,
                    stories_sound: None,
                }),
            })
            .await?;
        Ok(())
    }

    /// Уведомления о регистрации контактов (`account.setContactSignUpNotification`).
    pub async fn set_contact_signup_notify(&self, enabled: bool) -> Result<(), grammers_client::InvocationError> {
        use grammers_client::tl;
        self.client
            .invoke(&tl::functions::account::SetContactSignUpNotification { silent: !enabled })
            .await?;
        Ok(())
    }

    /// Таймер автоудаления новых чатов (`messages.setDefaultHistoryTTL`, секунды, 0 — выкл.).
    pub async fn set_default_history_ttl(&self, period: i32) -> Result<(), grammers_client::InvocationError> {
        use grammers_client::tl;
        self.client
            .invoke(&tl::functions::messages::SetDefaultHistoryTtl { period: period.max(0) })
            .await?;
        Ok(())
    }

    /// Срок удаления неактивного аккаунта (`account.setAccountTTL`, дни).
    pub async fn set_account_ttl_days(&self, days: i32) -> Result<(), grammers_client::InvocationError> {
        use grammers_client::tl;
        self.client
            .invoke(&tl::functions::account::SetAccountTtl {
                ttl: tl::enums::AccountDaysTtl::Ttl(tl::types::AccountDaysTtl { days: days.clamp(30, 730) }),
            })
            .await?;
        Ok(())
    }

    /// Завершить один сеанс по `hash` (`account.resetAuthorization`).
    pub async fn reset_authorization(&self, hash: i64) -> Result<(), grammers_client::InvocationError> {
        use grammers_client::tl;
        self.client
            .invoke(&tl::functions::account::ResetAuthorization { hash })
            .await?;
        Ok(())
    }

    /// Завершить все остальные сеансы (`auth.resetAuthorizations`).
    pub async fn reset_other_authorizations(&self) -> Result<(), grammers_client::InvocationError> {
        use grammers_client::tl;
        self.client
            .invoke(&tl::functions::auth::ResetAuthorizations {})
            .await?;
        Ok(())
    }

    /// Срок автоматического завершения неактивных сеансов (`account.setAuthorizationTTL`, дни).
    pub async fn set_authorization_ttl(&self, days: i32) -> Result<(), grammers_client::InvocationError> {
        use grammers_client::tl;
        self.client
            .invoke(&tl::functions::account::SetAuthorizationTtl {
                authorization_ttl_days: days.clamp(7, 365),
            })
            .await?;
        Ok(())
    }

    /// Разблокировать пользователя из списка заблокированных (`contacts.unblock`).
    pub async fn unblock_user(&self, id: i64, access_hash: i64) -> Result<(), grammers_client::InvocationError> {
        use grammers_client::tl;
        self.client
            .invoke(&tl::functions::contacts::Unblock {
                my_stories_from: false,
                id: tl::enums::InputPeer::User(tl::types::InputPeerUser {
                    user_id: id,
                    access_hash,
                }),
            })
            .await?;
        Ok(())
    }

    /// Очистить сохранённые платёжные данные и адреса доставки (`payments.clearSavedInfo`).
    pub async fn clear_payment_info(&self) -> Result<(), grammers_client::InvocationError> {
        use grammers_client::tl;
        self.client
            .invoke(&tl::functions::payments::ClearSavedInfo {
                credentials: true,
                info: true,
            })
            .await?;
        Ok(())
    }

    /// Загружает полный обзор аккаунта для настроек Telegram параллельно (в 5–8 раз быстрее последовательного обхода).
    pub async fn account_overview(&self) -> Result<AccountOverview, grammers_client::InvocationError> {
        use grammers_client::tl;
        let mut out = AccountOverview::default();

        let me_fut = async {
            if let Ok(me) = self.client.get_me().await
                && let Ok(Some(peer)) = me.to_ref().await
            {
                return self.peer_info(peer).await.ok();
            }
            None
        };
        let req_blocked = tl::functions::contacts::GetBlocked {
            my_stories_from: false,
            offset: 0,
            limit: 100,
        };
        let req_auths = tl::functions::account::GetAuthorizations {};
        let req_pwd = tl::functions::account::GetPassword {};
        let req_hist_ttl = tl::functions::messages::GetDefaultHistoryTtl {};
        let req_acc_ttl = tl::functions::account::GetAccountTtl {};
        let req_signup = tl::functions::account::GetContactSignUpNotification {};

        let blocked_fut = self.client.invoke(&req_blocked);
        let auths_fut = self.client.invoke(&req_auths);
        let pwd_fut = self.client.invoke(&req_pwd);
        let hist_ttl_fut = self.client.invoke(&req_hist_ttl);
        let acc_ttl_fut = self.client.invoke(&req_acc_ttl);
        let signup_fut = self.client.invoke(&req_signup);

        let notify_fut = async {
            let r_u = tl::functions::account::GetNotifySettings { peer: NotifyScope::Users.to_tl() };
            let r_c = tl::functions::account::GetNotifySettings { peer: NotifyScope::Chats.to_tl() };
            let r_b = tl::functions::account::GetNotifySettings { peer: NotifyScope::Broadcasts.to_tl() };
            let (u, c, b) = tokio::join!(
                self.client.invoke(&r_u),
                self.client.invoke(&r_c),
                self.client.invoke(&r_b),
            );
            let mut list = Vec::with_capacity(3);
            for (scope, res) in [(NotifyScope::Users, u), (NotifyScope::Chats, c), (NotifyScope::Broadcasts, b)] {
                if let Ok(tl::enums::PeerNotifySettings::Settings(s)) = res {
                    list.push(NotifyRow {
                        scope,
                        title: scope.title(),
                        enabled: s.mute_until.unwrap_or(0) == 0 && !s.silent.unwrap_or(false),
                        preview: s.show_previews.unwrap_or(true),
                    });
                }
            }
            list
        };

        let privacy_fut = async {
            let mut handles = Vec::with_capacity(PrivacyKeyKind::ALL.len());
            for k in PrivacyKeyKind::ALL {
                let c = self.client.clone();
                handles.push(tokio::spawn(async move {
                    let res = c.invoke(&tl::functions::account::GetPrivacy { key: k.to_tl() }).await;
                    (k, res)
                }));
            }
            let mut rows = Vec::with_capacity(PrivacyKeyKind::ALL.len());
            for h in handles {
                if let Ok((key, res)) = h.await {
                    let (rule, level) = match res {
                        Ok(tl::enums::account::PrivacyRules::Rules(r)) => (privacy_text(&r.rules), privacy_level(&r.rules)),
                        Err(_) => ("—".to_owned(), PrivacyLevel::Nobody),
                    };
                    rows.push(PrivacyRow {
                        key,
                        title: key.title(),
                        rule,
                        level,
                    });
                }
            }
            rows
        };

        let (me_res, privacy, notify, blocked_res, auths_res, pwd_res, hist_ttl_res, acc_ttl_res, signup_res) = tokio::join!(
            me_fut,
            privacy_fut,
            notify_fut,
            blocked_fut,
            auths_fut,
            pwd_fut,
            hist_ttl_fut,
            acc_ttl_fut,
            signup_fut,
        );

        if let Some(me) = me_res {
            out.me = me;
        }
        out.privacy = privacy;
        out.notify = notify;

        if let Ok(tl::enums::account::Password::Password(p)) = pwd_res {
            out.has_2fa = p.has_password;
            out.login_email = p.login_email_pattern.unwrap_or_default();
        }
        if let Ok(tl::enums::DefaultHistoryTtl::Ttl(t)) = hist_ttl_res {
            out.default_history_ttl = t.period;
        }
        if let Ok(tl::enums::AccountDaysTtl::Ttl(t)) = acc_ttl_res {
            out.account_ttl_days = t.days;
        }
        if let Ok(silent) = signup_res {
            out.contact_signup_notify = !silent;
        }
        if let Ok(b) = blocked_res {
            let users = match b {
                tl::enums::contacts::Blocked::Blocked(x) => x.users,
                tl::enums::contacts::Blocked::Slice(x) => x.users,
            };
            for u in users {
                if let tl::enums::User::User(u) = u {
                    let n = format!("{} {}", u.first_name.unwrap_or_default(), u.last_name.unwrap_or_default());
                    out.blocked.push(BlockedUser {
                        id: u.id,
                        access_hash: u.access_hash.unwrap_or(0),
                        name: n.trim().to_owned(),
                        username: u.username.unwrap_or_default(),
                    });
                }
            }
        }
        if let Ok(tl::enums::account::Authorizations::Authorizations(a)) = auths_res {
            out.session_ttl_days = a.authorization_ttl_days;
            for x in a.authorizations {
                let tl::enums::Authorization::Authorization(x) = x;
                out.sessions.push(SessionRow {
                    hash: x.hash,
                    current: x.current,
                    device: x.device_model,
                    app: format!("{} {}", x.app_name, x.app_version),
                    platform: format!("{} {}", x.platform, x.system_version),
                    place: [x.region, x.country].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(", "),
                    ip: x.ip,
                    active: i64::from(x.date_active),
                });
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn privacy_rules_are_summarised() {
        use grammers_client::tl::{enums::PrivacyRule as R, types};
        assert_eq!(privacy_text(&[R::PrivacyValueAllowAll]), "Все");
        assert_eq!(privacy_text(&[R::PrivacyValueDisallowAll]), "Никто");
        let rules = [
            R::PrivacyValueDisallowUsers(types::PrivacyValueDisallowUsers { users: vec![1, 2] }),
            R::PrivacyValueAllowUsers(types::PrivacyValueAllowUsers { users: vec![3] }),
            R::PrivacyValueAllowContacts,
        ];
        assert_eq!(privacy_text(&rules), "Мои контакты (−2) (+1)");
        assert_eq!(privacy_text(&[]), "Никто");
    }

    use super::*;
    use grammers_client::tl::{enums::MessageEntity, types};

    fn bold(offset: i32, length: i32) -> MessageEntity {
        MessageEntity::Bold(types::MessageEntityBold { offset, length })
    }

    #[test]
    fn entities_use_char_indices() {
        // «привет мир»: жирное «мир» — смещения те же (кириллица в BMP).
        let e = convert_entities("привет мир", &[bold(7, 3)]);
        assert_eq!(e, vec![Entity { kind: EntityKind::Bold, start: 7, end: 10, url: None }]);
    }

    #[test]
    fn astral_chars_count_as_two_utf16_units() {
        // «😀 ab»: эмодзи занимает 2 единицы UTF-16, но 1 символ; «ab» — смещение 3 → символ 2.
        let e = convert_entities("😀 ab", &[bold(3, 2)]);
        assert_eq!((e[0].start, e[0].end), (2, 4));
    }

    #[test]
    fn empty_and_unknown_entities_are_dropped() {
        assert!(convert_entities("abc", &[bold(1, 0)]).is_empty());
    }

    fn row(id: i64, kind: DialogKind) -> DialogRow {
        DialogRow {
            peer: grammers_session::types::PeerRef {
                id: grammers_session::types::PeerId::user_unchecked(1),
                auth: grammers_session::types::PeerAuth::default(),
            },
            id,
            name: String::new(),
            last: String::new(),
            date: 0,
            unread: 0,
            pinned: false,
            muted: false,
            saved: false,
            archived: false,
            kind,
            contact: false,
            read_out: 0,
            emoji_status: None,
            verified: false,
            premium: false,
        }
    }

    fn folder() -> FolderRow {
        FolderRow {
            id: 2,
            title: "Работа".into(),
            contacts: false,
            non_contacts: false,
            groups: true,
            broadcasts: false,
            bots: false,
            exclude_muted: false,
            exclude_read: false,
            exclude_archived: true,
            include: vec![10],
            exclude: vec![20],
        }
    }

    #[test]
    fn folder_includes_by_type_and_by_id() {
        let f = folder();
        assert!(folder_matches(&f, &row(5, DialogKind::Group)));
        assert!(!folder_matches(&f, &row(5, DialogKind::Channel)));
        // Явно добавленный чат входит независимо от типа.
        assert!(folder_matches(&f, &row(10, DialogKind::Channel)));
    }

    #[test]
    fn folder_exclusions_win() {
        let f = folder();
        assert!(!folder_matches(&f, &row(20, DialogKind::Group)));
        let mut archived = row(5, DialogKind::Group);
        archived.archived = true;
        assert!(!folder_matches(&f, &archived));
    }

    #[test]
    fn folder_contacts_vs_strangers() {
        let mut f = folder();
        f.groups = false;
        f.contacts = true;
        let mut c = row(7, DialogKind::User);
        assert!(!folder_matches(&f, &c));
        c.contact = true;
        assert!(folder_matches(&f, &c));
    }

    #[test]
    fn relative_time_text() {
        assert_eq!(fmt_ago(i64::MAX / 2), "только что");
    }

    #[test]
    fn custom_emoji_and_utf16_offsets_roundtrip() {
        use grammers_client::tl::{enums::MessageEntity as E, types};
        let text = "Привет 🔥 мир";
        // "Привет " = 7 UTF-16, "🔥" = 2 UTF-16 (char index 7, len 1).
        let (off, len) = utf16_range(text, 7, 1);
        assert_eq!((off, len), (7, 2));
        let ents = convert_entities(
            text,
            &[E::CustomEmoji(types::MessageEntityCustomEmoji {
                offset: off,
                length: len,
                document_id: 987_654_321,
            })],
        );
        assert_eq!(ents.len(), 1);
        assert_eq!(ents[0].kind, EntityKind::CustomEmoji(987_654_321));
        assert_eq!((ents[0].start, ents[0].end), (7, 8));
    }
}
