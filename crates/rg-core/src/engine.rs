//! Движок: высокоуровневые операции клиента с режимом призрака и локальным хранилищем.
//! Любая служебная отправка («прочитано») проходит через политику `rg-ayu::ghost`.

use std::sync::{Arc, RwLock};

use grammers_client::Client;
use grammers_client::update::Update;
use grammers_mtsender::UpdatesConfiguration;
use grammers_session::types::PeerRef;
use grammers_session::updates::UpdatesLike;
use rg_ayu::ghost::{self, Verdict};
use rg_ayu::store::MessageStore;
use rg_settings::Settings;
use tokio::sync::mpsc;

type BoxErr = Box<dyn std::error::Error + Send + Sync>;

/// Строка списка чатов для интерфейса.
#[derive(Debug, Clone)]
pub struct DialogRow {
    pub peer: PeerRef,
    pub id: i64,
    pub name: String,
    pub last: String,
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
}

#[derive(Clone)]
pub struct Engine {
    pub client: Client,
    pub settings: Arc<RwLock<Settings>>,
    store: Arc<MessageStore>,
}

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
        }
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

    pub async fn dialogs(&self, limit: usize) -> Result<Vec<DialogRow>, grammers_client::InvocationError> {
        let mut it = self.client.iter_dialogs();
        let mut out = Vec::new();
        while out.len() < limit {
            let Some(d) = it.next().await? else { break };
            out.push(DialogRow {
                peer: d.peer_ref(),
                id: d.peer_id().bot_api_dialog_id_unchecked(),
                name: d.peer().name().unwrap_or("Без имени").to_owned(),
                last: d
                    .last_message
                    .as_ref()
                    .map(|m| m.text().chars().take(80).collect())
                    .unwrap_or_default(),
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
        let key = peer_key(&peer);
        let mut it = self.client.iter_messages(peer);
        let mut rows = Vec::new();
        while rows.len() < limit {
            let Some(m) = it.next().await? else { break };
            self.remember(key, m.id(), m.text(), m.date().as_second()).await;
            rows.push(MessageRow {
                id: m.id(),
                text: m.text().to_owned(),
                outgoing: m.outgoing(),
                date: m.date().as_second(),
                deleted: false,
                edits: self.edit_count(key, m.id()).await,
            });
        }
        // Удалённые, но сохранённые у нас, возвращаем на место по времени.
        for s in self.deleted(key).await {
            if !rows.iter().any(|r| r.id == s.msg_id) {
                rows.push(MessageRow {
                    id: s.msg_id,
                    text: s.text,
                    outgoing: false,
                    date: s.date,
                    deleted: true,
                    edits: self.edit_count(key, s.msg_id).await,
                });
            }
        }
        rows.sort_by_key(|r| (r.date, r.id));
        Ok(rows)
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
        if self.settings().save_deleted {
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
            Update::NewMessage(m) if cfg.save_deleted => {
                let key = m.peer_id().bot_api_dialog_id_unchecked();
                self.remember(key, m.id(), m.text(), m.date().as_second())
                    .await;
            }
            Update::MessageEdited(m) if cfg.save_edit_history => {
                let key = m.peer_id().bot_api_dialog_id_unchecked();
                let _ = self
                    .store
                    .record_edit(key, m.id(), m.text(), m.date().as_second())
                    .await;
            }
            Update::MessageDeleted(d) if cfg.save_deleted => {
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
