//! Replay (§5.4): the event log carries enough state to rebuild the
//! conversation deterministically — no model calls, just recorded facts.
//! v1 ships a *replay audit*: reconstruct what memory should contain from
//! the log alone, diff it against the live database, and report drift.
//! Grounded in the determinism-faithfulness idea from the replayable-agent
//! literature: a replay you can't verify is a story, not a record.

use crate::core::eventlog::Event;
use serde::Serialize;
use std::collections::BTreeMap;

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

// --- Replay v2: step-through player, and an audit that covers more than chat ---

/// The whole reconstructed world at one point in the log.
///
/// Notes are tracked by size, not body: `note.saved` deliberately records a
/// character count rather than the content, so the log stays a record of what
/// happened instead of becoming a second copy of the user's notes. That means
/// note existence and size are replayable and auditable, and note text is not —
/// stated here rather than quietly pretended otherwise.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct ReplayState {
    pub messages: Vec<ReplayedMessage>,
    /// slug -> recorded size in characters.
    pub notes: BTreeMap<String, u64>,
    /// skill name -> version.
    pub skills: BTreeMap<String, u64>,
    /// Insight ids created by reflection and not since forgotten.
    pub insights: Vec<i64>,
}

/// Applies one event to the running state. The single source of truth for what
/// each event means, shared by the player and the audit.
pub fn apply(state: &mut ReplayState, event: &Event) {
    let p = &event.payload;
    match event.kind.as_str() {
        "chat.user" | "chat.assistant" => {
            if let Some(text) = p["text"].as_str() {
                state.messages.push(ReplayedMessage {
                    role: event.kind.trim_start_matches("chat.").to_string(),
                    content: text.to_string(),
                });
            }
        }
        "undo.chat" => {
            // The undo event names the memory id, not the text; drop the most
            // recent matching message when we can identify it, else the last one.
            if let Some(text) = p["text"].as_str() {
                if let Some(pos) = state.messages.iter().rposition(|m| m.content == text) {
                    state.messages.remove(pos);
                }
            } else {
                state.messages.pop();
            }
        }
        "note.saved" => {
            if let Some(slug) = p["slug"].as_str() {
                state
                    .notes
                    .insert(slug.to_string(), p["chars"].as_u64().unwrap_or(0));
            }
        }
        "note.deleted" => {
            if let Some(slug) = p["slug"].as_str() {
                state.notes.remove(slug);
            }
        }
        "undo.note" => {
            // Restores a deleted note. Its size is unknowable from this event
            // alone, so it returns at 0 — which is why the notes audit compares
            // existence, not size.
            if let Some(slug) = p["slug"].as_str() {
                state.notes.entry(slug.to_string()).or_insert(0);
            }
        }
        "skill.saved" | "skill.authored" => {
            if let Some(name) = p["name"].as_str() {
                state
                    .skills
                    .insert(name.to_string(), p["version"].as_u64().unwrap_or(1));
            }
        }
        "undo.skill" => {
            if let Some(name) = p["name"].as_str() {
                match p["version"].as_u64() {
                    Some(v) if v > 0 => {
                        state.skills.insert(name.to_string(), v);
                    }
                    _ => {
                        state.skills.remove(name);
                    }
                }
            }
        }
        "memory.reflected" => {
            if let Some(ids) = p["insight_ids"].as_array() {
                state.insights.extend(ids.iter().filter_map(|v| v.as_i64()));
            }
        }
        "memory.forgot_insights" | "undo.reflection" => {
            if let Some(ids) = p["insight_ids"].as_array() {
                let dropped: Vec<i64> = ids.iter().filter_map(|v| v.as_i64()).collect();
                state.insights.retain(|id| !dropped.contains(id));
            }
        }
        "memory.wiped" => {
            // Mirrors what wipe_memory actually does: clears conversation and
            // lessons, keeps notes.
            state.messages.clear();
            state.insights.clear();
        }
        _ => {}
    }
}

