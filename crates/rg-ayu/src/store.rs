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
        // Журнал WAL ускоряет запись; ошибка не критична (например, для памяти).
        let _ = conn.query("PRAGMA journal_mode=WAL", ()).await;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS messages(
                 peer INTEGER NOT NULL, msg_id INTEGER NOT NULL,
                 text TEXT NOT NULL, date INTEGER NOT NULL,
                 deleted INTEGER NOT NULL DEFAULT 0,
                 PRIMARY KEY(peer, msg_id));
             CREATE TABLE IF NOT EXISTS edits(
                 peer INTEGER NOT NULL, msg_id INTEGER NOT NULL,
                 rev INTEGER NOT NULL, old_text TEXT NOT NULL, edited_at INTEGER NOT NULL,
                 PRIMARY KEY(peer, msg_id, rev));",
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

    /// Пометить удалёнными; возвращает, сколько из них у нас сохранено.
    pub async fn mark_deleted(&self, peer: i64, ids: &[i32]) -> Result<u64> {
        let mut n = 0;
        for id in ids {
            n += self
                .conn
                .execute(
                    "UPDATE messages SET deleted=1 WHERE peer=?1 AND msg_id=?2",
                    params![peer, i64::from(*id)],
                )
                .await?;
        }
        Ok(n)
    }

    /// Удаление без указания чата (личные чаты и обычные группы): id уникальны на аккаунт.
    pub async fn mark_deleted_private(&self, ids: &[i32]) -> Result<u64> {
        let mut n = 0;
        for id in ids {
            n += self
                .conn
                .execute(
                    "UPDATE messages SET deleted=1 WHERE msg_id=?1 AND peer>?2",
                    params![i64::from(*id), CHANNEL_BOUND],
                )
                .await?;
        }
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
}
