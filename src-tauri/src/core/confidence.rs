//! Calibrated autonomy (§5.3): before an answer stands, the model rates its
//! own probability of being right; below the threshold it is instructed to
//! ask ONE clarifying question instead of guessing. Verbalized confidence is
//! imperfectly calibrated (see "Agentic Uncertainty Reveals Agentic
//! Overconfidence") but directionally useful — v0 treats it as a trust
//! signal to surface, not a guarantee. The marker is stripped from the text
//! shown to the user and carried as structured data instead.

/// Below this the assistant should clarify rather than answer.
pub const ASK_THRESHOLD: u8 = 40;

pub fn confidence_instruction() -> String {
    format!(
        "\n\nAfter your reply, end with one final line of exactly this form: \
         [confidence: NN] where NN is 0-100, your honest probability that the \
         answer is correct and genuinely helpful. Do not inflate it. If your \
         confidence would be below {ASK_THRESHOLD}, do not guess: ask ONE short \
         clarifying question instead (and still end with the confidence line)."
    )
}

/// Splits a trailing `[confidence: NN]` marker off a reply. Only a marker in
/// the final tail of the text counts — mentions mid-answer are left alone.
/// Returns the cleaned text and the parsed value clamped to 0-100.
pub fn extract_confidence(reply: &str) -> (String, Option<u8>) {
    let trimmed = reply.trim_end();
    let lower = trimmed.to_lowercase();
    let Some(start) = lower.rfind("[confidence") else {
        return (trimmed.to_string(), None);
    };
    // Marker must be at the very end (allowing trailing punctuation/space).
    let tail = &trimmed[start..];
    let Some(close) = tail.find(']') else {
        return (trimmed.to_string(), None);
    };
    if !tail[close + 1..].trim().is_empty() {
        return (trimmed.to_string(), None);
    }
    let inside = &tail[..close];
    let digits: String = inside
        .chars()
        .skip_while(|c| !c.is_ascii_digit())
        .take_while(|c| c.is_ascii_digit())
        .collect();
    let value = digits.parse::<u16>().ok().map(|v| v.min(100) as u8);
    if value.is_none() {
        return (trimmed.to_string(), None);
    }
    let cleaned = trimmed[..start].trim_end().to_string();
    (cleaned, value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_and_strips_a_trailing_marker() {
        let (text, conf) = extract_confidence("Paris is the capital.\n[confidence: 92]");
        assert_eq!(text, "Paris is the capital.");
        assert_eq!(conf, Some(92));
    }

    #[test]
    fn tolerates_case_spacing_and_clamps() {
        let (_, conf) = extract_confidence("Sure.\n[Confidence:  180 ]");
        assert_eq!(conf, Some(100), "values clamp to 100");
        let (_, conf) = extract_confidence("Sure. [confidence:7]");
        assert_eq!(conf, Some(7));
    }

    #[test]
    fn leaves_replies_without_marker_untouched() {
        let original = "No marker here at all.";
        let (text, conf) = extract_confidence(original);
        assert_eq!(text, original);
        assert_eq!(conf, None);
    }

    #[test]
    fn ignores_mid_text_mentions_and_malformed_markers() {
        let mid = "The [confidence: 50] marker is how I rate answers. Done.";
        let (text, conf) = extract_confidence(mid);
        assert_eq!(text, mid);
        assert_eq!(conf, None, "marker not at the tail is content, not data");

        let (_, conf) = extract_confidence("Hm. [confidence: high]");
        assert_eq!(conf, None, "non-numeric marker is ignored");
    }

    #[test]
    fn instruction_carries_threshold_and_contract() {
        let inst = confidence_instruction();
        assert!(inst.contains("[confidence: NN]"));
        assert!(inst.contains("below 40"));
        assert!(inst.contains("clarifying question"));
    }
}
