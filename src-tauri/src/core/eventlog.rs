//! Append-only event log — the seed of replay & undo (§5.4). Every action
//! the assistant takes becomes one JSON line in `events.jsonl`. The file is
//! never rewritten: appends only, ids monotonic, corrupt lines skipped on
//! read so one bad write can't poison the history.

use serde::{Deserialize, Serialize};
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub id: u64,
    pub ts: i64,
    pub kind: String,
    pub payload: serde_json::Value,
}

#[derive(Debug, thiserror::Error)]
pub enum EventLogError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),
}

pub struct EventLog {
    path: PathBuf,
    next_id: u64,
}

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn read_events(path: &Path) -> Vec<Event> {
    match File::open(path) {
        Err(_) => Vec::new(),
        Ok(file) => BufReader::new(file)
            .lines()
            .map_while(Result::ok)
            .filter_map(|line| serde_json::from_str::<Event>(&line).ok())
            .collect(),
    }
}

impl EventLog {
    /// Opens (creating if needed) the log and resumes the id sequence.
    pub fn open(path: &Path) -> Result<Self, EventLogError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let next_id = read_events(path)
            .iter()
            .map(|e| e.id)
            .max()
            .map(|max| max + 1)
            .unwrap_or(1);
        Ok(Self {
            path: path.to_path_buf(),
            next_id,
        })
    }

    pub fn append(
        &mut self,
        kind: &str,
        payload: serde_json::Value,
    ) -> Result<Event, EventLogError> {
        let event = Event {
            id: self.next_id,
            ts: now_unix(),
            kind: kind.to_string(),
            payload,
        };
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        let mut line = serde_json::to_string(&event)?;
        line.push('\n');
        file.write_all(line.as_bytes())?;
        self.next_id += 1;
        Ok(event)
    }

    /// Last `limit` events in chronological order.
    pub fn tail(&self, limit: usize) -> Result<Vec<Event>, EventLogError> {
        let all = read_events(&self.path);
        let start = all.len().saturating_sub(limit);
        Ok(all[start..].to_vec())
    }

    pub fn count(&self) -> Result<usize, EventLogError> {
        Ok(read_events(&self.path).len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appends_and_survives_reopen_with_monotonic_ids() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("events.jsonl");
        {
            let mut log = EventLog::open(&path).unwrap();
            let a = log
                .append("app.started", serde_json::json!({"v": "0.1.0"}))
                .unwrap();
            let b = log
                .append("chat.user", serde_json::json!({"text": "hi"}))
                .unwrap();
            assert_eq!((a.id, b.id), (1, 2));
        } // dropped — app shutdown

        let mut log = EventLog::open(&path).unwrap(); // relaunch
        let c = log.append("chat.assistant", serde_json::json!({})).unwrap();
        assert_eq!(c.id, 3, "id sequence must resume, not restart");
        let all = log.tail(10).unwrap();
        assert_eq!(all.len(), 3);
        assert_eq!(all[0].kind, "app.started");
        assert_eq!(all[2].id, 3);
    }

    #[test]
    fn tail_limits_and_keeps_chronological_order() {
        let dir = tempfile::tempdir().unwrap();
        let mut log = EventLog::open(&dir.path().join("e.jsonl")).unwrap();
        for i in 0..10 {
            log.append("tick", serde_json::json!({ "n": i })).unwrap();
        }
        let last3 = log.tail(3).unwrap();
        assert_eq!(last3.len(), 3);
        assert_eq!(last3[0].payload["n"], 7);
        assert_eq!(last3[2].payload["n"], 9);
    }

    #[test]
    fn corrupt_lines_are_skipped_not_fatal() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("e.jsonl");
        let mut log = EventLog::open(&path).unwrap();
        log.append("good", serde_json::json!({})).unwrap();
        // Simulate a torn write.
        let mut f = OpenOptions::new().append(true).open(&path).unwrap();
        f.write_all(b"{\"broken\": tru\n").unwrap();
        drop(f);
        let mut log = EventLog::open(&path).unwrap();
        assert_eq!(log.count().unwrap(), 1);
        let e = log.append("after", serde_json::json!({})).unwrap();
        assert_eq!(e.id, 2, "id continues past the corrupt line");
    }

    #[test]
    fn empty_log_reads_cleanly() {
        let dir = tempfile::tempdir().unwrap();
        let log = EventLog::open(&dir.path().join("none.jsonl")).unwrap();
        assert_eq!(log.count().unwrap(), 0);
        assert!(log.tail(5).unwrap().is_empty());
    }
}
