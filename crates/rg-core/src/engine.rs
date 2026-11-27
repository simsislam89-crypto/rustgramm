//! Движок: высокоуровневые операции клиента с режимом призрака и локальным хранилищем.
//! Любая служебная отправка («прочитано») проходит через политику `rg-ayu::ghost`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

#[path = "engine_ext.rs"]
mod ext;
pub use ext::*;

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
type RawDialogsBatch = (
    Vec<DialogRow>,
    bool,
    (i32, i32, grammers_client::tl::enums::InputPeer),
    Vec<grammers_client::tl::enums::User>,
    Vec<grammers_client::tl::enums::Chat>,
    Vec<(i64, i32)>,
);

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
    /// Явная настройка уведомлений чата: `Some(true)` — заглушен, `Some(false)` — явно включён, `None` — наследует глобальную категорию Telegram.
    pub notify_explicit: Option<bool>,
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
    /// Число непрочитанных упоминаний `@` в чате (`unread_mentions_count`).
    pub unread_mentions: i32,
    /// Число непрочитанных реакций `❤` на сообщения пользователя (`unread_reactions_count`).
    pub unread_reactions: i32,
    /// Наибольший прочитанный входящий ID (`Dialog.read_inbox_max_id`) для плашки «Непрочитанные сообщения».
    pub read_inbox_max_id: i32,
}

/// Фильтр типа сообщений для встроенного поиска в чате (`messages.search`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ChatSearchFilter {
    #[default]
    All,
    Photos,
    Videos,
    Files,
    Links,
    Voice,
}

impl ChatSearchFilter {
    pub const ALL: [Self; 6] = [
        Self::All,
        Self::Photos,
        Self::Videos,
        Self::Files,
        Self::Links,
        Self::Voice,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::All => "Все",
            Self::Photos => "Фото",
            Self::Videos => "Видео",
            Self::Files => "Файлы",
            Self::Links => "Ссылки",
            Self::Voice => "Голосовые",
        }
    }

    pub fn to_tl(self) -> grammers_client::tl::enums::MessagesFilter {
        use grammers_client::tl::enums::MessagesFilter as F;
        match self {
            Self::All => F::InputMessagesFilterEmpty,
            Self::Photos => F::InputMessagesFilterPhotos,
            Self::Videos => F::InputMessagesFilterVideo,
            Self::Files => F::InputMessagesFilterDocument,
            Self::Links => F::InputMessagesFilterUrl,
            Self::Voice => F::InputMessagesFilterVoice,
        }
    }
}

/// Форматирует счётчики (просмотры, реакции, участники, комментарии):
/// либо точно с пробелами между разрядами (`exact == true`), либо компактно (`K`/`M`).
pub fn format_compact_count(n: i32, exact: bool) -> String {
    let n = n.max(0);
    if exact || n < 1_000 {
        let s = n.to_string();
        let bytes = s.as_bytes();
        let mut out = String::with_capacity(s.len() + 2);
        for (i, &b) in bytes.iter().enumerate() {
            if i > 0 && (bytes.len() - i).is_multiple_of(3) {
                out.push(' ');
            }
            out.push(b as char);
        }
        return out;
    }
    if n >= 1_000_000 {
        let tenths = (n / 100_000) % 10;
        let whole = n / 1_000_000;
        if tenths == 0 {
            format!("{whole}M")
        } else {
            format!("{whole}.{tenths}M")
        }
    } else {
        let tenths = (n / 100) % 10;
        let whole = n / 1_000;
        if tenths == 0 {
            format!("{whole}K")
        } else {
            format!("{whole}.{tenths}K")
        }
    }
}

/// Экспорт загруженных сообщений чата в читаемый текстовый формат UTF-8.
pub fn format_chat_export_txt(chat_name: &str, chat_id: i64, rows: &[MessageRow]) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    let _ = writeln!(out, "Чат: {chat_name} (ID: {chat_id})");
    let _ = writeln!(out, "Сообщений в выборке: {}", rows.len());
    let _ = writeln!(out, "{}", "─".repeat(48));
    for m in rows {
        let dt = chrono_utc_hm(m.date);
        let who = if m.outgoing {
            "Вы"
        } else if m.sender.trim().is_empty() {
            "Собеседник"
        } else {
            m.sender.trim()
        };
        let _ = write!(out, "[{dt}] {who}");
        if m.sender_id != 0 {
            let _ = write!(out, " (#{})", m.sender_id);
        }
        if m.deleted {
            let _ = write!(out, " [удалено]");
        }
        if let Some((rid, _)) = m.reply_to {
            let _ = write!(out, " [ответ на #{rid}]");
        }
        let body = if !m.text.trim().is_empty() {
            m.text.trim().to_owned()
        } else if let Some(mi) = &m.media {
            use crate::media::MediaKind;
            let label = match &mi.kind {
                MediaKind::Photo => "Фото",
                MediaKind::Video => "Видео",
                MediaKind::Voice => "Голосовое",
                MediaKind::Round => "Видеосообщение",
                MediaKind::Audio => "Аудио",
                MediaKind::Gif => "GIF",
                MediaKind::Sticker | MediaKind::AnimatedSticker => "Стикер",
                MediaKind::File => {
                    if mi.name.is_empty() {
                        "Файл"
                    } else {
                        &mi.name
                    }
                }
                MediaKind::WebPage { title, url, .. } => {
                    if title.is_empty() {
                        url
                    } else {
                        title
                    }
                }
                MediaKind::Poll { question, .. } => question,
                MediaKind::Dice { emoticon, .. } => emoticon,
                MediaKind::Contact { first_name, .. } => first_name,
                MediaKind::Geo { title, .. } => {
                    if title.is_empty() {
                        "Геопозиция"
                    } else {
                        title
                    }
                }
                MediaKind::Other(s) => s,
            };
            format!("[{label}]")
        } else {
            String::new()
        };
        let _ = writeln!(out, ": {body}");
    }
    out
}

fn chrono_utc_hm(unix: i64) -> String {
    if unix <= 0 {
        return "0000-00-00 00:00:00".to_owned();
    }
    let days = unix.div_euclid(86_400);
    let secs = unix.rem_euclid(86_400);
    let h = secs / 3_600;
    let m = (secs % 3_600) / 60;
    let s = secs % 60;
    // Гражданский календарь (алгоритм Howard Hinnant).
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = (yoe as i64) + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mo = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = y + i64::from(mo <= 2);
    format!("{y:04}-{mo:02}-{d:02} {h:02}:{m:02}:{s:02}")
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

/// Какие реакции разрешены администратором в чате или канале (`available_reactions`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatReactionsPolicy {
    /// Разрешены все доступные реакции (`allow_custom` — можно ли кастомные эмодзи).
    All { allow_custom: bool },
    /// Разрешён только явный список эмодзи.
    Some(Vec<String>),
    /// Все реакции запрещены (`ChatReactions::None`).
    None,
}

impl Default for ChatReactionsPolicy {
    fn default() -> Self {
        Self::All { allow_custom: true }
    }
}

impl ChatReactionsPolicy {
    pub fn from_tl(cr: Option<&grammers_client::tl::enums::ChatReactions>) -> Self {
        use grammers_client::tl::enums::{ChatReactions as CR, Reaction as R};
        match cr {
            Some(CR::None) => Self::None,
            Some(CR::All(a)) => Self::All {
                allow_custom: a.allow_custom,
            },
            Some(CR::Some(s)) => {
                let list: Vec<String> = s
                    .reactions
                    .iter()
                    .filter_map(|r| match r {
                        R::Emoji(e) => Some(e.emoticon.clone()),
                        _ => Option::None,
                    })
                    .collect();
                if list.is_empty() {
                    Self::None
                } else {
                    Self::Some(list)
                }
            }
            Option::None => Self::default(),
        }
    }

    /// Проверяет, допустима ли данная эмодзи-реакция в чате/канале.
    pub fn allows(&self, emoji: &str) -> bool {
        let e = emoji.trim();
        if e.is_empty() {
            return false;
        }
        match self {
            Self::All { .. } => true,
            Self::Some(list) => list.iter().any(|x| x == e),
            Self::None => false,
        }
    }

    /// Список быстрых реакций для контекстного меню сообщения (пустой, если реакции запрещены).
    pub fn quick_list(&self, defaults: &[&str]) -> Vec<String> {
        match self {
            Self::All { .. } => defaults.iter().map(|s| (*s).to_owned()).collect(),
            Self::Some(list) => list.iter().take(7).cloned().collect(),
            Self::None => Vec::new(),
        }
    }

    /// Предпочитаемая быстрая реакция для двойного тапа (с учётом ограничений канала).
    pub fn default_allowed<'a>(&'a self, preferred: &'a str) -> Option<&'a str> {
        match self {
            Self::All { .. } => Some(if preferred.is_empty() { "👍" } else { preferred }),
            Self::Some(list) => {
                if !preferred.is_empty() && list.iter().any(|x| x == preferred) {
                    Some(preferred)
                } else {
                    list.first().map(String::as_str)
                }
            }
            Self::None => Option::None,
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
    /// Количество сохранённых подарков Telegram Stars в профиле.
    pub stargifts_count: i32,
    /// Сохранённая музыка в профиле (`UserFull.saved_music` + `users.getSavedMusic`).
    pub saved_music: Vec<AudioTrack>,
    pub forum: bool,
    pub linked_chat_id: Option<i64>,
    pub pinned_msg_id: Option<i32>,
    pub ttl_period: Option<i32>,
    /// Команды бота (`command`, `description`), если это бот.
    pub bot_commands: Vec<(String, String)>,
    /// Кнопка меню Mini App / WebApp бота (`(текст, url)`), если задана в `BotInfo.menu_button`.
    pub bot_menu_webapp: Option<(String, String)>,
    /// Описание бота из `BotInfo.description` (показывается на стартовом экране бота).
    pub bot_description: String,
    /// Включены ли платные звёздные реакции в этом канале (`ChannelFull.paid_reactions_available`).
    pub paid_reactions_available: bool,
    /// Политика разрешённых реакций в группе или канале (`ChannelFull`/`ChatFull.available_reactions`).
    pub available_reactions: ChatReactionsPolicy,
    /// Интервал медленного режима (Slow Mode), секунды (0 — выключен).
    pub slowmode_seconds: i32,
    /// Unix-время, когда можно отправить следующее сообщение при Slow Mode.
    pub slowmode_next_send_date: i32,
    /// Есть ли приветственные сообщения для новых участников канала/группы.
    pub has_welcome_messages: bool,
    /// Номер дата-центра Telegram, в котором хранится фото/профиль чата.
    pub dc_id: Option<i32>,
}

/// Действие кнопки бота (инлайн под сообщением или реплай-клавиатуры под полем ввода).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum BotBtnKind {
    /// Обычная текстовая кнопка реплай-клавиатуры (отправляет свой текст как сообщение).
    Text,
    /// Callback-кнопка (`messages.getBotCallbackAnswer` с `data`).
    Callback(Vec<u8>),
    /// Ссылка (`t.me/...`, `tg://...` или веб-ссылка).
    Url(String),
    /// Кнопка Mini App / WebView (`messages.requestWebView` или `messages.requestSimpleWebView`).
    WebView { url: String, simple: bool },
    /// Скопировать строку в буфер обмена.
    Copy(String),
    /// Вставить инлайн-запрос в поле ввода (`@bot query`).
    SwitchInline { query: String, same_peer: bool },
    /// Открыть профиль пользователя по `user_id`.
    UserProfile(i64),
}

/// Кнопка бота для отображения в интерфейсе.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct BotButton {
    pub text: String,
    pub kind: BotBtnKind,
    /// Стиль кнопки: 0 — обычный, 1 — primary (синий/акцент), 2 — danger (красный), 3 — success (зелёный).
    pub style: u8,
    /// Необязательный `document_id` пользовательского эмодзи-значка кнопки (`KeyboardButtonStyle.icon`).
    #[serde(default)]
    pub icon_id: Option<i64>,
}

/// Внутренняя ссылка Telegram (`tg://...`, `https://t.me/...`, `@username`), обрабатываемая внутри клиента без браузера.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TgLink {
    /// Открыть настройки (`tg://settings`).
    Settings,
    /// Настроить SOCKS5-прокси (`tg://socks?server=...&port=...` или `t.me/socks?...`).
    Proxy(String),
    /// Приглашение в группу/канал (`t.me/+HASH`, `t.me/joinchat/HASH`, `tg://join?invite=HASH`).
    Invite(String),
    /// Ссылка-приглашение на папку чатов (`t.me/addlist/SLUG`, `tg://addlist?slug=SLUG`).
    ChatlistInvite(String),
    /// Ссылка на пост приватного канала/группы (`t.me/c/<channel_id>/<msg_id>`, `tg://privatepost?...`).
    PrivatePost { channel_id: i64, msg_id: Option<i32> },
    /// Открыть пользователя по ID (`tg://user?id=<id>`).
    UserId(i64),
    /// Открыть `@username` / `t.me/<domain>` (опционально: номер поста, параметр `/start`, запуск Mini App `startapp`).
    Resolve {
        domain: String,
        post: Option<i32>,
        start: Option<String>,
        startapp: Option<String>,
    },
}

/// Разбирает любую ссылку Telegram (`tg://`, `https://t.me/`, `https://telegram.me/`, `t.me/`, `@username`)
/// во внутренний маршрут `TgLink`. Возвращает `None` для обычных внешних веб-сайтов.
pub fn parse_tg_link(raw: &str) -> Option<TgLink> {
    let s = raw.trim();
    if s.is_empty() {
        return None;
    }
    if let Some(u) = s.strip_prefix('@') {
        let clean: String = u
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        if clean.len() >= 3 {
            return Some(TgLink::Resolve {
                domain: clean,
                post: None,
                start: None,
                startapp: None,
            });
        }
        return None;
    }
    let query_param = |q: &str, key: &str| -> Option<String> {
        for part in q.split('&') {
            if let Some((k, v)) = part.split_once('=')
                && k.eq_ignore_ascii_case(key)
                && !v.is_empty()
            {
                return Some(v.to_owned());
            }
        }
        None
    };
    if let Some(rest) = s.strip_prefix("tg://") {
        let (path, q) = rest.split_once('?').unwrap_or((rest, ""));
        let path = path.trim_end_matches('/');
        return match path {
            "settings" => Some(TgLink::Settings),
            "join" => query_param(q, "invite").map(TgLink::Invite),
            "addlist" => query_param(q, "slug").map(TgLink::ChatlistInvite),
            "user" => query_param(q, "id")
                .and_then(|v| v.parse::<i64>().ok())
                .map(TgLink::UserId),
            "privatepost" => {
                let ch = query_param(q, "channel")?.parse::<i64>().ok()?;
                let post = query_param(q, "post").and_then(|p| p.parse::<i32>().ok());
                Some(TgLink::PrivatePost {
                    channel_id: ch,
                    msg_id: post,
                })
            }
            "socks" | "proxy" => {
                let srv = query_param(q, "server")?;
                let port = query_param(q, "port").unwrap_or_else(|| "1080".to_owned());
                match (query_param(q, "user"), query_param(q, "pass")) {
                    (Some(u), Some(p)) => Some(TgLink::Proxy(format!("socks5://{u}:{p}@{srv}:{port}"))),
                    _ => Some(TgLink::Proxy(format!("socks5://{srv}:{port}"))),
                }
            }
            "resolve" => {
                let domain = query_param(q, "domain")?;
                let post = query_param(q, "post").and_then(|p| p.parse::<i32>().ok());
                let start = query_param(q, "start").or_else(|| query_param(q, "startgroup"));
                let startapp = query_param(q, "startapp").or_else(|| query_param(q, "appname"));
                Some(TgLink::Resolve {
                    domain,
                    post,
                    start,
                    startapp,
                })
            }
            _ => None,
        };
    }
    let after_scheme = match s.split_once("://") {
        Some((scheme, tail)) if scheme.eq_ignore_ascii_case("https") || scheme.eq_ignore_ascii_case("http") => tail,
        _ => s,
    };
    let rest = after_scheme
        .strip_prefix("t.me/")
        .or_else(|| after_scheme.strip_prefix("telegram.me/"))
        .or_else(|| after_scheme.strip_prefix("telegram.dog/"))?;
    let (path, q) = rest.split_once('?').unwrap_or((rest, ""));
    let path = path.trim_matches('/');
    if path.is_empty() {
        return None;
    }
    if let Some(hash) = path.strip_prefix('+') {
        if !hash.is_empty() {
            return Some(TgLink::Invite(hash.to_owned()));
        }
        return None;
    }
    if let Some(hash) = path.strip_prefix("joinchat/") {
        if !hash.is_empty() {
            return Some(TgLink::Invite(hash.to_owned()));
        }
        return None;
    }
    if let Some(slug) = path.strip_prefix("addlist/") {
        if !slug.is_empty() {
            return Some(TgLink::ChatlistInvite(slug.to_owned()));
        }
        return None;
    }
    if path == "socks" || path == "proxy" {
        let srv = query_param(q, "server")?;
        let port = query_param(q, "port").unwrap_or_else(|| "1080".to_owned());
        return match (query_param(q, "user"), query_param(q, "pass")) {
            (Some(u), Some(p)) => Some(TgLink::Proxy(format!("socks5://{u}:{p}@{srv}:{port}"))),
            _ => Some(TgLink::Proxy(format!("socks5://{srv}:{port}"))),
        };
    }
    if let Some(c_rest) = path.strip_prefix("c/") {
        let mut parts = c_rest.split('/');
        let ch = parts.next()?.parse::<i64>().ok()?;
        let msg_id = parts.next().and_then(|p| p.parse::<i32>().ok());
        return Some(TgLink::PrivatePost {
            channel_id: ch,
            msg_id,
        });
    }
    let mut segs = path.split('/');
    let domain = segs.next()?.trim();
    if domain.is_empty()
        || !domain
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        return None;
    }
    let second = segs.next();
    let post = second.and_then(|p| p.parse::<i32>().ok());
    let app_seg = second.filter(|p| p.parse::<i32>().is_err() && !p.is_empty()).map(str::to_owned);
    let start = query_param(q, "start").or_else(|| query_param(q, "startgroup"));
    let startapp = query_param(q, "startapp").or(app_seg);
    Some(TgLink::Resolve {
        domain: domain.to_owned(),
        post,
        start,
        startapp,
    })
}

/// Опции тотальной очистки данных у других людей и на сервере без удаления аккаунта.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WipeScope {
    /// Удалить всю историю переписки в личных чатах и ботах для ОБЕИХ сторон (`revoke = true`).
    pub revoke_private_chats: bool,
    /// Найти и удалить свои отправленные сообщения в группах и супергруппах (`revoke = true`).
    pub delete_my_group_messages: bool,
    /// Стереть синхронизированные контакты и рейтинг частых собеседников (`contacts.deleteContacts`, `contacts.resetTopPeerRating`).
    pub clear_contacts_and_top_peers: bool,
    /// Очистить облачные черновики, недавние стикеры и платёжные данные (`messages.clearAllDrafts`, `messages.clearRecentStickers`, `payments.clearSavedInfo`).
    pub clear_drafts_stickers_payments: bool,
    /// Полностью очистить локальные кэши и базу сохранённых сообщений на устройстве.
    pub wipe_local_storage: bool,
}

impl Default for WipeScope {
    fn default() -> Self {
        Self {
            revoke_private_chats: true,
            delete_my_group_messages: true,
            clear_contacts_and_top_peers: true,
            clear_drafts_stickers_payments: true,
            wipe_local_storage: true,
        }
    }
}

/// Тема супергруппы-форума (`messages.getForumTopics`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForumTopicRow {
    pub id: i32,
    pub title: String,
    pub unread: i32,
    pub pinned: bool,
    pub closed: bool,
    pub top_message: i32,
}

/// Активная история пользователя/канала (`stories.getAllStories`).
#[derive(Debug, Clone)]
pub struct StoryRow {
    pub peer: PeerRef,
    pub peer_id: i64,
    pub author: String,
    pub story_id: i32,
    pub caption: String,
    pub date: i32,
    pub unread: bool,
    pub views: i32,
    pub reactions: i32,
    pub is_video: bool,
    pub count_in_peer: usize,
}

/// Ветка комментариев к посту канала (`messages.getDiscussionMessage` + `messages.getReplies`).
#[derive(Debug, Clone)]
pub struct DiscussionThread {
    pub disc_peer: PeerRef,
    pub root_msg_id: i32,
    pub post_id: i32,
    pub channel_name: String,
    pub post_preview: String,
    pub comments: Vec<MessageRow>,
}

/// Итог `Engine::fetch_media`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fetched {
    Ready(PathBuf),
    Cancelled,
    Missing,
}

/// Приватность публикуемой истории (`stories.sendStory`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoryPrivacy {
    Everyone,
    Contacts,
    CloseFriends,
    OnlyMe,
}

/// Гранулярные права администратора группы/канала (`channels.editAdmin`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AdminRights {
    pub change_info: bool,
    pub post_messages: bool,
    pub edit_messages: bool,
    pub delete_messages: bool,
    pub ban_users: bool,
    pub invite_users: bool,
    pub pin_messages: bool,
    pub add_admins: bool,
    pub anonymous: bool,
    pub manage_call: bool,
    pub manage_topics: bool,
}

impl AdminRights {
    /// Полный набор прав администратора (кроме «анонимность» и «добавление админов»).
    pub fn standard_moderator() -> Self {
        Self {
            change_info: true,
            post_messages: true,
            edit_messages: true,
            delete_messages: true,
            ban_users: true,
            invite_users: true,
            pin_messages: true,
            add_admins: false,
            anonymous: false,
            manage_call: true,
            manage_topics: true,
        }
    }

