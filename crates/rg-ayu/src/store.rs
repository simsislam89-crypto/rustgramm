//! Локальное хранилище: тексты сообщений, пометки об удалении и история правок.
//! Файл SQLite лежит только у пользователя; в сеть ничего не уходит.
//! Используется тот же `libsql`, что и в сессии grammers: второй SQLite в программу не тащим.

use libsql::{Builder, Connection, Row, params};

type Result<T> = std::result::Result<T, libsql::Error>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Saved {
    pub peer: i64,
    pub msg_id: i32,
    pub text: String,
    pub date: i64,
    pub deleted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedAudit {
    pub msg_id: i32,
    pub text: String,
    pub date: i64,
    pub deleted: bool,
    pub edits: Vec<(String, i64)>,
}

/// Запись секретного чата E2E в локальной БД слота (ключ хранится только на этом устройстве).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretChatRecord {
    pub chat_id: i32,
    pub access_hash: i64,
    pub peer_user_id: i64,
    pub peer_name: String,
    pub auth_key: Vec<u8>,
    pub key_fingerprint: i64,
    pub ttl_secs: i32,
    /// `waiting` | `requested` | `ready` | `discarded`
    pub state: String,
    pub created_at: i64,
    pub is_admin: bool,
}

/// Расшифрованное сообщение секретного чата с поддержкой таймера самоуничтожения (`expires_at`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretMessageRecord {
    pub id: i64,
    pub chat_id: i32,
    pub random_id: i64,
    pub outgoing: bool,
    pub text: String,
    pub date: i64,
    pub ttl_secs: i32,
    pub expires_at: i64,
}

/// Граница идентификаторов каналов в формате Bot API (-100…).
const CHANNEL_BOUND: i64 = -1_000_000_000_000;

pub struct MessageStore {
    _db: libsql::Database,
    conn: Connection,
}

impl MessageStore {
    pub async fn open(path: &str) -> Result<Self> {
        Self::init(Builder::new_local(path).build().await?).await
    }

    pub async fn in_memory() -> Result<Self> {
        Self::init(Builder::new_local(":memory:").build().await?).await
    }

