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

/// JSON schema for a skill draft, handed to Ollama's structured-output `format`
/// field. Constrained decoding beats asking politely: the model *cannot* emit
/// prose, fences, or a missing key, which removes the whole class of "reply
/// wasn't JSON" retries rather than recovering from them.
pub fn skill_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "name": { "type": "string" },
            "description": { "type": "string" },
            "code": { "type": "string" },
            "test": { "type": "string" }
        },
        "required": ["name", "description", "code", "test"]
    })
}

/// Why a draft failed. Classifying it is what lets a retry carry a *targeted*
/// counter-example instead of repeating the same generic rules — the failure
/// modes here are few and highly repetitive, so a specific example is worth far
/// more than another paragraph of instructions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureClass {
    /// Reply wasn't parseable JSON in the required shape.
    NotJson,
    /// `${...}` string interpolation — the most common Rhai mistake by far.
    Interpolation,
    /// Missing `fn run(input)`.
    NoRun,
    /// Missing `fn test()`.
    NoTest,
    /// Didn't compile, or threw at runtime.
    Broken,
    /// Ran fine, but `test()` returned false.
    TestFailed,
}

/// Best-effort classification from the failure text the engine produced. Order
/// matters: the most specific and most actionable signals win.
pub fn classify_failure(failure: &str) -> FailureClass {
    let f = failure.to_lowercase();
    if f.contains("${") || f.contains("interpolation") {
        FailureClass::Interpolation
    } else if f.contains("must define fn run") {
        FailureClass::NoRun
    } else if f.contains("must define fn test") {
        FailureClass::NoTest
    } else if f.contains("not a valid json") || f.contains("missing a name") {
        FailureClass::NotJson
    } else if f.contains("returned false") || f.contains("test failed") {
        FailureClass::TestFailed
    } else {
        FailureClass::Broken
    }
}

/// A short, concrete correction per failure class.
pub fn failure_hint(class: FailureClass) -> &'static str {
    match class {
        FailureClass::NotJson => {
            "Reply with the JSON object ONLY — no prose, no markdown fences, no \
             trailing explanation. All four keys are required."
        }
        FailureClass::Interpolation => {
            "You used ${...} interpolation, which does not exist in Rhai. Build \
             strings with +. WRONG: \"Hello ${name}\"  RIGHT: \"Hello \" + name"
        }
        FailureClass::NoRun => {
            "The \"code\" value must define exactly fn run(input). Example: \
             \"fn run(input) { input.trim() }\""
        }
        FailureClass::NoTest => {
            "The \"test\" value must define fn test() and call run() with a \
             concrete example."
        }
        FailureClass::Broken => {
            "The script did not run. Keep to plain Rhai: let bindings, if/else, \
             for loops, string methods (.len(), .trim(), .to_upper(), .split(s), \
             .sub_string(start, len)). No imports, no files, no network."
        }
        FailureClass::TestFailed => {
            "The script ran but test() returned false, so run() and the expected \
             value disagree. Trace your example by hand and fix whichever is \
             wrong — the LAST EXPRESSION is the return value, no `return` needed."
        }
    }
}

/// Follow-up when a draft failed its test or couldn't be parsed. Carries a
/// class-specific counter-example, not just the raw error.
pub fn refinement_message(failure: &str) -> ChatMessage {
    let hint = failure_hint(classify_failure(failure));
    ChatMessage {
        role: "user".into(),
        content: format!(
            "That attempt failed: {failure}\n\n{hint}\n\nFix it and reply again with ONLY the corrected JSON object in the same shape."
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
    fn schema_requires_all_four_keys_and_only_strings() {
        let schema = skill_schema();
        assert_eq!(schema["type"], "object");
        let required: Vec<&str> = schema["required"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        for key in ["name", "description", "code", "test"] {
            assert!(required.contains(&key), "{key} must be required");
            assert_eq!(schema["properties"][key]["type"], "string");
        }
    }

    #[test]
    fn interpolation_is_the_first_thing_we_look_for() {
        // The most common Rhai mistake, and the one a generic hint never fixes.
        assert_eq!(
            classify_failure("script error: ${name} is not valid"),
            FailureClass::Interpolation
        );
        assert_eq!(
            classify_failure("avoid interpolation in strings"),
            FailureClass::Interpolation
        );
        assert!(failure_hint(FailureClass::Interpolation).contains("RIGHT:"));
    }

    #[test]
    fn structural_failures_beat_the_generic_json_message() {
        // The engine's "must define fn run" is more specific than any JSON
        // complaint, so it has to win even when both words appear.
        assert_eq!(
            classify_failure("code must define fn run(input)"),
            FailureClass::NoRun
        );
        assert_eq!(
            classify_failure("test must define fn test()"),
            FailureClass::NoTest
        );
        assert_eq!(
            classify_failure("reply was not a valid JSON object with name/description/code/test"),
            FailureClass::NotJson
        );
    }

    #[test]
    fn a_failing_test_is_distinguished_from_a_broken_script() {
        assert_eq!(
            classify_failure("test() returned false"),
            FailureClass::TestFailed
        );
        assert_eq!(
            classify_failure("Runtime error: unknown function 'frobnicate'"),
            FailureClass::Broken
        );
        // The two need different advice, which is the whole point.
        assert_ne!(
            failure_hint(FailureClass::TestFailed),
            failure_hint(FailureClass::Broken)
        );
    }

    #[test]
    fn every_class_has_a_distinct_non_empty_hint() {
        use FailureClass::*;
        let all = [NotJson, Interpolation, NoRun, NoTest, Broken, TestFailed];
        let hints: Vec<&str> = all.iter().map(|c| failure_hint(*c)).collect();
        assert!(hints.iter().all(|h| h.len() > 20));
        for (i, a) in hints.iter().enumerate() {
            for b in hints.iter().skip(i + 1) {
                assert_ne!(a, b, "hints must be class-specific");
            }
        }
    }

    #[test]
    fn refinement_carries_both_the_error_and_a_targeted_hint() {
        let msg = refinement_message("script error: ${x} invalid");
        assert!(msg.content.contains("${x} invalid"), "keeps the raw error");
        assert!(
            msg.content.contains("does not exist in Rhai"),
            "adds the interpolation counter-example"
        );
        assert!(msg.content.contains("ONLY the corrected JSON"));
    }

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