    /// Преобразование в TL `ChatAdminRights`.
    pub fn to_tl(self) -> grammers_client::tl::enums::ChatAdminRights {
        use grammers_client::tl;
        tl::enums::ChatAdminRights::Rights(tl::types::ChatAdminRights {
            change_info: self.change_info,
            post_messages: self.post_messages,
            edit_messages: self.edit_messages,
            delete_messages: self.delete_messages,
            ban_users: self.ban_users,
            invite_users: self.invite_users,
            pin_messages: self.pin_messages,
            add_admins: self.add_admins,
            anonymous: self.anonymous,
            manage_call: self.manage_call,
            other: true,
            manage_topics: self.manage_topics,
            post_stories: self.post_messages,
            edit_stories: self.edit_messages,
            delete_stories: self.delete_messages,
            manage_direct_messages: false,
            manage_ranks: false,
            manage_linked_peers: false,
            manage_welcome_messages: false,
        })
    }
}

/// Ограничения участника или общие права группы (`channels.editBanned` / `messages.editChatDefaultBannedRights`).
/// `true` означает ЗАПРЕЩЕНО (семантика Telegram `ChatBannedRights`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BannedRights {
    pub view_messages: bool,
    pub send_messages: bool,
    pub send_media: bool,
    pub send_stickers: bool,
    pub send_polls: bool,
    pub embed_links: bool,
    pub change_info: bool,
    pub invite_users: bool,
    pub pin_messages: bool,
    pub manage_topics: bool,
    /// UNIX-время окончания ограничения (`0` — навсегда).
    pub until_date: i32,
}

impl BannedRights {
    /// Полный бан (исключение из группы/канала).
    pub fn full_ban() -> Self {
        Self {
            view_messages: true,
            send_messages: true,
            send_media: true,
            send_stickers: true,
            send_polls: true,
            embed_links: true,
            change_info: true,
            invite_users: true,
            pin_messages: true,
            manage_topics: true,
            until_date: 0,
        }
    }

    /// Мут (запрет писать сообщения и медиа) до `until_date`.
    pub fn mute(until_date: i32) -> Self {
        Self {
            send_messages: true,
            send_media: true,
            send_stickers: true,
            send_polls: true,
            embed_links: true,
            until_date,
            ..Self::default()
        }
    }

    /// Преобразование в TL `ChatBannedRights`.
    pub fn to_tl(self) -> grammers_client::tl::enums::ChatBannedRights {
        use grammers_client::tl;
        tl::enums::ChatBannedRights::Rights(tl::types::ChatBannedRights {
            view_messages: self.view_messages,
            send_messages: self.send_messages,
            send_media: self.send_media,
            send_stickers: self.send_stickers,
            send_gifs: self.send_stickers,
            send_games: self.send_stickers,
            send_inline: self.send_stickers,
            embed_links: self.embed_links,
            send_polls: self.send_polls,
            change_info: self.change_info,
            invite_users: self.invite_users,
            pin_messages: self.pin_messages,
            manage_topics: self.manage_topics,
            send_photos: self.send_media,
            send_videos: self.send_media,
            send_roundvideos: self.send_media,
            send_audios: self.send_media,
            send_voices: self.send_media,
            send_docs: self.send_media,
            send_plain: self.send_messages,
            edit_rank: false,
            send_reactions: self.send_messages,
            manage_linked_peers: false,
            until_date: self.until_date,
        })
    }
}

/// Запись журнала недавних действий канала/группы (`channels.getAdminLog`).
#[derive(Debug, Clone)]
pub struct AdminLogRow {
    pub id: i64,
    pub date: i32,
    pub actor: String,
    pub summary: String,
}

/// Ссылка-приглашение группы/канала (`messages.getExportedChatInvites`).
#[derive(Debug, Clone)]
pub struct InviteLinkRow {
    pub link: String,
    pub title: String,
    pub usage: i32,
    pub usage_limit: Option<i32>,
    pub expire_date: Option<i32>,
    pub revoked: bool,
    pub request_needed: bool,
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
    /// Собеседник печатает, записывает голосовое или отправляет файл (ключ чата, текст; пусто — закончил).
    Typing(i64, String),
    /// Событие голосового/видеозвонка (`Update::PhoneCall`).
    PhoneCall(crate::calls::CallEvent),
    /// Обновление состояния секретного чата (`Update::Encryption`).
    SecretChatUpdate(rg_ayu::store::SecretChatRecord),
    /// Новое расшифрованное сообщение секретного чата (`Update::NewEncryptedMessage`).
    SecretMsg(rg_ayu::store::SecretMessageRecord),
    /// Сообщения чата прочитаны на другом устройстве (сброс счётчика непрочитанных).
    ReadInbox(i64, i32),
    /// Облачный черновик синхронизирован на другом устройстве (`(peer_id, text, reply_to)`).
    Draft(i64, String, Option<i32>),
    /// Изменилось количество реакций на сообщение (`(peer_id, msg_id, total_reactions)`).
    MessageReactions(i64, i32, i32),
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

/// Пункт интерактивного чеклиста (`TodoItem` + статус выполнения).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TodoItemRow {
    pub id: i32,
    pub text: String,
    pub done: bool,
}

/// Интерактивный чеклист задач в сообщении (`MessageMedia::ToDo`).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TodoListInfo {
    pub title: String,
    pub others_can_complete: bool,
    pub others_can_append: bool,
    pub items: Vec<TodoItemRow>,
}

/// Реакция под сообщением для интерактивных пилюль UI.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ReactionPill {
    pub emoji: String,
    #[serde(default)]
    pub custom_doc_id: Option<i64>,
    pub count: i32,
    #[serde(default)]
    pub chosen: bool,
    #[serde(default)]
    pub paid: bool,
}

/// Фильтр списка участников группы или канала.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParticipantFilter {
    All,
    Admins,
    Bots,
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
    /// Число платных звёздных реакций (`ReactionPaid`).
    #[serde(default)]
    pub paid_reactions: i32,
    /// Детализированные реакции под сообщением (`ReactionPill`: эмодзи/кастом/звёзды + счётчик + выбор).
    #[serde(default)]
    pub reaction_items: Vec<ReactionPill>,
    /// Отправитель (формат Bot API), 0 если неизвестен.
    pub sender_id: i64,
    /// Форматирование текста.
    pub entities: Vec<Entity>,
    /// Номер альбома: сообщения с одним номером показываются одним пузырём.
    pub grouped: Option<i64>,
    /// Инлайн-кнопки бота под сообщением (`ReplyInlineMarkup`).
    #[serde(default)]
    pub inline_buttons: Vec<Vec<BotButton>>,
    /// Реплай-клавиатура бота под полем ввода (`ReplyKeyboardMarkup`).
    #[serde(default)]
    pub reply_keyboard: Vec<Vec<BotButton>>,
    /// Блоки Telegram Layer 233 `RichMessage` (таблицы, цитаты, заголовки, списки современных ботов).
    #[serde(default)]
    pub rich_blocks: Vec<crate::rich_msg::RichBlock>,
    /// Идентификатор анимированного эффекта сообщения (`Message.effect`).
    #[serde(default)]
    pub effect: Option<i64>,
    /// Интерактивный чеклист (`MessageMedia::ToDo`).
    #[serde(default)]
    pub todo: Option<TodoListInfo>,
    /// Беззвучное сообщение Telegram (`Message.silent == true`) — не издаёт звук и не шлёт уведомление.
    #[serde(default)]
    pub silent: bool,
    /// Число комментариев к посту канала (`Message.replies.replies`).
    #[serde(default)]
    pub replies_count: i32,
    /// Включены ли комментарии у поста канала (`Message.replies.comments`).
    #[serde(default)]
    pub has_comments: bool,
    /// Идентификаторы до 3 последних комментаторов поста (`MessageReplies.recent_repliers`).
    #[serde(default)]
    pub recent_repliers: Vec<i64>,
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
            paid_reactions: 0,
            reaction_items: Vec::new(),
            sender_id: 0,
            entities: Vec::new(),
            grouped: None,
            inline_buttons: Vec::new(),
            reply_keyboard: Vec::new(),
            rich_blocks: Vec::new(),
            effect: None,
            todo: None,
            silent: false,
            replies_count: 0,
            has_comments: false,
            recent_repliers: Vec::new(),
        }
    }
}

type PendingDhMap = Arc<std::sync::Mutex<HashMap<i64, (Vec<u8>, Vec<u8>, Vec<u8>)>>>;
type InlineBotCacheMap = Arc<std::sync::Mutex<HashMap<String, grammers_client::tl::enums::InputUser>>>;
type SharedSession = Arc<crate::GuardedSession<grammers_session::storages::SqliteSession>>;

#[derive(Clone)]
pub struct Engine {
    pub client: Client,
    pub settings: Arc<RwLock<Settings>>,
    store: Arc<MessageStore>,
    session: Option<SharedSession>,
    dirs: Arc<Dirs>,
    /// Незавершённые обмены DH для секретных чатов и звонков: `id -> (our_exp, our_pub, dh_p)`.
    pending_dh: PendingDhMap,
    /// Прекомпилированный набор фильтров сообщений (пересобирается только при смене настроек).
    compiled_filters: Arc<RwLock<Arc<FilterSet>>>,
    /// Счётчик записей превью для редкого вызова `prune_cache`.
    preview_writes: Arc<std::sync::atomic::AtomicU32>,
    /// Кэш разрешённых `@bot` в `InputUser` для мгновенных инлайн-запросов.
    pub(crate) inline_bots_cache: InlineBotCacheMap,
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
            MediaKind::Dice { emoticon, value } => format!("{emoticon} ({value})"),
            MediaKind::Contact { phone, first_name, last_name, .. } => {
                let full = format!("{first_name} {last_name}").trim().to_owned();
                format!("👤 {}", if full.is_empty() { phone } else { full })
            }
            MediaKind::Geo { title, .. } => {
                if title.is_empty() {
                    "📍 Геопозиция".to_owned()
                } else {
                    format!("📍 {title}")
                }
            }
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
        Some(M::Dice(d)) => format!("{} ({})", d.emoticon, d.value),
        Some(M::Geo(_) | M::GeoLive(_) | M::Venue(_)) => "📍 Геопозиция".to_owned(),
        Some(M::Contact(_)) => "👤 Контакт".to_owned(),
        _ => String::new(),
    }
}

/// Параметры отправки медиа из встроенного редактора (сжатие, спойлер, самоуничтожение).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SendMediaOptions {
    /// Сжимать изображение в стандартное фото Telegram (`false` — отправить документом без сжатия).
    pub compress: bool,
    /// Скрыть медиа под спойлером.
    pub spoiler: bool,
    /// Таймер самоуничтожения в секундах (`None` — бессрочно).
    pub ttl_seconds: Option<i32>,
}

impl Default for SendMediaOptions {
    fn default() -> Self {
        Self {
            compress: true,
            spoiler: false,
            ttl_seconds: None,
        }
    }
}

impl Engine {
    pub fn new(client: Client, settings: Settings, store: MessageStore) -> Self {
        let (fset, _) = FilterSet::build(&settings.filters);
        Self {
            client,
            settings: Arc::new(RwLock::new(settings)),
            store: Arc::new(store),
            session: None,
            dirs: Arc::new(Dirs::default()),
            pending_dh: Arc::new(std::sync::Mutex::new(HashMap::new())),
            compiled_filters: Arc::new(RwLock::new(Arc::new(fset))),
            preview_writes: Arc::new(std::sync::atomic::AtomicU32::new(0)),
            inline_bots_cache: Arc::new(std::sync::Mutex::new(HashMap::new())),
        }
    }

    /// Привязывает кэш сессии к движку для сохранения `PeerInfo`/`ChannelState` из сырых RPC-ответов.
    pub fn with_session(mut self, session: SharedSession) -> Self {
        self.session = Some(session);
        self
    }

    /// Извлекает текст для локального архива (если текст пуст — описание медиа/стикера/голоса).
    pub(crate) fn archive_text_from_row(r: &MessageRow) -> String {
        if !r.text.trim().is_empty() {
            return r.text.clone();
        }
        if let Some(m) = &r.media {
            let tag = match &m.kind {
                media::MediaKind::Photo => "Фото",
                media::MediaKind::Video => "Видео",
                media::MediaKind::Audio => "Аудио",
                media::MediaKind::Gif => "GIF",
                media::MediaKind::Sticker | media::MediaKind::AnimatedSticker => "Стикер",
                media::MediaKind::Round => "Видеосообщение",
                media::MediaKind::Voice => "Голосовое",
                media::MediaKind::Poll { question, .. } => {
                    return format!("[Опрос: {}]", question.trim());
                }
                media::MediaKind::File => "Файл",
                media::MediaKind::WebPage { .. } => "Ссылка",
                media::MediaKind::Dice { emoticon, .. } => {
                    return format!("[Бросок {emoticon}]");
                }
                media::MediaKind::Contact { .. } => "Контакт",
                media::MediaKind::Geo { .. } => "Геопозиция",
                media::MediaKind::Other(s) => s.as_str(),
            };
            if m.name.trim().is_empty() {
                format!("[{tag}]")
            } else {
                format!("[{tag}: {}]", m.name.trim())
            }
        } else {
            String::new()
        }
    }

    pub(crate) async fn remember_row_sync(&self, peer: PeerRef, row: &MessageRow) {
        if row.id <= 0 || !self.settings().ayu.save_deleted_messages {
            return;
        }
        let key = peer_key(&peer);
        let archive = Self::archive_text_from_row(row);
        let _ = self
            .store
            .remember_full(key, row.id, &archive, row.date, row.outgoing, &row.sender)
            .await;
    }

    /// Фоновая пост-обработка после успешной отправки сообщения: отметка прочтения и «не в сети» по политике призрака + сохранение в локальную БД.
    pub(crate) fn spawn_after_send(
        &self,
        peer: PeerRef,
        ghost_mode: rg_settings::GhostMode,
        row: &MessageRow,
        mark_read_after: bool,
    ) {
        let eng = self.clone();
        let peer_id = peer_key(&peer);
        let msg_id = row.id;
        let text = Self::archive_text_from_row(row);
        let date = row.date;
        let outgoing = row.outgoing;
        let sender = row.sender.clone();
        tokio::spawn(async move {
            if mark_read_after && ghost::should_mark_read_after_action(&ghost_mode) {
                let _ = eng.client.mark_as_read(peer).await;
            }
            eng.send_offline_if_needed(&ghost_mode).await;
            if msg_id > 0 {
                eng.remember_full(peer_id, msg_id, &text, date, outgoing, &sender)
                    .await;
            }
        });
    }