    async fn init(db: libsql::Database) -> Result<Self> {
        let conn = db.connect()?;
        // Журнал WAL и настройки скорости записи без лишних fsync на каждый коммит.
        let _ = conn.query("PRAGMA journal_mode=WAL", ()).await;
        let _ = conn.execute_batch(
            "PRAGMA synchronous=NORMAL;
             PRAGMA busy_timeout=5000;
             PRAGMA temp_store=MEMORY;
             PRAGMA cache_size=-8000;",
        ).await;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS messages(
                 peer INTEGER NOT NULL, msg_id INTEGER NOT NULL,
                 text TEXT NOT NULL, date INTEGER NOT NULL,
                 deleted INTEGER NOT NULL DEFAULT 0,
                 PRIMARY KEY(peer, msg_id));
             CREATE INDEX IF NOT EXISTS idx_messages_msg_peer ON messages(msg_id, peer);
             CREATE INDEX IF NOT EXISTS idx_messages_peer_del_date ON messages(peer, deleted, date);
             CREATE TABLE IF NOT EXISTS edits(
                 peer INTEGER NOT NULL, msg_id INTEGER NOT NULL,
                 rev INTEGER NOT NULL, old_text TEXT NOT NULL, edited_at INTEGER NOT NULL,
                 PRIMARY KEY(peer, msg_id, rev));
             CREATE TABLE IF NOT EXISTS secret_chats(
                 chat_id INTEGER PRIMARY KEY,
                 access_hash INTEGER NOT NULL,
                 peer_user_id INTEGER NOT NULL,
                 peer_name TEXT NOT NULL,
                 auth_key BLOB NOT NULL,
                 key_fingerprint INTEGER NOT NULL,
                 ttl_secs INTEGER NOT NULL DEFAULT 0,
                 state TEXT NOT NULL,
                 created_at INTEGER NOT NULL,
                 is_admin INTEGER NOT NULL DEFAULT 0);
             CREATE TABLE IF NOT EXISTS secret_messages(
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 chat_id INTEGER NOT NULL,
                 random_id INTEGER NOT NULL,
                 outgoing INTEGER NOT NULL,
                 text TEXT NOT NULL,
                 date INTEGER NOT NULL,
                 ttl_secs INTEGER NOT NULL DEFAULT 0,
                 expires_at INTEGER NOT NULL DEFAULT 0);
             CREATE INDEX IF NOT EXISTS idx_secret_msgs_chat_date ON secret_messages(chat_id, date, id);
             CREATE INDEX IF NOT EXISTS idx_secret_msgs_expires ON secret_messages(expires_at);",
        )
        .await?;
        Ok(Self { _db: db, conn })
    }

    /// Запомнить сообщение (новое или обновлённое).
    pub async fn remember(&self, peer: i64, msg_id: i32, text: &str, date: i64) -> Result<()> {
        self.conn
            .execute(
                "INSERT INTO messages(peer,msg_id,text,date) VALUES(?1,?2,?3,?4)
                 ON CONFLICT(peer,msg_id) DO UPDATE SET text=excluded.text",
                params![peer, i64::from(msg_id), text, date],
            )
            .await?;
        Ok(())
    }

    /// Запомнить пачку сообщений одной транзакцией (в разы быстрее поштучной записи).
    pub async fn remember_many(&self, peer: i64, items: &[(i32, String, i64)]) -> Result<()> {
        if items.is_empty() {
            return Ok(());
        }
        self.conn.execute("BEGIN", ()).await?;
        for (id, text, date) in items {
            let r = self
                .conn
                .execute(
                    "INSERT INTO messages(peer,msg_id,text,date) VALUES(?1,?2,?3,?4)
                     ON CONFLICT(peer,msg_id) DO UPDATE SET text=excluded.text",
                    params![peer, i64::from(*id), text.as_str(), *date],
                )
                .await;
            if let Err(e) = r {
                let _ = self.conn.execute("ROLLBACK", ()).await;
                return Err(e);
            }
        }
        self.conn.execute("COMMIT", ()).await?;
        Ok(())
    }

    /// Сколько прежних версий есть у каждого сообщения чата одним запросом.
    pub async fn edit_counts(&self, peer: i64) -> Result<std::collections::HashMap<i32, usize>> {
        let mut out = std::collections::HashMap::new();
        let mut rows = self
            .conn
            .query(
                "SELECT msg_id, COUNT(*) FROM edits WHERE peer=?1 GROUP BY msg_id",
                params![peer],
            )
            .await?;
        while let Some(r) = rows.next().await? {
            let id: i64 = r.get(0)?;
            let n: i64 = r.get(1)?;
            out.insert(id as i32, n.max(0) as usize);
        }
        Ok(out)
    }

    /// Пометить удалёнными одной транзакцией; возвращает, сколько из них у нас сохранено.
    pub async fn mark_deleted(&self, peer: i64, ids: &[i32]) -> Result<u64> {
        if ids.is_empty() {
            return Ok(0);
        }
        self.conn.execute("BEGIN", ()).await?;
        let mut n = 0;
        for id in ids {
            match self
                .conn
                .execute(
                    "UPDATE messages SET deleted=1 WHERE peer=?1 AND msg_id=?2",
                    params![peer, i64::from(*id)],
                )
                .await
            {
                Ok(v) => n += v,
                Err(e) => {
                    let _ = self.conn.execute("ROLLBACK", ()).await;
                    return Err(e);
                }
            }
        }
        self.conn.execute("COMMIT", ()).await?;
        Ok(n)
    }

    /// Удаление без указания чата (личные чаты и обычные группы) одной транзакцией.
    pub async fn mark_deleted_private(&self, ids: &[i32]) -> Result<u64> {
        if ids.is_empty() {
            return Ok(0);
        }
        self.conn.execute("BEGIN", ()).await?;
        let mut n = 0;
        for id in ids {
            match self
                .conn
                .execute(
                    "UPDATE messages SET deleted=1 WHERE msg_id=?1 AND peer>?2",
                    params![i64::from(*id), CHANNEL_BOUND],
                )
                .await
            {
                Ok(v) => n += v,
                Err(e) => {
                    let _ = self.conn.execute("ROLLBACK", ()).await;
                    return Err(e);
                }
            }
        }
        self.conn.execute("COMMIT", ()).await?;
        Ok(n)
    }

    /// Правка: старый текст уходит в историю, новый запоминается.
    pub async fn record_edit(&self, peer: i64, msg_id: i32, new_text: &str, at: i64) -> Result<()> {
        let id = i64::from(msg_id);
        if let Some(old) = self.get(peer, msg_id).await?
            && old.text != new_text
        {
            let mut rows = self
                .conn
                .query(
                    "SELECT COALESCE(MAX(rev),0)+1 FROM edits WHERE peer=?1 AND msg_id=?2",
                    params![peer, id],
                )
                .await?;
            let rev: i64 = match rows.next().await? {
                Some(r) => r.get(0)?,
                None => 1,
            };
            self.conn
                .execute(
                    "INSERT INTO edits(peer,msg_id,rev,old_text,edited_at) VALUES(?1,?2,?3,?4,?5)",
                    params![peer, id, rev, old.text, at],
                )
                .await?;
        }
        self.conn
            .execute(
                "UPDATE messages SET text=?3 WHERE peer=?1 AND msg_id=?2",
                params![peer, id, new_text],
            )
            .await?;
        Ok(())
    }

    pub async fn get(&self, peer: i64, msg_id: i32) -> Result<Option<Saved>> {
        let mut rows = self
            .conn
            .query(
                "SELECT peer,msg_id,text,date,deleted FROM messages WHERE peer=?1 AND msg_id=?2",
                params![peer, i64::from(msg_id)],
            )
            .await?;
        match rows.next().await? {
            Some(r) => Ok(Some(row(&r)?)),
            None => Ok(None),
        }
    }

    /// Удалённые сообщения чата, от старых к новым.
    pub async fn deleted(&self, peer: i64) -> Result<Vec<Saved>> {
        let mut rows = self
            .conn
            .query(
                "SELECT peer,msg_id,text,date,deleted FROM messages
                 WHERE peer=?1 AND deleted=1 ORDER BY date, msg_id",
                params![peer],
            )
            .await?;
        let mut out = Vec::new();
        while let Some(r) = rows.next().await? {
            out.push(row(&r)?);
        }
        Ok(out)
    }

    /// Прежние версии текста, от первой к последней.
    pub async fn history(&self, peer: i64, msg_id: i32) -> Result<Vec<String>> {
        let mut rows = self
            .conn
            .query(
                "SELECT old_text FROM edits WHERE peer=?1 AND msg_id=?2 ORDER BY rev",
                params![peer, i64::from(msg_id)],
            )
            .await?;
        let mut out = Vec::new();
        while let Some(r) = rows.next().await? {
            out.push(r.get(0)?);
        }
        Ok(out)
    }

    /// Удалить сохранённое старше `before` (unix-время), кроме помеченных удалёнными.
    pub async fn prune(&self, before: i64) -> Result<u64> {
        self.conn
            .execute(
                "DELETE FROM messages WHERE deleted=0 AND date<?1",
                params![before],
            )
            .await
    }

    /// Окончательно удалить уже удалённое сообщение только у себя из локальной базы, чтобы оно больше не показывалось.
    pub async fn purge_deleted(&self, peer: i64, msg_id: i32) -> Result<u64> {
        let id = i64::from(msg_id);
        let _ = self
            .conn
            .execute(
                "DELETE FROM edits WHERE peer=?1 AND msg_id=?2",
                params![peer, id],
            )
            .await;
        self.conn
            .execute(
                "DELETE FROM messages WHERE peer=?1 AND msg_id=?2 AND deleted=1",
                params![peer, id],
            )
            .await
    }

    /// Окончательно убрать все удалённые сообщения данного чата из локальной базы.
    pub async fn purge_all_deleted(&self, peer: i64) -> Result<u64> {
        let _ = self
            .conn
            .execute(
                "DELETE FROM edits WHERE peer=?1 AND msg_id IN (SELECT msg_id FROM messages WHERE peer=?1 AND deleted=1)",
                params![peer],
            )
            .await;
        self.conn
            .execute(
                "DELETE FROM messages WHERE peer=?1 AND deleted=1",
                params![peer],
            )
            .await
    }

    /// Все сохранённые удалённые или изменённые сообщения чата с историей правок (`ayu/ui/message_history`) без N+1 запросов.
    pub async fn audit_peer(&self, peer: i64) -> Result<Vec<SavedAudit>> {
        let mut edits_by_msg: std::collections::HashMap<i32, Vec<(String, i64)>> =
            std::collections::HashMap::new();
        let mut erows = self
            .conn
            .query(
                "SELECT msg_id, old_text, edited_at FROM edits WHERE peer=?1 ORDER BY msg_id, rev",
                params![peer],
            )
            .await?;
        while let Some(er) = erows.next().await? {
            let mid = i32::try_from(er.get::<i64>(0)?).unwrap_or_default();
            edits_by_msg.entry(mid).or_default().push((er.get(1)?, er.get(2)?));
        }

        let mut rows = self
            .conn
            .query(
                "SELECT m.msg_id, m.text, m.date, m.deleted FROM messages m
                 WHERE m.peer=?1 AND (m.deleted=1 OR EXISTS(
                     SELECT 1 FROM edits e WHERE e.peer=m.peer AND e.msg_id=m.msg_id
                 ))
                 ORDER BY m.date DESC, m.msg_id DESC",
                params![peer],
            )
            .await?;
        let mut out = Vec::new();
        while let Some(r) = rows.next().await? {
            let msg_id = i32::try_from(r.get::<i64>(0)?).unwrap_or_default();
            let text: String = r.get(1)?;
            let date: i64 = r.get(2)?;
            let deleted = r.get::<i64>(3)? != 0;
            let edits = edits_by_msg.remove(&msg_id).unwrap_or_default();
            out.push(SavedAudit { msg_id, text, date, deleted, edits });
        }
        Ok(out)
    }

    /// Глобальный поиск по всем сохранённым удалённым сообщениям во всех чатах (от свежих к старым).
    pub async fn search_deleted_global(&self, query: &str, limit: usize) -> Result<Vec<Saved>> {
        let q = query.trim().to_lowercase();
        let cap = limit.clamp(1, 500) as i64;
        let mut out = Vec::new();
        if q.is_empty() {
            let mut rows = self
                .conn
                .query(
                    "SELECT peer,msg_id,text,date,deleted FROM messages
                     WHERE deleted=1 ORDER BY date DESC, msg_id DESC LIMIT ?1",
                    params![cap],
                )
                .await?;
            while let Some(r) = rows.next().await? {
                out.push(row(&r)?);
            }
        } else {
            let mut rows = self
                .conn
                .query(
                    "SELECT peer,msg_id,text,date,deleted FROM messages
                     WHERE deleted=1 ORDER BY date DESC, msg_id DESC",
                    (),
                )
                .await?;
            while let Some(r) = rows.next().await? {
                let item = row(&r)?;
                if item.text.to_lowercase().contains(&q) {
                    out.push(item);
                    if out.len() >= limit.clamp(1, 500) {
                        break;
                    }
                }
            }
        }
        Ok(out)
    }

    /// Общая статистика локального хранилища: `(всего_сообщений, удалено, записей_правок)`.
    pub async fn stats(&self) -> Result<(usize, usize, usize)> {
        let mut total = 0usize;
        let mut deleted = 0usize;
        let mut edits = 0usize;
        let mut r1 = self
            .conn
            .query("SELECT COUNT(*), COALESCE(SUM(CASE WHEN deleted!=0 THEN 1 ELSE 0 END),0) FROM messages", ())
            .await?;
        if let Some(r) = r1.next().await? {
            total = usize::try_from(r.get::<i64>(0)?).unwrap_or(0);
            deleted = usize::try_from(r.get::<i64>(1)?).unwrap_or(0);
        }
        let mut r2 = self.conn.query("SELECT COUNT(*) FROM edits", ()).await?;
        if let Some(r) = r2.next().await? {
            edits = usize::try_from(r.get::<i64>(0)?).unwrap_or(0);
        }
        Ok((total, deleted, edits))
    }

    /// Сохранить или обновить секретный чат E2E.
    pub async fn save_secret_chat(&self, c: &SecretChatRecord) -> Result<()> {
        self.conn
            .execute(
                "INSERT INTO secret_chats(chat_id,access_hash,peer_user_id,peer_name,auth_key,key_fingerprint,ttl_secs,state,created_at,is_admin)
                 VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)
                 ON CONFLICT(chat_id) DO UPDATE SET
                   access_hash=excluded.access_hash,
                   peer_user_id=excluded.peer_user_id,
                   peer_name=CASE WHEN excluded.peer_name!='' THEN excluded.peer_name ELSE secret_chats.peer_name END,
                   auth_key=CASE WHEN length(excluded.auth_key)>0 THEN excluded.auth_key ELSE secret_chats.auth_key END,
                   key_fingerprint=CASE WHEN excluded.key_fingerprint!=0 THEN excluded.key_fingerprint ELSE secret_chats.key_fingerprint END,
                   ttl_secs=excluded.ttl_secs,
                   state=excluded.state,
                   is_admin=excluded.is_admin",
                params![
                    i64::from(c.chat_id),
                    c.access_hash,
                    c.peer_user_id,
                    c.peer_name.as_str(),
                    c.auth_key.clone(),
                    c.key_fingerprint,
                    i64::from(c.ttl_secs),
                    c.state.as_str(),
                    c.created_at,
                    i64::from(u8::from(c.is_admin)),
                ],
            )
            .await?;
        Ok(())
    }

    /// Список всех локальных секретных чатов.
    pub async fn list_secret_chats(&self) -> Result<Vec<SecretChatRecord>> {
        let mut rows = self
            .conn
            .query(
                "SELECT chat_id,access_hash,peer_user_id,peer_name,auth_key,key_fingerprint,ttl_secs,state,created_at,is_admin
                 FROM secret_chats ORDER BY created_at DESC",
                (),
            )
            .await?;
        let mut out = Vec::new();
        while let Some(r) = rows.next().await? {
            out.push(SecretChatRecord {
                chat_id: i32::try_from(r.get::<i64>(0)?).unwrap_or_default(),
                access_hash: r.get(1)?,
                peer_user_id: r.get(2)?,
                peer_name: r.get(3)?,
                auth_key: r.get(4)?,
                key_fingerprint: r.get(5)?,
                ttl_secs: i32::try_from(r.get::<i64>(6)?).unwrap_or_default(),
                state: r.get(7)?,
                created_at: r.get(8)?,
                is_admin: r.get::<i64>(9)? != 0,
            });
        }
        Ok(out)
    }

    /// Обновить таймер самоуничтожения секретного чата.
    pub async fn set_secret_ttl(&self, chat_id: i32, ttl_secs: i32) -> Result<()> {
        self.conn
            .execute(
                "UPDATE secret_chats SET ttl_secs=?1 WHERE chat_id=?2",
                params![i64::from(ttl_secs), i64::from(chat_id)],
            )
            .await?;
        Ok(())
    }

    /// Полностью удалить секретный чат и его сообщения с устройства.
    pub async fn delete_secret_chat(&self, chat_id: i32) -> Result<()> {
        self.conn
            .execute("DELETE FROM secret_messages WHERE chat_id=?1", params![i64::from(chat_id)])
            .await?;
        self.conn
            .execute("DELETE FROM secret_chats WHERE chat_id=?1", params![i64::from(chat_id)])
            .await?;
        Ok(())
    }

    /// Сохранить сообщение секретного чата.
    pub async fn save_secret_message(&self, m: &SecretMessageRecord) -> Result<()> {
        self.conn
            .execute(
                "INSERT INTO secret_messages(chat_id,random_id,outgoing,text,date,ttl_secs,expires_at)
                 VALUES(?1,?2,?3,?4,?5,?6,?7)",
                params![
                    i64::from(m.chat_id),
                    m.random_id,
                    i64::from(u8::from(m.outgoing)),
                    m.text.as_str(),
                    m.date,
                    i64::from(m.ttl_secs),
                    m.expires_at,
                ],
            )
            .await?;
        Ok(())
    }

    /// Удалить просроченные самоуничтожающиеся сообщения (`expires_at > 0 && expires_at <= now`).
    pub async fn purge_expired_secret_messages(&self, now: i64) -> Result<u64> {
        self.conn
            .execute(
                "DELETE FROM secret_messages WHERE expires_at > 0 AND expires_at <= ?1",
                params![now],
            )
            .await
    }

    /// Загрузить актуальные сообщения секретного чата (с очисткой просроченных).
    pub async fn secret_messages(&self, chat_id: i32, now: i64) -> Result<Vec<SecretMessageRecord>> {
        let _ = self.purge_expired_secret_messages(now).await;
        let mut rows = self
            .conn
            .query(
                "SELECT id,chat_id,random_id,outgoing,text,date,ttl_secs,expires_at
                 FROM secret_messages WHERE chat_id=?1 ORDER BY date ASC, id ASC",
                params![i64::from(chat_id)],
            )
            .await?;
        let mut out = Vec::new();
        while let Some(r) = rows.next().await? {
            out.push(SecretMessageRecord {
                id: r.get(0)?,
                chat_id: i32::try_from(r.get::<i64>(1)?).unwrap_or_default(),
                random_id: r.get(2)?,
                outgoing: r.get::<i64>(3)? != 0,
                text: r.get(4)?,
                date: r.get(5)?,
                ttl_secs: i32::try_from(r.get::<i64>(6)?).unwrap_or_default(),
                expires_at: r.get(7)?,
            });
        }
        Ok(out)
    }
}