/// Full state after applying the first `steps` events. A `steps` past the end
/// just means the whole log, so a UI slider cannot go out of bounds.
pub fn state_at(events: &[Event], steps: usize) -> ReplayState {
    let mut state = ReplayState::default();
    for event in events.iter().take(steps) {
        apply(&mut state, event);
    }
    state
}

/// One frame of the step-through player: what happened, and the shape of the
/// world immediately after it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Step {
    /// 1-based count of events applied, so `state_at(events, step)` matches.
    pub step: usize,
    pub event_id: u64,
    pub kind: String,
    pub summary: String,
    /// True when this event changed the reconstructed state, so the player can
    /// skip the noise (startups, telemetry) and land on real changes.
    pub changed: bool,
    pub messages: usize,
    pub notes: usize,
    pub skills: usize,
    pub insights: usize,
}

/// Builds every player frame in one pass.
pub fn timeline(events: &[Event]) -> Vec<Step> {
    let mut state = ReplayState::default();
    let mut out = Vec::with_capacity(events.len());
    for (i, event) in events.iter().enumerate() {
        let before = state.clone();
        apply(&mut state, event);
        out.push(Step {
            step: i + 1,
            event_id: event.id,
            kind: event.kind.clone(),
            summary: describe(event),
            changed: before != state,
            messages: state.messages.len(),
            notes: state.notes.len(),
            skills: state.skills.len(),
            insights: state.insights.len(),
        });
    }
    out
}

/// Human-readable one-liner for an event, for the player's list.
pub fn describe(event: &Event) -> String {
    let p = &event.payload;
    let clip = |s: &str| -> String {
        let t: String = s.chars().take(60).collect();
        if s.chars().count() > 60 {
            format!("{t}...")
        } else {
            t
        }
    };
    let name = |key: &str| p[key].as_str().unwrap_or("?").to_string();
    match event.kind.as_str() {
        "chat.user" => format!("you said: {}", clip(p["text"].as_str().unwrap_or(""))),
        "chat.assistant" => format!("Jarvis replied: {}", clip(p["text"].as_str().unwrap_or(""))),
        "chat.failed" => "a reply failed".to_string(),
        "note.saved" => format!("saved note {}", name("slug")),
        "note.deleted" => format!("deleted note {}", name("slug")),
        "skill.saved" => format!("saved skill {}", name("name")),
        "skill.authored" => format!("wrote skill {}", name("name")),
        "skill.ran" => format!("ran skill {}", name("name")),
        "memory.reflected" => format!(
            "reflected: {} lesson(s) learned",
            p["insights"].as_u64().unwrap_or(0)
        ),
        "memory.forgot_insights" => {
            format!("forgot {} lesson(s)", p["forgot"].as_u64().unwrap_or(0))
        }
        "memory.indexed" => format!(
            "indexed {} message(s) for recall",
            p["indexed"].as_u64().unwrap_or(0)
        ),
        "memory.wiped" => "wiped memory".to_string(),
        "voice.transcribed" => "transcribed speech".to_string(),
        "voice.heard_nothing" => "heard nothing".to_string(),
        "voice.model_downloaded" => "downloaded the speech model".to_string(),
        "settings.providers_changed" => "changed provider settings".to_string(),
        "app.started" => "app started".to_string(),
        other if other.starts_with("undo.") => format!("undid an earlier action ({other})"),
        other => other.to_string(),
    }
}

/// Drift in one keyed domain: what the log expects vs what is actually there.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct KeyedDrift {
    /// Expected from the log, absent in reality.
    pub missing: Vec<String>,
    /// Present in reality, not derivable from the log.
    pub extra: Vec<String>,
    /// In both, but with a different value (e.g. skill version).
    pub differing: Vec<String>,
}

impl KeyedDrift {
    pub fn is_clean(&self) -> bool {
        self.missing.is_empty() && self.extra.is_empty() && self.differing.is_empty()
    }
}

/// The v2 audit: chat, notes and skills together.
#[derive(Debug, Clone, Serialize)]
pub struct StateReport {
    pub messages: ReplayReport,
    pub notes: KeyedDrift,
    pub skills: KeyedDrift,
    /// True only when every domain reconciles.
    pub deterministic: bool,
    pub summary: String,
}