    /// Преобразует сырые `tl::enums::Updates` (от `SendMessage` / `SendMedia`) в `MessageRow` за 0 лишних RPC.
    pub(crate) fn row_from_tl_updates(
        &self,
        peer: PeerRef,
        updates: &grammers_client::tl::enums::Updates,
        fallback_text: &str,
        fallback_silent: bool,
    ) -> MessageRow {
        use grammers_client::tl::enums::{Update as U, Updates as Us};
        let key = peer_key(&peer);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64);
        match updates {
            Us::UpdateShortSentMessage(s) => {
                let fake = grammers_client::tl::enums::Message::Message(grammers_client::tl::types::Message {
                    out: s.out,
                    mentioned: false,
                    media_unread: false,
                    silent: fallback_silent,
                    post: false,
                    from_scheduled: false,
                    legacy: false,
                    edit_hide: false,
                    pinned: false,
                    noforwards: false,
                    invert_media: false,
                    video_processing_pending: false,
                    paid_suggested_post_stars: false,
                    reactions: None,
                    id: s.id,
                    from_id: None,
                    from_rank: None,
                    from_boosts_applied: None,
                    peer_id: peer.id.into(),
                    saved_peer_id: None,
                    fwd_from: None,
                    via_bot_id: None,
                    reply_to: None,
                    date: s.date,
                    message: fallback_text.to_owned(),
                    media: s.media.clone(),
                    reply_markup: None,
                    entities: s.entities.clone(),
                    views: None,
                    forwards: None,
                    replies: None,
                    edit_date: None,
                    post_author: None,
                    grouped_id: None,
                    restriction_reason: None,
                    ttl_period: s.ttl_period,
                    quick_reply_shortcut_id: None,
                    offline: false,
                    via_business_bot_id: None,
                    guestchat_via_from: None,
                    effect: None,
                    factcheck: None,
                    report_delivery_until_date: None,
                    paid_message_stars: None,
                    paid_suggested_post_ton: false,
                    suggested_post: None,
                    schedule_repeat_period: None,
                    summary_from_language: None,
                    rich_message: None,
                });
                return self.tl_message_to_row(key, &fake);
            }
            Us::Updates(u) => {
                for upd in &u.updates {
                    match upd {
                        U::NewMessage(m) => return self.tl_message_to_row(key, &m.message),
                        U::NewChannelMessage(m) => return self.tl_message_to_row(key, &m.message),
                        _ => {}
                    }
                }
            }
            Us::Combined(u) => {
                for upd in &u.updates {
                    match upd {
                        U::NewMessage(m) => return self.tl_message_to_row(key, &m.message),
                        U::NewChannelMessage(m) => return self.tl_message_to_row(key, &m.message),
                        _ => {}
                    }
                }
            }
            _ => {}
        }
        MessageRow {
            silent: fallback_silent,
            ..MessageRow::plain(0, fallback_text.to_owned(), true, now)
        }
    }

    /// Преобразует сырой `tl::enums::Message` в `MessageRow` без сетевых запросов.
    fn tl_message_to_row(&self, key: i64, raw: &grammers_client::tl::enums::Message) -> MessageRow {
        use grammers_client::tl::enums::{Message as M, MessageMedia as MM};
        let mut rows = Vec::with_capacity(1);
        match raw {
            M::Message(x) => {
                let entities = x
                    .entities
                    .as_deref()
                    .map(|e| convert_entities(&x.message, e))
                    .unwrap_or_default();
                let (inline_buttons, reply_keyboard) = extract_reply_markup(x.reply_markup.as_ref());
                let reply_to = x.reply_to.as_ref().and_then(|rt| match rt {
                    grammers_client::tl::enums::MessageReplyHeader::Header(h) => {
                        h.reply_to_msg_id.map(|id| (id, String::new()))
                    }
                    _ => None,
                });
                let (replies_count, has_comments, recent_repliers) = match &x.replies {
                    Some(grammers_client::tl::enums::MessageReplies::Replies(r)) => {
                        (r.replies, r.comments, extract_recent_repliers(r.recent_repliers.as_deref()))
                    }
                    None => (0, false, Vec::new()),
                };
                let (rblocks, rsum) = x
                    .rich_message
                    .as_ref()
                    .map(crate::rich_msg::parse_rich_message)
                    .unwrap_or_default();
                let media = match x.media.as_ref() {
                    Some(MM::Dice(d)) => Some(MediaInfo::basic(
                        MediaKind::Dice {
                            emoticon: d.emoticon.clone(),
                            value: d.value,
                        },
                        format!("{} ({})", d.emoticon, d.value),
                        0,
                    )),
                    Some(MM::Document(d)) => {
                        let mut mime = String::new();
                        let mut is_sticker = false;
                        let mut alt = String::new();
                        let mut sticker_set = None;
                        if let Some(grammers_client::tl::enums::Document::Document(doc)) = &d.document {
                            mime = doc.mime_type.clone();
                            for a in &doc.attributes {
                                if let grammers_client::tl::enums::DocumentAttribute::Sticker(st) = a {
                                    is_sticker = true;
                                    alt = st.alt.clone();
                                    if let grammers_client::tl::enums::InputStickerSet::Id(id) = &st.stickerset {
                                        sticker_set = Some((id.id, id.access_hash));
                                    }
                                }
                            }
                        }
                        if is_sticker {
                            let kind = if mime == "image/webp" {
                                MediaKind::Sticker
                            } else {
                                MediaKind::AnimatedSticker
                            };
                            let lbl = if alt.is_empty() { "Стикер".to_owned() } else { format!("Стикер {alt}") };
                            let mut mi = MediaInfo::basic(kind, lbl, 0);
                            mi.mime = mime;
                            mi.sticker_set = sticker_set;
                            Some(mi)
                        } else {
                            None
                        }
                    }
                    _ => x.rich_message.as_ref().and_then(extract_rich_primary_media),
                };
                let text = if !x.message.is_empty() {
                    x.message.clone()
                } else {
                    rsum
                };
                rows.push(MessageRow {
                    entities,
                    reply_to,
                    media,
                    inline_buttons,
                    reply_keyboard,
                    rich_blocks: rblocks,
                    effect: x.effect,
                    todo: extract_todo_media(x.media.as_ref()),
                    edit_date: x.edit_date.map(i64::from),
                    views: x.views,
                    pinned: x.pinned,
                    grouped: x.grouped_id,
                    paid_reactions: extract_paid_reactions(x.reactions.as_ref()),
                    reaction_items: extract_reaction_pills(x.reactions.as_ref()),
                    silent: x.silent,
                    replies_count,
                    has_comments,
                    recent_repliers,
                    ..MessageRow::plain(x.id, text, x.out, i64::from(x.date))
                });
            }
            M::Service(s) => {
                let text = format_service_action(&s.action).unwrap_or_default();
                rows.push(MessageRow {
                    silent: s.silent,
                    ..MessageRow::plain(s.id, text, s.out, i64::from(s.date))
                });
            }
            M::Empty(_) => {}
        }
        let fallback = rows.first().cloned().unwrap_or_else(|| MessageRow::plain(0, String::new(), true, 0));
        self.finish(key, rows).into_iter().next().unwrap_or(fallback)
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
            let (fset, _) = FilterSet::build(&s.filters);
            if let Ok(mut cf) = self.compiled_filters.write() {
                *cf = Arc::new(fset);
            }
        }
    }

    fn ghost_schedule_time(g: &rg_settings::GhostMode) -> Option<std::time::SystemTime> {
        ghost::schedule_date(g)
            .map(|ts| std::time::UNIX_EPOCH + std::time::Duration::from_secs(ts.max(0) as u64))
    }

    async fn send_offline_if_needed(&self, g: &rg_settings::GhostMode) {
        if ghost::should_send_offline_after(g) {
            let _ = self
                .client
                .invoke(&grammers_client::tl::functions::account::UpdateStatus { offline: true })
                .await;
        }
    }

    /// Отметить чат прочитанным, если режим призрака и щит приватности это позволяют. `Ok(false)` — запрос погашен.
    pub async fn mark_read(&self, peer: PeerRef) -> Result<bool, grammers_client::InvocationError> {
        let cfg = self.settings();
        if cfg.privacy_shield.block_activity_telemetry {
            return Ok(false);
        }
        let id = grammers_tl_types_id::READ_HISTORY;
        if ghost::verdict(&cfg.ghost, id) == Verdict::Drop {
            return Ok(false);
        }
        self.apply_traffic_jitter().await;
        self.client.mark_as_read(peer).await?;
        Ok(true)
    }

    /// Фоновая подрезка старых живых сообщений в локальной БД (удалённые сохраняются полностью).
    pub async fn prune_old_live(&self, max_live_per_peer: usize) {
        let _ = self.store.prune_old_live(max_live_per_peer).await;
    }

    /// Случайная микрозадержка перед сетевым запросом (`traffic_jitter_ms`) для защиты от тайминг-корреляции трафика.
    pub async fn apply_traffic_jitter(&self) {
        let max_ms = u64::from(self.settings().privacy_shield.traffic_jitter_ms.min(600));
        if max_ms < 10 {
            return;
        }
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| u64::from(d.subsec_nanos()));
        let ms = 8 + (nanos % max_ms);
        tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
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

    /// Вызывает любой MTProto RPC с автоматическими повторами при временных ошибках сервера
    /// (500 `RPC_MCGET_FAIL` / `RPC_CALL_FAIL` / `INTERNAL` / `TIMEOUT`, 502/503, короткий `FLOOD_WAIT` и сбои сокета).
    pub async fn invoke_retry<R>(
        &self,
        tag: &str,
        req: &R,
    ) -> Result<R::Return, grammers_client::InvocationError>
    where
        R: grammers_client::tl::RemoteCall + Send + Sync,
    {
        use grammers_client::InvocationError;
        let delays = [250u64, 600, 1300, 2400];
        let mut attempt = 0usize;
        loop {
            match self.client.invoke(req).await {
                Ok(v) => return Ok(v),
                Err(e) => {
                    let retry_ms = match &e {
                        InvocationError::Rpc(r)
                            if r.code == 500
                                || r.code == 502
                                || r.code == 503
                                || r.code == -503
                                || r.name.contains("MCGET")
                                || r.name.contains("CALL_FAIL")
                                || r.name.contains("TIMEOUT")
                                || r.name.contains("INTERNAL") =>
                        {
                            delays.get(attempt).copied()
                        }
                        InvocationError::Rpc(r) if r.code == 420 => {
                            let secs = r.value.unwrap_or(0);
                            if secs <= 12 && attempt < 2 {
                                Some(u64::from(secs) * 1000 + 250)
                            } else {
                                None
                            }
                        }
                        InvocationError::Io(_) => delays.get(attempt).copied(),
                        _ => None,
                    };
                    if let Some(ms) = retry_ms {
                        crate::diag::warn(
                            "RPC",
                            &format!("{tag}: попытка {} не удалась ({e}), повтор через {ms} мс", attempt + 1),
                        );
                        tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
                        attempt += 1;
                    } else {
                        crate::diag::error("RPC", &format!("{tag}: ошибка {e}"));
                        return Err(e);
                    }
                }
            }
        }
    }

    /// Разбирает сырую страницу ответа `messages.getDialogs` (основную или архивную) в `DialogRow` за O(N).
    fn parse_raw_dialogs(
        res: grammers_client::tl::enums::messages::Dialogs,
        archived_flag: bool,
        my_id: Option<i64>,
    ) -> RawDialogsBatch {
        use grammers_client::tl;
        let (dialogs, messages, chats, users, more_hint) = match res {
            tl::enums::messages::Dialogs::Dialogs(d) => (d.dialogs, d.messages, d.chats, d.users, false),
            tl::enums::messages::Dialogs::Slice(d) => (d.dialogs, d.messages, d.chats, d.users, true),
            tl::enums::messages::Dialogs::NotModified(_) => {
                return (
                    Vec::new(),
                    false,
                    (0, 0, tl::enums::InputPeer::Empty),
                    Vec::new(),
                    Vec::new(),
                    Vec::new(),
                );
            }
        };
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64);

        // O(N) индексация вместо O(N^2) перебора users/chats/messages на каждый диалог.
        let mut users_by_id: HashMap<i64, &tl::enums::User> = HashMap::with_capacity(users.len());
        for u in &users {
            if let tl::enums::User::User(y) = u {
                users_by_id.insert(y.id, u);
            }
        }
        let mut chats_by_id: HashMap<i64, &tl::enums::Chat> = HashMap::with_capacity(chats.len());
        let mut channels_by_id: HashMap<i64, &tl::enums::Chat> = HashMap::with_capacity(chats.len());
        for c in &chats {
            match c {
                tl::enums::Chat::Chat(y) => {
                    chats_by_id.insert(y.id, c);
                }
                tl::enums::Chat::Channel(y) => {
                    channels_by_id.insert(y.id, c);
                }
                _ => {}
            }
        }
        let mut msg_by_key: HashMap<(grammers_session::types::PeerId, i32), &tl::enums::Message> =
            HashMap::with_capacity(messages.len());
        for m in &messages {
            match m {
                tl::enums::Message::Message(x) => {
                    msg_by_key.insert((grammers_session::types::PeerId::from(&x.peer_id), x.id), m);
                }
                tl::enums::Message::Service(s) => {
                    msg_by_key.insert((grammers_session::types::PeerId::from(&s.peer_id), s.id), m);
                }
                _ => {}
            }
        }

        let mut out = Vec::with_capacity(dialogs.len());
        let mut channel_pts = Vec::new();
        let mut last_off = (0i32, 0i32, tl::enums::InputPeer::Empty);
        for d in dialogs {
            let tl::enums::Dialog::Dialog(d) = d else { continue };
            let pid = grammers_session::types::PeerId::from(&d.peer);
            let mut kind = DialogKind::Group;
            let mut contact = false;
            let mut emoji_status = None;
            let mut verified = false;
            let mut premium = false;
            let mut is_self = false;
            let (name, peer) = match &d.peer {
                tl::enums::Peer::User(u) => {
                    let Some(&user) = users_by_id.get(&u.user_id) else { continue };
                    let name = match user {
                        tl::enums::User::User(y) => {
                            kind = if y.bot { DialogKind::Bot } else { DialogKind::User };
                            contact = y.contact;
                            is_self = y.is_self || my_id == Some(y.id);
                            emoji_status = emoji_status_id(y.emoji_status.as_ref());
                            verified = y.verified;
                            premium = y.premium;
                            format!(
                                "{} {}",
                                y.first_name.clone().unwrap_or_default(),
                                y.last_name.clone().unwrap_or_default()
                            )
                            .trim()
                            .to_owned()
                        }
                        tl::enums::User::Empty(_) => String::new(),
                    };
                    (name, grammers_session::types::PeerRef::from(user))
                }
                tl::enums::Peer::Chat(c) => {
                    let Some(&chat) = chats_by_id.get(&c.chat_id) else { continue };
                    let name = match chat {
                        tl::enums::Chat::Chat(y) => y.title.clone(),
                        _ => String::new(),
                    };
                    (name, grammers_session::types::PeerRef::from(chat))
                }
                tl::enums::Peer::Channel(c) => {
                    let Some(&chat) = channels_by_id.get(&c.channel_id) else { continue };
                    if let Some(pts) = d.pts {
                        channel_pts.push((c.channel_id, pts));
                    }
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
            let (last, date, msg_id_raw) = msg_by_key
                .get(&(pid, d.top_message))
                .and_then(|m| match m {
                    tl::enums::Message::Message(m) => {
                        let preview = if !m.message.trim().is_empty() {
                            m.message.chars().take(80).collect()
                        } else if let Some(rm) = m.rich_message.as_ref() {
                            let (_, sum) = crate::rich_msg::parse_rich_message(rm);
                            sum.lines().next().unwrap_or("Сообщение").chars().take(80).collect()
                        } else {
                            raw_media_label(m.media.as_ref())
                        };
                        Some((preview, i64::from(m.date), m.id))
                    }
                    tl::enums::Message::Service(s) => {
                        let txt = format_service_action(&s.action).unwrap_or_else(|| "Событие".to_owned());
                        Some((txt, i64::from(s.date), s.id))
                    }
                    _ => None,
                })
                .unwrap_or_default();
            if date > 0 {
                last_off = (date as i32, msg_id_raw, tl::enums::InputPeer::from(peer));
            }
            let (muted, notify_explicit) = parse_notify_settings(&d.notify_settings, now);
            let is_archived = archived_flag || d.folder_id == Some(1);
            out.push(DialogRow {
                peer,
                id: pid.bot_api_dialog_id_unchecked(),
                name: if is_self {
                    "Избранное".to_owned()
                } else if name.is_empty() {
                    "Без имени".to_owned()
                } else {
                    name
                },
                last,
                date,
                unread: d.unread_count.max(i32::from(d.unread_mark)),
                pinned: d.pinned,
                muted,
                notify_explicit,
                saved: is_self,
                archived: is_archived,
                kind,
                contact,
                read_out: d.read_outbox_max_id,
                emoji_status,
                verified,
                premium,
                unread_mentions: d.unread_mentions_count,
                unread_reactions: d.unread_reactions_count,
                read_inbox_max_id: d.read_inbox_max_id,
            });
        }
        (out, more_hint, last_off, users, chats, channel_pts)
    }

    async fn persist_dialogs_peers(
        &self,
        users: &[grammers_client::tl::enums::User],
        chats: &[grammers_client::tl::enums::Chat],
        channel_pts: &[(i64, i32)],
    ) {
        let Some(sess) = &self.session else { return };
        crate::cache_tl_peers(&**sess, users, chats).await;
        for &(ch_id, pts) in channel_pts {
            use grammers_session::Session;
            let _ = sess
                .set_update_state(grammers_session::types::UpdateState::Channel { id: ch_id, pts })
                .await;
        }
    }

    /// Чаты из папки «Архив» (folder_id = 1) — загружаются лениво только при открытии архива.
    pub async fn archived(&self, limit: usize) -> Result<Vec<DialogRow>, grammers_client::InvocationError> {
        use grammers_client::tl;
        let res = self
            .invoke_retry(
                "getDialogs(archive)",
                &tl::functions::messages::GetDialogs {
                    exclude_pinned: false,
                    folder_id: Some(1),
                    offset_date: 0,
                    offset_id: 0,
                    offset_peer: tl::enums::InputPeer::Empty,
                    limit: limit.min(100) as i32,
                    hash: 0,
                },
            )
            .await?;
        let (rows, _, _, users, chats, ch_pts) = Self::parse_raw_dialogs(res, true, None);
        self.persist_dialogs_peers(&users, &chats, &ch_pts).await;
        crate::diag::info("DLG", &format!("Загружен архив: {} чатов", rows.len()));
        Ok(rows)
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
        use grammers_client::tl;
        let mut out = Vec::new();
        let mut off_date = 0i32;
        let mut off_id = 0i32;
        let mut off_peer = tl::enums::InputPeer::Empty;
        let mut exclude_pinned = false;
        while out.len() < limit {
            let chunk_lim = (limit - out.len()).min(60) as i32;
            let res = self
                .invoke_retry(
                    "getDialogs(main)",
                    &tl::functions::messages::GetDialogs {
                        exclude_pinned,
                        folder_id: Some(0),
                        offset_date: off_date,
                        offset_id: off_id,
                        offset_peer: off_peer.clone(),
                        limit: chunk_lim,
                        hash: 0,
                    },
                )
                .await?;
            let (rows, more, (nd, ni, np), users, chats, ch_pts) = Self::parse_raw_dialogs(res, false, None);
            self.persist_dialogs_peers(&users, &chats, &ch_pts).await;
            let got = rows.len();
            for r in rows {
                if !out.iter().any(|x: &DialogRow| x.id == r.id) {
                    out.push(r);
                }
            }
            if !more || got == 0 || nd == 0 || (nd == off_date && ni == off_id) {
                break;
            }
            off_date = nd;
            off_id = ni;
            off_peer = np;
            exclude_pinned = true;
            tokio::time::sleep(std::time::Duration::from_millis(60)).await;
        }
        Ok(out)
    }

    /// Умная потоковая загрузка ТОЛЬКО основного списка чатов (`folder_id = Some(0)`, архив НЕ трогается!).
    /// Первые `first_batch` (например 5) чатов приходят мгновенно одним маленьким запросом,
    /// далее остальные (до `max_total`) догружаются мягкими порциями с автоповтором при RPC 500 (`RPC_MCGET_FAIL`).
    pub async fn dialogs_stream(
        &self,
        first_batch: usize,
        max_total: usize,
        tx: mpsc::Sender<(Vec<DialogRow>, bool)>,
    ) -> Result<(), grammers_client::InvocationError> {
        use grammers_client::tl;
        let t0 = std::time::Instant::now();
        let mut seen = std::collections::HashSet::<i64>::new();
        let mut off_date = 0i32;
        let mut off_id = 0i32;
        let mut off_peer = tl::enums::InputPeer::Empty;
        let mut exclude_pinned = false;
        let mut total = 0usize;
        let mut first = true;

        while total < max_total {
            let page_limit = if first {
                first_batch.clamp(5, 25)
            } else if total < 80 {
                30
            } else {
                50
            }
            .min(max_total - total) as i32;

            let res = match self
                .invoke_retry(
                    if first { "getDialogs(first5)" } else { "getDialogs(stream)" },
                    &tl::functions::messages::GetDialogs {
                        exclude_pinned,
                        folder_id: Some(0),
                        offset_date: off_date,
                        offset_id: off_id,
                        offset_peer: off_peer.clone(),
                        limit: page_limit,
                        hash: 0,
                    },
                )
                .await
            {
                Ok(r) => r,
                Err(e) => {
                    crate::diag::error("DLG", &format!("Остановка потока чатов после {total} шт.: {e}"));
                    if total == 0 {
                        let _ = tx.send((Vec::new(), true)).await;
                        return Err(e);
                    }
                    break;
                }
            };
            let (rows, more, (nd, ni, np), users, chats, ch_pts) = Self::parse_raw_dialogs(res, false, None);
            self.persist_dialogs_peers(&users, &chats, &ch_pts).await;
            let got = rows.len();
            let mut fresh = Vec::with_capacity(got);
            for r in rows {
                if seen.insert(r.id) {
                    fresh.push(r);
                }
            }
            total += fresh.len();
            if first {
                crate::diag::info(
                    "DLG",
                    &format!("Первые {} чатов загружены за {} мс (без архива)", fresh.len(), t0.elapsed().as_millis()),
                );
            }
            first = false;
            let done_now = !more || got == 0 || nd == 0 || (nd == off_date && ni == off_id) || total >= max_total;
            if tx.send((fresh, done_now)).await.is_err() || done_now {
                if done_now {
                    crate::diag::info(
                        "DLG",
                        &format!("Поток чатов завершён: всего {total} шт. за {} мс", t0.elapsed().as_millis()),
                    );
                }
                return Ok(());
            }
            off_date = nd;
            off_id = ni;
            off_peer = np;
            exclude_pinned = true;
            // Бережная пауза между фоновыми страницами защищает от 500 RPC_MCGET_FAIL и FLOOD_WAIT.
            tokio::time::sleep(std::time::Duration::from_millis(35)).await;
        }
        let _ = tx.send((Vec::new(), true)).await;
        Ok(())
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
                info.pinned_msg_id = full.pinned_msg_id;
                info.ttl_period = full.ttl_period;
                if let Some(tl::enums::BotInfo::Info(bi)) = &full.bot_info {
                    info.bot_description = bi.description.clone().unwrap_or_default();
                    if let Some(tl::enums::BotMenuButton::Button(mb)) = &bi.menu_button {
                        info.bot_menu_webapp = Some((mb.text.clone(), mb.url.clone()));
                    }
                    if let Some(cmds) = &bi.commands {
                        info.bot_commands = cmds
                            .iter()
                            .map(|c| {
                                let tl::enums::BotCommand::Command(cmd) = c;
                                (cmd.command.clone(), cmd.description.clone())
                            })
                            .collect();
                    }
                }
                if let Some(b) = &full.birthday {
                    info.birthday = fmt_birthday(b);
                }
                if let Some(bh) = &full.business_work_hours {
                    info.business_hours = format_business_hours(bh);
                }
                info.stargifts_count = full.stargifts_count.unwrap_or(0);
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
                    info.dc_id = match x.photo.as_ref() {
                        Some(tl::enums::UserProfilePhoto::Photo(p)) => Some(p.dc_id),
                        _ => None,
                    };
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
                    info.linked_chat_id = full.linked_chat_id;
                    info.pinned_msg_id = full.pinned_msg_id;
                    info.ttl_period = full.ttl_period;
                    info.paid_reactions_available = full.paid_reactions_available;
                    info.available_reactions = ChatReactionsPolicy::from_tl(full.available_reactions.as_ref());
                    info.slowmode_seconds = full.slowmode_seconds.unwrap_or(0);
                    info.slowmode_next_send_date = full.slowmode_next_send_date.unwrap_or(0);
                    info.has_welcome_messages = full.has_welcome_messages;
                    if let Some(tl::enums::ExportedChatInvite::ChatInviteExported(inv)) = full.exported_invite {
                        info.invite_link = inv.link;
                    }
                }
                if let Some(tl::enums::Chat::Channel(y)) = f.chats.iter().find(|x| matches!(x, tl::enums::Chat::Channel(y) if y.id == c.channel_id)) {
                    info.name = y.title.clone();
                    info.username = y.username.clone().unwrap_or_default();
                    info.broadcast = y.broadcast;
                    info.forum = y.forum;
                    info.emoji_status = emoji_status_id(y.emoji_status.as_ref());
                    info.verified = y.verified;
                    info.dc_id = match &y.photo {
                        tl::enums::ChatPhoto::Photo(p) => Some(p.dc_id),
                        tl::enums::ChatPhoto::Empty => None,
                    };
                }
            }
            tl::enums::InputPeer::Chat(c) => {
                let tl::enums::messages::ChatFull::Full(f) = self
                    .client
                    .invoke(&tl::functions::messages::GetFullChat { chat_id: c.chat_id })
                    .await?;
                if let tl::enums::ChatFull::Full(full) = f.full_chat {
                    info.about = full.about;
                    info.pinned_msg_id = full.pinned_msg_id;
                    info.ttl_period = full.ttl_period;
                    info.available_reactions = ChatReactionsPolicy::from_tl(full.available_reactions.as_ref());
                    if let Some(tl::enums::ChatParticipants::Participants(p)) = Some(full.participants) {
                        info.members = p.participants.len() as i32;
                    }
                }
                if let Some(tl::enums::Chat::Chat(y)) = f.chats.iter().find(|x| matches!(x, tl::enums::Chat::Chat(y) if y.id == c.chat_id)) {
                    info.name = y.title.clone();
                    info.dc_id = match &y.photo {
                        tl::enums::ChatPhoto::Photo(p) => Some(p.dc_id),
                        tl::enums::ChatPhoto::Empty => None,
                    };
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
        let mut msgs = Vec::with_capacity(limit);
        let mut cur_offset = before_id;
        let mut attempt = 0usize;
        let delays = [250u64, 600, 1200];
        loop {
            let rem = limit.saturating_sub(msgs.len());
            if rem == 0 {
                break;
            }
            let mut it = self.client.iter_messages(peer).offset_id(cur_offset).limit(rem);
            let mut step_err = None;
            while msgs.len() < limit {
                match it.next().await {
                    Ok(Some(m)) => {
                        cur_offset = m.id();
                        msgs.push(m);
                    }
                    Ok(None) => {
                        step_err = None;
                        break;
                    }
                    Err(e) => {
                        step_err = Some(e);
                        break;
                    }
                }
            }
            match step_err {
                None => break,
                Some(e) if attempt < delays.len() => {
                    use grammers_client::InvocationError;
                    let wait_ms = match &e {
                        InvocationError::Rpc(r) if r.name == "FLOOD_WAIT" && r.value.unwrap_or(999) <= 10 => {
                            u64::from(r.value.unwrap_or(1)) * 1000 + 100
                        }
                        InvocationError::Rpc(r)
                            if r.code == 500
                                || r.code == 502
                                || r.code == 503
                                || r.code == -503
                                || r.name.contains("MCGET")
                                || r.name.contains("CALL_FAIL")
                                || r.name.contains("TIMEOUT")
                                || r.name.contains("INTERNAL") =>
                        {
                            delays[attempt]
                        }
                        InvocationError::Io(_) => delays[attempt],
                        _ => return Err(e),
                    };
                    attempt += 1;
                    tokio::time::sleep(std::time::Duration::from_millis(wait_ms)).await;
                }
                Some(e) => return Err(e),
            }
        }
        // Загрузка сообщений и быстрое индексированное слияние; запись в БД уходит в фон.
        let counts = self.store.edit_counts(key).await.unwrap_or_default();
        let cfg = self.settings();
        let is_bot_chat = msgs.iter().any(|m| {
            !m.outgoing()
                && matches!(m.sender(), Some(grammers_client::peer::Peer::User(u)) if u.is_bot())
        });
        let mut rows: Vec<MessageRow> = msgs
            .iter()
            .map(|m| Self::row(key, m, counts.get(&m.id()).copied().unwrap_or(0)))
            .collect();
        let save_ok = cfg.ayu.save_deleted_messages && (!is_bot_chat || cfg.ayu.save_for_bots);
        let (win_min, win_max) = if before_id == 0 {
            let min_id = rows.iter().map(|r| r.id).min().unwrap_or(1);
            let wmin = if rows.is_empty() || rows.len() < limit {
                1
            } else {
                min_id
            };
            (wmin, i32::MAX)
        } else {
            let min_id = rows.iter().map(|r| r.id).min().unwrap_or(before_id);
            let wmin = if rows.is_empty() || rows.len() < limit {
                1
            } else {
                min_id
            };
            (wmin, before_id.saturating_sub(1))
        };
        if save_ok {
            let mut present_set: std::collections::HashSet<i32> =
                rows.iter().filter(|r| r.id > 0).map(|r| r.id).collect();
            let mut missing_ids = Vec::new();
            if win_min <= win_max
                && let Ok((saved_del, live_in_range)) =
                    self.store.scan_range_for_merge(key, win_min, win_max).await
            {
                for live in live_in_range {
                    if live.msg_id > 0 && present_set.insert(live.msg_id) {
                        missing_ids.push(live.msg_id);
                        let mut r = MessageRow::plain(live.msg_id, live.text, live.outgoing, live.date);
                        r.sender = live.sender;
                        r.deleted = true;
                        r.edits = counts.get(&live.msg_id).copied().unwrap_or(0);
                        rows.push(r);
                    }
                }
                for s in saved_del {
                    if present_set.insert(s.msg_id) {
                        let mut r = MessageRow::plain(s.msg_id, s.text, s.outgoing, s.date);
                        r.sender = s.sender;
                        r.deleted = true;
                        r.edits = counts.get(&s.msg_id).copied().unwrap_or(0);
                        rows.push(r);
                    }
                }
            }
            let batch: Vec<(i32, String, i64, bool, String)> = rows
                .iter()
                .filter(|r| r.id > 0 && !r.deleted)
                .map(|r| {
                    (
                        r.id,
                        Self::archive_text_from_row(r),
                        r.date,
                        r.outgoing,
                        r.sender.clone(),
                    )
                })
                .collect();
            if !missing_ids.is_empty() || !batch.is_empty() {
                let store_bg = std::sync::Arc::clone(&self.store);
                tokio::spawn(async move {
                    if !missing_ids.is_empty() {
                        let _ = store_bg.mark_deleted(key, &missing_ids).await;
                    }
                    if !batch.is_empty() {
                        let _ = store_bg.remember_many_full(key, &batch).await;
                    }
                });
            }
        }
        rows.sort_by_key(|r| (r.date, r.id));
        Ok(self.finish(key, rows))
    }

    /// Фильтры и «зальго» из настроек; подписи ответов по сообщениям выборки.
    fn finish(&self, key: i64, mut rows: Vec<MessageRow>) -> Vec<MessageRow> {
        let cfg = self.settings();
        if cfg.ayu.filters_enabled
            && let Ok(cf) = self.compiled_filters.read()
            && !cf.is_empty()
        {
            let allow_global = cfg.ayu.filters_enabled_in_chats;
            rows.retain(|r| !cf.hides_with(key, &r.text, allow_global));
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
        let (inline_buttons, reply_keyboard, paid_reactions, service_text, rich_blocks, rich_summary, rich_media, effect, todo, silent, replies_count, has_comments, recent_repliers) = match &m.raw {
            grammers_client::tl::enums::Message::Message(x) => {
                let (ib, rk) = extract_reply_markup(x.reply_markup.as_ref());
                let (rblocks, rsum) = x
                    .rich_message
                    .as_ref()
                    .map(crate::rich_msg::parse_rich_message)
                    .unwrap_or_default();
                let rmedia = x.rich_message.as_ref().and_then(extract_rich_primary_media);
                let todo = extract_todo_media(x.media.as_ref());
                let (rep_cnt, has_c, repliers) = match &x.replies {
                    Some(grammers_client::tl::enums::MessageReplies::Replies(r)) => {
                        (r.replies, r.comments, extract_recent_repliers(r.recent_repliers.as_deref()))
                    }
                    None => (0, false, Vec::new()),
                };
                (
                    ib,
                    rk,
                    extract_paid_reactions(x.reactions.as_ref()),
                    None,
                    rblocks,
                    rsum,
                    rmedia,
                    x.effect,
                    todo,
                    x.silent,
                    rep_cnt,
                    has_c,
                    repliers,
                )
            }
            grammers_client::tl::enums::Message::Service(s) => (
                Vec::new(),
                Vec::new(),
                0,
                format_service_action(&s.action),
                Vec::new(),
                String::new(),
                None,
                None,
                None,
                s.silent,
                0,
                false,
                Vec::new(),
            ),
            _ => (
                Vec::new(),
                Vec::new(),
                0,
                None,
                Vec::new(),
                String::new(),
                None,
                None,
                None,
                false,
                0,
                false,
                Vec::new(),
            ),
        };
        let text = if !m.text().is_empty() {
            m.text().to_owned()
        } else if !rich_summary.is_empty() {
            rich_summary
        } else {
            service_text.unwrap_or_default()
        };
        let media = m.media().as_ref().map(media::describe).or(rich_media);
        MessageRow {
            id: m.id(),
            text,
            outgoing: m.outgoing(),
            date: m.date().as_second(),
            deleted: false,
            edits,
            media,
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
            paid_reactions,
            reaction_items: match &m.raw {
                grammers_client::tl::enums::Message::Message(x) => extract_reaction_pills(x.reactions.as_ref()),
                _ => Vec::new(),
            },
            sender_id: m
                .sender_id()
                .map_or(0, |p| p.bot_api_dialog_id_unchecked()),
            entities: m
                .fmt_entities()
                .map(|e| convert_entities(m.text(), e))
                .unwrap_or_default(),
            grouped: m.grouped_id(),
            inline_buttons,
            reply_keyboard,
            rich_blocks,
            effect,
            todo,
            silent,
            replies_count,
            has_comments,
            recent_repliers,
        }
    }

    /// Загрузить окно истории вокруг указанного сообщения (для переходов к закрепам, ответам и поиску).
    pub async fn history_around(
        &self,
        peer: PeerRef,
        target_msg_id: i32,
        limit: usize,
    ) -> Result<Vec<MessageRow>, grammers_client::InvocationError> {
        use grammers_client::tl;
        let key = peer_key(&peer);
        let lim = limit.clamp(10, 80) as i32;
        let res = self
            .invoke_retry(
                "messages.getHistory(around)",
                &tl::functions::messages::GetHistory {
                    peer: peer.into(),
                    offset_id: target_msg_id.saturating_add(1),
                    offset_date: 0,
                    add_offset: -(lim / 2),
                    limit: lim,
                    max_id: 0,
                    min_id: 0,
                    hash: 0,
                },
            )
            .await?;
        let mut rows = tl_messages_to_rows(&res);
        rows.sort_by_key(|r| (r.date, r.id));
        Ok(self.finish(key, rows))
    }

    /// Поиск по тексту и типу вложений внутри чата (`messages.search`).
    pub async fn search_filtered(
        &self,
        peer: PeerRef,
        query: &str,
        filter: ChatSearchFilter,
        limit: usize,
    ) -> Result<Vec<MessageRow>, String> {
        if filter == ChatSearchFilter::All && !query.trim().is_empty() {
            return self
                .search(peer, query, limit)
                .await
                .map_err(|e| format!("Поиск: {e}"));
        }
        use grammers_client::tl;
        let res = self
            .invoke_retry(
                "messages.search(filtered)",
                &tl::functions::messages::Search {
                    peer: peer.into(),
                    q: query.to_owned(),
                    from_id: None,
                    saved_peer_id: None,
                    saved_reaction: None,
                    top_msg_id: None,
                    filter: filter.to_tl(),
                    min_date: 0,
                    max_date: 0,
                    offset_id: 0,
                    add_offset: 0,
                    limit: limit.clamp(1, 100) as i32,
                    max_id: 0,
                    min_id: 0,
                    hash: 0,
                },
            )
            .await
            .map_err(|e| format!("Поиск: {e}"))?;
        let mut rows = tl_messages_to_rows(&res);
        rows.sort_by_key(|r| (r.date, r.id));
        Ok(rows)
    }

    /// Получить или разрешить привязанную группу обсуждения канала (`channels.getFullChannel` -> `linked_chat_id`).
    pub async fn resolve_linked_chat(
        &self,
        channel_peer: PeerRef,
        linked_chat_id: i64,
    ) -> Result<DialogRow, String> {
        use grammers_client::tl;
        let tl::enums::InputPeer::Channel(ch) = tl::enums::InputPeer::from(channel_peer) else {
            return Err("Обсуждение доступно только для каналов".to_owned());
        };
        let tl::enums::messages::ChatFull::Full(full) = self
            .invoke_retry(
                "channels.getFullChannel(linked)",
                &tl::functions::channels::GetFullChannel {
                    channel: tl::enums::InputChannel::Channel(tl::types::InputChannel {
                        channel_id: ch.channel_id,
                        access_hash: ch.access_hash,
                    }),
                },
            )
            .await
            .map_err(|e| format!("Открытие группы обсуждения: {e}"))?;
        if let Some(sess) = &self.session {
            crate::cache_tl_peers(&**sess, &full.users, &full.chats).await;
        }
        let target_raw_id = (-linked_chat_id).saturating_sub(1_000_000_000_000);
        for c in &full.chats {
            if let tl::enums::Chat::Channel(chan) = c
                && (chan.id == linked_chat_id || chan.id == target_raw_id)
            {
                let peer = grammers_session::types::PeerRef::from(c);
                let did = peer.id.bot_api_dialog_id_unchecked();
                return Ok(DialogRow {
                    peer,
                    id: did,
                    name: chan.title.clone(),
                    last: String::new(),
                    date: 0,
                    unread: 0,
                    pinned: false,
                    muted: false,
                    notify_explicit: None,
                    saved: false,
                    archived: false,
                    kind: if chan.broadcast { DialogKind::Channel } else { DialogKind::Group },
                    contact: false,
                    read_out: 0,
                    emoji_status: emoji_status_id(chan.emoji_status.as_ref()),
                    verified: chan.verified,
                    premium: false,
                    unread_mentions: 0,
                    unread_reactions: 0,
                    read_inbox_max_id: 0,
                });
            }
        }
        Err("У этого канала нет доступной группы обсуждения".to_owned())
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

    /// Отправить текст, при `reply_to` — ответом на сообщение; сразу возвращает `MessageRow` без лишнего `history()`.
    pub async fn send_reply(
        &self,
        peer: PeerRef,
        text: &str,
        reply_to: Option<i32>,
    ) -> Result<MessageRow, grammers_client::InvocationError> {
        let ghost_mode = self.settings().ghost;
        let msg = grammers_client::message::InputMessage::new()
            .text(text)
            .reply_to(reply_to)
            .silent(ghost_mode.should_send_without_sound())
            .schedule_date(Self::ghost_schedule_time(&ghost_mode));
        let sent = self.client.send_message(peer, msg).await?;
        let key = peer_key(&peer);
        let row = self
            .finish(key, vec![Self::row(key, &sent, 0)])
            .into_iter()
            .next()
            .unwrap_or_else(|| Self::row(key, &sent, 0));
        self.remember_row_sync(peer, &row).await;
        self.spawn_after_send(peer, ghost_mode, &row, true);
        Ok(row)
    }

    /// Отправить файл (картинки уходят как фото, остальное — документом).
    /// При включённом `privacy_shield.anonymize_filenames` локальное имя файла заменяется нейтральным `file_<id>.<ext>`.
    pub async fn send_file(
        &self,
        peer: PeerRef,
        path: &Path,
        caption: &str,
        reply_to: Option<i32>,
    ) -> Result<(), BoxErr> {
        self.send_media_with_options(peer, path, caption, reply_to, SendMediaOptions::default())
            .await
    }

    /// Отправить фото/видео/файл с параметрами редактора: сжатие, спойлер и таймер самоуничтожения (`ttl_seconds`).
    pub async fn send_media_with_options(
        &self,
        peer: PeerRef,
        path: &Path,
        caption: &str,
        reply_to: Option<i32>,
        opts: SendMediaOptions,
    ) -> Result<(), BoxErr> {
        use grammers_client::tl;
        let cfg = self.settings();
        let ghost_mode = cfg.ghost;
        self.apply_traffic_jitter().await;
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_lowercase)
            .unwrap_or_default();
        let orig_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("file.bin")
            .to_owned();
        let send_name = if cfg.privacy_shield.anonymize_filenames {
            if ext.is_empty() {
                "file.bin".to_owned()
            } else {
                format!("file.{ext}")
            }
        } else {
            orig_name
        };
        let up = if cfg.privacy_shield.anonymize_filenames {
            let mut f = tokio::fs::File::open(path).await?;
            let len = f.metadata().await?.len() as usize;
            self.client.upload_stream(&mut f, len, send_name.clone()).await?
        } else {
            self.client.upload_file(path).await?
        };
        let base = grammers_client::message::InputMessage::new()
            .text(caption)
            .reply_to(reply_to)
            .silent(ghost_mode.should_send_without_sound())
            .schedule_date(Self::ghost_schedule_time(&ghost_mode));
        let is_photo = matches!(ext.as_str(), "jpg" | "jpeg" | "png" | "webp");
        let msg = if is_photo && opts.compress {
            if !opts.spoiler && opts.ttl_seconds.is_none() {
                base.photo(up)
            } else {
                base.media(tl::types::InputMediaUploadedPhoto {
                    spoiler: opts.spoiler,
                    file: up.raw,
                    stickers: None,
                    ttl_seconds: opts.ttl_seconds,
                    live_photo: false,
                    video: None,
                })
            }
        } else if !opts.spoiler && opts.ttl_seconds.is_none() && (!is_photo || opts.compress) {
            base.file(up)
        } else {
            let mime_type = match ext.as_str() {
                "jpg" | "jpeg" => "image/jpeg",
                "png" => "image/png",
                "webp" => "image/webp",
                "mp4" => "video/mp4",
                "webm" => "video/webm",
                "mov" => "video/quicktime",
                "gif" => "image/gif",
                _ => "application/octet-stream",
            }
            .to_owned();
            let attrs = vec![tl::enums::DocumentAttribute::Filename(
                tl::types::DocumentAttributeFilename {
                    file_name: send_name,
                },
            )];
            base.media(tl::types::InputMediaUploadedDocument {
                nosound_video: false,
                force_file: is_photo && !opts.compress,
                spoiler: opts.spoiler,
                file: up.raw,
                thumb: None,
                mime_type,
                attributes: attrs,
                stickers: None,
                video_cover: None,
                video_timestamp: None,
                ttl_seconds: opts.ttl_seconds,
            })
        };
        let sent = self.client.send_message(peer, msg).await?;
        let key = peer_key(&peer);
        let row = Self::row(key, &sent, 0);
        self.remember_row_sync(peer, &row).await;
        self.send_offline_if_needed(&ghost_mode).await;
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

    /// Удалить сообщения. Копия в локальном хранилище остаётся, даже если удалил сам пользователь.
    pub async fn delete(
        &self,
        peer: PeerRef,
        ids: &[i32],
    ) -> Result<usize, grammers_client::InvocationError> {
        if self.settings().ayu.save_deleted_messages && !ids.is_empty() {
            let _ = self.store.mark_deleted(peer_key(&peer), ids).await;
        }
        self.client.delete_messages(peer, ids).await
    }

    /// Удалить сообщения на сервере с гарантированным сохранением текста и метаданных у себя (self-delete AyuGram).
    pub async fn delete_with_backup(
        &self,
        peer: PeerRef,
        backups: &[(i32, String, i64, bool, String)],
    ) -> Result<usize, grammers_client::InvocationError> {
        let key = peer_key(&peer);
        let ids: Vec<i32> = backups.iter().map(|b| b.0).filter(|&id| id > 0).collect();
        if self.settings().ayu.save_deleted_messages {
            for (id, text, date, outgoing, sender) in backups {
                if *id > 0 {
                    let _ = self
                        .store
                        .upsert_deleted(key, *id, text, *date, *outgoing, sender)
                        .await;
                }
            }
        }
        self.client.delete_messages(peer, &ids).await
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

    /// Пакетная пересылка нескольких сообщений (портом логики `ayu_forward.cpp`: сохранение порядка или снятие автора).
    pub async fn forward_batch(
        &self,
        from: PeerRef,
        to: PeerRef,
        ids: &[i32],
        no_author: bool,
    ) -> Result<(), grammers_client::InvocationError> {
        if ids.is_empty() {
            return Ok(());
        }
        let mut sorted = ids.to_vec();
        sorted.sort_unstable();
        if !no_author {
            self.client.forward_messages(to, &sorted, from).await?;
        } else {
            for &id in &sorted {
                self.forward_no_author(from, to, id).await?;
            }
        }
        let ghost_mode = self.settings().ghost;
        self.send_offline_if_needed(&ghost_mode).await;
        Ok(())
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
        let ghost_mode = self.settings().ghost;
        self.send_offline_if_needed(&ghost_mode).await;
        Ok(())
    }

    /// Переключить реакцию: если `remove = true`, снимает выбранную реакцию пользователя.
    pub async fn toggle_reaction(
        &self,
        peer: PeerRef,
        id: i32,
        emoji: &str,
        remove: bool,
    ) -> Result<(), grammers_client::InvocationError> {
        self.react(peer, id, if remove { "" } else { emoji }).await
    }

    /// Проверяет, разрешена ли отправка статуса набора текста с учётом режима призрака и Privacy Shield.
    pub fn typing_allowed(cfg: &Settings) -> bool {
        !cfg.ghost.active && cfg.ghost.send_upload_progress && !cfg.privacy_shield.block_activity_telemetry
    }

    /// Отправляет или отменяет статус «печатает…» (`messages.setTyping`) только если режим призрака не блокирует его.
    pub async fn set_typing(
        &self,
        peer: PeerRef,
        typing: bool,
    ) -> Result<bool, grammers_client::InvocationError> {
        use grammers_client::tl;
        if !Self::typing_allowed(&self.settings()) {
            return Ok(false);
        }
        let action = if typing {
            tl::enums::SendMessageAction::SendMessageTypingAction
        } else {
            tl::enums::SendMessageAction::SendMessageCancelAction
        };
        let _ = self
            .client
            .invoke(&tl::functions::messages::SetTyping {
                peer: peer.into(),
                top_msg_id: None,
                action,
            })
            .await?;
        Ok(true)
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
        if let Some(media) = m.media() {
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
            self.maybe_prune_preview_cache();
            return Ok(Some(file));
        }
        // Layer 233 RichMessage: встроенные фото/GIF/видео-превью ботов (например eroot).
        if let Some(rm) = m.rich_message() {
            let grammers_client::tl::enums::RichMessage::Message(rm) = rm;
            if let Some(bytes) = self.download_rich_preview_bytes(rm).await? {
                let _ = std::fs::create_dir_all(&self.dirs.cache);
                std::fs::write(&file, bytes)?;
                self.maybe_prune_preview_cache();
                return Ok(Some(file));
            }
        }
        Ok(None)
    }

    fn maybe_prune_preview_cache(&self) {
        let n = self.preview_writes.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if n.is_multiple_of(40) {
            let dir = self.dirs.cache.clone();
            tokio::task::spawn_blocking(move || media::prune_cache(&dir, CACHE_LIMIT));
        }
    }

    /// Скачивает превью (фото или обложку GIF/видео) первого встроенного медиа из `RichMessage`.
    async fn download_rich_preview_bytes(
        &self,
        rm: &grammers_client::tl::types::RichMessage,
    ) -> Result<Option<Vec<u8>>, BoxErr> {
        use grammers_client::tl;
        let loc_dc: Option<(tl::enums::InputFileLocation, Option<i32>)> = rm
            .photos
            .iter()
            .find_map(|p| {
                let tl::enums::Photo::Photo(ph) = p else { return None };
                let sz = ph
                    .sizes
                    .iter()
                    .rev()
                    .find_map(|s| match s {
                        tl::enums::PhotoSize::Size(x) => Some(x.r#type.clone()),
                        tl::enums::PhotoSize::Progressive(x) => Some(x.r#type.clone()),
                        _ => None,
                    })
                    .unwrap_or_else(|| "x".to_owned());
                Some((
                    tl::enums::InputFileLocation::InputPhotoFileLocation(tl::types::InputPhotoFileLocation {
                        id: ph.id,
                        access_hash: ph.access_hash,
                        file_reference: ph.file_reference.clone(),
                        thumb_size: sz,
                    }),
                    Some(ph.dc_id),
                ))
            })
            .or_else(|| {
                rm.documents.iter().find_map(|d| {
                    let tl::enums::Document::Document(doc) = d else { return None };
                    let thumb = doc
                        .thumbs
                        .as_ref()
                        .and_then(|ts| {
                            ts.iter().rev().find_map(|s| match s {
                                tl::enums::PhotoSize::Size(x) => Some(x.r#type.clone()),
                                tl::enums::PhotoSize::Progressive(x) => Some(x.r#type.clone()),
                                _ => None,
                            })
                        })
                        .unwrap_or_default();
                    Some((
                        tl::enums::InputFileLocation::InputDocumentFileLocation(
                            tl::types::InputDocumentFileLocation {
                                id: doc.id,
                                access_hash: doc.access_hash,
                                file_reference: doc.file_reference.clone(),
                                thumb_size: thumb,
                            },
                        ),
                        Some(doc.dc_id),
                    ))
                })
            });
        let Some((loc, dc_id)) = loc_dc else {
            return Ok(None);
        };
        let dl = RawLoc { loc, dc_id };
        let mut it = self.client.iter_download(&dl);
        let mut out = Vec::new();
        while let Some(chunk) = it.next().await? {
            out.extend_from_slice(&chunk);
            if out.len() > 6 * 1024 * 1024 {
                break;
            }
        }
        Ok((!out.is_empty()).then_some(out))
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

    /// Пакетная проверка кэша медиа одним обходом `cache/media/` (`O(files)` вместо `O(ids * files)`).
    pub fn cached_media_batch(&self, peer: PeerRef, msg_ids: &[i32]) -> Vec<i32> {
        if msg_ids.is_empty() {
            return Vec::new();
        }
        let wanted: std::collections::HashSet<i32> = msg_ids.iter().copied().collect();
        let peer_prefix = format!("{}_", peer_key(&peer));
        let Ok(entries) = std::fs::read_dir(self.dirs.cache.join("media")) else {
            return Vec::new();
        };
        let mut hit = Vec::new();
        for entry in entries.flatten() {
            let fname = entry.file_name();
            let Some(name) = fname.to_str() else { continue };
            if name.ends_with(".part") {
                continue;
            }
            let Some(rest) = name.strip_prefix(&peer_prefix) else { continue };
            let Some((id_str, _)) = rest.split_once('.') else { continue };
            if let Ok(mid) = id_str.parse::<i32>()
                && wanted.contains(&mid)
            {
                hit.push(mid);
            }
        }
        hit
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
        let file = std::fs::File::create(&part)?;
        let mut writer = std::io::BufWriter::with_capacity(64 * 1024, file);
        let mut done = 0u64;
        let mut streamed = false;
        let stream_at = (384 * 1024u64).min(total);
        let mut it = self.client.iter_download(&media);
        progress(0, total);
        loop {
            if cancel.load(Ordering::Relaxed) {
                drop(writer);
                let _ = std::fs::remove_file(&part);
                return Ok(Fetched::Cancelled);
            }
            match it.next().await {
                Ok(Some(chunk)) => {
                    writer.write_all(&chunk)?;
                    done += chunk.len() as u64;
                    progress(done.min(total), total);
                    if !to_downloads && !streamed && done >= stream_at && done < total {
                        writer.flush()?;
                        streamed = true;
                        on_stream(&part, &dest, total, ext);
                    }
                }
                Ok(None) => break,
                Err(e) => {
                    drop(writer);
                    let _ = std::fs::remove_file(&part);
                    return Err(Box::new(e));
                }
            }
        }
        writer.flush()?;
        drop(writer);
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
        let msg = grammers_client::message::InputMessage::new()
            .text(text)
            .silent(silent)
            .schedule_date(Self::ghost_schedule_time(&ghost_mode));
        self.client.send_message(peer, msg).await?;
        if ghost::should_mark_read_after_action(&ghost_mode) {
            // Пользователь ответил — отметка о прочтении ожидаема.
            self.client.mark_as_read(peer).await?;
        }
        self.send_offline_if_needed(&ghost_mode).await;
        Ok(())
    }

    pub(crate) async fn remember_full(
        &self,
        peer: i64,
        id: i32,
        text: &str,
        date: i64,
        outgoing: bool,
        sender: &str,
    ) {
        if id > 0 && self.settings().ayu.save_deleted_messages {
            let _ = self
                .store
                .remember_full(peer, id, text, date, outgoing, sender)
                .await;
        }
    }

    /// Полный журнал сохранённых удалённых и изменённых сообщений чата (`ayu/ui/message_history`).
    pub async fn audit_peer(&self, peer: &PeerRef) -> Vec<rg_ayu::store::SavedAudit> {
        self.store
            .audit_peer(peer_key(peer))
            .await
            .unwrap_or_default()
    }

    /// Глобальный поиск по всем сохранённым удалённым сообщениям во всех чатах.
    pub async fn global_deleted(&self, query: &str, limit: usize) -> Vec<rg_ayu::store::Saved> {
        self.store
            .search_deleted_global(query, limit)
            .await
            .unwrap_or_default()
    }

    /// Статистика локальной базы (всего сообщений, удалённых, правок).
    pub async fn store_stats(&self) -> (usize, usize, usize) {
        self.store.stats().await.unwrap_or((0, 0, 0))
    }

    /// Окончательно удалить уже удалённое сообщение только у себя из локальной БД.
    pub async fn purge_local_deleted(&self, peer: &PeerRef, msg_id: i32) {
        let _ = self.store.purge_deleted(peer_key(peer), msg_id).await;
    }

    /// Окончательно убрать все сохранённые удалённые сообщения чата из локальной БД.
    pub async fn purge_all_local_deleted(&self, peer: &PeerRef) {
        let _ = self.store.purge_all_deleted(peer_key(peer)).await;
    }


    /// Прежние версии правок сообщения.
    pub async fn edit_history(&self, peer: &PeerRef, id: i32) -> Vec<String> {
        self.store
            .history(peer_key(peer), id)
            .await
            .unwrap_or_default()
    }

    /// Поток обновлений в реальном времени:
    /// 1) сразу доставляет live-событие в UI ДО фоновой записи в SQLite (0 мс задержки!),
    /// 2) гарантирует наличие `updates.getState` в `UpdatesReceiver`,
    /// 3) не умирает на временных RPC 500 / сетевых сбоях.
    pub async fn run_updates(
        &self,
        updates: mpsc::Receiver<UpdatesLike>,
        mut on_event: impl FnMut(&Update) + Send,
    ) -> Result<(), BoxErr> {
        let mut stream = self
            .client
            .stream_updates(updates, UpdatesConfiguration { catch_up: true })
            .await?;
        crate::diag::info("UPD", "Поток live-обновлений запущен (0-ms dispatch)");
        // Последовательный потребитель сохраняет строгий порядок NewMessage -> Edit/Delete в SQLite.
        let (apply_tx, mut apply_rx) = mpsc::unbounded_channel::<Update>();
        let eng_seq = self.clone();
        tokio::spawn(async move {
            while let Some(upd) = apply_rx.recv().await {
                eng_seq.apply(&upd).await;
            }
        });
        let mut consecutive_err = 0u32;
        loop {
            match stream.next().await {
                Ok(upd) => {
                    consecutive_err = 0;
                    on_event(&upd);
                    let _ = apply_tx.send(upd);
                }
                Err(e) => {
                    consecutive_err += 1;
                    let es = e.to_string();
                    crate::diag::warn("UPD", &format!("Сбой потока обновлений (#{consecutive_err}): {es}"));
                    if es.contains("Dropped") || es.contains("closed") || consecutive_err > 20 {
                        crate::diag::error("UPD", "Канал обновлений закрыт — выход из цикла");
                        return Err(Box::new(e));
                    }
                    let wait_ms = (300u64 * u64::from(consecutive_err)).min(3000);
                    tokio::time::sleep(std::time::Duration::from_millis(wait_ms)).await;
                }
            }
        }
    }

    /// Превращает обновление в событие для интерфейса (без сетевых запросов).
    pub fn live(&self, upd: &Update) -> Option<Live> {
        let peer_bot_id = |p: &grammers_client::tl::enums::Peer| -> i64 {
            use grammers_client::tl::enums::Peer;
            match p {
                Peer::User(u) => u.user_id,
                Peer::Chat(c) => -c.chat_id,
                Peer::Channel(c) => -1_000_000_000_000 - c.channel_id,
            }
        };
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
                use grammers_client::tl::enums::{
                    DraftMessage as DM, InputReplyTo as IRT, MessageReactions as MR, ReactionCount as RC,
                    SendMessageAction as A, Update as U,
                };
                match &r.raw {
                    U::ReadHistoryOutbox(x) => Some(Live::ReadOutbox(peer_bot_id(&x.peer), x.max_id)),
                    U::ReadHistoryInbox(x) => Some(Live::ReadInbox(peer_bot_id(&x.peer), x.max_id)),
                    U::ReadChannelInbox(x) => {
                        Some(Live::ReadInbox(-1_000_000_000_000 - x.channel_id, x.max_id))
                    }
                    U::DraftMessage(x) => {
                        let key = peer_bot_id(&x.peer);
                        match &x.draft {
                            DM::Message(d) => {
                                let reply_to = d.reply_to.as_ref().and_then(|rt| match rt {
                                    IRT::Message(m) => Some(m.reply_to_msg_id),
                                    _ => None,
                                });
                                Some(Live::Draft(key, d.message.clone(), reply_to))
                            }
                            DM::Empty(_) => Some(Live::Draft(key, String::new(), None)),
                        }
                    }
                    U::MessageReactions(x) => {
                        let key = peer_bot_id(&x.peer);
                        let MR::Reactions(rx) = &x.reactions;
                        let total: i32 = rx
                            .results
                            .iter()
                            .map(|c| {
                                let RC::Count(cnt) = c;
                                cnt.count
                            })
                            .sum();
                        Some(Live::MessageReactions(key, x.msg_id, total))
                    }
                    U::UserTyping(x) => Some(Live::Typing(x.user_id, typing_label(&x.action).to_owned())),
                    U::ChatUserTyping(x) => Some(Live::Typing(-x.chat_id, typing_label(&x.action).to_owned())),
                    U::ChannelUserTyping(x) => Some(Live::Typing(
                        -1_000_000_000_000 - x.channel_id,
                        match &x.action {
                            A::SendMessageCancelAction => String::new(),
                            other => typing_label(other).to_owned(),
                        },
                    )),
                    U::PhoneCall(pc) => {
                        use grammers_client::tl::enums::PhoneCall as PC;
                        match &pc.phone_call {
                            PC::Requested(c) => Some(Live::PhoneCall(crate::calls::CallEvent::Incoming {
                                call_id: c.id,
                                access_hash: c.access_hash,
                                admin_id: c.admin_id,
                                video: c.video,
                                g_a_hash: c.g_a_hash.clone(),
                            })),
                            PC::Waiting(c) => Some(Live::PhoneCall(crate::calls::CallEvent::Waiting {
                                call_id: c.id,
                                access_hash: c.access_hash,
                                received: c.receive_date.is_some(),
                            })),
                            PC::Accepted(c) => Some(Live::PhoneCall(crate::calls::CallEvent::Accepted {
                                call_id: c.id,
                                access_hash: c.access_hash,
                                g_b: c.g_b.clone(),
                            })),
                            PC::Call(c) => Some(Live::PhoneCall(crate::calls::CallEvent::Active {
                                call_id: c.id,
                                access_hash: c.access_hash,
                                g_a_or_b: c.g_a_or_b.clone(),
                                key_fingerprint: c.key_fingerprint,
                                video: c.video,
                                p2p_allowed: c.p2p_allowed,
                                relays: c.connections.len(),
                                endpoints: crate::calls::parse_phone_connections(&c.connections),
                            })),
                            PC::Discarded(c) => Some(Live::PhoneCall(crate::calls::CallEvent::Discarded {
                                call_id: c.id,
                                reason: crate::calls::discard_reason_label(c.reason.as_ref()).to_owned(),
                                duration: c.duration.unwrap_or(0),
                            })),
                            PC::Empty(_) => None,
                        }
                    }
                    U::PhoneCallSignalingData(s) => Some(Live::PhoneCall(
                        crate::calls::CallEvent::SignalingData {
                            call_id: s.phone_call_id,
                            data: s.data.clone(),
                        },
                    )),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    async fn apply(&self, upd: &Update) {
        let cfg = self.settings();
        let is_bot_msg = |m: &grammers_client::message::Message| {
            !m.outgoing()
                && matches!(m.sender(), Some(grammers_client::peer::Peer::User(u)) if u.is_bot())
        };
        match upd {
            Update::NewMessage(m) if cfg.ayu.save_deleted_messages && (!is_bot_msg(m) || cfg.ayu.save_for_bots) => {
                let key = m.peer_id().bot_api_dialog_id_unchecked();
                let r = Self::row(key, m, 0);
                let txt = Self::archive_text_from_row(&r);
                self.remember_full(key, r.id, &txt, r.date, r.outgoing, &r.sender)
                    .await;
            }
            Update::MessageEdited(m) if cfg.ayu.save_messages_history && (!is_bot_msg(m) || cfg.ayu.save_for_bots) => {
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
            .invoke_retry("messages.getAllStickers", &tl::functions::messages::GetAllStickers { hash: 0 })
            .await?;
        let tl::enums::messages::AllStickers::Stickers(s) = res else {
            return Ok(Vec::new());
        };
        let out: Vec<StickerSetRow> = s
            .sets
            .iter()
            .filter_map(|x| {
                let tl::enums::StickerSet::Set(x) = x;
                (!x.masks && !x.emojis && !x.archived).then(|| set_row_of(x, true))
            })
            .collect();
        crate::diag::info("STICKER", &format!("Загружено наборов стикеров: {}", out.len()));
        Ok(out)
    }

    /// Избранные стикеры аккаунта (`messages.getFavedStickers`).
    pub async fn faved_stickers(&self) -> Result<Vec<StickerRow>, grammers_client::InvocationError> {
        use grammers_client::tl;
        let res = self
            .invoke_retry("messages.getFavedStickers", &tl::functions::messages::GetFavedStickers { hash: 0 })
            .await?;
        let tl::enums::messages::FavedStickers::Stickers(f) = res else {
            return Ok(Vec::new());
        };
        let out: Vec<StickerRow> = f
            .stickers
            .iter()
            .filter_map(|d| match d {
                tl::enums::Document::Document(d) => Some(sticker_row_of(d)),
                _ => None,
            })
            .collect();
        crate::diag::info("STICKER", &format!("Избранных стикеров: {}", out.len()));
        Ok(out)
    }

    /// Недавние стикеры аккаунта (`messages.getRecentStickers`).
    pub async fn recent_stickers(&self) -> Result<Vec<StickerRow>, grammers_client::InvocationError> {
        use grammers_client::tl;
        let res = self
            .invoke_retry(
                "messages.getRecentStickers",
                &tl::functions::messages::GetRecentStickers {
                    attached: false,
                    hash: 0,
                },
            )
            .await?;
        let tl::enums::messages::RecentStickers::Stickers(r) = res else {
            return Ok(Vec::new());
        };
        let out: Vec<StickerRow> = r
            .stickers
            .iter()
            .filter_map(|d| match d {
                tl::enums::Document::Document(d) => Some(sticker_row_of(d)),
                _ => None,
            })
            .collect();
        crate::diag::info("STICKER", &format!("Недавних стикеров: {}", out.len()));
        Ok(out)
    }

    /// Сохранённые GIF пользователя (документы с миниатюрами).
    pub async fn saved_gifs(&self) -> Result<Vec<StickerRow>, grammers_client::InvocationError> {
        use grammers_client::tl;
        let res = self
            .invoke_retry("messages.getSavedGifs", &tl::functions::messages::GetSavedGifs { hash: 0 })
            .await?;
        let tl::enums::messages::SavedGifs::Gifs(g) = res else {
            return Ok(Vec::new());
        };
        let out: Vec<StickerRow> = g
            .gifs
            .iter()
            .filter_map(|d| match d {
                tl::enums::Document::Document(d) => Some(sticker_row_of(d)),
                _ => None,
            })
            .collect();
        crate::diag::info("GIF", &format!("Сохранённых GIF: {}", out.len()));
        Ok(out)
    }

    /// Отправка сообщения с необязательным ответом, форматированием и пользовательскими Premium-эмодзи.
    pub async fn send_rich(
        &self,
        peer: PeerRef,
        text: &str,
        custom_emojis: &[(usize, usize, i64)],
        reply_to: Option<i32>,
    ) -> Result<MessageRow, grammers_client::InvocationError> {
        let ents: Vec<Entity> = custom_emojis
            .iter()
            .map(|&(start, len, doc_id)| Entity {
                kind: EntityKind::CustomEmoji(doc_id),
                start,
                end: start + len,
                url: None,
            })
            .collect();
        self.send_formatted(peer, text, &ents, reply_to).await
    }

    /// Отправка сообщения с произвольными сущностями (`Bold`, `Italic`, `Code`, `Pre`, `Spoiler`, `Quote`, `CustomEmoji` и др.).
    pub async fn send_formatted(
        &self,
        peer: PeerRef,
        text: &str,
        entities: &[Entity],
        reply_to: Option<i32>,
    ) -> Result<MessageRow, grammers_client::InvocationError> {
        let ghost_mode = self.settings().ghost;
        let silent = ghost_mode.should_send_without_sound();
        let mut msg = grammers_client::message::InputMessage::new()
            .text(text)
            .silent(silent)
            .schedule_date(Self::ghost_schedule_time(&ghost_mode));
        if let Some(id) = reply_to {
            msg = msg.reply_to(Some(id));
        }
        if !entities.is_empty() {
            let tl_ents = entities_to_tl(text, entities);
            if !tl_ents.is_empty() {
                msg = msg.fmt_entities(tl_ents);
            }
        }
        let sent = self.client.send_message(peer, msg).await?;
        let key = peer_key(&peer);
        let row = self
            .finish(key, vec![Self::row(key, &sent, 0)])
            .into_iter()
            .next()
            .unwrap_or_else(|| Self::row(key, &sent, 0));
        self.remember_row_sync(peer, &row).await;
        self.spawn_after_send(peer, ghost_mode, &row, true);
        Ok(row)
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

    /// Список участников канала/супергруппы/группы для панели профиля (с пагинацией до 1000+).
    pub async fn channel_participants(
        &self,
        peer: PeerRef,
        limit: i32,
    ) -> Result<Vec<ContactRow>, grammers_client::InvocationError> {
        let (rows, _) = self.channel_participants_paged(peer, 0, limit).await?;
        Ok(rows)
    }

    /// Постраничная загрузка участников канала/супергруппы (`offset`, `limit` до 200) + общее число.
    pub async fn channel_participants_paged(
        &self,
        peer: PeerRef,
        offset: i32,
        limit: i32,
    ) -> Result<(Vec<ContactRow>, i32), grammers_client::InvocationError> {
        self.channel_participants_filtered(peer, ParticipantFilter::All, "", offset, limit)
            .await
    }

    /// Поиск и фильтрация участников канала/группы по вкладке (`Все` / `Админы` / `Боты`) и запросу.
    pub async fn channel_participants_filtered(
        &self,
        peer: PeerRef,
        filter: ParticipantFilter,
        query: &str,
        offset: i32,
        limit: i32,
    ) -> Result<(Vec<ContactRow>, i32), grammers_client::InvocationError> {
        use grammers_client::tl;
        let q = query.trim();
        let tl_filter = if !q.is_empty() {
            tl::enums::ChannelParticipantsFilter::ChannelParticipantsSearch(
                tl::types::ChannelParticipantsSearch { q: q.to_owned() },
            )
        } else {
            match filter {
                ParticipantFilter::All => tl::enums::ChannelParticipantsFilter::ChannelParticipantsRecent,
                ParticipantFilter::Admins => tl::enums::ChannelParticipantsFilter::ChannelParticipantsAdmins,
                ParticipantFilter::Bots => tl::enums::ChannelParticipantsFilter::ChannelParticipantsBots,
            }
        };
        let q_low = q.to_lowercase();
        let (users, total, is_legacy_chat) = match tl::enums::InputPeer::from(peer) {
            tl::enums::InputPeer::Channel(c) => {
                let res = self
                    .client
                    .invoke(&tl::functions::channels::GetParticipants {
                        channel: tl::enums::InputChannel::Channel(tl::types::InputChannel {
                            channel_id: c.channel_id,
                            access_hash: c.access_hash,
                        }),
                        filter: tl_filter,
                        offset: offset.max(0),
                        limit: limit.clamp(1, 200),
                        hash: 0,
                    })
                    .await?;
                match res {
                    tl::enums::channels::ChannelParticipants::Participants(p) => (p.users, p.count, false),
                    tl::enums::channels::ChannelParticipants::NotModified => (Vec::new(), 0, false),
                }
            }
            tl::enums::InputPeer::Chat(c) => {
                let tl::enums::messages::ChatFull::Full(f) = self
                    .client
                    .invoke(&tl::functions::messages::GetFullChat { chat_id: c.chat_id })
                    .await?;
                let n = f.users.len() as i32;
                (f.users, n, true)
            }
            _ => (Vec::new(), 0, false),
        };
        let rows: Vec<ContactRow> = users
            .iter()
            .filter_map(|u| match u {
                tl::enums::User::User(y) if !y.deleted => {
                    if filter == ParticipantFilter::Bots && !y.bot {
                        return None;
                    }
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
                    if is_legacy_chat && !q_low.is_empty() {
                        let nm = name.to_lowercase();
                        let st = status.to_lowercase();
                        if !nm.contains(&q_low) && !st.contains(&q_low) {
                            return None;
                        }
                    }
                    Some(ContactRow {
                        peer: PeerRef::from(u),
                        id: y.id,
                        name: if name.is_empty() { "Без имени".to_owned() } else { name },
                        status,
                    })
                }
                _ => None,
            })
            .collect();
        let eff_total = if is_legacy_chat { rows.len() as i32 } else { total };
        Ok((rows, eff_total))
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

    /// Загружает и кэширует исходные `.tgs` байты анимированного стикера/эмодзи (`cache/tgs/<id>.tgs`).
    pub async fn tgs_sticker_bytes(&self, st: &StickerRow) -> Result<Option<Vec<u8>>, BoxErr> {
        use grammers_client::tl;
        if st.mime != "application/x-tgsticker" {
            return Ok(None);
        }
        let dir = self.dirs.cache.join("tgs");
        std::fs::create_dir_all(&dir)?;
        let file = dir.join(format!("{}.tgs", st.id));
        if file.exists()
            && let Ok(bytes) = std::fs::read(&file)
            && !bytes.is_empty()
        {
            return Ok(Some(bytes));
        }
        let loc = RawLocation(tl::enums::InputFileLocation::InputDocumentFileLocation(
            tl::types::InputDocumentFileLocation {
                id: st.id,
                access_hash: st.access_hash,
                file_reference: st.file_ref.clone(),
                thumb_size: String::new(),
            },
        ));
        self.client.download_media(&loc, &file).await?;
        Ok(std::fs::read(&file).ok().filter(|b| !b.is_empty()))
    }

    /// Скачивает и кэширует `.tgs` анимацию стикера в сообщении ленты (`cache/tgs/m_<peer>_<id>.tgs`).
    pub async fn tgs_message_bytes(&self, peer: PeerRef, msg_id: i32) -> Result<Option<Vec<u8>>, BoxErr> {
        let dir = self.dirs.cache.join("tgs");
        std::fs::create_dir_all(&dir)?;
        let file = dir.join(format!("m_{}_{msg_id}.tgs", peer_key(&peer)));
        if file.exists()
            && let Ok(bytes) = std::fs::read(&file)
            && !bytes.is_empty()
        {
            return Ok(Some(bytes));
        }
        let Some(m) = self.message_by_id(peer, msg_id).await? else {
            return Ok(None);
        };
        let Some(media) = m.media() else { return Ok(None) };
        let info = media::describe(&media);
        if info.mime != "application/x-tgsticker" || info.size > 512 * 1024 {
            return Ok(None);
        }
        self.client.download_media(&media, &file).await?;
        Ok(std::fs::read(&file).ok().filter(|b| !b.is_empty()))
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

    /// Отправить стикер из набора и вернуть созданный `MessageRow`.
    pub async fn send_sticker(
        &self,
        peer: PeerRef,
        st: &StickerRow,
        reply_to: Option<i32>,
    ) -> Result<MessageRow, grammers_client::InvocationError> {
        use grammers_client::tl;
        use std::sync::atomic::{AtomicI64, Ordering};
        static SEQ: AtomicI64 = AtomicI64::new(0);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos() as i64);
        let random_id = nanos ^ (SEQ.fetch_add(1, Ordering::Relaxed) << 20);
        let ghost = self.settings().ghost;
        let silent = ghost.should_send_without_sound();
        let updates = self
            .client
            .invoke(&tl::functions::messages::SendMedia {
                silent,
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
                schedule_date: ghost::schedule_date(&ghost),
                schedule_repeat_period: None,
                send_as: None,
                quick_reply_shortcut: None,
                effect: None,
                allow_paid_stars: None,
                suggested_post: None,
            })
            .await?;
        let row = self.row_from_tl_updates(peer, &updates, &st.emoji, silent);
        self.remember_row_sync(peer, &row).await;
        self.spawn_after_send(peer, ghost, &row, true);
        Ok(row)
    }

    async fn send_raw_media(
        &self,
        peer: PeerRef,
        media: grammers_client::tl::enums::InputMedia,
        fallback_text: &str,
        reply_to: Option<i32>,
    ) -> Result<MessageRow, grammers_client::InvocationError> {
        use grammers_client::tl;
        use std::sync::atomic::{AtomicI64, Ordering};
        static SEQ: AtomicI64 = AtomicI64::new(0);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos() as i64);
        let random_id = nanos ^ (SEQ.fetch_add(1, Ordering::Relaxed) << 20);
        let ghost = self.settings().ghost;
        let silent = ghost.should_send_without_sound();
        self.apply_traffic_jitter().await;
        let updates = self
            .client
            .invoke(&tl::functions::messages::SendMedia {
                silent,
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
                media,
                message: String::new(),
                random_id,
                reply_markup: None,
                entities: None,
                schedule_date: ghost::schedule_date(&ghost),
                schedule_repeat_period: None,
                send_as: None,
                quick_reply_shortcut: None,
                effect: None,
                allow_paid_stars: None,
                suggested_post: None,
            })
            .await?;
        let row = self.row_from_tl_updates(peer, &updates, fallback_text, silent);
        self.remember_row_sync(peer, &row).await;
        self.spawn_after_send(peer, ghost, &row, true);
        Ok(row)
    }

    /// Отправить карточку контакта (`InputMedia::Contact`).
    pub async fn send_contact(
        &self,
        peer: PeerRef,
        phone: &str,
        first_name: &str,
        last_name: &str,
        reply_to: Option<i32>,
    ) -> Result<MessageRow, grammers_client::InvocationError> {
        use grammers_client::tl;
        let full = format!("{} {}", first_name.trim(), last_name.trim()).trim().to_owned();
        let label = if full.is_empty() { phone.trim().to_owned() } else { full };
        self.send_raw_media(
            peer,
            tl::enums::InputMedia::Contact(tl::types::InputMediaContact {
                phone_number: phone.trim().to_owned(),
                first_name: first_name.trim().to_owned(),
                last_name: last_name.trim().to_owned(),
                vcard: String::new(),
            }),
            &label,
            reply_to,
        )
        .await
    }

    /// Отправить геопозицию (`InputMedia::GeoPoint`).
    pub async fn send_geo(
        &self,
        peer: PeerRef,
        lat: f64,
        lon: f64,
        reply_to: Option<i32>,
    ) -> Result<MessageRow, grammers_client::InvocationError> {
        use grammers_client::tl;
        self.send_raw_media(
            peer,
            tl::enums::InputMedia::GeoPoint(tl::types::InputMediaGeoPoint {
                geo_point: tl::enums::InputGeoPoint::Point(tl::types::InputGeoPoint {
                    lat,
                    long: lon,
                    accuracy_radius: None,
                }),
            }),
            "Геопозиция",
            reply_to,
        )
        .await
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
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs() as i64);
            for (scope, res) in [(NotifyScope::Users, u), (NotifyScope::Chats, c), (NotifyScope::Broadcasts, b)] {
                if let Ok(tl::enums::PeerNotifySettings::Settings(s)) = res {
                    list.push(NotifyRow {
                        scope,
                        title: scope.title(),
                        enabled: !s.silent.unwrap_or(false) && !s.mute_until.is_some_and(|t| i64::from(t) > now),
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

    /// Получает конкретное сообщение чата (например, закреплённое) как `MessageRow`.
    pub async fn message_row_by_id(
        &self,
        peer: PeerRef,
        msg_id: i32,
    ) -> Result<Option<MessageRow>, grammers_client::InvocationError> {
        let key = peer_key(&peer);
        let Some(m) = self.message_by_id(peer, msg_id).await? else {
            return Ok(None);
        };
        Ok(self.finish(key, vec![Self::row(key, &m, 0)]).into_iter().next())
    }

    /// Темы супергруппы-форума (`messages.getForumTopics`).
    pub async fn forum_topics(&self, peer: PeerRef) -> Result<Vec<ForumTopicRow>, grammers_client::InvocationError> {
        use grammers_client::tl;
        let tl::enums::messages::ForumTopics::Topics(res) = self
            .client
            .invoke(&tl::functions::messages::GetForumTopics {
                peer: peer.into(),
                q: None,
                offset_date: 0,
                offset_id: 0,
                offset_topic: 0,
                limit: 60,
            })
            .await?;
        Ok(res
            .topics
            .into_iter()
            .filter_map(|t| match t {
                tl::enums::ForumTopic::Topic(x) => Some(ForumTopicRow {
                    id: x.id,
                    title: x.title,
                    unread: x.unread_count,
                    pinned: x.pinned,
                    closed: x.closed,
                    top_message: x.top_message,
                }),
                _ => None,
            })
            .collect())
    }

    /// Сообщения темы форума (`messages.getReplies`).
    pub async fn topic_messages(
        &self,
        peer: PeerRef,
        topic_id: i32,
        limit: usize,
    ) -> Result<Vec<MessageRow>, grammers_client::InvocationError> {
        use grammers_client::tl;
        let res = self
            .client
            .invoke(&tl::functions::messages::GetReplies {
                peer: peer.into(),
                msg_id: topic_id,
                offset_id: 0,
                offset_date: 0,
                add_offset: 0,
                limit: limit.clamp(1, 100) as i32,
                max_id: 0,
                min_id: 0,
                hash: 0,
            })
            .await?;
        let mut rows = tl_messages_to_rows(&res);
        rows.sort_by_key(|r| (r.date, r.id));
        Ok(rows)
    }

    /// Комментарии к посту канала (`messages.getDiscussionMessage` + `messages.getReplies`).
    pub async fn discussion_comments(
        &self,
        peer: PeerRef,
        post_id: i32,
    ) -> Result<Option<DiscussionThread>, grammers_client::InvocationError> {
        use grammers_client::tl;
        let tl::enums::messages::DiscussionMessage::Message(dm) = self
            .client
            .invoke(&tl::functions::messages::GetDiscussionMessage {
                peer: peer.into(),
                msg_id: post_id,
            })
            .await?;
        let Some(tl::enums::Message::Message(root)) = dm.messages.first() else {
            return Ok(None);
        };
        let disc_peer_id = match &root.peer_id {
            tl::enums::Peer::Channel(c) => Some(c.channel_id),
            tl::enums::Peer::Chat(c) => Some(c.chat_id),
            _ => None,
        };
        let disc_peer = dm
            .chats
            .iter()
            .find(|c| match c {
                tl::enums::Chat::Channel(ch) => Some(ch.id) == disc_peer_id,
                tl::enums::Chat::Chat(ch) => Some(ch.id) == disc_peer_id,
                _ => false,
            })
            .map(PeerRef::from);
        let Some(disc_peer) = disc_peer else {
            return Ok(None);
        };
        self.persist_dialogs_peers(&dm.users, &dm.chats, &[]).await;
        let res = self
            .client
            .invoke(&tl::functions::messages::GetReplies {
                peer: disc_peer.into(),
                msg_id: root.id,
                offset_id: 0,
                offset_date: 0,
                add_offset: 0,
                limit: 80,
                max_id: 0,
                min_id: 0,
                hash: 0,
            })
            .await?;
        let mut comments = tl_messages_to_rows(&res);
        comments.sort_by_key(|r| (r.date, r.id));
        let channel_name = dm
            .chats
            .iter()
            .find_map(|c| match c {
                tl::enums::Chat::Channel(ch) if Some(ch.id) == disc_peer_id => Some(ch.title.clone()),
                tl::enums::Chat::Chat(ch) if Some(ch.id) == disc_peer_id => Some(ch.title.clone()),
                _ => None,
            })
            .unwrap_or_default();
        let post_preview = if root.message.trim().is_empty() {
            raw_media_label(root.media.as_ref())
        } else {
            root.message.chars().take(160).collect()
        };
        Ok(Some(DiscussionThread {
            disc_peer,
            root_msg_id: root.id,
            post_id,
            channel_name,
            post_preview,
            comments,
        }))
    }

    /// Лента активных историй (`stories.getAllStories`) с просмотрами, реакциями и числом историй автора.
    pub async fn stories(&self) -> Result<Vec<StoryRow>, grammers_client::InvocationError> {
        use grammers_client::tl;
        let tl::enums::stories::AllStories::Stories(st) = self
            .invoke_retry(
                "stories.getAllStories",
                &tl::functions::stories::GetAllStories {
                    next: false,
                    hidden: false,
                    state: None,
                },
            )
            .await?
        else {
            return Ok(Vec::new());
        };
        let mut out = Vec::new();
        for ps in st.peer_stories {
            let tl::enums::PeerStories::Stories(ps) = ps;
            let total_in_peer = ps
                .stories
                .iter()
                .filter(|s| matches!(s, tl::enums::StoryItem::Item(_)))
                .count();
            let Some(last) = ps.stories.iter().rfind(|s| matches!(s, tl::enums::StoryItem::Item(_))) else {
                continue;
            };
            let tl::enums::StoryItem::Item(item) = last else { continue };
            let (peer, peer_id, author) = match &ps.peer {
                tl::enums::Peer::User(u) => {
                    let Some(usr) = st.users.iter().find(|x| matches!(x, tl::enums::User::User(y) if y.id == u.user_id)) else {
                        continue;
                    };
                    let tl::enums::User::User(y) = usr else { continue };
                    let name = format!("{} {}", y.first_name.as_deref().unwrap_or(""), y.last_name.as_deref().unwrap_or("")).trim().to_owned();
                    (PeerRef::from(usr), u.user_id, name)
                }
                tl::enums::Peer::Channel(c) => {
                    let Some(ch) = st.chats.iter().find(|x| matches!(x, tl::enums::Chat::Channel(y) if y.id == c.channel_id)) else {
                        continue;
                    };
                    let tl::enums::Chat::Channel(y) = ch else { continue };
                    (PeerRef::from(ch), -1_000_000_000_000 - c.channel_id, y.title.clone())
                }
                _ => continue,
            };
            let unread = ps.max_read_id.is_none_or(|m| item.id > m);
            let (views, reactions) = match item.views.as_ref() {
                Some(tl::enums::StoryViews::Views(v)) => (v.views_count, v.reactions_count.unwrap_or(0)),
                None => (0, 0),
            };
            let is_video = matches!(item.media, tl::enums::MessageMedia::Document(_));
            out.push(StoryRow {
                peer,
                peer_id,
                author: if author.is_empty() { "История".to_owned() } else { author },
                story_id: item.id,
                caption: item.caption.clone().unwrap_or_default(),
                date: item.date,
                unread,
                views,
                reactions,
                is_video,
                count_in_peer: total_in_peer.max(1),
            });
        }
        crate::diag::info("STORY", &format!("Загружено активных историй: {}", out.len()));
        Ok(out)
    }

    /// Отмечает историю прочитанной (`stories.readStories`), соблюдая режим призрака (`send_read_stories`) и щит приватности.
    pub async fn read_story(
        &self,
        peer: PeerRef,
        story_id: i32,
    ) -> Result<bool, grammers_client::InvocationError> {
        use grammers_client::tl::{Identifiable, functions};
        let cfg = self.settings();
        if cfg.privacy_shield.block_activity_telemetry {
            return Ok(false);
        }
        if ghost::verdict(&cfg.ghost, functions::stories::ReadStories::CONSTRUCTOR_ID) == Verdict::Drop {
            return Ok(false);
        }
        self.apply_traffic_jitter().await;
        self.client
            .invoke(&functions::stories::ReadStories {
                peer: peer.into(),
                max_id: story_id,
            })
            .await?;
        Ok(true)
    }

    /// Таймер автоудаления сообщений в открытом чате (`messages.setHistoryTTL`).
    pub async fn set_chat_ttl(&self, peer: PeerRef, period: i32) -> Result<(), grammers_client::InvocationError> {
        self.client
            .invoke(&grammers_client::tl::functions::messages::SetHistoryTtl {
                peer: peer.into(),
                period,
            })
            .await?;
        Ok(())
    }

    /// Нажатие инлайн callback-кнопки бота (`messages.getBotCallbackAnswer`).
    /// Возвращает `(сообщение_всплывашки, url_для_открытия, это_alert)`.
    pub async fn press_bot_callback(
        &self,
        peer: PeerRef,
        msg_id: i32,
        data: Vec<u8>,
    ) -> Result<(Option<String>, Option<String>, bool), grammers_client::InvocationError> {
        use grammers_client::tl;
        self.apply_traffic_jitter().await;
        let res = self
            .client
            .invoke(&tl::functions::messages::GetBotCallbackAnswer {
                game: false,
                peer: peer.into(),
                msg_id,
                data: Some(data),
                password: None,
            })
            .await?;
        let tl::enums::messages::BotCallbackAnswer::Answer(ans) = res;
        Ok((ans.message, ans.url, ans.alert))
    }

    /// Запрашивает URL для открытия Mini App / WebApp бота (`messages.requestWebView` или `messages.requestSimpleWebView`).
    pub async fn request_webview_url(
        &self,
        peer: PeerRef,
        msg_id: Option<i32>,
        url: Option<String>,
        start_param: Option<String>,
        simple: bool,
        spoof_android: bool,
    ) -> Result<String, BoxErr> {
        use grammers_client::tl;
        self.apply_traffic_jitter().await;
        let platform = if spoof_android {
            "android".to_owned()
        } else {
            "tdesktop".to_owned()
        };
        let in_peer = tl::enums::InputPeer::from(peer);
        let bot_user = match &in_peer {
            tl::enums::InputPeer::User(u) => tl::enums::InputUser::User(tl::types::InputUser {
                user_id: u.user_id,
                access_hash: u.access_hash,
            }),
            _ => tl::enums::InputUser::Empty,
        };
        let theme_params = Some(tl::enums::DataJson::Json(tl::types::DataJson {
            data: r##"{"bg_color":"#17212b","secondary_bg_color":"#0e1621","text_color":"#f5f5f5","hint_color":"#708499","link_color":"#f58220","button_color":"#f58220","button_text_color":"#ffffff"}"##.to_owned(),
        }));
        let res = if simple || matches!(bot_user, tl::enums::InputUser::Empty) && msg_id.is_none() {
            self.client
                .invoke(&tl::functions::messages::RequestSimpleWebView {
                    from_switch_webview: false,
                    from_side_menu: false,
                    compact: false,
                    fullscreen: false,
                    bot: bot_user,
                    url: url.clone(),
                    start_param,
                    theme_params,
                    platform,
                })
                .await
        } else {
            let reply_to = msg_id.map(|id| {
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
            });
            self.client
                .invoke(&tl::functions::messages::RequestWebView {
                    from_bot_menu: msg_id.is_none() && url.is_some(),
                    silent: self.settings().ghost.should_send_without_sound(),
                    compact: false,
                    fullscreen: false,
                    peer: in_peer,
                    bot: bot_user,
                    url: url.clone(),
                    start_param,
                    theme_params,
                    platform,
                    reply_to,
                    send_as: None,
                })
                .await
        };
        match res {
            Ok(tl::enums::WebViewResult::Url(u)) => Ok(u.url),
            Err(e) => {
                if let Some(fallback) = url.filter(|u| {
                    u.split_once("://")
                        .is_some_and(|(sch, _)| sch.eq_ignore_ascii_case("https") || sch.eq_ignore_ascii_case("http"))
                }) {
                    Ok(fallback)
                } else {
                    Err(Box::new(e))
                }
            }
        }
    }

    /// Разрешает `@username` (`contacts.resolveUsername`) во внутреннюю ссылку `(PeerRef, dialog_id, name)`.
    pub async fn resolve_username(&self, domain: &str) -> Result<(PeerRef, i64, String), BoxErr> {
        use grammers_client::tl;
        let clean = domain.trim().trim_start_matches('@');
        let tl::enums::contacts::ResolvedPeer::Peer(res) = self
            .client
            .invoke(&tl::functions::contacts::ResolveUsername {
                username: clean.to_owned(),
                referer: None,
            })
            .await?;
        for u in &res.users {
            if let tl::enums::User::User(x) = u {
                let pref = PeerRef::from(u);
                let name = [x.first_name.clone(), x.last_name.clone()]
                    .into_iter()
                    .flatten()
                    .collect::<Vec<_>>()
                    .join(" ");
                return Ok((
                    pref,
                    x.id,
                    if name.trim().is_empty() {
                        format!("@{clean}")
                    } else {
                        name
                    },
                ));
            }
        }
        for c in &res.chats {
            match c {
                tl::enums::Chat::Channel(ch) => {
                    return Ok((PeerRef::from(c), -1_000_000_000_000 - ch.id, ch.title.clone()));
                }
                tl::enums::Chat::Chat(g) => {
                    return Ok((PeerRef::from(c), -g.id, g.title.clone()));
                }
                _ => {}
            }
        }
        Err(format!("Пользователь или канал @{clean} не найден").into())
    }

    /// Проверяет и принимает приглашение `t.me/+HASH` (`messages.checkChatInvite` / `messages.importChatInvite`).
    pub async fn open_or_import_invite(&self, hash: &str) -> Result<(PeerRef, i64, String), BoxErr> {
        use grammers_client::tl;
        let checked = self
            .client
            .invoke(&tl::functions::messages::CheckChatInvite {
                hash: hash.to_owned(),
            })
            .await?;
        let extract_chat = |chat: &tl::enums::Chat| -> Option<(PeerRef, i64, String)> {
            match chat {
                tl::enums::Chat::Channel(ch) => Some((
                    PeerRef::from(chat),
                    -1_000_000_000_000 - ch.id,
                    ch.title.clone(),
                )),
                tl::enums::Chat::Chat(g) => Some((
                    PeerRef::from(chat),
                    -g.id,
                    g.title.clone(),
                )),
                _ => None,
            }
        };
        match &checked {
            tl::enums::ChatInvite::Already(a) => {
                if let Some(found) = extract_chat(&a.chat) {
                    return Ok(found);
                }
            }
            tl::enums::ChatInvite::Peek(p) => {
                if let Some(found) = extract_chat(&p.chat) {
                    return Ok(found);
                }
            }
            tl::enums::ChatInvite::Invite(_) => {
                let res = self
                    .client
                    .invoke(&tl::functions::messages::ImportChatInvite {
                        hash: hash.to_owned(),
                    })
                    .await?;
                if let tl::enums::messages::ChatInviteJoinResult::Ok(ok) = &res {
                    let chats = match &ok.updates {
                        tl::enums::Updates::Updates(u) => &u.chats[..],
                        tl::enums::Updates::Combined(u) => &u.chats[..],
                        _ => &[],
                    };
                    for c in chats {
                        if let Some(found) = extract_chat(c) {
                            return Ok(found);
                        }
                    }
                }
            }
        }
        Err("Не удалось открыть приглашение".into())
    }

    /// Тотальная очистка данных у других людей и на сервере без потери аккаунта:
    /// - Удаляет историю личных чатов и ботов для ОБЕИХ сторон (`revoke = true`);
    /// - Находит и удаляет собственные сообщения пользователя в группах (`revoke = true`);
    /// - Стирает синхронизированные контакты, рейтинг частых собеседников, черновики, недавние стикеры и платёжные данные.
    pub async fn wipe_external_data(&self, scope: WipeScope) -> Result<String, BoxErr> {
        use grammers_client::tl;
        let dlgs = self.dialogs(300).await?;
        let mut wiped_private = 0usize;
        let mut wiped_group_msgs = 0usize;
        let mut wiped_contacts = 0usize;

        for d in &dlgs {
            if d.saved {
                continue;
            }
            let in_peer = tl::enums::InputPeer::from(d.peer);
            if scope.clear_contacts_and_top_peers {
                for cat in [
                    tl::enums::TopPeerCategory::Correspondents,
                    tl::enums::TopPeerCategory::BotsPm,
                    tl::enums::TopPeerCategory::Groups,
                    tl::enums::TopPeerCategory::Channels,
                    tl::enums::TopPeerCategory::ForwardUsers,
                    tl::enums::TopPeerCategory::ForwardChats,
                ] {
                    let _ = self
                        .client
                        .invoke(&tl::functions::contacts::ResetTopPeerRating {
                            category: cat,
                            peer: in_peer.clone(),
                        })
                        .await;
                }
            }
            match d.kind {
                DialogKind::User | DialogKind::Bot if scope.revoke_private_chats => {
                    for _ in 0..8 {
                        let res = self
                            .client
                            .invoke(&tl::functions::messages::DeleteHistory {
                                just_clear: false,
                                revoke: true,
                                peer: in_peer.clone(),
                                max_id: i32::MAX,
                                min_date: None,
                                max_date: None,
                            })
                            .await;
                        match res {
                            Ok(tl::enums::messages::AffectedHistory::History(h)) => {
                                if h.offset <= 0 {
                                    break;
                                }
                            }
                            Err(_) => break,
                        }
                    }
                    wiped_private += 1;
                }
                DialogKind::Group if scope.delete_my_group_messages => {
                    let mut it = self.client.iter_messages(d.peer);
                    let mut my_ids = Vec::new();
                    let mut scanned = 0usize;
                    while scanned < 200 {
                        let Ok(Some(m)) = it.next().await else { break };
                        scanned += 1;
                        if m.outgoing() {
                            my_ids.push(m.id());
                        }
                    }
                    if !my_ids.is_empty()
                        && self.client.delete_messages(d.peer, &my_ids).await.is_ok()
                    {
                        wiped_group_msgs += my_ids.len();
                    }
                }
                _ => {}
            }
        }

        if scope.clear_contacts_and_top_peers
            && let Ok(tl::enums::contacts::Contacts::Contacts(c)) = self
                .client
                .invoke(&tl::functions::contacts::GetContacts { hash: 0 })
                .await
        {
            let mut input_users = Vec::new();
            for u in &c.users {
                if let tl::enums::User::User(x) = u
                    && !x.is_self
                    && let Some(ah) = x.access_hash
                {
                    input_users.push(tl::enums::InputUser::User(tl::types::InputUser {
                        user_id: x.id,
                        access_hash: ah,
                    }));
                }
            }
            if !input_users.is_empty() {
                wiped_contacts = input_users.len();
                let _ = self
                    .client
                    .invoke(&tl::functions::contacts::DeleteContacts { id: input_users })
                    .await;
            }
        }

        if scope.clear_drafts_stickers_payments {
            let _ = tokio::join!(
                self.client
                    .invoke(&tl::functions::messages::ClearAllDrafts {}),
                self.client
                    .invoke(&tl::functions::messages::ClearRecentStickers { attached: false }),
                self.client
                    .invoke(&tl::functions::messages::ClearRecentStickers { attached: true }),
                self.client
                    .invoke(&tl::functions::payments::ClearSavedInfo {
                        credentials: true,
                        info: true,
                    }),
            );
        }

        Ok(format!(
            "Очищено: личных переписок (у обеих сторон) — {wiped_private}, сообщений в группах — {wiped_group_msgs}, контактов — {wiped_contacts}"
        ))
    }
}

fn button_style_info(style: Option<&grammers_client::tl::enums::KeyboardButtonStyle>) -> (u8, Option<i64>) {
    use grammers_client::tl::enums::KeyboardButtonStyle as S;
    match style {
        Some(S::Style(s)) => {
            let code = if s.bg_primary {
                1
            } else if s.bg_danger {
                2
            } else if s.bg_success {
                3
            } else {
                0
            };
            (code, s.icon)
        }
        None => (0, None),
    }
}

fn extract_reply_markup(
    rm: Option<&grammers_client::tl::enums::ReplyMarkup>,
) -> (Vec<Vec<BotButton>>, Vec<Vec<BotButton>>) {
    use grammers_client::tl::enums::{
        ButtonType as BT, InlineButtonType as IBT, KeyboardButton as KB,
        KeyboardButtonRow as KBR, KeyboardInlineButton as KIB, KeyboardInlineButtonRow as KIBR,
        ReplyMarkup as RM,
    };
    let mut inline_rows = Vec::new();
    let mut reply_rows = Vec::new();
    let Some(rm) = rm else {
        return (inline_rows, reply_rows);
    };
    match rm {
        RM::ReplyInlineMarkup(im) => {
            for KIBR::Row(row) in &im.rows {
                let mut r = Vec::new();
                for KIB::Button(b) in &row.buttons {
                    let (style, icon_id) = button_style_info(b.style.as_ref());
                    let kind = match &b.r#type {
                        IBT::Callback(c) => BotBtnKind::Callback(c.data.clone()),
                        IBT::Url(u) => BotBtnKind::Url(u.url.clone()),
                        IBT::UrlAuth(u) => BotBtnKind::Url(u.url.clone()),
                        IBT::WebView(w) => BotBtnKind::WebView {
                            url: w.url.clone(),
                            simple: false,
                        },
                        IBT::Copy(c) => BotBtnKind::Copy(c.copy_text.clone()),
                        IBT::SwitchInline(s) => BotBtnKind::SwitchInline {
                            query: s.query.clone(),
                            same_peer: s.same_peer,
                        },
                        IBT::UserProfile(u) => BotBtnKind::UserProfile(u.user_id),
                        _ => BotBtnKind::Callback(Vec::new()),
                    };
                    r.push(BotButton {
                        text: b.text.clone(),
                        kind,
                        style,
                        icon_id,
                    });
                }
                if !r.is_empty() {
                    inline_rows.push(r);
                }
            }
        }
        RM::ReplyKeyboardMarkup(km) => {
            for KBR::Row(row) in &km.rows {
                let mut r = Vec::new();
                for KB::Button(b) in &row.buttons {
                    let (style, icon_id) = button_style_info(b.style.as_ref());
                    let kind = match &b.r#type {
                        BT::SimpleWebView(w) => BotBtnKind::WebView {
                            url: w.url.clone(),
                            simple: true,
                        },
                        _ => BotBtnKind::Text,
                    };
                    r.push(BotButton {
                        text: b.text.clone(),
                        kind,
                        style,
                        icon_id,
                    });
                }
                if !r.is_empty() {
                    reply_rows.push(r);
                }
            }
        }
        RM::ReplyKeyboardHide(_) | RM::ReplyKeyboardForceReply(_) => {}
    }
    (inline_rows, reply_rows)
}

fn typing_label(a: &grammers_client::tl::enums::SendMessageAction) -> &'static str {
    use grammers_client::tl::enums::SendMessageAction as A;
    match a {
        A::SendMessageCancelAction => "",
        A::SendMessageRecordAudioAction => "записывает голосовое…",
        A::SendMessageRecordRoundAction => "записывает кружок…",
        A::SendMessageRecordVideoAction => "записывает видео…",
        A::SendMessageUploadAudioAction(_)
        | A::SendMessageUploadDocumentAction(_)
        | A::SendMessageUploadPhotoAction(_)
        | A::SendMessageUploadRoundAction(_)
        | A::SendMessageUploadVideoAction(_) => "отправляет файл…",
        A::SendMessageChooseStickerAction => "выбирает стикер…",
        _ => "печатает…",
    }
}

fn tl_messages_to_rows(res: &grammers_client::tl::enums::messages::Messages) -> Vec<MessageRow> {
    use grammers_client::tl;
    let (msgs, chats, users) = match res {
        tl::enums::messages::Messages::Messages(x) => (&x.messages, &x.chats, &x.users),
        tl::enums::messages::Messages::Slice(x) => (&x.messages, &x.chats, &x.users),
        tl::enums::messages::Messages::ChannelMessages(x) => (&x.messages, &x.chats, &x.users),
        tl::enums::messages::Messages::NotModified(_) => return Vec::new(),
    };
    let mut user_names: HashMap<i64, String> = HashMap::with_capacity(users.len());
    for x in users {
        if let tl::enums::User::User(y) = x {
            let full = format!(
                "{} {}",
                y.first_name.as_deref().unwrap_or(""),
                y.last_name.as_deref().unwrap_or("")
            )
            .trim()
            .to_owned();
            user_names.insert(y.id, full);
        }
    }
    let mut chat_names: HashMap<i64, String> = HashMap::with_capacity(chats.len());
    for c in chats {
        match c {
            tl::enums::Chat::Channel(y) => {
                chat_names.insert(y.id, y.title.clone());
            }
            tl::enums::Chat::Chat(y) => {
                chat_names.insert(y.id, y.title.clone());
            }
            _ => {}
        }
    }
    let name_for_peer = |p: &Option<tl::enums::Peer>| -> (i64, String) {
        match p {
            Some(tl::enums::Peer::User(u)) => {
                (u.user_id, user_names.get(&u.user_id).cloned().unwrap_or_default())
            }
            Some(tl::enums::Peer::Channel(c)) => (
                -1_000_000_000_000 - c.channel_id,
                chat_names.get(&c.channel_id).cloned().unwrap_or_default(),
            ),
            Some(tl::enums::Peer::Chat(c)) => {
                (-c.chat_id, chat_names.get(&c.chat_id).cloned().unwrap_or_default())
            }
            None => (0, String::new()),
        }
    };
    let mut rows: Vec<MessageRow> = msgs
        .iter()
        .filter_map(|m| {
            let tl::enums::Message::Message(x) = m else { return None };
            let (sender_id, sender) = name_for_peer(&x.from_id);
            let entities = x
                .entities
                .as_deref()
                .map(|e| convert_entities(&x.message, e))
                .unwrap_or_default();
            let (inline_buttons, reply_keyboard) = extract_reply_markup(x.reply_markup.as_ref());
            let reply_to = x.reply_to.as_ref().and_then(|rt| match rt {
                tl::enums::MessageReplyHeader::Header(h) => h.reply_to_msg_id.map(|id| (id, String::new())),
                _ => None,
            });
            let reactions = x
                .reactions
                .as_ref()
                .map(|r| {
                    let tl::enums::MessageReactions::Reactions(rx) = r;
                    rx.results
                        .iter()
                        .map(|rc| {
                            let tl::enums::ReactionCount::Count(c) = rc;
                            c.count
                        })
                        .sum()
                })
                .unwrap_or(0);
            let (replies_count, has_comments, recent_repliers) = match &x.replies {
                Some(tl::enums::MessageReplies::Replies(r)) => (
                    r.replies,
                    r.comments,
                    extract_recent_repliers(r.recent_repliers.as_deref()),
                ),
                None => (0, false, Vec::new()),
            };
            let text = if !x.message.is_empty() {
                x.message.clone()
            } else {
                raw_media_label(x.media.as_ref())
            };
            Some(MessageRow {
                sender,
                sender_id,
                entities,
                reply_to,
                reactions,
                paid_reactions: extract_paid_reactions(x.reactions.as_ref()),
                reaction_items: extract_reaction_pills(x.reactions.as_ref()),
                edit_date: x.edit_date.map(i64::from),
                views: x.views,
                pinned: x.pinned,
                inline_buttons,
                reply_keyboard,
                silent: x.silent,
                replies_count,
                has_comments,
                recent_repliers,
                ..MessageRow::plain(x.id, text, x.out, i64::from(x.date))
            })
        })
        .collect();
    let snippet_by_id: HashMap<i32, String> = rows
        .iter()
        .map(|r| (r.id, r.text.chars().take(72).collect()))
        .collect();
    for r in &mut rows {
        if let Some((rid, snip)) = &mut r.reply_to
            && let Some(t) = snippet_by_id.get(rid)
        {
            snip.clone_from(t);
        }
    }
    rows
}

/// Конвертирует список `Entity` в TL-сущности Telegram (`Vec<tl::enums::MessageEntity>`).
pub(crate) fn entities_to_tl(text: &str, entities: &[Entity]) -> Vec<grammers_client::tl::enums::MessageEntity> {
    use grammers_client::tl;
    entities
        .iter()
        .filter_map(|e| {
            let (offset, length) = utf16_range(text, e.start, e.end.saturating_sub(e.start));
            if length <= 0 {
                return None;
            }
            Some(match e.kind {
                EntityKind::Bold => tl::types::MessageEntityBold { offset, length }.into(),
                EntityKind::Italic => tl::types::MessageEntityItalic { offset, length }.into(),
                EntityKind::Underline => tl::types::MessageEntityUnderline { offset, length }.into(),
                EntityKind::Strike => tl::types::MessageEntityStrike { offset, length }.into(),
                EntityKind::Code => tl::types::MessageEntityCode { offset, length }.into(),
                EntityKind::Pre => tl::types::MessageEntityPre {
                    offset,
                    length,
                    language: e.url.clone().unwrap_or_default(),
                }
                .into(),
                EntityKind::Spoiler => tl::types::MessageEntitySpoiler { offset, length }.into(),
                EntityKind::Quote => tl::types::MessageEntityBlockquote {
                    offset,
                    length,
                    collapsed: false,
                }
                .into(),
                EntityKind::Link => {
                    if let Some(url) = &e.url {
                        tl::types::MessageEntityTextUrl { offset, length, url: url.clone() }.into()
                    } else {
                        tl::types::MessageEntityUrl { offset, length }.into()
                    }
                }
                EntityKind::CustomEmoji(doc_id) => tl::types::MessageEntityCustomEmoji {
                    offset,
                    length,
                    document_id: doc_id,
                }
                .into(),
                _ => return None,
            })
        })
        .collect()
}

/// Обёртка `Downloadable` для любого `InputFileLocation` с указанием DC — автоматически
/// переносит авторизацию между дата-центрами (`FILE_MIGRATE_X`).
pub(crate) struct RawLoc {
    pub loc: grammers_client::tl::enums::InputFileLocation,
    pub dc_id: Option<i32>,
}

impl grammers_client::media::Downloadable for RawLoc {
    fn to_raw_input_location(&self) -> Option<grammers_client::tl::enums::InputFileLocation> {
        Some(self.loc.clone())
    }
    fn dc_id(&self) -> Option<i32> {
        self.dc_id
    }
}

fn extract_rich_primary_media(rm: &grammers_client::tl::enums::RichMessage) -> Option<MediaInfo> {
    use grammers_client::tl;
    let tl::enums::RichMessage::Message(rm) = rm;
    if !rm.photos.is_empty() {
        let mut mi = MediaInfo::basic(MediaKind::Photo, "Фото", 0);
        mi.has_thumb = true;
        return Some(mi);
    }
    if let Some(tl::enums::Document::Document(doc)) = rm.documents.first() {
        let mut is_gif = false;
        let mut is_vid = false;
        let mut dims = (0, 0);
        let mut dur = 0u32;
        for a in &doc.attributes {
            match a {
                tl::enums::DocumentAttribute::Animated => is_gif = true,
                tl::enums::DocumentAttribute::Video(v) => {
                    is_vid = true;
                    dims = (v.w, v.h);
                    dur = v.duration.max(0.0) as u32;
                }
                _ => {}
            }
        }
        let kind = if is_gif || doc.mime_type == "image/gif" {
            MediaKind::Gif
        } else if is_vid || doc.mime_type.starts_with("video/") {
            MediaKind::Video
        } else {
            MediaKind::Photo
        };
        let mut mi = MediaInfo::basic(kind, "Медиа", doc.size.max(0) as u64);
        mi.dims = dims;
        mi.duration = dur;
        mi.mime = doc.mime_type.clone();
        mi.has_thumb = doc.thumbs.as_ref().is_some_and(|t| !t.is_empty()) || !is_vid;
        return Some(mi);
    }
    None
}

fn extract_todo_media(media: Option<&grammers_client::tl::enums::MessageMedia>) -> Option<TodoListInfo> {
    use grammers_client::tl::enums::{MessageMedia, TextWithEntities, TodoCompletion, TodoItem, TodoList};
    let Some(MessageMedia::ToDo(td)) = media else {
        return None;
    };
    let TodoList::List(list) = &td.todo;
    let TextWithEntities::Entities(title_e) = &list.title;
    let completed_ids: std::collections::HashSet<i32> = td
        .completions
        .as_deref()
        .unwrap_or(&[])
        .iter()
        .map(|c| {
            let TodoCompletion::Completion(x) = c;
            x.id
        })
        .collect();
    let items = list
        .list
        .iter()
        .map(|it| {
            let TodoItem::Item(x) = it;
            let TextWithEntities::Entities(t) = &x.title;
            TodoItemRow {
                id: x.id,
                text: t.text.clone(),
                done: completed_ids.contains(&x.id),
            }
        })
        .collect();
    Some(TodoListInfo {
        title: title_e.text.clone(),
        others_can_complete: list.others_can_complete,
        others_can_append: list.others_can_append,
        items,
    })
}

fn parse_notify_settings(
    ns: &grammers_client::tl::enums::PeerNotifySettings,
    now: i64,
) -> (bool, Option<bool>) {
    match ns {
        grammers_client::tl::enums::PeerNotifySettings::Settings(s) => {
            let explicit_muted = s.silent == Some(true) || s.mute_until.is_some_and(|t| i64::from(t) > now);
            let explicit_unmuted = !explicit_muted && (s.mute_until == Some(0) || s.silent == Some(false));
            let explicit = if explicit_muted {
                Some(true)
            } else if explicit_unmuted {
                Some(false)
            } else {
                None
            };
            (explicit_muted, explicit)
        }
    }
}

#[allow(dead_code)]
fn dialog_row_from(d: &grammers_client::peer::Dialog, now: i64) -> DialogRow {
    let archived = matches!(
        &d.raw,
        grammers_client::tl::enums::Dialog::Dialog(x) if x.folder_id == Some(1)
    );
    let (unread, pinned, muted, notify_explicit) = match &d.raw {
        grammers_client::tl::enums::Dialog::Dialog(x) => {
            let (m, exp) = parse_notify_settings(&x.notify_settings, now);
            (x.unread_count.max(i32::from(x.unread_mark)), x.pinned, m, exp)
        }
        _ => (0, false, false, None),
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
    DialogRow {
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
        notify_explicit,
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
        unread_mentions: match &d.raw {
            grammers_client::tl::enums::Dialog::Dialog(x) => x.unread_mentions_count,
            _ => 0,
        },
        unread_reactions: match &d.raw {
            grammers_client::tl::enums::Dialog::Dialog(x) => x.unread_reactions_count,
            _ => 0,
        },
        read_inbox_max_id: match &d.raw {
            grammers_client::tl::enums::Dialog::Dialog(x) => x.read_inbox_max_id,
            _ => 0,
        },
    }
}

fn format_business_hours(bh: &grammers_client::tl::enums::BusinessWorkHours) -> String {
    let grammers_client::tl::enums::BusinessWorkHours::Hours(h) = bh;
    let status = if h.open_now { "Открыто сейчас" } else { "Закрыто" };
    if let Some(grammers_client::tl::enums::BusinessWeeklyOpen::Open(first)) = h.weekly_open.first() {
        let sm = first.start_minute.rem_euclid(1440);
        let em = first.end_minute.rem_euclid(1440);
        format!(
            "{status} · {:02}:{:02}–{:02}:{:02} ({})",
            sm / 60,
            sm % 60,
            em / 60,
            em % 60,
            h.timezone_id
        )
    } else {
        format!("{status} ({})", h.timezone_id)
    }
}

/// Извлекает детализированный список реакций (`ReactionPill`) из `MessageReactions`.
pub fn extract_reaction_pills(reactions: Option<&grammers_client::tl::enums::MessageReactions>) -> Vec<ReactionPill> {
    use grammers_client::tl::enums::{MessageReactions as MR, Reaction as R, ReactionCount as RC};
    let Some(MR::Reactions(r)) = reactions else {
        return Vec::new();
    };
    r.results
        .iter()
        .filter_map(|rc| {
            let RC::Count(c) = rc;
            if c.count <= 0 {
                return None;
            }
            let chosen = c.chosen_order.is_some();
            match &c.reaction {
                R::Emoji(e) => Some(ReactionPill {
                    emoji: e.emoticon.clone(),
                    custom_doc_id: None,
                    count: c.count,
                    chosen,
                    paid: false,
                }),
                R::CustomEmoji(ce) => Some(ReactionPill {
                    emoji: "✨".to_owned(),
                    custom_doc_id: Some(ce.document_id),
                    count: c.count,
                    chosen,
                    paid: false,
                }),
                R::Paid => Some(ReactionPill {
                    emoji: "⭐".to_owned(),
                    custom_doc_id: None,
                    count: c.count,
                    chosen,
                    paid: true,
                }),
                R::Empty => None,
            }
        })
        .collect()
}

fn extract_recent_repliers(repliers: Option<&[grammers_client::tl::enums::Peer]>) -> Vec<i64> {
    use grammers_client::tl::enums::Peer as P;
    let Some(list) = repliers else {
        return Vec::new();
    };
    list.iter()
        .take(3)
        .map(|p| match p {
            P::User(u) => u.user_id,
            P::Chat(c) => -c.chat_id,
            P::Channel(c) => -1_000_000_000_000 - c.channel_id,
        })
        .collect()
}

fn extract_paid_reactions(reactions: Option<&grammers_client::tl::enums::MessageReactions>) -> i32 {
    use grammers_client::tl::enums::{MessageReactions as MR, Reaction as R, ReactionCount as RC};
    let Some(MR::Reactions(r)) = reactions else {
        return 0;
    };
    r.results
        .iter()
        .filter_map(|rc| {
            let RC::Count(c) = rc;
            matches!(c.reaction, R::Paid).then_some(c.count)
        })
        .sum()
}

fn format_service_action(action: &grammers_client::tl::enums::MessageAction) -> Option<String> {
    use grammers_client::tl::enums::{MessageAction as MA, StarGift as SG, TextWithEntities as T};
    match action {
        MA::StarGift(g) => {
            let stars = match &g.gift {
                SG::Gift(sg) => sg.stars,
                SG::Unique(_) => 0,
            };
            let note = g
                .message
                .as_ref()
                .map(|T::Entities(t)| t.text.trim())
                .filter(|s| !s.is_empty());
            Some(match note {
                Some(txt) => format!("🎁 Подарок ({stars} ★): {txt}"),
                None => format!("🎁 Подарок Telegram ({stars} ★)"),
            })
        }
        MA::StarGiftUnique(u) => {
            let title = match &u.gift {
                SG::Unique(ug) => format!("{} #{}", ug.title, ug.num),
                SG::Gift(sg) => sg.title.clone().unwrap_or_else(|| "Уникальный подарок".to_owned()),
            };
            Some(format!("✨ Коллекционный подарок: {title}"))
        }
        MA::PhoneCall(pc) => {
            let kind = if pc.video { "Видеозвонок" } else { "Звонок" };
            let reason = crate::calls::discard_reason_label(pc.reason.as_ref());
            Some(match pc.duration {
                Some(d) if d > 0 => format!("📞 {kind} ({d} с)"),
                _ => format!("📞 {kind}: {reason}"),
            })
        }
        _ => None,
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
            notify_explicit: None,
            saved: false,
            archived: false,
            kind,
            contact: false,
            read_out: 0,
            emoji_status: None,
            verified: false,
            premium: false,
            unread_mentions: 0,
            unread_reactions: 0,
            read_inbox_max_id: 0,
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

    #[test]
    fn typing_label_maps_actions_correctly() {
        use grammers_client::tl::enums::SendMessageAction as A;
        use grammers_client::tl::types;
        assert_eq!(typing_label(&A::SendMessageCancelAction), "");
        assert_eq!(typing_label(&A::SendMessageTypingAction), "печатает…");
        assert_eq!(typing_label(&A::SendMessageRecordAudioAction), "записывает голосовое…");
        assert_eq!(typing_label(&A::SendMessageRecordRoundAction), "записывает кружок…");
        assert_eq!(typing_label(&A::SendMessageRecordVideoAction), "записывает видео…");
        assert_eq!(typing_label(&A::SendMessageChooseStickerAction), "выбирает стикер…");
        assert_eq!(
            typing_label(&A::SendMessageUploadPhotoAction(types::SendMessageUploadPhotoAction { progress: 10 })),
            "отправляет файл…"
        );
    }

    #[test]
    fn tl_messages_to_rows_resolves_sender_and_text() {
        use grammers_client::tl::{enums, types};
        let user = enums::User::User(types::User {
            is_self: false,
            contact: true,
            mutual_contact: false,
            deleted: false,
            bot: false,
            bot_chat_history: false,
            bot_nochats: false,
            verified: false,
            restricted: false,
            min: false,
            bot_inline_geo: false,
            support: false,
            scam: false,
            apply_min_photo: false,
            fake: false,
            bot_attach_menu: false,
            premium: false,
            attach_menu_enabled: false,
            bot_can_edit: false,
            close_friend: false,
            stories_hidden: false,
            stories_unavailable: false,
            contact_require_premium: false,
            bot_business: false,
            bot_has_main_app: false,
            bot_forum_view: false,
            bot_forum_can_manage_topics: false,
            bot_can_manage_bots: false,
            bot_guestchat: false,
            bot_guard: false,
            bot_forum_sidebar: false,
            id: 77,
            access_hash: None,
            first_name: Some("Алиса".into()),
            last_name: Some("К.".into()),
            username: None,
            phone: None,
            photo: None,
            status: None,
            bot_info_version: None,
            restriction_reason: None,
            bot_inline_placeholder: None,
            lang_code: None,
            emoji_status: None,
            usernames: None,
            stories_max_id: None,
            color: None,
            profile_color: None,
            bot_active_users: None,
            bot_verification_icon: None,
            send_paid_messages_stars: None,
            linked_community_id: None,
        });
        let msg = enums::Message::Message(types::Message {
            out: false,
            mentioned: false,
            media_unread: false,
            silent: false,
            post: false,
            from_scheduled: false,
            legacy: false,
            edit_hide: false,
            pinned: true,
            noforwards: false,
            invert_media: false,
            offline: false,
            video_processing_pending: false,
            paid_suggested_post_stars: false,
            paid_suggested_post_ton: false,
            id: 501,
            from_id: Some(enums::Peer::User(types::PeerUser { user_id: 77 })),
            from_boosts_applied: None,
            from_rank: None,
            peer_id: enums::Peer::User(types::PeerUser { user_id: 77 }),
            saved_peer_id: None,
            fwd_from: None,
            via_bot_id: None,
            via_business_bot_id: None,
            guestchat_via_from: None,
            reply_to: None,
            date: 1_700_000_000,
            message: "Комментарий".into(),
            media: None,
            reply_markup: None,
            entities: None,
            views: Some(12),
            forwards: None,
            replies: None,
            edit_date: None,
            post_author: None,
            grouped_id: None,
            reactions: None,
            restriction_reason: None,
            ttl_period: None,
            quick_reply_shortcut_id: None,
            effect: None,
            factcheck: None,
            report_delivery_until_date: None,
            paid_message_stars: None,
            suggested_post: None,
            schedule_repeat_period: None,
            summary_from_language: None,
            rich_message: None,
        });
        let rows = tl_messages_to_rows(&enums::messages::Messages::Messages(types::messages::Messages {
            messages: vec![msg],
            topics: vec![],
            chats: vec![],
            users: vec![user],
        }));
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, 501);
        assert_eq!(rows[0].sender, "Алиса К.");
        assert_eq!(rows[0].text, "Комментарий");
        assert!(rows[0].pinned);
        assert_eq!(rows[0].views, Some(12));
    }

    #[test]
    fn parse_tg_link_routes_all_telegram_formats() {
        assert_eq!(
            parse_tg_link("https://t.me/durov/123"),
            Some(TgLink::Resolve {
                domain: "durov".to_owned(),
                post: Some(123),
                start: None,
                startapp: None,
            })
        );
        assert_eq!(
            parse_tg_link("https://t.me/+AbCdEfGh123"),
            Some(TgLink::Invite("AbCdEfGh123".to_owned()))
        );
        assert_eq!(
            parse_tg_link("https://t.me/joinchat/SecretHash99"),
            Some(TgLink::Invite("SecretHash99".to_owned()))
        );
        assert_eq!(
            parse_tg_link("https://t.me/c/123456789/42"),
            Some(TgLink::PrivatePost {
                channel_id: 123456789,
                msg_id: Some(42),
            })
        );
        assert_eq!(
            parse_tg_link("https://t.me/my_bot?start=ref77"),
            Some(TgLink::Resolve {
                domain: "my_bot".to_owned(),
                post: None,
                start: Some("ref77".to_owned()),
                startapp: None,
            })
        );
        assert_eq!(
            parse_tg_link("https://t.me/my_bot/wallet?startapp=main"),
            Some(TgLink::Resolve {
                domain: "my_bot".to_owned(),
                post: None,
                start: None,
                startapp: Some("main".to_owned()),
            })
        );
        assert_eq!(
            parse_tg_link("tg://resolve?domain=eroot_bot&post=15&start=hi"),
            Some(TgLink::Resolve {
                domain: "eroot_bot".to_owned(),
                post: Some(15),
                start: Some("hi".to_owned()),
                startapp: None,
            })
        );
        assert_eq!(
            parse_tg_link("tg://join?invite=Invite777"),
            Some(TgLink::Invite("Invite777".to_owned()))
        );
        assert_eq!(
            parse_tg_link("tg://privatepost?channel=987654&post=9"),
            Some(TgLink::PrivatePost {
                channel_id: 987654,
                msg_id: Some(9),
            })
        );
        assert_eq!(
            parse_tg_link("tg://socks?server=127.0.0.1&port=9050"),
            Some(TgLink::Proxy("socks5://127.0.0.1:9050".to_owned()))
        );
        assert_eq!(
            parse_tg_link("@erootsupport"),
            Some(TgLink::Resolve {
                domain: "erootsupport".to_owned(),
                post: None,
                start: None,
                startapp: None,
            })
        );
        assert_eq!(
            parse_tg_link("https://t.me/addlist/RustList42"),
            Some(TgLink::ChatlistInvite("RustList42".to_owned()))
        );
        assert_eq!(
            parse_tg_link("tg://addlist?slug=DevChats99"),
            Some(TgLink::ChatlistInvite("DevChats99".to_owned()))
        );
        assert_eq!(parse_tg_link("mailto:test@localhost"), None);
    }

    #[test]
    fn extract_reply_markup_parses_inline_and_reply_buttons() {
        use grammers_client::tl::{enums, types};

        let inline_markup = enums::ReplyMarkup::ReplyInlineMarkup(types::ReplyInlineMarkup {
            force_reply: false,
            rows: vec![enums::KeyboardInlineButtonRow::Row(types::KeyboardInlineButtonRow {
                buttons: vec![
                    enums::KeyboardInlineButton::Button(types::KeyboardInlineButton {
                        style: None,
                        text: "Оплатить".to_owned(),
                        r#type: enums::InlineButtonType::Callback(types::InlineButtonTypeCallback {
                            requires_password: false,
                            data: b"pay_1".to_vec(),
                        }),
                    }),
                    enums::KeyboardInlineButton::Button(types::KeyboardInlineButton {
                        style: None,
                        text: "Mini App".to_owned(),
                        r#type: enums::InlineButtonType::WebView(types::InlineButtonTypeWebView {
                            url: "https://t.me/my_bot/app".to_owned(),
                        }),
                    }),
                    enums::KeyboardInlineButton::Button(types::KeyboardInlineButton {
                        style: None,
                        text: "Канал".to_owned(),
                        r#type: enums::InlineButtonType::Url(types::InlineButtonTypeUrl {
                            url: "https://t.me/durov".to_owned(),
                        }),
                    }),
                ],
            })],
        });
        let (inline, reply) = extract_reply_markup(Some(&inline_markup));
        assert_eq!(inline.len(), 1);
        assert_eq!(inline[0].len(), 3);
        assert!(reply.is_empty());
        assert_eq!(inline[0][0].text, "Оплатить");
        assert!(matches!(inline[0][0].kind, BotBtnKind::Callback(ref d) if d == b"pay_1"));
        assert!(matches!(inline[0][1].kind, BotBtnKind::WebView { ref url, simple: false } if url == "https://t.me/my_bot/app"));
        assert!(matches!(inline[0][2].kind, BotBtnKind::Url(ref u) if u == "https://t.me/durov"));

        let reply_markup = enums::ReplyMarkup::ReplyKeyboardMarkup(types::ReplyKeyboardMarkup {
            resize: true,
            single_use: false,
            selective: false,
            persistent: true,
            force_reply: false,
            rows: vec![enums::KeyboardButtonRow::Row(types::KeyboardButtonRow {
                buttons: vec![
                    enums::KeyboardButton::Button(types::KeyboardButton {
                        style: None,
                        text: "Профиль".to_owned(),
                        r#type: enums::ButtonType::Default,
                    }),
                    enums::KeyboardButton::Button(types::KeyboardButton {
                        style: None,
                        text: "Открыть".to_owned(),
                        r#type: enums::ButtonType::SimpleWebView(types::ButtonTypeSimpleWebView {
                            url: "https://t.me/my_bot/simple".to_owned(),
                        }),
                    }),
                ],
            })],
            placeholder: None,
        });
        let (inline2, reply2) = extract_reply_markup(Some(&reply_markup));
        assert!(inline2.is_empty());
        assert_eq!(reply2.len(), 1);
        assert_eq!(reply2[0].len(), 2);
        assert_eq!(reply2[0][0].text, "Профиль");
        assert_eq!(reply2[0][0].kind, BotBtnKind::Text);
        assert!(matches!(reply2[0][1].kind, BotBtnKind::WebView { simple: true, .. }));
    }

    #[test]
    fn admin_and_banned_rights_convert_to_tl_and_button_styles_preserve_icon() {
        let adm = AdminRights::standard_moderator();
        let grammers_client::tl::enums::ChatAdminRights::Rights(tl_adm) = adm.to_tl();
        assert!(tl_adm.delete_messages && tl_adm.ban_users && tl_adm.pin_messages);
        assert!(!tl_adm.add_admins && !tl_adm.anonymous);

        let ban = BannedRights::full_ban();
        let grammers_client::tl::enums::ChatBannedRights::Rights(tl_ban) = ban.to_tl();
        assert!(tl_ban.view_messages && tl_ban.send_messages && tl_ban.until_date == 0);

        let mute = BannedRights::mute(1_700_000_000);
        let grammers_client::tl::enums::ChatBannedRights::Rights(tl_mute) = mute.to_tl();
        assert!(!tl_mute.view_messages && tl_mute.send_messages && tl_mute.until_date == 1_700_000_000);

        let st = grammers_client::tl::enums::KeyboardButtonStyle::Style(
            grammers_client::tl::types::KeyboardButtonStyle {
                bg_primary: false,
                bg_danger: false,
                bg_success: true,
                icon: Some(54321),
            },
        );
        assert_eq!(button_style_info(Some(&st)), (3, Some(54321)));
    }

    #[test]
    fn extract_reaction_pills_handles_emoji_custom_paid_and_zero() {
        use grammers_client::tl::{enums, types};
        let reactions = enums::MessageReactions::Reactions(types::MessageReactions {
            min: false,
            can_see_list: true,
            reactions_as_tags: false,
            results: vec![
                enums::ReactionCount::Count(types::ReactionCount {
                    chosen_order: Some(0),
                    reaction: enums::Reaction::Emoji(types::ReactionEmoji {
                        emoticon: "🔥".into(),
                    }),
                    count: 5,
                }),
                enums::ReactionCount::Count(types::ReactionCount {
                    chosen_order: None,
                    reaction: enums::Reaction::CustomEmoji(types::ReactionCustomEmoji {
                        document_id: 987_654,
                    }),
                    count: 2,
                }),
                enums::ReactionCount::Count(types::ReactionCount {
                    chosen_order: Some(1),
                    reaction: enums::Reaction::Paid,
                    count: 10,
                }),
                enums::ReactionCount::Count(types::ReactionCount {
                    chosen_order: None,
                    reaction: enums::Reaction::Empty,
                    count: 1,
                }),
            ],
            recent_reactions: None,
            top_reactors: None,
        });
        let pills = extract_reaction_pills(Some(&reactions));
        assert_eq!(pills.len(), 3);
        assert_eq!(pills[0].emoji, "🔥");
        assert!(pills[0].chosen);
        assert_eq!(pills[0].count, 5);
        assert_eq!(pills[1].custom_doc_id, Some(987_654));
        assert!(!pills[1].chosen);
        assert!(pills[2].paid && pills[2].chosen);
        assert_eq!(extract_paid_reactions(Some(&reactions)), 10);
    }

    #[test]
    fn typing_allowed_respects_ghost_and_privacy_shield() {
        let mut s = Settings::default();
        // По умолчанию PrivacyShieldSettings::block_activity_telemetry = true.
        assert!(!Engine::typing_allowed(&s));

        s.privacy_shield.block_activity_telemetry = false;
        assert!(Engine::typing_allowed(&s));

        s.ghost.active = true;
        assert!(!Engine::typing_allowed(&s));

        s.ghost.active = false;
        s.ghost.send_upload_progress = false;
        assert!(!Engine::typing_allowed(&s));

        s.ghost.send_upload_progress = true;
        s.privacy_shield.block_activity_telemetry = true;
        assert!(!Engine::typing_allowed(&s));
    }

    #[test]
    fn message_row_plain_has_empty_reaction_items_and_defaults() {
        let row = MessageRow::plain(42, "hi".into(), true, 1_700_000);
        assert_eq!(row.id, 42);
        assert!(row.reaction_items.is_empty());
        assert_eq!(ParticipantFilter::All, ParticipantFilter::All);
        assert_ne!(ParticipantFilter::Admins, ParticipantFilter::Bots);
    }

    #[test]
    fn archive_text_and_raw_labels_cover_contact_and_geo() {
        let mut r_c = MessageRow::plain(1, String::new(), true, 100);
        r_c.media = Some(MediaInfo::basic(
            MediaKind::Contact {
                phone: "+79990001122".into(),
                first_name: "Иван".into(),
                last_name: "".into(),
                user_id: 7,
            },
            "Иван",
            0,
        ));
        assert!(Engine::archive_text_from_row(&r_c).contains("Контакт"));

        let mut r_g = MessageRow::plain(2, String::new(), false, 101);
        r_g.media = Some(MediaInfo::basic(
            MediaKind::Geo {
                lat_micro: 55_755_800,
                lon_micro: 37_617_300,
                title: "Москва".into(),
            },
            "Москва",
            0,
        ));
        assert!(Engine::archive_text_from_row(&r_g).contains("Геопозиция"));
        assert!(raw_media_label(Some(&grammers_client::tl::enums::MessageMedia::Empty)).is_empty());
    }

    #[test]
    fn chat_reactions_policy_handles_all_some_and_none() {
        use grammers_client::tl::{enums, types};

        let p_none = ChatReactionsPolicy::from_tl(Some(&enums::ChatReactions::None));
        assert_eq!(p_none, ChatReactionsPolicy::None);
        assert!(!p_none.allows("👍"));
        assert!(p_none.quick_list(&["👍", "❤"]).is_empty());
        assert_eq!(p_none.default_allowed("👍"), None);

        let p_some = ChatReactionsPolicy::from_tl(Some(&enums::ChatReactions::Some(
            types::ChatReactionsSome {
                reactions: vec![
                    enums::Reaction::Emoji(types::ReactionEmoji {
                        emoticon: "🔥".into(),
                    }),
                    enums::Reaction::Emoji(types::ReactionEmoji {
                        emoticon: "❤".into(),
                    }),
                ],
            },
        )));
        assert!(p_some.allows("🔥"));
        assert!(!p_some.allows("👍"));
        assert_eq!(p_some.quick_list(&["👍", "❤"]), vec!["🔥", "❤"]);
        assert_eq!(p_some.default_allowed("👍"), Some("🔥"));
        assert_eq!(p_some.default_allowed("❤"), Some("❤"));

        let p_all = ChatReactionsPolicy::from_tl(Some(&enums::ChatReactions::All(
            types::ChatReactionsAll { allow_custom: false },
        )));
        assert!(p_all.allows("👍"));
        assert_eq!(p_all.default_allowed(""), Some("👍"));
    }

    #[test]
    fn recent_repliers_capped_at_three_and_mapped_for_all_peer_types() {
        use grammers_client::tl::{enums, types};
        let peers = [
            enums::Peer::User(types::PeerUser { user_id: 101 }),
            enums::Peer::Chat(types::PeerChat { chat_id: 202 }),
            enums::Peer::Channel(types::PeerChannel { channel_id: 303 }),
            enums::Peer::User(types::PeerUser { user_id: 404 }),
        ];
        let ids = extract_recent_repliers(Some(&peers));
        assert_eq!(ids, vec![101, -202, -1_000_000_000_303]);
        assert!(extract_recent_repliers(None).is_empty());
    }

    #[test]
    fn chat_search_filter_labels_and_tl_variants_match() {
        use grammers_client::tl::enums::MessagesFilter as MF;
        assert_eq!(ChatSearchFilter::ALL.len(), 6);
        assert_eq!(ChatSearchFilter::All.label(), "Все");
        assert_eq!(ChatSearchFilter::Photos.label(), "Фото");
        assert_eq!(ChatSearchFilter::Videos.label(), "Видео");
        assert_eq!(ChatSearchFilter::Files.label(), "Файлы");
        assert_eq!(ChatSearchFilter::Links.label(), "Ссылки");
        assert_eq!(ChatSearchFilter::Voice.label(), "Голосовые");
        assert!(matches!(ChatSearchFilter::All.to_tl(), MF::InputMessagesFilterEmpty));
        assert!(matches!(ChatSearchFilter::Photos.to_tl(), MF::InputMessagesFilterPhotos));
        assert!(matches!(ChatSearchFilter::Videos.to_tl(), MF::InputMessagesFilterVideo));
        assert!(matches!(ChatSearchFilter::Files.to_tl(), MF::InputMessagesFilterDocument));
        assert!(matches!(ChatSearchFilter::Links.to_tl(), MF::InputMessagesFilterUrl));
        assert!(matches!(ChatSearchFilter::Voice.to_tl(), MF::InputMessagesFilterVoice));
    }

    #[test]
    fn format_compact_count_and_chat_export_behave_honestly() {
        assert_eq!(format_compact_count(42, false), "42");
        assert_eq!(format_compact_count(12_430, false), "12.4K");
        assert_eq!(format_compact_count(12_430, true), "12 430");
        assert_eq!(format_compact_count(2_500_000, false), "2.5M");
        assert_eq!(format_compact_count(2_500_000, true), "2 500 000");

        let mut rows = vec![
            MessageRow::plain(1, "Привет".to_owned(), false, 1_700_000_000),
            MessageRow::plain(2, String::new(), true, 1_700_000_060),
        ];
        rows[0].sender = "Анна".to_owned();
        rows[0].sender_id = 777;
        rows[1].deleted = true;
        rows[1].reply_to = Some((1, String::new()));
        let txt = format_chat_export_txt("Тест-чат", -100, &rows);
        assert!(txt.contains("Чат: Тест-чат (ID: -100)"));
        assert!(txt.contains("Анна (#777): Привет"));
        assert!(txt.contains("[удалено]"));
        assert!(txt.contains("[ответ на #1]"));
    }

    #[test]
    fn filter_set_respects_filters_enabled_in_chats_flag() {
        let filters = vec![
            rg_settings::Filter {
                text: "спам".to_owned(),
                dialog_id: None,
                enabled: true,
                ..rg_settings::Filter::default()
            },
            rg_settings::Filter {
                text: "секретно".to_owned(),
                dialog_id: Some(555),
                enabled: true,
                ..rg_settings::Filter::default()
            },
        ];
        let (set, bad) = rg_ayu::filters::FilterSet::build(&filters);
        assert!(bad.is_empty());
        // Когда глобальные фильтры в чатах включены (`allow_global = true`) — скрываются оба.
        assert!(set.hides_with(555, "обычный спам", true));
        assert!(set.hides_with(555, "секретно", true));
        // Когда `filters_enabled_in_chats = false` — глобальные молчат, а правила конкретного чата работают!
        assert!(!set.hides_with(555, "обычный спам", false));
        assert!(set.hides_with(555, "секретно", false));
    }
}
