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
            let (name, peer) = match &d.peer {
                tl::enums::Peer::User(u) => {
                    let Some(user) = users.iter().find(|x| matches!(x, tl::enums::User::User(y) if y.id == u.user_id)) else { continue };
                    let name = match user {
                        tl::enums::User::User(y) => {
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
                        tl::enums::Chat::Channel(y) => y.title.clone(),
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
                archived: false,
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

    /// Сообщения старше `before_id` (0 — с самых новых). Для подгрузки при прокрутке вверх.
    pub async fn history_before(
        &self,
        peer: PeerRef,
        before_id: i32,
        limit: usize,
    ) -> Result<Vec<MessageRow>, grammers_client::InvocationError> {
        let key = peer_key(&peer);
        let mut it = self.client.iter_messages(peer).offset_id(before_id);
        let mut rows = Vec::new();
        while rows.len() < limit {
            let Some(m) = it.next().await? else { break };
            self.remember(key, m.id(), m.text(), m.date().as_second()).await;
            rows.push(self.row(key, &m).await);
        }
        if before_id == 0 {
            // Удалённые, но сохранённые у нас, возвращаем на место по времени.
            for s in self.deleted(key).await {
                if !rows.iter().any(|r| r.id == s.msg_id) {
                    let mut r = MessageRow::plain(s.msg_id, s.text, false, s.date);
                    r.deleted = true;
                    r.edits = self.edit_count(key, s.msg_id).await;
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

    async fn row(&self, key: i64, m: &grammers_client::message::Message) -> MessageRow {
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
            edits: self.edit_count(key, m.id()).await,
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
            rows.push(self.row(key, &m).await);
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
        if !info.has_preview() || info.size > PREVIEW_LIMIT {
            return Ok(None);
        }
        self.client.download_media(&media, &file).await?;
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
        let name = match info.kind {
            MediaKind::Photo => format!("photo_{msg_id}.jpg"),
            MediaKind::Sticker => format!("sticker_{msg_id}.webp"),
            _ => media::safe_file_name(&info.name),
        };
        let mut dest = self.dirs.downloads.join(&name);
        if dest.exists() {
            dest = self.dirs.downloads.join(format!("{msg_id}_{name}"));
        }
        self.client.download_media(&media, &dest).await?;
        Ok(Some(dest))
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

    async fn edit_count(&self, peer: i64, id: i32) -> usize {
        self.store.history(peer, id).await.map_or(0, |h| h.len())
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