fn diff_keyed(
    expected: &BTreeMap<String, u64>,
    actual: &BTreeMap<String, u64>,
    compare_values: bool,
) -> KeyedDrift {
    let mut drift = KeyedDrift::default();
    for (key, want) in expected {
        match actual.get(key) {
            None => drift.missing.push(key.clone()),
            Some(have) if compare_values && have != want => drift.differing.push(key.clone()),
            _ => {}
        }
    }
    for key in actual.keys() {
        if !expected.contains_key(key) {
            drift.extra.push(key.clone());
        }
    }
    drift
}

/// Reconciles the replayed world against reality.
///
/// Skill versions are compared; note sizes are not, because a note restored by
/// undo comes back with an unknown size. Existence is the honest guarantee for
/// notes.
pub fn audit_state(replayed: &ReplayState, actual: &ReplayState) -> StateReport {
    let messages = audit(&replayed.messages, &actual.messages);
    let notes = diff_keyed(&replayed.notes, &actual.notes, false);
    let skills = diff_keyed(&replayed.skills, &actual.skills, true);
    let deterministic = messages.deterministic && notes.is_clean() && skills.is_clean();
    let summary = if deterministic {
        format!(
            "The log reproduces reality exactly: {} message(s), {} note(s), {} skill(s).",
            messages.matched,
            actual.notes.len(),
            actual.skills.len()
        )
    } else {
        let mut parts = Vec::new();
        if !messages.deterministic {
            parts.push(format!(
                "{} message(s) missing and {} unexplained",
                messages.missing_in_db.len(),
                messages.extra_in_db.len()
            ));
        }
        if !notes.is_clean() {
            parts.push(format!(
                "notes: {} missing, {} unexplained",
                notes.missing.len(),
                notes.extra.len()
            ));
        }
        if !skills.is_clean() {
            parts.push(format!(
                "skills: {} missing, {} unexplained, {} at another version",
                skills.missing.len(),
                skills.extra.len(),
                skills.differing.len()
            ));
        }
        format!(
            "Drift found - {}. Anything predating the event log shows up as unexplained, which is expected on an upgraded install.",
            parts.join("; ")
        )
    };
    StateReport {
        messages,
        notes,
        skills,
        deterministic,
        summary,
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

    // --- Replay v2 ---

    fn note_saved(id: u64, slug: &str, chars: u64) -> Event {
        event(
            id,
            "note.saved",
            serde_json::json!({"slug": slug, "chars": chars}),
        )
    }

    fn skill_saved(id: u64, name: &str, version: u64) -> Event {
        event(
            id,
            "skill.saved",
            serde_json::json!({"name": name, "version": version}),
        )
    }

    #[test]
    fn state_tracks_notes_and_skills_not_just_chat() {
        let events = vec![
            event(
                1,
                "chat.user",
                serde_json::json!({"text": "hi", "msg_id": 1}),
            ),
            note_saved(2, "groceries", 42),
            skill_saved(3, "shout", 1),
            note_saved(4, "ideas", 10),
        ];
        let state = state_at(&events, events.len());
        assert_eq!(state.messages.len(), 1);
        assert_eq!(state.notes.get("groceries"), Some(&42));
        assert_eq!(state.notes.len(), 2);
        assert_eq!(state.skills.get("shout"), Some(&1));
    }

    #[test]
    fn a_later_save_supersedes_an_earlier_one() {
        let events = vec![
            note_saved(1, "n", 10),
            note_saved(2, "n", 99),
            skill_saved(3, "s", 1),
            skill_saved(4, "s", 2),
        ];
        let state = state_at(&events, events.len());
        assert_eq!(state.notes.len(), 1, "same slug is one note, not two");
        assert_eq!(state.notes.get("n"), Some(&99));
        assert_eq!(state.skills.get("s"), Some(&2), "version advances");
    }

    #[test]
    fn deletes_and_undos_move_state_both_ways() {
        let events = vec![
            note_saved(1, "keep", 5),
            note_saved(2, "gone", 7),
            event(3, "note.deleted", serde_json::json!({"slug": "gone"})),
        ];
        let after_delete = state_at(&events, 3);
        assert!(!after_delete.notes.contains_key("gone"));
        assert!(after_delete.notes.contains_key("keep"));

        let mut with_undo = events.clone();
        with_undo.push(event(4, "undo.note", serde_json::json!({"slug": "gone"})));
        let restored = state_at(&with_undo, with_undo.len());
        assert!(
            restored.notes.contains_key("gone"),
            "undo brings the note back"
        );
        assert_eq!(
            restored.notes.get("gone"),
            Some(&0),
            "size is unknown from the undo event alone"
        );
    }

    #[test]
    fn undoing_a_skill_rolls_back_or_removes_it() {
        let base = vec![skill_saved(1, "s", 2)];
        let mut rolled = base.clone();
        rolled.push(event(
            2,
            "undo.skill",
            serde_json::json!({"name": "s", "version": 1}),
        ));
        assert_eq!(state_at(&rolled, 2).skills.get("s"), Some(&1));

        let mut removed = base.clone();
        removed.push(event(2, "undo.skill", serde_json::json!({"name": "s"})));
        assert!(
            state_at(&removed, 2).skills.is_empty(),
            "no prior version means gone"
        );
    }

    #[test]
    fn reflection_insights_are_tracked_and_forgotten() {
        let events = vec![
            event(
                1,
                "memory.reflected",
                serde_json::json!({"insights": 2, "insight_ids": [7, 8]}),
            ),
            event(
                2,
                "memory.forgot_insights",
                serde_json::json!({"forgot": 1, "insight_ids": [7]}),
            ),
        ];
        assert_eq!(state_at(&events, 1).insights, vec![7, 8]);
        assert_eq!(
            state_at(&events, 2).insights,
            vec![8],
            "forgetting removes it"
        );
    }

    #[test]
    fn a_wipe_clears_chat_and_lessons_but_keeps_notes() {
        // Mirrors what wipe_memory actually does — a replay that clears notes
        // too would report false drift on every wiped install.
        let events = vec![
            event(
                1,
                "chat.user",
                serde_json::json!({"text": "old", "msg_id": 1}),
            ),
            note_saved(2, "survives", 3),
            event(
                3,
                "memory.reflected",
                serde_json::json!({"insights": 1, "insight_ids": [1]}),
            ),
            event(4, "memory.wiped", serde_json::json!({})),
        ];
        let state = state_at(&events, events.len());
        assert!(state.messages.is_empty());
        assert!(state.insights.is_empty());
        assert!(state.notes.contains_key("survives"), "notes outlive a wipe");
    }

    #[test]
    fn state_at_walks_forward_one_step_at_a_time() {
        let events = vec![
            event(
                1,
                "chat.user",
                serde_json::json!({"text": "a", "msg_id": 1}),
            ),
            event(
                2,
                "chat.assistant",
                serde_json::json!({"text": "b", "msg_id": 2}),
            ),
            note_saved(3, "n", 1),
        ];
        assert_eq!(
            state_at(&events, 0),
            ReplayState::default(),
            "step 0 is empty"
        );
        assert_eq!(state_at(&events, 1).messages.len(), 1);
        assert_eq!(state_at(&events, 2).messages.len(), 2);
        assert_eq!(state_at(&events, 3).notes.len(), 1);
        // Past the end is the whole log, not a panic — a UI slider can't overrun.
        assert_eq!(state_at(&events, 999), state_at(&events, 3));
    }

    #[test]
    fn timeline_numbers_steps_so_they_index_state_at() {
        let events = vec![
            event(10, "app.started", serde_json::json!({})),
            event(
                11,
                "chat.user",
                serde_json::json!({"text": "hi", "msg_id": 1}),
            ),
        ];
        let steps = timeline(&events);
        assert_eq!(steps.len(), 2);
        assert_eq!(
            steps[0].step, 1,
            "1-based so state_at(events, step) lines up"
        );
        assert_eq!(steps[0].event_id, 10);
        assert_eq!(steps[1].messages, 1);
        for step in &steps {
            assert_eq!(
                state_at(&events, step.step).messages.len(),
                step.messages,
                "the frame's counts must match state_at at that step"
            );
        }
    }

    #[test]
    fn timeline_flags_which_events_actually_changed_anything() {
        let events = vec![
            event(1, "app.started", serde_json::json!({})),
            event(
                2,
                "chat.user",
                serde_json::json!({"text": "hi", "msg_id": 1}),
            ),
            event(3, "voice.heard_nothing", serde_json::json!({"seconds": 1})),
        ];
        let steps = timeline(&events);
        assert!(!steps[0].changed, "a startup changes no state");
        assert!(steps[1].changed, "a message does");
        assert!(!steps[2].changed, "noise events don't");
    }

    #[test]
    fn describe_is_human_readable_and_clips_long_text() {
        let long = "x".repeat(200);
        let d = describe(&event(
            1,
            "chat.user",
            serde_json::json!({"text": long, "msg_id": 1}),
        ));
        assert!(d.starts_with("you said:"));
        assert!(d.ends_with("..."), "long text is clipped: {d}");
        assert!(d.len() < 100);

        assert_eq!(
            describe(&note_saved(2, "groceries", 1)),
            "saved note groceries"
        );
        assert_eq!(
            describe(&event(3, "memory.wiped", serde_json::json!({}))),
            "wiped memory"
        );
        // Unknown kinds fall back to the kind itself rather than a blank line.
        assert_eq!(
            describe(&event(4, "some.future.event", serde_json::json!({}))),
            "some.future.event"
        );
        assert!(describe(&event(5, "undo.chat", serde_json::json!({}))).contains("undid"));
    }

    #[test]
    fn full_audit_passes_when_every_domain_reconciles() {
        let events = vec![
            event(
                1,
                "chat.user",
                serde_json::json!({"text": "hi", "msg_id": 1}),
            ),
            note_saved(2, "n", 4),
            skill_saved(3, "s", 1),
        ];
        let replayed = state_at(&events, events.len());
        let report = audit_state(&replayed, &replayed.clone());
        assert!(report.deterministic);
        assert!(report.summary.contains("reproduces reality exactly"));
    }

    #[test]
    fn full_audit_reports_note_and_skill_drift_separately() {
        let events = vec![note_saved(1, "expected", 4), skill_saved(2, "s", 2)];
        let replayed = state_at(&events, events.len());

        let mut actual = ReplayState::default();
        actual.notes.insert("unexplained".into(), 9); // in db, not in log
        actual.skills.insert("s".into(), 1); // wrong version

        let report = audit_state(&replayed, &actual);
        assert!(!report.deterministic);
        assert_eq!(report.notes.missing, vec!["expected"]);
        assert_eq!(report.notes.extra, vec!["unexplained"]);
        assert_eq!(
            report.skills.differing,
            vec!["s"],
            "version mismatch is drift"
        );
        assert!(report.summary.contains("notes:"));
        assert!(report.summary.contains("skills:"));
    }

    #[test]
    fn note_size_alone_is_not_treated_as_drift() {
        // A note restored by undo comes back at size 0; that must not be
        // reported as corruption, because the log never recorded the body.
        let events = vec![
            note_saved(1, "n", 50),
            event(2, "note.deleted", serde_json::json!({"slug": "n"})),
            event(3, "undo.note", serde_json::json!({"slug": "n"})),
        ];
        let replayed = state_at(&events, events.len());
        assert_eq!(replayed.notes.get("n"), Some(&0));

        let mut actual = ReplayState::default();
        actual.notes.insert("n".into(), 50); // real file still has its content
        let report = audit_state(&replayed, &actual);
        assert!(
            report.notes.is_clean(),
            "existence matches, so this is not drift"
        );
        assert!(report.deterministic);
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
