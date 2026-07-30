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

// --- Confidence v2: use the measured calibration, don't just display it ---

/// Tells the model what its own track record says, so it can correct for a
/// known bias instead of repeating it. This is the cheap half of calibration:
/// the literature finds verbalized confidence saturates high, and simply
/// naming the measured error is a documented way to pull it back.
///
/// Empty until there is enough evidence — inventing a correction from three
/// samples would be exactly the overconfidence being corrected.
pub fn bias_instruction(report: &crate::core::calibration::CalibrationReport) -> String {
    if !report.trustworthy {
        return String::new();
    }
    let points = (report.bias.abs() * 100.0).round() as i64;
    if report.bias > crate::core::calibration::NOTABLE_BIAS {
        format!(
            "

Calibration note: across {} graded answers your stated confidence              has run about {points} points HIGHER than your actual accuracy. Correct              for that — subtract roughly that much before you state a number.",
            report.sample_size
        )
    } else if report.bias < -crate::core::calibration::NOTABLE_BIAS {
        format!(
            "

Calibration note: across {} graded answers your stated confidence              has run about {points} points LOWER than your actual accuracy. You are              underselling yourself — do not hedge more than the evidence warrants.",
            report.sample_size
        )
    } else {
        format!(
            "

Calibration note: across {} graded answers your stated confidence has              matched your actual accuracy well. Keep rating the same way.",
            report.sample_size
        )
    }
}

/// How much an answer should actually be trusted, after applying the measured
/// bias. `raw` is what the model claimed; `adjusted` is what the track record
/// says that claim is worth.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
pub struct Trust {
    pub raw: u8,
    pub adjusted: u8,
    /// True when the *calibrated* confidence lands below the ask threshold —
    /// the answer reads confident but the record says treat it as a guess.
    pub verify: bool,
    /// True only when calibration actually changed the picture, so the UI can
    /// explain itself rather than showing an unexplained warning.
    pub demoted: bool,
}

/// Applies measured calibration to one stated confidence.
///
/// The point of v2: an answer claiming 85 from a model that runs 30 points hot
/// is really a coin flip, and the user deserves to know that *before* acting on
/// it. With no trustworthy calibration yet, this is a no-op — never invent a
/// correction.
pub fn assess(
    raw: Option<u8>,
    report: &crate::core::calibration::CalibrationReport,
) -> Option<Trust> {
    let raw = raw?;
    let adjusted = crate::core::calibration::debias(raw, report);
    let verify = adjusted < ASK_THRESHOLD;
    Some(Trust {
        raw,
        adjusted,
        verify,
        // Only "demoted" if the raw number would have passed but the calibrated
        // one doesn't; that's the case worth a visible warning.
        demoted: verify && raw >= ASK_THRESHOLD,
    })
}

/// The line shown with a demoted answer. Kept here so the wording is tested
/// alongside the rule that triggers it.
pub fn verify_notice(trust: &Trust) -> String {
    format!(
        "This answer claims {}% but your calibration record puts it nearer {}% —          worth verifying before you rely on it.",
        trust.raw, trust.adjusted
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

    use crate::core::calibration::{report as calib_report, Prediction};

    /// A report showing the model runs `hot` points overconfident, from a
    /// sample big enough to be trustworthy.
    fn overconfident_by(points: u8) -> crate::core::calibration::CalibrationReport {
        let stated = 90u8;
        let accuracy = stated.saturating_sub(points);
        let right = accuracy as usize / 10;
        let mut preds = vec![
            Prediction {
                confidence: stated,
                correct: true
            };
            right
        ];
        preds.extend(vec![
            Prediction {
                confidence: stated,
                correct: false
            };
            10 - right
        ]);
        calib_report(&preds)
    }

    #[test]
    fn bias_instruction_is_silent_without_evidence() {
        let thin = calib_report(&[Prediction {
            confidence: 90,
            correct: false,
        }]);
        assert!(!thin.trustworthy);
        assert_eq!(bias_instruction(&thin), "", "must not invent a correction");
        assert_eq!(bias_instruction(&calib_report(&[])), "");
    }

    #[test]
    fn bias_instruction_names_the_direction_and_size() {
        let hot = overconfident_by(30);
        let text = bias_instruction(&hot);
        assert!(text.contains("HIGHER"), "got: {text}");
        assert!(text.contains("30 points"), "got: {text}");

        // Underconfident: says 40, right 9 of 10.
        let mut preds = vec![
            Prediction {
                confidence: 40,
                correct: true
            };
            9
        ];
        preds.push(Prediction {
            confidence: 40,
            correct: false,
        });
        let cold = calib_report(&preds);
        assert!(bias_instruction(&cold).contains("LOWER"));

        // Well calibrated: 70 stated, right 7 of 10.
        let mut ok = vec![
            Prediction {
                confidence: 70,
                correct: true
            };
            7
        ];
        ok.extend(vec![
            Prediction {
                confidence: 70,
                correct: false
            };
            3
        ]);
        assert!(bias_instruction(&calib_report(&ok)).contains("matched"));
    }

    #[test]
    fn assess_is_a_no_op_before_calibration_exists() {
        let thin = calib_report(&[]);
        let t = assess(Some(85), &thin).unwrap();
        assert_eq!(t.raw, 85);
        assert_eq!(t.adjusted, 85, "no evidence means no adjustment");
        assert!(!t.verify);
        assert!(!t.demoted);
    }

    #[test]
    fn assess_demotes_a_confident_answer_from_an_overconfident_model() {
        // Claims 85, but the record runs 30 points hot -> really ~55... still
        // above the threshold. Push it further: 60 claimed becomes 30.
        let hot = overconfident_by(30);
        let t = assess(Some(60), &hot).unwrap();
        assert_eq!(t.adjusted, 30);
        assert!(t.verify, "calibrated value is below the ask threshold");
        assert!(
            t.demoted,
            "raw passed but calibrated did not — warn the user"
        );
        assert!(verify_notice(&t).contains("60%"));
        assert!(verify_notice(&t).contains("30%"));
    }

    #[test]
    fn assess_does_not_flag_an_answer_that_was_already_low() {
        let hot = overconfident_by(30);
        // Raw 20 is already under the threshold: it needs verifying, but it was
        // never *demoted* — the model was honest about being unsure.
        let t = assess(Some(20), &hot).unwrap();
        assert!(t.verify);
        assert!(
            !t.demoted,
            "an honestly-low answer isn't a calibration story"
        );
    }

    #[test]
    fn assess_leaves_a_genuinely_confident_answer_alone() {
        let hot = overconfident_by(30);
        let t = assess(Some(95), &hot).unwrap();
        assert_eq!(t.adjusted, 65);
        assert!(!t.verify, "still comfortably above the threshold");
    }

    #[test]
    fn assess_passes_through_a_missing_confidence() {
        assert!(assess(None, &calib_report(&[])).is_none());
    }

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
