//! Persistent memory v0: SQLite on disk, auto-migrated, survives restarts.
//! Tier one of the memory architecture — durable structured store for
//! conversation history and key-value profile facts. Exportable and wipeable:
//! the user's data stays under the user's control.

use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, thiserror::Error)]
pub enum MemoryError {
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone, Serialize)]
pub struct StoredMessage {
    pub id: i64,
    pub role: String,
    pub content: String,
    pub created_at: i64,
}

/// A distilled lesson from experience (§5.2): what worked, what failed,
/// what to avoid — not just facts about the user.
#[derive(Debug, Clone, Serialize)]
pub struct Insight {
    pub id: i64,
    pub kind: String,
    pub content: String,
    pub source: String,
    pub created_at: i64,
}

pub struct MemoryStore {
    conn: Connection,
    path: PathBuf,
}

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

impl MemoryStore {
    /// Opens (creating if needed) the store at `path` and applies migrations.
    pub fn open(path: &Path) -> Result<Self, MemoryError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        // WAL keeps reads fast while the app writes; returns a row, so query it.
        let _: String = conn.query_row("PRAGMA journal_mode=WAL", [], |r| r.get(0))?;
        Self::migrate(&conn)?;
        Ok(Self {
            conn,
            path: path.to_path_buf(),
        })
    }

    /// Versioned, additive migrations so upgrades never lose data.
    fn migrate(conn: &Connection) -> Result<(), MemoryError> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS schema_version (version INTEGER NOT NULL);",
        )?;
        let current: i64 = conn
            .query_row("SELECT MAX(version) FROM schema_version", [], |r| {
                r.get::<_, Option<i64>>(0)
            })?
            .unwrap_or(0);
        if current < 1 {
            conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS facts (
                     key        TEXT PRIMARY KEY,
                     value      TEXT NOT NULL,
                     updated_at INTEGER NOT NULL
                 );
                 CREATE TABLE IF NOT EXISTS messages (
                     id         INTEGER PRIMARY KEY AUTOINCREMENT,
                     role       TEXT NOT NULL,
                     content    TEXT NOT NULL,
                     created_at INTEGER NOT NULL
                 );
                 INSERT INTO schema_version (version) VALUES (1);",
            )?;
        }
        if current < 2 {
            conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS insights (
                     id         INTEGER PRIMARY KEY AUTOINCREMENT,
                     kind       TEXT NOT NULL,
                     content    TEXT NOT NULL,
                     source     TEXT NOT NULL DEFAULT '',
                     created_at INTEGER NOT NULL
                 );
                 INSERT INTO schema_version (version) VALUES (2);",
            )?;
        }
        Ok(())
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn set_fact(&self, key: &str, value: &str) -> Result<(), MemoryError> {
        self.conn.execute(
            "INSERT INTO facts (key, value, updated_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(key) DO UPDATE SET value = ?2, updated_at = ?3",
            params![key, value, now_unix()],
        )?;
        Ok(())
    }

    pub fn get_fact(&self, key: &str) -> Result<Option<String>, MemoryError> {
        Ok(self
            .conn
            .query_row(
                "SELECT value FROM facts WHERE key = ?1",
                params![key],
                |r| r.get(0),
            )
            .optional()?)
    }

    pub fn fact_count(&self) -> Result<u64, MemoryError> {
        Ok(self
            .conn
            .query_row("SELECT COUNT(*) FROM facts", [], |r| r.get::<_, i64>(0))? as u64)
    }

    pub fn append_message(&self, role: &str, content: &str) -> Result<i64, MemoryError> {
        self.conn.execute(
            "INSERT INTO messages (role, content, created_at) VALUES (?1, ?2, ?3)",
            params![role, content, now_unix()],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Last `limit` messages in chronological order.
    pub fn recent_messages(&self, limit: usize) -> Result<Vec<StoredMessage>, MemoryError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, role, content, created_at FROM messages
             ORDER BY id DESC LIMIT ?1",
        )?;
        let mut rows: Vec<StoredMessage> = stmt
            .query_map(params![limit as i64], |r| {
                Ok(StoredMessage {
                    id: r.get(0)?,
                    role: r.get(1)?,
                    content: r.get(2)?,
                    created_at: r.get(3)?,
                })
            })?
            .collect::<Result<_, _>>()?;
        rows.reverse();
        Ok(rows)
    }

    pub fn add_insight(&self, kind: &str, content: &str, source: &str) -> Result<i64, MemoryError> {
        self.conn.execute(
            "INSERT INTO insights (kind, content, source, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![kind, content, source, now_unix()],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Newest lessons first — the freshest experience matters most.
    pub fn recent_insights(&self, limit: usize) -> Result<Vec<Insight>, MemoryError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, kind, content, source, created_at FROM insights
             ORDER BY id DESC LIMIT ?1",
        )?;
        let rows: Vec<Insight> = stmt
            .query_map(params![limit as i64], |r| {
                Ok(Insight {
                    id: r.get(0)?,
                    kind: r.get(1)?,
                    content: r.get(2)?,
                    source: r.get(3)?,
                    created_at: r.get(4)?,
                })
            })?
            .collect::<Result<_, _>>()?;
        Ok(rows)
    }

    pub fn insight_count(&self) -> Result<u64, MemoryError> {
        Ok(self
            .conn
            .query_row("SELECT COUNT(*) FROM insights", [], |r| r.get::<_, i64>(0))?
            as u64)
    }

    pub fn message_count(&self) -> Result<u64, MemoryError> {
        Ok(self
            .conn
            .query_row("SELECT COUNT(*) FROM messages", [], |r| r.get::<_, i64>(0))?
            as u64)
    }

    /// Full dump — the user's data is theirs to take elsewhere.
    pub fn export_json(&self) -> Result<serde_json::Value, MemoryError> {
        let messages = self.recent_messages(usize::MAX / 2)?;
        let mut stmt = self
            .conn
            .prepare("SELECT key, value, updated_at FROM facts ORDER BY key")?;
        let facts: Vec<serde_json::Value> = stmt
            .query_map([], |r| {
                Ok(serde_json::json!({
                    "key": r.get::<_, String>(0)?,
                    "value": r.get::<_, String>(1)?,
                    "updated_at": r.get::<_, i64>(2)?,
                }))
            })?
            .collect::<Result<_, _>>()?;
        let insights = self.recent_insights(usize::MAX / 2)?;
        Ok(serde_json::json!({
            "schema_version": 2,
            "exported_at": now_unix(),
            "facts": facts,
            "messages": messages,
            "insights": insights,
        }))
    }

    /// Deletes all user data but keeps the schema — the user's call, always.
    pub fn wipe(&self) -> Result<(), MemoryError> {
        self.conn
            .execute_batch("DELETE FROM facts; DELETE FROM messages; DELETE FROM insights;")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_survives_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("jarvis.sqlite3");
        {
            let store = MemoryStore::open(&db).unwrap();
            store.set_fact("user.name", "Hotragn").unwrap();
            store.append_message("user", "remember me").unwrap();
            store.append_message("assistant", "I will.").unwrap();
        } // store dropped — simulates app shutdown

        let store = MemoryStore::open(&db).unwrap(); // simulates relaunch
        assert_eq!(
            store.get_fact("user.name").unwrap().as_deref(),
            Some("Hotragn")
        );
        let history = store.recent_messages(10).unwrap();
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].role, "user");
        assert_eq!(history[1].content, "I will.");
    }

    #[test]
    fn migrations_are_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("m.sqlite3");
        for _ in 0..3 {
            let store = MemoryStore::open(&db).unwrap();
            let (rows, version): (i64, i64) = store
                .conn
                .query_row(
                    "SELECT COUNT(*), MAX(version) FROM schema_version",
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .unwrap();
            assert_eq!(version, 2, "schema is at the current version");
            assert_eq!(rows, 2, "one row per migration step, never re-run");
        }
    }

    #[test]
    fn recent_messages_are_chronological_and_limited() {
        let dir = tempfile::tempdir().unwrap();
        let store = MemoryStore::open(&dir.path().join("c.sqlite3")).unwrap();
        for i in 0..30 {
            store.append_message("user", &format!("msg {i}")).unwrap();
        }
        let recent = store.recent_messages(5).unwrap();
        assert_eq!(recent.len(), 5);
        assert_eq!(recent[0].content, "msg 25");
        assert_eq!(recent[4].content, "msg 29");
    }

    #[test]
    fn facts_upsert_instead_of_duplicating() {
        let dir = tempfile::tempdir().unwrap();
        let store = MemoryStore::open(&dir.path().join("f.sqlite3")).unwrap();
        store.set_fact("theme", "dark").unwrap();
        store.set_fact("theme", "light").unwrap();
        assert_eq!(store.get_fact("theme").unwrap().as_deref(), Some("light"));
        assert_eq!(store.fact_count().unwrap(), 1);
    }

    #[test]
    fn export_then_wipe() {
        let dir = tempfile::tempdir().unwrap();
        let store = MemoryStore::open(&dir.path().join("e.sqlite3")).unwrap();
        store.set_fact("k", "v").unwrap();
        store.append_message("user", "hello").unwrap();
        store
            .add_insight("skill", "tests catch bugs", "events 1..5")
            .unwrap();

        let dump = store.export_json().unwrap();
        assert_eq!(dump["facts"][0]["key"], "k");
        assert_eq!(dump["messages"][0]["content"], "hello");
        assert_eq!(dump["insights"][0]["content"], "tests catch bugs");

        store.wipe().unwrap();
        assert_eq!(store.message_count().unwrap(), 0);
        assert_eq!(store.fact_count().unwrap(), 0);
        assert_eq!(store.insight_count().unwrap(), 0);
    }

    #[test]
    fn insights_roundtrip_newest_first() {
        let dir = tempfile::tempdir().unwrap();
        let store = MemoryStore::open(&dir.path().join("i.sqlite3")).unwrap();
        store
            .add_insight("provider", "ollama answers fast", "events 1..3")
            .unwrap();
        store
            .add_insight("skill", "avoid ${} in rhai", "events 4..9")
            .unwrap();

        let recent = store.recent_insights(10).unwrap();
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0].content, "avoid ${} in rhai", "newest first");
        assert_eq!(recent[1].kind, "provider");
    }

    #[test]
    fn v1_database_migrates_to_v2_without_losing_data() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("old.sqlite3");
        {
            // Hand-build a v1 database, as shipped in the bootstrap release.
            let conn = Connection::open(&db).unwrap();
            conn.execute_batch(
                "CREATE TABLE schema_version (version INTEGER NOT NULL);
                 CREATE TABLE facts (key TEXT PRIMARY KEY, value TEXT NOT NULL, updated_at INTEGER NOT NULL);
                 CREATE TABLE messages (id INTEGER PRIMARY KEY AUTOINCREMENT, role TEXT NOT NULL, content TEXT NOT NULL, created_at INTEGER NOT NULL);
                 INSERT INTO schema_version (version) VALUES (1);
                 INSERT INTO facts VALUES ('user.name', 'Hotragn', 0);
                 INSERT INTO messages (role, content, created_at) VALUES ('user', 'old message', 0);",
            )
            .unwrap();
        }

        let store = MemoryStore::open(&db).unwrap(); // runs v2 migration
        assert_eq!(
            store.get_fact("user.name").unwrap().as_deref(),
            Some("Hotragn"),
            "v1 data survives the upgrade"
        );
        assert_eq!(store.message_count().unwrap(), 1);
        store.add_insight("general", "works", "").unwrap();
        assert_eq!(store.insight_count().unwrap(), 1);
    }
}
