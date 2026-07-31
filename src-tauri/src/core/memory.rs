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
    /// Times a later reflection re-derived this same lesson (Reflection v1).
    pub corroborations: u32,
    /// Times it has been injected into a prompt.
    pub uses: u32,
    /// When the app decided to drop this lesson, if it has. `None` means live.
    /// Forgetting is a soft delete so the user can see it and disagree.
    pub forgotten_at: Option<i64>,
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
        if current < 3 {
            // Reflection v1 bookkeeping: what a lesson has earned, so scoring
            // and selective forgetting have something to work from. Additive
            // with defaults, so existing insights keep working untouched.
            conn.execute_batch(
                "ALTER TABLE insights ADD COLUMN corroborations INTEGER NOT NULL DEFAULT 0;
                 ALTER TABLE insights ADD COLUMN uses INTEGER NOT NULL DEFAULT 0;
                 INSERT INTO schema_version (version) VALUES (3);",
            )?;
        }
        if current < 4 {
            // Semantic recall: one embedding per message, stored as an LE f32
            // BLOB. Keyed by message id; the model column exists so vectors
            // from different embedding models are never compared to each other.
            conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS embeddings (
                     message_id INTEGER PRIMARY KEY,
                     model      TEXT NOT NULL,
                     vector     BLOB NOT NULL,
                     created_at INTEGER NOT NULL
                 );
                 INSERT INTO schema_version (version) VALUES (4);",
            )?;
        }
        if current < 5 {
            // Reflection v2. Two additions:
            //
            // `forgotten_at` makes forgetting a soft delete. A lesson the app
            // decided to drop on its own should be inspectable and reversible,
            // and a DELETE gave the user no way to disagree.
            //
            // `insight_embeddings` mirrors the message embeddings table, with
            // the same `model` column for the same reason: vectors from
            // different embedding models must never be compared.
            conn.execute_batch(
                "ALTER TABLE insights ADD COLUMN forgotten_at INTEGER;
                 CREATE TABLE IF NOT EXISTS insight_embeddings (
                     insight_id INTEGER PRIMARY KEY,
                     model      TEXT NOT NULL,
                     vector     BLOB NOT NULL,
                     created_at INTEGER NOT NULL
                 );
                 INSERT INTO schema_version (version) VALUES (5);",
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

    /// Credits a lesson for being independently re-derived (Reflection v1).
    pub fn corroborate_insight(&self, id: i64) -> Result<(), MemoryError> {
        self.conn.execute(
            "UPDATE insights SET corroborations = corroborations + 1 WHERE id = ?1",
            params![id],
        )?;
        Ok(())
    }

    /// Records that these lessons were used in a prompt, which feeds scoring.
    pub fn mark_insights_used(&self, ids: &[i64]) -> Result<(), MemoryError> {
        for id in ids {
            self.conn.execute(
                "UPDATE insights SET uses = uses + 1 WHERE id = ?1",
                params![id],
            )?;
        }
        Ok(())
    }

    /// Forgets a lesson. Selective forgetting is the point of Reflection v1;
    /// the caller logs what went and why, so it stays auditable.
    /// Marks a lesson forgotten. A soft delete on purpose: the app decided this
    /// on its own, so the user needs to be able to see it and disagree.
    pub fn forget_insight(&self, id: i64) -> Result<(), MemoryError> {
        self.conn.execute(
            "UPDATE insights SET forgotten_at = ?2 WHERE id = ?1 AND forgotten_at IS NULL",
            params![id, now_unix()],
        )?;
        Ok(())
    }

    /// Stores (or replaces) the embedding for a message. Replacing matters:
    /// re-indexing with a different model must overwrite, never duplicate.
    pub fn upsert_embedding(
        &self,
        message_id: i64,
        model: &str,
        vector: &[f32],
    ) -> Result<(), MemoryError> {
        self.conn.execute(
            "INSERT INTO embeddings (message_id, model, vector, created_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(message_id) DO UPDATE SET model = ?2, vector = ?3, created_at = ?4",
            params![
                message_id,
                model,
                crate::core::embedding::to_bytes(vector),
                now_unix()
            ],
        )?;
        Ok(())
    }

    /// Every stored vector for one model. Vectors written by other models are
    /// skipped, not compared: cross-model cosine is meaningless.
    pub fn embeddings_for_model(&self, model: &str) -> Result<Vec<(i64, Vec<f32>)>, MemoryError> {
        let mut stmt = self
            .conn
            .prepare("SELECT message_id, vector FROM embeddings WHERE model = ?1")?;
        let rows: Vec<(i64, Vec<u8>)> = stmt
            .query_map(params![model], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<Result<_, _>>()?;
        Ok(rows
            .into_iter()
            .filter_map(|(id, blob)| crate::core::embedding::from_bytes(&blob).map(|v| (id, v)))
            .collect())
    }

    /// Message ids that have no embedding yet — the backfill work list.
    pub fn unembedded_message_ids(&self, limit: usize) -> Result<Vec<i64>, MemoryError> {
        let mut stmt = self.conn.prepare(
            "SELECT m.id FROM messages m
             LEFT JOIN embeddings e ON e.message_id = m.id
             WHERE e.message_id IS NULL
             ORDER BY m.id DESC LIMIT ?1",
        )?;
        let rows: Vec<i64> = stmt
            .query_map(params![limit as i64], |r| r.get(0))?
            .collect::<Result<_, _>>()?;
        Ok(rows)
    }

    /// Fetches specific messages by id (recall hits), in the order given.
    pub fn messages_by_ids(&self, ids: &[i64]) -> Result<Vec<StoredMessage>, MemoryError> {
        let mut out = Vec::with_capacity(ids.len());
        for id in ids {
            let row = self
                .conn
                .query_row(
                    "SELECT id, role, content, created_at FROM messages WHERE id = ?1",
                    params![id],
                    |r| {
                        Ok(StoredMessage {
                            id: r.get(0)?,
                            role: r.get(1)?,
                            content: r.get(2)?,
                            created_at: r.get(3)?,
                        })
                    },
                )
                .optional()?;
            if let Some(m) = row {
                out.push(m);
            }
        }
        Ok(out)
    }

    pub fn embedding_count(&self) -> Result<u64, MemoryError> {
        Ok(self
            .conn
            .query_row("SELECT COUNT(*) FROM embeddings", [], |r| {
                r.get::<_, i64>(0)
            })? as u64)
    }

    /// Newest lessons first — the freshest experience matters most.
    pub fn recent_insights(&self, limit: usize) -> Result<Vec<Insight>, MemoryError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, kind, content, source, created_at, corroborations, uses, forgotten_at
             FROM insights WHERE forgotten_at IS NULL
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
                    corroborations: r.get(5)?,
                    uses: r.get(6)?,
                    forgotten_at: r.get(7)?,
                })
            })?
            .collect::<Result<_, _>>()?;
        Ok(rows)
    }

    /// Brings a forgotten lesson back. Returns whether it was actually forgotten,
    /// so the caller can tell "restored" from "there was nothing to restore".
    pub fn restore_insight(&self, id: i64) -> Result<bool, MemoryError> {
        let affected = self.conn.execute(
            "UPDATE insights SET forgotten_at = NULL WHERE id = ?1 AND forgotten_at IS NOT NULL",
            params![id],
        )?;
        Ok(affected > 0)
    }

    /// Lessons the app dropped, most recently forgotten first.
    pub fn forgotten_insights(&self, limit: usize) -> Result<Vec<Insight>, MemoryError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, kind, content, source, created_at, corroborations, uses, forgotten_at
             FROM insights WHERE forgotten_at IS NOT NULL
             ORDER BY forgotten_at DESC, id DESC LIMIT ?1",
        )?;
        let rows: Vec<Insight> = stmt
            .query_map(params![limit as i64], |r| {
                Ok(Insight {
                    id: r.get(0)?,
                    kind: r.get(1)?,
                    content: r.get(2)?,
                    source: r.get(3)?,
                    created_at: r.get(4)?,
                    corroborations: r.get(5)?,
                    uses: r.get(6)?,
                    forgotten_at: r.get(7)?,
                })
            })?
            .collect::<Result<_, _>>()?;
        Ok(rows)
    }

    /// Stores a lesson's embedding. Replaces any existing one, so re-indexing
    /// after a model change overwrites rather than accumulating.
    pub fn set_insight_embedding(
        &self,
        insight_id: i64,
        model: &str,
        vector: &[f32],
    ) -> Result<(), MemoryError> {
        self.conn.execute(
            "INSERT INTO insight_embeddings (insight_id, model, vector, created_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(insight_id) DO UPDATE SET model = ?2, vector = ?3, created_at = ?4",
            params![
                insight_id,
                model,
                crate::core::embedding::to_bytes(vector),
                now_unix()
            ],
        )?;
        Ok(())
    }

    pub fn insight_embedding(&self, insight_id: i64) -> Result<Option<Vec<f32>>, MemoryError> {
        let raw: Option<Vec<u8>> = self
            .conn
            .query_row(
                "SELECT vector FROM insight_embeddings WHERE insight_id = ?1",
                params![insight_id],
                |r| r.get(0),
            )
            .optional()?;
        Ok(raw.and_then(|b| crate::core::embedding::from_bytes(&b)))
    }

    /// Live lessons with no embedding yet, for the backfill pass.
    pub fn unembedded_insight_ids(&self, limit: usize) -> Result<Vec<i64>, MemoryError> {
        let mut stmt = self.conn.prepare(
            "SELECT i.id FROM insights i
             LEFT JOIN insight_embeddings e ON e.insight_id = i.id
             WHERE e.insight_id IS NULL AND i.forgotten_at IS NULL
             ORDER BY i.id DESC LIMIT ?1",
        )?;
        let ids: Vec<i64> = stmt
            .query_map(params![limit as i64], |r| r.get(0))?
            .collect::<Result<_, _>>()?;
        Ok(ids)
    }

    pub fn insight_count(&self) -> Result<u64, MemoryError> {
        Ok(self.conn.query_row(
            "SELECT COUNT(*) FROM insights WHERE forgotten_at IS NULL",
            [],
            |r| r.get::<_, i64>(0),
        )? as u64)
    }

    /// Removes one message (undo support). Returns whether a row existed.
    pub fn delete_message(&self, id: i64) -> Result<bool, MemoryError> {
        let affected = self
            .conn
            .execute("DELETE FROM messages WHERE id = ?1", params![id])?;
        Ok(affected > 0)
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
        self.conn.execute_batch(
            "DELETE FROM facts; DELETE FROM messages; DELETE FROM insights;
                 DELETE FROM insight_embeddings;",
        )?;
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
            // Bump both when a migration is added: v5 adds soft-deleted lessons
            // and the lesson embeddings table.
            assert_eq!(version, 5, "schema is at the current version");
            assert_eq!(rows, 5, "one row per migration step, never re-run");
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
        // Reflection v1 bookkeeping starts at zero.
        assert_eq!(recent[0].corroborations, 0);
        assert_eq!(recent[0].uses, 0);
    }

    #[test]
    fn insight_scoring_bookkeeping_accumulates_and_forgetting_removes() {
        let dir = tempfile::tempdir().unwrap();
        let store = MemoryStore::open(&dir.path().join("m.sqlite3")).unwrap();
        let keep = store
            .add_insight("user", "prefers short answers", "e1")
            .unwrap();
        let drop = store
            .add_insight("provider", "groq was slow", "e2")
            .unwrap();

        store.corroborate_insight(keep).unwrap();
        store.corroborate_insight(keep).unwrap();
        store.mark_insights_used(&[keep, drop]).unwrap();

        let all = store.recent_insights(10).unwrap();
        let kept = all.iter().find(|i| i.id == keep).unwrap();
        assert_eq!(kept.corroborations, 2, "corroborations should accumulate");
        assert_eq!(kept.uses, 1);

        store.forget_insight(drop).unwrap();
        let after = store.recent_insights(10).unwrap();
        assert_eq!(after.len(), 1);
        assert_eq!(after[0].id, keep, "only the forgotten one should be gone");
    }

    #[test]
    fn embeddings_roundtrip_replace_and_report_coverage() {
        let dir = tempfile::tempdir().unwrap();
        let store = MemoryStore::open(&dir.path().join("m.sqlite3")).unwrap();
        let a = store
            .append_message("user", "the sky was clear last night")
            .unwrap();
        let b = store
            .append_message("assistant", "good for the telescope")
            .unwrap();

        store
            .upsert_embedding(a, "test-model", &[1.0, 0.0])
            .unwrap();
        store
            .upsert_embedding(b, "test-model", &[0.0, 1.0])
            .unwrap();
        assert_eq!(store.embedding_count().unwrap(), 2);

        let vecs = store.embeddings_for_model("test-model").unwrap();
        assert_eq!(vecs.len(), 2);
        assert!(vecs.iter().any(|(id, v)| *id == a && *v == vec![1.0, 0.0]));

        // Replacement, not duplication — and other models don't cross-read.
        store
            .upsert_embedding(a, "other-model", &[0.5, 0.5])
            .unwrap();
        assert_eq!(
            store.embedding_count().unwrap(),
            2,
            "upsert must not duplicate"
        );
        assert_eq!(store.embeddings_for_model("test-model").unwrap().len(), 1);
        assert_eq!(store.embeddings_for_model("other-model").unwrap().len(), 1);
    }

    #[test]
    fn unembedded_backlog_shrinks_as_vectors_land() {
        let dir = tempfile::tempdir().unwrap();
        let store = MemoryStore::open(&dir.path().join("m.sqlite3")).unwrap();
        let a = store.append_message("user", "one").unwrap();
        let _b = store.append_message("user", "two").unwrap();
        assert_eq!(store.unembedded_message_ids(10).unwrap().len(), 2);
        store.upsert_embedding(a, "m", &[1.0]).unwrap();
        let left = store.unembedded_message_ids(10).unwrap();
        assert_eq!(left.len(), 1);
        assert_ne!(left[0], a);
    }

    #[test]
    fn messages_by_ids_preserves_request_order_and_skips_missing() {
        let dir = tempfile::tempdir().unwrap();
        let store = MemoryStore::open(&dir.path().join("m.sqlite3")).unwrap();
        let a = store.append_message("user", "first").unwrap();
        let b = store.append_message("assistant", "second").unwrap();
        let got = store.messages_by_ids(&[b, 9999, a]).unwrap();
        assert_eq!(got.len(), 2, "missing ids are skipped");
        assert_eq!(got[0].id, b, "order follows the request, not the table");
        assert_eq!(got[1].id, a);
    }

    #[test]
    fn a_v2_database_gains_the_scoring_columns_without_losing_insights() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("legacy.sqlite3");
        // Build a store at v2 exactly as the previous release left it.
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(
                "CREATE TABLE schema_version (version INTEGER NOT NULL);
                 CREATE TABLE insights (
                     id INTEGER PRIMARY KEY AUTOINCREMENT,
                     kind TEXT NOT NULL,
                     content TEXT NOT NULL,
                     source TEXT NOT NULL DEFAULT '',
                     created_at INTEGER NOT NULL
                 );
                 INSERT INTO insights (kind, content, source, created_at)
                     VALUES ('user', 'an older lesson', 'e1', 1700000000);
                 INSERT INTO schema_version (version) VALUES (1);
                 INSERT INTO schema_version (version) VALUES (2);",
            )
            .unwrap();
        }
        // Opening it applies v3.
        let store = MemoryStore::open(&path).unwrap();
        let all = store.recent_insights(10).unwrap();
        assert_eq!(
            all.len(),
            1,
            "the existing lesson must survive the migration"
        );
        assert_eq!(all[0].content, "an older lesson");
        assert_eq!(all[0].corroborations, 0, "new columns default to zero");
        assert_eq!(all[0].uses, 0);
        // And the new bookkeeping works on the migrated row.
        store.corroborate_insight(all[0].id).unwrap();
        assert_eq!(store.recent_insights(10).unwrap()[0].corroborations, 1);
    }

    #[test]
    fn forgetting_is_reversible() {
        // The app decides this on its own, so the user has to be able to see it
        // and disagree. A hard DELETE gave them no way to.
        let dir = tempfile::tempdir().unwrap();
        let store = MemoryStore::open(&dir.path().join("m.sqlite3")).unwrap();
        let id = store
            .add_insight("user", "prefers short answers", "e1")
            .unwrap();
        store.forget_insight(id).unwrap();

        assert!(
            store.recent_insights(10).unwrap().is_empty(),
            "a forgotten lesson is out of the live set"
        );
        assert_eq!(store.insight_count().unwrap(), 0, "and out of the count");
        let gone = store.forgotten_insights(10).unwrap();
        assert_eq!(gone.len(), 1, "but still on the record");
        assert!(gone[0].forgotten_at.is_some());
        assert_eq!(gone[0].content, "prefers short answers");

        assert!(store.restore_insight(id).unwrap());
        assert_eq!(store.recent_insights(10).unwrap().len(), 1);
        assert!(store.forgotten_insights(10).unwrap().is_empty());
        assert!(
            store.recent_insights(10).unwrap()[0].forgotten_at.is_none(),
            "a restored lesson reads as live"
        );
    }

    #[test]
    fn restoring_reports_whether_there_was_anything_to_restore() {
        // "Restored" and "there was nothing to restore" must be distinguishable,
        // or the UI has to guess what happened.
        let dir = tempfile::tempdir().unwrap();
        let store = MemoryStore::open(&dir.path().join("m.sqlite3")).unwrap();
        let id = store.add_insight("user", "a live lesson", "e1").unwrap();
        assert!(
            !store.restore_insight(id).unwrap(),
            "it was never forgotten"
        );
        assert!(!store.restore_insight(9_999).unwrap(), "no such lesson");
    }

    #[test]
    fn forgetting_twice_keeps_the_first_timestamp() {
        // Otherwise a maintenance pass that re-drops an already-dropped lesson
        // would keep bumping it to the top of the forgotten list forever.
        let dir = tempfile::tempdir().unwrap();
        let store = MemoryStore::open(&dir.path().join("m.sqlite3")).unwrap();
        let id = store.add_insight("user", "a lesson", "e1").unwrap();
        store.forget_insight(id).unwrap();
        let first = store.forgotten_insights(10).unwrap()[0].forgotten_at;
        store.forget_insight(id).unwrap();
        assert_eq!(store.forgotten_insights(10).unwrap()[0].forgotten_at, first);
    }

    #[test]
    fn lesson_embeddings_roundtrip_and_backfill_finds_the_gaps() {
        let dir = tempfile::tempdir().unwrap();
        let store = MemoryStore::open(&dir.path().join("m.sqlite3")).unwrap();
        let a = store.add_insight("skill", "lesson a", "e1").unwrap();
        let b = store.add_insight("skill", "lesson b", "e2").unwrap();

        assert_eq!(store.unembedded_insight_ids(10).unwrap().len(), 2);
        store
            .set_insight_embedding(a, "nomic", &[0.5, -0.25])
            .unwrap();
        let got = store.insight_embedding(a).unwrap().unwrap();
        assert_eq!(got, vec![0.5, -0.25], "exact f32 roundtrip");
        assert_eq!(store.unembedded_insight_ids(10).unwrap(), vec![b]);
        assert!(store.insight_embedding(b).unwrap().is_none());

        // Re-indexing replaces rather than accumulating.
        store.set_insight_embedding(a, "nomic", &[1.0]).unwrap();
        assert_eq!(store.insight_embedding(a).unwrap().unwrap(), vec![1.0]);

        // A forgotten lesson isn't work for the backfill pass.
        store.forget_insight(b).unwrap();
        assert!(store.unembedded_insight_ids(10).unwrap().is_empty());
    }

    #[test]
    fn a_v4_database_gains_soft_deletes_without_losing_lessons() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("v4.sqlite3");
        {
            // A v4 database: insights with the v3 scoring columns, no forgotten_at.
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(
                "CREATE TABLE schema_version (version INTEGER NOT NULL);
                 CREATE TABLE facts (key TEXT PRIMARY KEY, value TEXT NOT NULL,
                                     updated_at INTEGER NOT NULL);
                 CREATE TABLE messages (id INTEGER PRIMARY KEY AUTOINCREMENT,
                                        role TEXT NOT NULL, content TEXT NOT NULL,
                                        created_at INTEGER NOT NULL);
                 CREATE TABLE insights (id INTEGER PRIMARY KEY AUTOINCREMENT,
                                        kind TEXT NOT NULL, content TEXT NOT NULL,
                                        source TEXT NOT NULL DEFAULT '',
                                        created_at INTEGER NOT NULL,
                                        corroborations INTEGER NOT NULL DEFAULT 0,
                                        uses INTEGER NOT NULL DEFAULT 0);
                 CREATE TABLE embeddings (message_id INTEGER PRIMARY KEY, model TEXT NOT NULL,
                                          vector BLOB NOT NULL, created_at INTEGER NOT NULL);
                 INSERT INTO insights (kind, content, source, created_at, corroborations, uses)
                     VALUES ('user', 'a lesson from before v5', 'e1', 1, 2, 3);
                 INSERT INTO schema_version (version) VALUES (4);",
            )
            .unwrap();
        }
        let store = MemoryStore::open(&path).unwrap();
        let all = store.recent_insights(10).unwrap();
        assert_eq!(all.len(), 1, "the existing lesson must survive");
        assert_eq!(all[0].content, "a lesson from before v5");
        assert_eq!(all[0].corroborations, 2, "earlier bookkeeping is intact");
        assert_eq!(all[0].uses, 3);
        assert!(
            all[0].forgotten_at.is_none(),
            "a lesson that predates soft deletes reads as live, not as forgotten"
        );
        // And the new behaviour works on the migrated row.
        store.forget_insight(all[0].id).unwrap();
        assert_eq!(store.forgotten_insights(10).unwrap().len(), 1);
        store
            .set_insight_embedding(all[0].id, "nomic", &[0.1])
            .unwrap();
        assert!(store.insight_embedding(all[0].id).unwrap().is_some());
    }

    #[test]
    fn a_wipe_clears_lesson_vectors_too() {
        // Otherwise "wipe my memory" leaves embeddings of the wiped lessons on
        // disk, which is exactly the promise it breaks.
        let dir = tempfile::tempdir().unwrap();
        let store = MemoryStore::open(&dir.path().join("m.sqlite3")).unwrap();
        let id = store.add_insight("user", "a lesson", "e1").unwrap();
        store.set_insight_embedding(id, "nomic", &[0.5]).unwrap();
        store.wipe().unwrap();
        assert!(store.insight_embedding(id).unwrap().is_none());
        assert!(store.forgotten_insights(10).unwrap().is_empty());
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
