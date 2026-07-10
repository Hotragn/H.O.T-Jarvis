//! Reflective reasoning-memory (§5.2): the assistant periodically re-reads
//! its own action log and distills *lessons* — what worked, what failed,
//! what to avoid — into durable insights that feed back into future prompts.
//! This module is the pure half (digesting, prompting, parsing, injecting);
//! orchestration lives in the command layer. Grounded in the ReasoningBank /
//! Reflexion idea: store reasoning outcomes, not just facts.

use crate::core::eventlog::Event;
use crate::core::router::ChatMessage;
use serde::Deserialize;

/// Reflect after this many new chat messages accumulate.
pub const REFLECT_EVERY_MESSAGES: u64 = 20;
/// Keep passes small: a few good lessons beat a wall of noise.
pub const MAX_INSIGHTS_PER_PASS: usize = 3;
/// How many recent lessons ride along in prompts.
pub const INSIGHTS_IN_PROMPT: usize = 3;

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct InsightDraft {
    pub kind: String,
    pub content: String,
}

fn short(text: &str, max: usize) -> String {
    let text = text.replace('\n', " ");
    if text.chars().count() > max {
        let cut: String = text.chars().take(max.saturating_sub(1)).collect();
        format!("{cut}…")
    } else {
        text
    }
}

/// Compact, deterministic text digest of recent events for the model to
/// reflect over. Chat text is truncated hard — the digest is about outcomes,
/// not transcripts.
pub fn digest_events(events: &[Event]) -> String {
    events
        .iter()
        .map(|e| {
            let p = &e.payload;
            let extra = match e.kind.as_str() {
                "chat.user" => short(p["text"].as_str().unwrap_or(""), 60),
                "chat.assistant" => format!(
                    "{} via {} in {}ms",
                    short(p["text"].as_str().unwrap_or(""), 60),
                    p["provider"].as_str().unwrap_or("?"),
                    p["duration_ms"].as_u64().unwrap_or(0)
                ),
                "chat.failed" => short(p["error"].as_str().unwrap_or(""), 100),
                "skill.saved" | "skill.authored" | "skill.tested" => format!(
                    "{} test={}",
                    p["name"].as_str().unwrap_or("?"),
                    p["test_status"]["status"].as_str().unwrap_or("?")
                ),
                "skill.run" => format!(
                    "{} ok={} {}",
                    p["name"].as_str().unwrap_or("?"),
                    p["ok"].as_bool().unwrap_or(false),
                    short(p["error"].as_str().unwrap_or(""), 80)
                ),
                _ => String::new(),
            };
            format!("#{} {} {}", e.id, e.kind, extra.trim())
        })
        .collect::<Vec<_>>()
        .join("\n")
}

const REFLECTION_SYSTEM: &str = r#"You are the reflection pass of the H.O.T-Jarvis assistant. You will be shown a log of the assistant's recent actions and their outcomes.

Extract at most 3 short, general lessons that would genuinely improve future behavior: things that worked, things that failed and why, patterns to avoid. Skip trivia; if nothing is worth learning, return an empty array.

Reply with ONLY a JSON array, no fences, no commentary:
[{"kind": "skill", "content": "one-sentence lesson"}]

"kind" must be one of: "skill", "provider", "user", "general"."#;

pub fn reflection_messages(digest: &str) -> Vec<ChatMessage> {
    vec![
        ChatMessage {
            role: "system".into(),
            content: REFLECTION_SYSTEM.into(),
        },
        ChatMessage {
            role: "user".into(),
            content: format!("Recent activity log:\n{digest}"),
        },
    ]
}

/// Tolerant parse of the model's lesson list: fences and chatter stripped,
/// empty or malformed entries dropped, capped at MAX_INSIGHTS_PER_PASS.
pub fn parse_insights(reply: &str) -> Result<Vec<InsightDraft>, String> {
    let cleaned = reply
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();
    let candidate = if let (Some(start), Some(end)) = (cleaned.find('['), cleaned.rfind(']')) {
        if end > start {
            &cleaned[start..=end]
        } else {
            cleaned
        }
    } else {
        cleaned
    };
    // Small models often reply with a bare object instead of an array —
    // observed live with llama3.2. Accept both.
    let drafts: Vec<InsightDraft> = match serde_json::from_str(candidate) {
        Ok(list) => list,
        Err(_) => serde_json::from_str::<InsightDraft>(cleaned)
            .map(|single| vec![single])
            .map_err(|_| "reply was not a JSON array of {kind, content}".to_string())?,
    };
    Ok(drafts
        .into_iter()
        .filter(|d| !d.content.trim().is_empty())
        .map(|d| InsightDraft {
            kind: match d.kind.as_str() {
                "skill" | "provider" | "user" | "general" => d.kind,
                _ => "general".into(),
            },
            content: d.content.trim().to_string(),
        })
        .take(MAX_INSIGHTS_PER_PASS)
        .collect())
}