fn row(r: &Row) -> Result<Saved> {
    Ok(Saved {
        peer: r.get(0)?,
        msg_id: i32::try_from(r.get::<i64>(1)?).unwrap_or_default(),
        text: r.get(2)?,
        date: r.get(3)?,
        deleted: r.get::<i64>(4)? != 0,
    })
}

/// Экспорт журнала удалённых и изменённых сообщений чата в читаемый TXT.
pub fn export_audit_txt(chat_title: &str, items: &[SavedAudit]) -> String {
    let mut out = String::new();
    out.push_str(&format!("Журнал удалённых и правок: {chat_title}\nЗаписей: {}\n\n", items.len()));
    for e in items {
        let kind = if e.deleted { "УДАЛЕНО" } else { "ИЗМЕНЕНО" };
        out.push_str(&format!("[{kind}] #{} (unix={})\n", e.msg_id, e.date));
        for (idx, (old, at)) in e.edits.iter().enumerate() {
            out.push_str(&format!("  Версия {} (unix={}): {}\n", idx + 1, at, old));
        }
        out.push_str(&format!("  Текст: {}\n\n", e.text));
    }
    out
}

/// Экспорт журнала удалённых и изменённых сообщений чата в JSON.
pub fn export_audit_json(chat_title: &str, items: &[SavedAudit]) -> String {
    let entries: Vec<serde_json::Value> = items
        .iter()
        .map(|e| {
            let edits: Vec<serde_json::Value> = e
                .edits
                .iter()
                .map(|(old, at)| serde_json::json!({ "text": old, "edited_at": at }))
                .collect();
            serde_json::json!({
                "msg_id": e.msg_id,
                "date": e.date,
                "deleted": e.deleted,
                "text": e.text,
                "edits": edits,
            })
        })
        .collect();
    serde_json::to_string_pretty(&serde_json::json!({
        "chat": chat_title,
        "count": entries.len(),
        "items": entries,
    }))
    .unwrap_or_else(|_| "{}".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn deleted_message_stays_readable() {
        let s = MessageStore::in_memory().await.unwrap();
        s.remember(1, 10, "привет", 100).await.unwrap();
        s.remember(1, 11, "второе", 101).await.unwrap();
        assert_eq!(s.mark_deleted(1, &[10, 99]).await.unwrap(), 1);
        let d = s.deleted(1).await.unwrap();
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].text, "привет");
        assert!(d[0].deleted);
        assert!(s.deleted(2).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn private_deletion_ignores_channels() {
        let s = MessageStore::in_memory().await.unwrap();
        s.remember(42, 7, "dm", 1).await.unwrap();
        s.remember(-1_000_000_000_123, 7, "chan", 1).await.unwrap();
        assert_eq!(s.mark_deleted_private(&[7]).await.unwrap(), 1);
        assert!(s.get(42, 7).await.unwrap().unwrap().deleted);
        assert!(!s.get(-1_000_000_000_123, 7).await.unwrap().unwrap().deleted);
    }

    #[tokio::test]
    async fn edits_keep_history() {
        let s = MessageStore::in_memory().await.unwrap();
        s.remember(5, 1, "v1", 10).await.unwrap();
        s.record_edit(5, 1, "v2", 20).await.unwrap();
        s.record_edit(5, 1, "v2", 21).await.unwrap(); // без изменений — не копится
        s.record_edit(5, 1, "v3", 30).await.unwrap();
        assert_eq!(s.history(5, 1).await.unwrap(), vec!["v1", "v2"]);
        assert_eq!(s.get(5, 1).await.unwrap().unwrap().text, "v3");
    }

    #[tokio::test]
    async fn prune_keeps_deleted() {
        let s = MessageStore::in_memory().await.unwrap();
        s.remember(1, 1, "old", 10).await.unwrap();
        s.remember(1, 2, "old deleted", 10).await.unwrap();
        s.mark_deleted(1, &[2]).await.unwrap();
        assert_eq!(s.prune(50).await.unwrap(), 1);
        assert!(s.get(1, 1).await.unwrap().is_none());
        assert!(s.get(1, 2).await.unwrap().is_some());
    }

    #[tokio::test]
    async fn remember_does_not_resurrect_deleted_flag() {
        let s = MessageStore::in_memory().await.unwrap();
        s.remember(1, 1, "a", 10).await.unwrap();
        s.mark_deleted(1, &[1]).await.unwrap();
        s.remember(1, 1, "a", 10).await.unwrap();
        assert!(s.get(1, 1).await.unwrap().unwrap().deleted);
    }

    #[tokio::test]
    async fn remember_many_writes_batch_and_counts_edits() {
        let s = MessageStore::in_memory().await.unwrap();
        s.remember_many(5, &[(1, "a".into(), 10), (2, "b".into(), 11)]).await.unwrap();
        assert_eq!(s.get(5, 2).await.unwrap().unwrap().text, "b");
        s.record_edit(5, 1, "a2", 20).await.unwrap();
        let c = s.edit_counts(5).await.unwrap();
        assert_eq!(c.get(&1), Some(&1));
        assert_eq!(c.get(&2), None);
    }

    #[tokio::test]
    async fn audit_peer_combines_deleted_and_edited_entries() {
        let s = MessageStore::in_memory().await.unwrap();
        s.remember(9, 100, "draft", 10).await.unwrap();
        s.record_edit(9, 100, "edited", 20).await.unwrap();
        s.remember(9, 101, "gone", 30).await.unwrap();
        s.mark_deleted(9, &[101]).await.unwrap();
        s.remember(9, 102, "untouched", 40).await.unwrap();
        let audit = s.audit_peer(9).await.unwrap();
        assert_eq!(audit.len(), 2);
        assert_eq!(audit[0].msg_id, 101);
        assert!(audit[0].deleted);
        assert_eq!(audit[1].msg_id, 100);
        assert_eq!(audit[1].edits, vec![("draft".to_owned(), 20)]);
    }

    #[tokio::test]
    async fn purge_deleted_removes_only_target_deleted_message() {
        let s = MessageStore::in_memory().await.unwrap();
        s.remember(10, 1, "keep_live", 10).await.unwrap();
        s.remember(10, 2, "del_a", 11).await.unwrap();
        s.remember(10, 3, "del_b", 12).await.unwrap();
        s.mark_deleted(10, &[2, 3]).await.unwrap();
        // Живое сообщение (msg_id=1) purge_deleted не трогает.
        assert_eq!(s.purge_deleted(10, 1).await.unwrap(), 0);
        assert!(s.get(10, 1).await.unwrap().is_some());
        // Удалённое msg_id=2 стирается у себя навсегда.
        assert_eq!(s.purge_deleted(10, 2).await.unwrap(), 1);
        assert!(s.get(10, 2).await.unwrap().is_none());
        assert_eq!(s.deleted(10).await.unwrap().len(), 1);
        // Массовая очистка удалённых стирает оставшиеся deleted=1, сохраняя живое.
        assert_eq!(s.purge_all_deleted(10).await.unwrap(), 1);
        assert!(s.deleted(10).await.unwrap().is_empty());
        assert!(s.get(10, 1).await.unwrap().is_some());
    }

    #[tokio::test]
    async fn secret_chat_storage_and_ttl_purge_work() {
        let s = MessageStore::in_memory().await.unwrap();
        let rec = SecretChatRecord {
            chat_id: 777,
            access_hash: 123456789,
            peer_user_id: 42,
            peer_name: "Алиса".to_owned(),
            auth_key: vec![0xAB; 256],
            key_fingerprint: 987654321,
            ttl_secs: 15,
            state: "ready".to_owned(),
            created_at: 1000,
            is_admin: true,
        };
        s.save_secret_chat(&rec).await.unwrap();
        let chats = s.list_secret_chats().await.unwrap();
        assert_eq!(chats.len(), 1);
        assert_eq!(chats[0].peer_name, "Алиса");
        assert_eq!(chats[0].auth_key.len(), 256);

        s.save_secret_message(&SecretMessageRecord {
            id: 0,
            chat_id: 777,
            random_id: 1,
            outgoing: true,
            text: "самоуничтожится".to_owned(),
            date: 1000,
            ttl_secs: 15,
            expires_at: 1015,
        })
        .await
        .unwrap();
        s.save_secret_message(&SecretMessageRecord {
            id: 0,
            chat_id: 777,
            random_id: 2,
            outgoing: false,
            text: "вечное".to_owned(),
            date: 1001,
            ttl_secs: 0,
            expires_at: 0,
        })
        .await
        .unwrap();
        assert_eq!(s.secret_messages(777, 1010).await.unwrap().len(), 2);
        let after = s.secret_messages(777, 1020).await.unwrap();
        assert_eq!(after.len(), 1);
        assert_eq!(after[0].text, "вечное");

        s.delete_secret_chat(777).await.unwrap();
        assert!(s.list_secret_chats().await.unwrap().is_empty());
        assert!(s.secret_messages(777, 1020).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn sqlite_indexes_present_for_messages_and_secrets() {
        let s = MessageStore::in_memory().await.unwrap();
        let mut rows = s
            .conn
            .query("SELECT name FROM sqlite_master WHERE type='index'", ())
            .await
            .unwrap();
        let mut names = Vec::new();
        while let Some(r) = rows.next().await.unwrap() {
            let n: String = r.get(0).unwrap();
            names.push(n);
        }
        for req in [
            "idx_messages_msg_peer",
            "idx_messages_peer_del_date",
            "idx_secret_msgs_chat_date",
            "idx_secret_msgs_expires",
        ] {
            assert!(
                names.iter().any(|n| n == req),
                "missing expected SQLite index {req} in {names:?}"
            );
        }
    }

    #[tokio::test]
    async fn global_deleted_search_stats_and_exports_work() {
        let s = MessageStore::in_memory().await.unwrap();
        s.remember(101, 1, "секретный тест", 1000).await.unwrap();
        s.remember(101, 2, "обычное сообщение", 1005).await.unwrap();
        s.remember(202, 5, "другой удалённый тест", 1020).await.unwrap();
        s.record_edit(101, 2, "обновлённое сообщение", 1010).await.unwrap();
        s.mark_deleted(101, &[1]).await.unwrap();
        s.mark_deleted(202, &[5]).await.unwrap();

        let (total, deleted, edits) = s.stats().await.unwrap();
        assert_eq!((total, deleted, edits), (3, 2, 1));

        let all_del = s.search_deleted_global("", 50).await.unwrap();
        assert_eq!(all_del.len(), 2);
        assert_eq!(all_del[0].peer, 202);

        let filtered = s.search_deleted_global("СЕКРЕТ", 50).await.unwrap();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].msg_id, 1);

        let audit = s.audit_peer(101).await.unwrap();
        let txt = export_audit_txt("Тест чат", &audit);
        assert!(txt.contains("УДАЛЕНО") && txt.contains("ИЗМЕНЕНО"));
        let js = export_audit_json("Тест чат", &audit);
        let parsed: serde_json::Value = serde_json::from_str(&js).unwrap();
        assert_eq!(parsed["count"], 2);
    }
}
