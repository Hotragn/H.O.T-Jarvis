//! Replay (§5.4): the event log carries enough state to rebuild the
//! conversation deterministically — no model calls, just recorded facts.
//! v1 ships a *replay audit*: reconstruct what memory should contain from
//! the log alone, diff it against the live database, and report drift.
//! Grounded in the determinism-faithfulness idea from the replayable-agent
//! literature: a replay you can't verify is a story, not a record.

use crate::core::eventlog::Event;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ReplayedMessage {
    pub role: String,
    pub content: String,
}

/// Rebuilds the message sequence from the log: chat events applied in order,
/// undone messages removed, a wipe clearing everything before it.
pub fn rebuild_messages(events: &[Event]) -> Vec<ReplayedMessage> {
    // (memory message id, message)
    let mut timeline: Vec<(Option<i64>, ReplayedMessage)> = Vec::new();
    for event in events {
        let p = &event.payload;
        match event.kind.as_str() {
            "chat.user" | "chat.assistant" => {
                if let Some(text) = p["text"].as_str() {
                    timeline.push((
                        p["msg_id"].as_i64(),
                        ReplayedMessage {
                            role: event.kind.trim_start_matches("chat.").to_string(),
                            content: text.to_string(),
                        },
                    ));
                }
            }
            "undo.chat" => {
                if let Some(undone) = p["msg_id"].as_i64() {
                    timeline.retain(|(id, _)| *id != Some(undone));
                }
            }
            "memory.wiped" => timeline.clear(),
            _ => {}
        }
    }
    timeline.into_iter().map(|(_, m)| m).collect()
}

#[derive(Debug, Clone, Serialize)]
pub struct ReplayReport {
    /// Messages present in both the replayed state and the database.
    pub matched: usize,
    /// In the log's reconstruction but missing from the database.
    pub missing_in_db: Vec<ReplayedMessage>,
    /// In the database but not derivable from the log (pre-log history
    /// or events lost to a log wipe).
    pub extra_in_db: Vec<ReplayedMessage>,
    pub deterministic: bool,
}

/// Order-preserving diff between the replayed state and the actual database
/// contents. Uses a two-pointer walk: messages must match in order, which is
/// exactly what determinism promises.
pub fn audit(replayed: &[ReplayedMessage], actual: &[ReplayedMessage]) -> ReplayReport {
    let mut matched = 0;
    let mut missing_in_db = Vec::new();
    let mut extra_in_db = Vec::new();
    let mut ai = 0;

    for r in replayed {
        // Advance through actual until we find this replayed message.
        let mut found = None;
        for (offset, a) in actual[ai..].iter().enumerate() {
            if a == r {
                found = Some(ai + offset);
                break;
            }
        }
        match found {
            Some(pos) => {
                extra_in_db.extend(actual[ai..pos].iter().cloned());
                matched += 1;
                ai = pos + 1;
            }
            None => missing_in_db.push(r.clone()),
        }
    }
    extra_in_db.extend(actual[ai..].iter().cloned());

    let deterministic = missing_in_db.is_empty() && extra_in_db.is_empty();
    ReplayReport {
        matched,
        missing_in_db,
        extra_in_db,
        deterministic,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(id: u64, kind: &str, payload: serde_json::Value) -> Event {
        Event {
            id,
            ts: 0,
            kind: kind.into(),
            payload,
        }
    }

    fn msg(role: &str, content: &str) -> ReplayedMessage {
        ReplayedMessage {
            role: role.into(),
            content: content.into(),
        }
    }

    #[test]
    fn rebuilds_conversation_in_order() {
        let events = vec![
            event(1, "app.started", serde_json::json!({})),
            event(
                2,
                "chat.user",
                serde_json::json!({"text": "hi", "msg_id": 1}),
            ),
            event(
                3,
                "chat.assistant",
                serde_json::json!({"text": "hello", "msg_id": 2}),
            ),
            event(4, "note.saved", serde_json::json!({"slug": "x"})),
        ];
        let replayed = rebuild_messages(&events);
        assert_eq!(replayed, vec![msg("user", "hi"), msg("assistant", "hello")]);
    }

    #[test]
    fn replay_honors_undo_and_wipe() {
        let events = vec![
            event(
                1,
                "chat.user",
                serde_json::json!({"text": "a", "msg_id": 1}),
            ),
            event(
                2,
                "chat.user",
                serde_json::json!({"text": "b", "msg_id": 2}),
            ),
            event(3, "undo.chat", serde_json::json!({"msg_id": 2})),
            event(
                4,
                "chat.user",
                serde_json::json!({"text": "c", "msg_id": 3}),
            ),
        ];
        assert_eq!(
            rebuild_messages(&events),
            vec![msg("user", "a"), msg("user", "c")],
            "undone message drops out of the replay"
        );

        let with_wipe = vec![
            event(
                1,
                "chat.user",
                serde_json::json!({"text": "old", "msg_id": 1}),
            ),
            event(2, "memory.wiped", serde_json::json!({})),
            event(
                3,
                "chat.user",
                serde_json::json!({"text": "new", "msg_id": 2}),
            ),
        ];
        assert_eq!(rebuild_messages(&with_wipe), vec![msg("user", "new")]);
    }

    #[test]
    fn audit_reports_a_faithful_db() {
        let state = vec![msg("user", "hi"), msg("assistant", "hello")];
        let report = audit(&state, &state.clone());
        assert!(report.deterministic);
        assert_eq!(report.matched, 2);
    }

    #[test]
    fn audit_reports_drift_in_both_directions() {
        let replayed = vec![msg("user", "hi"), msg("assistant", "hello")];
        let actual = vec![
            msg("user", "pre-log history"),
            msg("user", "hi"),
            // "hello" missing from db
        ];
        let report = audit(&replayed, &actual);
        assert!(!report.deterministic);
        assert_eq!(report.matched, 1);
        assert_eq!(report.missing_in_db, vec![msg("assistant", "hello")]);
        assert_eq!(report.extra_in_db, vec![msg("user", "pre-log history")]);
    }
}