/// Rides the freshest lessons along in a system prompt.
pub fn with_lessons(base: &str, lessons: &[String]) -> String {
    if lessons.is_empty() {
        return base.to_string();
    }
    let list = lessons
        .iter()
        .map(|l| format!("- {l}"))
        .collect::<Vec<_>>()
        .join("\n");
    format!("{base}\n\nLessons from your own recent experience (apply when relevant):\n{list}")
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

    #[test]
    fn digest_covers_outcomes_and_truncates_text() {
        let long = "x".repeat(500);
        let events = vec![
            event(1, "chat.user", serde_json::json!({ "text": long })),
            event(
                2,
                "chat.assistant",
                serde_json::json!({ "text": "hi", "provider": "ollama", "duration_ms": 1500 }),
            ),
            event(
                3,
                "skill.authored",
                serde_json::json!({ "name": "shout", "test_status": { "status": "failed" } }),
            ),
        ];
        let digest = digest_events(&events);
        assert!(digest.contains("#2 chat.assistant"));
        assert!(digest.contains("via ollama in 1500ms"));
        assert!(digest.contains("shout test=failed"));
        assert!(digest.len() < 400, "long chat text must be truncated");
    }

    #[test]
    fn parses_clean_fenced_and_chattered_lesson_lists() {
        let clean = r#"[{"kind": "skill", "content": "always call run() in tests"}]"#;
        assert_eq!(parse_insights(clean).unwrap().len(), 1);

        let fenced = format!("```json\n{clean}\n```");
        assert_eq!(parse_insights(&fenced).unwrap().len(), 1);

        let chattered = format!("Here are my lessons: {clean} — hope that helps");
        let parsed = parse_insights(&chattered).unwrap();
        assert_eq!(parsed[0].content, "always call run() in tests");
    }

    #[test]
    fn caps_normalizes_and_filters_drafts() {
        let many = r#"[
            {"kind": "skill", "content": "a"},
            {"kind": "banana", "content": "b"},
            {"kind": "user", "content": "   "},
            {"kind": "general", "content": "c"},
            {"kind": "general", "content": "d"}
        ]"#;
        let parsed = parse_insights(many).unwrap();
        assert_eq!(parsed.len(), MAX_INSIGHTS_PER_PASS, "capped");
        assert_eq!(parsed[1].kind, "general", "unknown kind normalized");
        assert!(parsed.iter().all(|d| !d.content.trim().is_empty()));
    }

    #[test]
    fn rejects_non_json_replies_and_accepts_empty() {
        assert!(parse_insights("nothing to report").is_err());
        assert!(parse_insights("[]").unwrap().is_empty());
    }

    #[test]
    fn accepts_a_bare_object_as_seen_from_llama32() {
        // Live llama3.2 replied with a single object, not an array.
        let bare = r#"{"kind": "skill", "content": "Re-test after failing."}"#;
        let parsed = parse_insights(bare).unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].content, "Re-test after failing.");
    }

    #[test]
    fn lessons_ride_along_in_the_system_prompt() {
        assert_eq!(with_lessons("base", &[]), "base");
        let with = with_lessons("base", &["avoid ${} in rhai".to_string()]);
        assert!(with.starts_with("base"));
        assert!(with.contains("- avoid ${} in rhai"));
        assert!(with.contains("recent experience"));
    }

    #[test]
    fn reflection_messages_carry_contract_and_digest() {
        let messages = reflection_messages("#1 chat.user hello");
        assert_eq!(messages[0].role, "system");
        assert!(messages[0].content.contains("ONLY a JSON array"));
        assert!(messages[1].content.contains("#1 chat.user hello"));
    }
}
