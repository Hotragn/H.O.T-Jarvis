//! Skill authoring (§5.1 payoff): the assistant writes its own skills.
//! The LLM gets a strict contract — reply with one JSON object holding
//! name/description/code/test — and the engine validates by actually
//! running the bundled test. Failures loop back to the model with the
//! error (Reflexion-style) for a bounded number of refinement rounds.
//! This module is the pure half: prompt building and reply parsing,
//! fully unit-tested; the orchestration lives in the command layer.

use crate::core::router::ChatMessage;
use serde::Deserialize;

pub const MAX_ATTEMPTS: u32 = 3;

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct SkillDraft {
    pub name: String,
    pub description: String,
    pub code: String,
    pub test: String,
}

const AUTHORING_SYSTEM: &str = r#"You write skills for the H.O.T-Jarvis assistant. A skill is a small Rhai script.

Rhai is like a JavaScript/Rust hybrid: double-quoted strings, + for string concat, `let x = ...;`, if/else, for loops, arrays like [1, 2]. Useful string methods: .len(), .to_upper(), .to_lower(), .trim(), .contains(s), .replace(a, b), .split(s), .sub_string(start, len). No files, no network, no imports — pure computation only.

Reply with ONLY one JSON object, no markdown fences, no commentary, exactly this shape:
{"name": "kebab-case-name", "description": "one short line", "code": "fn run(input) { ... }", "test": "fn test() { run(\"example\") == \"expected\" }"}

Example of a CORRECT skill (note: return the value directly, the last expression is the return value):
{"name": "shout", "description": "Uppercases the input.", "code": "fn run(input) { input.to_upper() }", "test": "fn test() { run(\"hi\") == \"HI\" }"}

Hard rules:
- "code" defines fn run(input): takes one string, returns a value.
- "test" defines fn test(): returns true when the skill is correct, and calls run() with at least one concrete example.
- NEVER use ${...} interpolation inside double-quoted strings — it is not Rhai. Build strings with + or return expressions directly.
- Escape all double quotes inside code and test for valid JSON."#;

/// `lessons` are skill-related insights from the reflection pass (§5.2) —
/// the assistant's own past authoring mistakes ride along as guidance.
pub fn authoring_messages(request: &str, lessons: &[String]) -> Vec<ChatMessage> {
    vec![
        ChatMessage {
            role: "system".into(),
            content: crate::core::reflection::with_lessons(AUTHORING_SYSTEM, lessons),
        },
        ChatMessage {
            role: "user".into(),
            content: format!("Write a skill that does the following: {request}"),
        },
    ]
}

/// Follow-up when a draft failed its test or couldn't be parsed.
pub fn refinement_message(failure: &str) -> ChatMessage {
    ChatMessage {
        role: "user".into(),
        content: format!(
            "That attempt failed: {failure}\nFix it and reply again with ONLY the corrected JSON object in the same shape."
        ),
    }
}

/// Pulls the JSON object out of a model reply, tolerating markdown fences
/// and surrounding chatter: tries the whole trimmed reply first, then the
/// outermost brace span.
pub fn parse_skill_draft(reply: &str) -> Result<SkillDraft, String> {
    let cleaned = reply
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();
    if let Ok(draft) = serde_json::from_str::<SkillDraft>(cleaned) {
        return validate(draft);
    }
    let start = cleaned.find('{');
    let end = cleaned.rfind('}');
    if let (Some(start), Some(end)) = (start, end) {
        if end > start {
            if let Ok(draft) = serde_json::from_str::<SkillDraft>(&cleaned[start..=end]) {
                return validate(draft);
            }
        }
    }
    Err("reply was not a valid JSON object with name/description/code/test".into())
}

fn validate(draft: SkillDraft) -> Result<SkillDraft, String> {
    if draft.name.trim().is_empty() {
        return Err("draft is missing a name".into());
    }
    if !draft.code.contains("fn run") {
        return Err("code must define fn run(input)".into());
    }
    if !draft.test.contains("fn test") {
        return Err("test must define fn test()".into());
    }
    Ok(draft)
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD_JSON: &str = r#"{"name": "shout", "description": "uppercases input", "code": "fn run(input) { input.to_upper() }", "test": "fn test() { run(\"hi\") == \"HI\" }"}"#;

    #[test]
    fn parses_a_clean_json_reply() {
        let draft = parse_skill_draft(GOOD_JSON).unwrap();
        assert_eq!(draft.name, "shout");
        assert!(draft.code.contains("to_upper"));
    }

    #[test]
    fn parses_fenced_and_chattered_replies() {
        let fenced = format!("```json\n{GOOD_JSON}\n```");
        assert!(parse_skill_draft(&fenced).is_ok());

        let chattered = format!("Sure! Here is the skill:\n{GOOD_JSON}\nHope that helps!");
        assert!(parse_skill_draft(&chattered).is_ok());
    }

    #[test]
    fn rejects_replies_without_the_contract() {
        assert!(parse_skill_draft("I cannot do that").is_err());
        let no_run = r#"{"name": "x", "description": "d", "code": "let a = 1;", "test": "fn test() { true }"}"#;
        assert!(parse_skill_draft(no_run).unwrap_err().contains("fn run"));
        let no_test = r#"{"name": "x", "description": "d", "code": "fn run(input) { input }", "test": "true"}"#;
        assert!(parse_skill_draft(no_test).unwrap_err().contains("fn test"));
    }

    #[test]
    fn authoring_messages_carry_contract_and_request() {
        let messages = authoring_messages("reverse the input string", &[]);
        assert_eq!(messages[0].role, "system");
        assert!(messages[0].content.contains("ONLY one JSON object"));
        assert!(messages[1].content.contains("reverse the input string"));
    }

    #[test]
    fn authoring_messages_carry_learned_lessons() {
        let lessons = vec!["never use ${} interpolation".to_string()];
        let messages = authoring_messages("anything", &lessons);
        assert!(messages[0]
            .content
            .contains("- never use ${} interpolation"));
    }

    #[test]
    fn refinement_message_carries_failure_detail() {
        let msg = refinement_message("test() returned false");
        assert_eq!(msg.role, "user");
        assert!(msg.content.contains("test() returned false"));
        assert!(msg.content.contains("corrected JSON"));
    }
}
