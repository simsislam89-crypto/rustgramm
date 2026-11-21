//! Локальное хранилище: тексты сообщений, пометки об удалении и история правок.
//! Файл SQLite лежит только у пользователя; в сеть ничего не уходит.

use rusqlite::{Connection, OptionalExtension, params};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Saved {
    pub peer: i64,
    pub msg_id: i32,
    pub text: String,
    pub date: i64,
    pub deleted: bool,
}

pub struct MessageStore {
    db: Connection,
}

impl MessageStore {
    pub fn open(path: &str) -> rusqlite::Result<Self> {
        Self::init(Connection::open(path)?)
    }

    pub fn in_memory() -> rusqlite::Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(db: Connection) -> rusqlite::Result<Self> {
        db.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA synchronous=NORMAL;
             CREATE TABLE IF NOT EXISTS messages(
                 peer INTEGER NOT NULL, msg_id INTEGER NOT NULL,
                 text TEXT NOT NULL, date INTEGER NOT NULL,
                 deleted INTEGER NOT NULL DEFAULT 0,
                 PRIMARY KEY(peer, msg_id));
             CREATE TABLE IF NOT EXISTS edits(
                 peer INTEGER NOT NULL, msg_id INTEGER NOT NULL,
                 rev INTEGER NOT NULL, old_text TEXT NOT NULL, edited_at INTEGER NOT NULL,
                 PRIMARY KEY(peer, msg_id, rev));",
        )?;
        Ok(Self { db })
    }

    /// Запомнить сообщение (новое или обновлённое).
    pub fn remember(&self, peer: i64, msg_id: i32, text: &str, date: i64) -> rusqlite::Result<()> {
        self.db.execute(
            "INSERT INTO messages(peer,msg_id,text,date) VALUES(?1,?2,?3,?4)
             ON CONFLICT(peer,msg_id) DO UPDATE SET text=excluded.text",
            params![peer, msg_id, text, date],
        )?;
        Ok(())
    }

    /// Пометить удалёнными; возвращает, сколько из них у нас сохранено.
    pub fn mark_deleted(&self, peer: i64, ids: &[i32]) -> rusqlite::Result<usize> {
        let mut n = 0;
        let mut st = self
            .db
            .prepare("UPDATE messages SET deleted=1 WHERE peer=?1 AND msg_id=?2")?;
        for id in ids {
            n += st.execute(params![peer, id])?;
        }
        Ok(n)
    }

    /// Правка: старый текст уходит в историю, новый запоминается.
    pub fn record_edit(
        &self,
        peer: i64,
        msg_id: i32,
        new_text: &str,
        at: i64,
    ) -> rusqlite::Result<()> {
        if let Some(old) = self.get(peer, msg_id)?
            && old.text != new_text
        {
            let rev: i64 = self.db.query_row(
                "SELECT COALESCE(MAX(rev),0)+1 FROM edits WHERE peer=?1 AND msg_id=?2",
                params![peer, msg_id],
                |r| r.get(0),
            )?;
            self.db.execute(
                "INSERT INTO edits(peer,msg_id,rev,old_text,edited_at) VALUES(?1,?2,?3,?4,?5)",
                params![peer, msg_id, rev, old.text, at],
            )?;
        }
        self.db.execute(
            "UPDATE messages SET text=?3 WHERE peer=?1 AND msg_id=?2",
            params![peer, msg_id, new_text],
        )?;
        Ok(())
    }

    pub fn get(&self, peer: i64, msg_id: i32) -> rusqlite::Result<Option<Saved>> {
        self.db
            .query_row(
                "SELECT peer,msg_id,text,date,deleted FROM messages WHERE peer=?1 AND msg_id=?2",
                params![peer, msg_id],
                row,
            )
            .optional()
    }

    /// Удалённые сообщения чата, от старых к новым.
    pub fn deleted(&self, peer: i64) -> rusqlite::Result<Vec<Saved>> {
        let mut st = self.db.prepare(
            "SELECT peer,msg_id,text,date,deleted FROM messages
             WHERE peer=?1 AND deleted=1 ORDER BY date, msg_id",
        )?;
        st.query_map(params![peer], row)?.collect()
    }

    /// Прежние версии текста, от первой к последней.
    pub fn history(&self, peer: i64, msg_id: i32) -> rusqlite::Result<Vec<String>> {
        let mut st = self
            .db
            .prepare("SELECT old_text FROM edits WHERE peer=?1 AND msg_id=?2 ORDER BY rev")?;
        st.query_map(params![peer, msg_id], |r| r.get(0))?.collect()
    }

    /// Удалить сохранённое старше `before` (unix-время), кроме помеченных удалёнными.
    pub fn prune(&self, before: i64) -> rusqlite::Result<usize> {
        self.db.execute(
            "DELETE FROM messages WHERE deleted=0 AND date<?1",
            params![before],
        )
    }
}

fn row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Saved> {
    Ok(Saved {
        peer: r.get(0)?,
        msg_id: r.get(1)?,
        text: r.get(2)?,
        date: r.get(3)?,
        deleted: r.get::<_, i64>(4)? != 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deleted_message_stays_readable() {
        let s = MessageStore::in_memory().unwrap();
        s.remember(1, 10, "привет", 100).unwrap();
        s.remember(1, 11, "второе", 101).unwrap();
        assert_eq!(s.mark_deleted(1, &[10, 99]).unwrap(), 1);
        let d = s.deleted(1).unwrap();
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].text, "привет");
        assert!(d[0].deleted);
        assert!(s.deleted(2).unwrap().is_empty());
    }

    #[test]
    fn edits_keep_history() {
        let s = MessageStore::in_memory().unwrap();
        s.remember(5, 1, "v1", 10).unwrap();
        s.record_edit(5, 1, "v2", 20).unwrap();
        s.record_edit(5, 1, "v2", 21).unwrap(); // без изменений — не копится
        s.record_edit(5, 1, "v3", 30).unwrap();
        assert_eq!(s.history(5, 1).unwrap(), vec!["v1", "v2"]);
        assert_eq!(s.get(5, 1).unwrap().unwrap().text, "v3");
    }

    #[test]
    fn prune_keeps_deleted() {
        let s = MessageStore::in_memory().unwrap();
        s.remember(1, 1, "old", 10).unwrap();
        s.remember(1, 2, "old deleted", 10).unwrap();
        s.mark_deleted(1, &[2]).unwrap();
        assert_eq!(s.prune(50).unwrap(), 1);
        assert!(s.get(1, 1).unwrap().is_none());
        assert!(s.get(1, 2).unwrap().is_some());
    }

    #[test]
    fn remember_does_not_resurrect_deleted_flag() {
        let s = MessageStore::in_memory().unwrap();
        s.remember(1, 1, "a", 10).unwrap();
        s.mark_deleted(1, &[1]).unwrap();
        s.remember(1, 1, "a", 10).unwrap();
        assert!(s.get(1, 1).unwrap().unwrap().deleted);
    }
}
