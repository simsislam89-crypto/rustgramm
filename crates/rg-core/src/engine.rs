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
}

/// Сведения о собеседнике, группе или канале для правой панели.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PeerInfo {
    pub name: String,
    pub username: String,
    pub about: String,
    pub phone: String,
    pub status: String,
    pub members: i32,
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
}

/// Фрагмент форматирования в символах (не в UTF-16, как у Telegram).
#[derive(Debug, Clone, PartialEq, Eq)]
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
            _ => continue,
        };
        let (start, end) = (to_char(off), to_char(off + len));
        if end > start {
            out.push(Entity { kind, start, end, url });
        }
    }
    out
}

/// Сообщение для интерфейса.
#[derive(Debug, Clone, PartialEq, Eq)]
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

    /// «Прочитать на сервере»: отметка о прочтении отправляется всегда, даже в режиме призрака.
    pub async fn force_read(&self, peer: PeerRef) -> Result<(), grammers_client::InvocationError> {
        self.client.mark_as_read(peer).await
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
            let (name, peer) = match &d.peer {
                tl::enums::Peer::User(u) => {
                    let Some(user) = users.iter().find(|x| matches!(x, tl::enums::User::User(y) if y.id == u.user_id)) else { continue };
                    let name = match user {
                        tl::enums::User::User(y) => {
                            kind = if y.bot { DialogKind::Bot } else { DialogKind::User };
                            contact = y.contact;
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
                        Some((m.message.chars().take(80).collect::<String>(), i64::from(m.date)))
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
            out.push(DialogRow {
                peer: d.peer_ref(),
                id: d.peer_id().bot_api_dialog_id_unchecked(),
                name: d.peer().name().unwrap_or("Без имени").to_owned(),
                last: d
                    .last_message
                    .as_ref()
                    .map(|m| m.text().chars().take(80).collect())
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

    /// Сведения о чате для правой панели (имя, @имя, о себе, телефон, статус, число участников).
    pub async fn peer_info(&self, peer: PeerRef) -> Result<PeerInfo, grammers_client::InvocationError> {
        use grammers_client::tl;
        let mut info = PeerInfo::default();
        match tl::enums::InputPeer::from(peer) {
            tl::enums::InputPeer::User(u) => {
                let tl::enums::users::UserFull::Full(f) = self
                    .client
                    .invoke(&tl::functions::users::GetFullUser {
                        id: tl::enums::InputUser::User(tl::types::InputUser {
                            user_id: u.user_id,
                            access_hash: u.access_hash,
                        }),
                    })
                    .await?;
                let tl::enums::UserFull::Full(full) = f.full_user;
                info.about = full.about.unwrap_or_default();
                if let Some(tl::enums::User::User(x)) = f.users.iter().find(|x| matches!(x, tl::enums::User::User(y) if y.id == u.user_id)) {
                    info.name = [x.first_name.clone(), x.last_name.clone()]
                        .into_iter()
                        .flatten()
                        .collect::<Vec<_>>()
                        .join(" ");
                    info.username = x.username.clone().unwrap_or_default();
                    info.phone = x.phone.clone().unwrap_or_default();
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
                }
                if let Some(tl::enums::Chat::Channel(y)) = f.chats.iter().find(|x| matches!(x, tl::enums::Chat::Channel(y) if y.id == c.channel_id)) {
                    info.name = y.title.clone();
                    info.username = y.username.clone().unwrap_or_default();
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
        let is_thumb = matches!(info.kind, MediaKind::Video | MediaKind::Gif | MediaKind::Round);
        if !info.has_preview() || (!is_thumb && info.size > PREVIEW_LIMIT) {
            return Ok(None);
        }
        match &media {
            // У видео, GIF и кружков для предпросмотра берём самую большую скачиваемую миниатюру.
            grammers_client::media::Media::Document(d) if is_thumb => {
                use grammers_client::media::Downloadable;
                let thumbs = d.thumbs();
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
                    done += chunk.len() as u64;
                    progress(done.min(total), total);
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

    /// Создать канал (`broadcast`) или супергруппу.
    pub async fn create_channel(&self, title: &str, about: &str, broadcast: bool) -> Result<(), grammers_client::InvocationError> {
        use grammers_client::tl;
        self.client
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
    pub current: bool,
    pub device: String,
    pub app: String,
    pub platform: String,
    pub place: String,
    pub ip: String,
    pub active: i64,
}

/// Строка приватности: название и краткое описание правила («Все», «Мои контакты» …).
#[derive(Debug, Clone)]
pub struct PrivacyRow {
    pub title: &'static str,
    pub rule: String,
}

/// Уведомления Telegram для типа чатов: включены ли и показывается ли текст.
#[derive(Debug, Clone)]
pub struct NotifyRow {
    pub title: &'static str,
    pub enabled: bool,
    pub preview: bool,
}

/// Всё, что нужно страницам «Настройки Telegram»: данные аккаунта, приватность, уведомления,
/// заблокированные, сеансы. Только чтение.
#[derive(Debug, Clone, Default)]
pub struct AccountOverview {
    pub me: PeerInfo,
    pub privacy: Vec<PrivacyRow>,
    pub notify: Vec<NotifyRow>,
    pub blocked: Vec<String>,
    pub sessions: Vec<SessionRow>,
    pub session_ttl_days: i32,
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

    /// Загружает обзор аккаунта для настроек Telegram. Все запросы — чтение.
    pub async fn account_overview(&self) -> Result<AccountOverview, grammers_client::InvocationError> {
        use grammers_client::tl;
        let mut out = AccountOverview::default();
        if let Ok(me) = self.client.get_me().await
            && let Ok(peer) = me.to_ref().await
            && let Some(peer) = peer
        {
            out.me = self.peer_info(peer).await?;
        }
        let keys: [(&'static str, tl::enums::InputPrivacyKey); 6] = [
            ("Номер телефона", tl::enums::InputPrivacyKey::PhoneNumber),
            ("Последняя активность", tl::enums::InputPrivacyKey::StatusTimestamp),
            ("Фото профиля", tl::enums::InputPrivacyKey::ProfilePhoto),
            ("Пересланные сообщения", tl::enums::InputPrivacyKey::Forwards),
            ("Звонки", tl::enums::InputPrivacyKey::PhoneCall),
            ("Группы и каналы", tl::enums::InputPrivacyKey::ChatInvite),
        ];
        for (title, key) in keys {
            let rule = match self.client.invoke(&tl::functions::account::GetPrivacy { key }).await {
                Ok(tl::enums::account::PrivacyRules::Rules(r)) => privacy_text(&r.rules),
                Err(_) => "—".to_owned(),
            };
            out.privacy.push(PrivacyRow { title, rule });
        }
        let notify: [(&'static str, tl::enums::InputNotifyPeer); 3] = [
            ("Личные чаты", tl::enums::InputNotifyPeer::InputNotifyUsers),
            ("Группы", tl::enums::InputNotifyPeer::InputNotifyChats),
            ("Каналы", tl::enums::InputNotifyPeer::InputNotifyBroadcasts),
        ];
        for (title, peer) in notify {
            if let Ok(tl::enums::PeerNotifySettings::Settings(s)) =
                self.client.invoke(&tl::functions::account::GetNotifySettings { peer }).await
            {
                out.notify.push(NotifyRow {
                    title,
                    enabled: s.mute_until.unwrap_or(0) == 0,
                    preview: s.show_previews.unwrap_or(true),
                });
            }
        }
        if let Ok(b) = self
            .client
            .invoke(&tl::functions::contacts::GetBlocked { my_stories_from: false, offset: 0, limit: 100 })
            .await
        {
            let users = match b {
                tl::enums::contacts::Blocked::Blocked(x) => x.users,
                tl::enums::contacts::Blocked::Slice(x) => x.users,
            };
            for u in users {
                if let tl::enums::User::User(u) = u {
                    let n = format!("{} {}", u.first_name.unwrap_or_default(), u.last_name.unwrap_or_default());
                    out.blocked.push(n.trim().to_owned());
                }
            }
        }
        let tl::enums::account::Authorizations::Authorizations(a) =
            self.client.invoke(&tl::functions::account::GetAuthorizations {}).await?;
        out.session_ttl_days = a.authorization_ttl_days;
        for x in a.authorizations {
            let tl::enums::Authorization::Authorization(x) = x;
            out.sessions.push(SessionRow {
                current: x.current,
                device: x.device_model,
                app: format!("{} {}", x.app_name, x.app_version),
                platform: format!("{} {}", x.platform, x.system_version),
                place: [x.region, x.country].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(", "),
                ip: x.ip,
                active: i64::from(x.date_active),
            });
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
}
