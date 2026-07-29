//! Calibration tracking (§5.3, Confidence v1).
//!
//! v0 asked the model to rate itself and showed the number. That is a claim, not
//! evidence. This module holds the claim to account: every rated answer becomes a
//! (predicted, actual) pair, and from those we compute whether the confidence
//! means anything.
//!
//! Two standard proper-scoring measures, both lower-is-better:
//!   * **Brier score** — mean squared error between stated probability and
//!     outcome. Sharpness and calibration together.
//!   * **Expected calibration error (ECE)** — bin predictions by confidence, then
//!     average |confidence − accuracy| weighted by bin size. Answers "when it
//!     says 90, is it right 90% of the time?"
//!
//! The literature is consistent that verbalized confidence saturates high: models
//! say 90 and mean 70. So the headline number here is signed **bias** (mean
//! confidence − mean accuracy), because a user cares less about an abstract score
//! than about "it runs 15 points hot." Bias is what lets the assistant say so out
//! loud, and what a future version can subtract before gating a risky action.
//!
//! Pure arithmetic over plain data — no storage, no Tauri, no model.

/// One graded answer: what the assistant claimed, and how it actually went.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Prediction {
    /// Self-rated 0-100 at the time of answering.
    pub confidence: u8,
    /// Whether the answer turned out to be right and genuinely useful.
    pub correct: bool,
}

/// One confidence band of the reliability diagram.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Bin {
    /// Inclusive lower bound of the band, in percent.
    pub low: u8,
    /// Exclusive upper bound (except the last band, which includes 100).
    pub high: u8,
    pub count: usize,
    /// Mean stated confidence in this band, 0-1.
    pub mean_confidence: f64,
    /// Share actually correct, 0-1. Perfect calibration puts this on the diagonal.
    pub accuracy: f64,
}

/// The verdict, ready to serialize to the UI.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct CalibrationReport {
    /// How many rated answers this is based on.
    pub sample_size: usize,
    /// Mean squared error, 0-1. 0.25 is what you get by always saying 50.
    pub brier: f64,
    /// Expected calibration error, 0-1. 0 is perfect.
    pub ece: f64,
    /// Mean stated confidence, 0-1.
    pub mean_confidence: f64,
    /// Share correct, 0-1.
    pub accuracy: f64,
    /// Signed: positive means overconfident (talks bigger than it performs).
    pub bias: f64,
    pub bins: Vec<Bin>,
    /// True once there is enough data to say anything without embarrassing
    /// ourselves. Below this, the UI should show "still learning", not a number.
    pub trustworthy: bool,
    /// One plain-English line, the thing actually worth reading.
    pub summary: String,
}

/// Below this many rated answers, calibration numbers are noise.
pub const MIN_SAMPLE: usize = 10;

/// Band width in percent. Ten bands is the usual choice for reliability diagrams
/// and keeps each band populated at realistic sample sizes.
const BIN_WIDTH: u8 = 10;

/// A bias past this (in probability, so 0.10 = 10 points) is worth telling the
/// user about rather than treating as noise.
pub const NOTABLE_BIAS: f64 = 0.10;

/// Computes the report. An empty or tiny sample still returns a well-formed
/// value — the caller shouldn't have to special-case "no data yet".
pub fn report(predictions: &[Prediction]) -> CalibrationReport {
    let n = predictions.len();
    if n == 0 {
        return CalibrationReport {
            sample_size: 0,
            brier: 0.0,
            ece: 0.0,
            mean_confidence: 0.0,
            accuracy: 0.0,
            bias: 0.0,
            bins: Vec::new(),
            trustworthy: false,
            summary: "No rated answers yet. Rate a few replies and Jarvis will \
                      start checking its own confidence against reality."
                .to_string(),
        };
    }

    let probs: Vec<f64> = predictions
        .iter()
        .map(|p| p.confidence.min(100) as f64 / 100.0)
        .collect();
    let outcomes: Vec<f64> = predictions
        .iter()
        .map(|p| if p.correct { 1.0 } else { 0.0 })
        .collect();

    let brier = probs
        .iter()
        .zip(&outcomes)
        .map(|(p, o)| (p - o).powi(2))
        .sum::<f64>()
        / n as f64;
    let mean_confidence = probs.iter().sum::<f64>() / n as f64;
    let accuracy = outcomes.iter().sum::<f64>() / n as f64;

    let bins = build_bins(predictions);
    // ECE: size-weighted average gap between stated confidence and reality.
    let ece = bins
        .iter()
        .map(|b| (b.count as f64 / n as f64) * (b.mean_confidence - b.accuracy).abs())
        .sum::<f64>();

    let bias = mean_confidence - accuracy;
    let trustworthy = n >= MIN_SAMPLE;
    let summary = summarize(n, bias, ece, trustworthy);

    CalibrationReport {
        sample_size: n,
        brier,
        ece,
        mean_confidence,
        accuracy,
        bias,
        bins,
        trustworthy,
        summary,
    }
}

/// Only non-empty bands are returned: a reliability diagram full of empty bins
/// reads as broken rather than sparse.
fn build_bins(predictions: &[Prediction]) -> Vec<Bin> {
    let mut buckets: Vec<Vec<&Prediction>> = vec![Vec::new(); 10];
    for p in predictions {
        let c = p.confidence.min(100);
        // 100 belongs in the top band, not an eleventh one.
        let idx = ((c / BIN_WIDTH) as usize).min(9);
        buckets[idx].push(p);
    }
    buckets
        .into_iter()
        .enumerate()
        .filter(|(_, items)| !items.is_empty())
        .map(|(i, items)| {
            let count = items.len();
            let mean_confidence = items
                .iter()
                .map(|p| p.confidence.min(100) as f64 / 100.0)
                .sum::<f64>()
                / count as f64;
            let accuracy = items.iter().filter(|p| p.correct).count() as f64 / count as f64;
            Bin {
                low: i as u8 * BIN_WIDTH,
                high: if i == 9 {
                    100
                } else {
                    (i as u8 + 1) * BIN_WIDTH
                },
                count,
                mean_confidence,
                accuracy,
            }
        })
        .collect()
}

fn summarize(n: usize, bias: f64, ece: f64, trustworthy: bool) -> String {
    if !trustworthy {
        return format!(
            "Only {n} rated answer{} so far — {} needed before the numbers mean \
             anything.",
            if n == 1 { "" } else { "s" },
            MIN_SAMPLE
        );
    }
    let points = (bias.abs() * 100.0).round() as i64;
    if bias > NOTABLE_BIAS {
        format!(
            "Overconfident by about {points} points: when Jarvis says it's sure, \
             it's right less often than that. Treat high confidence as a lead, \
             not a guarantee."
        )
    } else if bias < -NOTABLE_BIAS {
        format!(
            "Underconfident by about {points} points: Jarvis is right more often \
             than it claims, so it may be hedging more than it needs to."
        )
    } else {
        format!(
            "Well calibrated so far (within {points} points, ECE {:.2}). Stated \
             confidence roughly matches how often it's actually right.",
            ece
        )
    }
}

/// Event kind written when the user grades a reply.
pub const RATED_EVENT: &str = "chat.rated";

/// Rebuilds the (predicted, actual) pairs from the append-only event log, which
/// already records every answer's confidence. Nothing new to store: the log stays
/// the single source of truth, so calibration is replayable like everything else.
///
/// A later rating supersedes an earlier one for the same message, so changing your
/// mind corrects the record instead of double-counting it.
pub fn pair_from_events(events: &[crate::core::eventlog::Event]) -> Vec<Prediction> {
    use std::collections::HashMap;

    let mut confidence_by_msg: HashMap<i64, u8> = HashMap::new();
    let mut rating_by_msg: HashMap<i64, bool> = HashMap::new();

    for e in events {
        let msg_id = e.payload.get("msg_id").and_then(|v| v.as_i64());
        match e.kind.as_str() {
            "chat.assistant" => {
                if let (Some(id), Some(c)) =
                    (msg_id, e.payload.get("confidence").and_then(|v| v.as_u64()))
                {
                    confidence_by_msg.insert(id, c.min(100) as u8);
                }
            }
            k if k == RATED_EVENT => {
                if let (Some(id), Some(helpful)) =
                    (msg_id, e.payload.get("helpful").and_then(|v| v.as_bool()))
                {
                    // Later events overwrite earlier ones.
                    rating_by_msg.insert(id, helpful);
                }
            }
            _ => {}
        }
    }

    // Only answers that were both self-rated and graded can be scored. Sorted by
    // message id so the result is deterministic (HashMap order is not).
    let mut ids: Vec<i64> = rating_by_msg.keys().copied().collect();
    ids.sort_unstable();
    ids.into_iter()
        .filter_map(|id| {
            let confidence = *confidence_by_msg.get(&id)?;
            let correct = *rating_by_msg.get(&id)?;
            Some(Prediction {
                confidence,
                correct,
            })
        })
        .collect()
}

/// Confidence adjusted for measured bias, clamped to 0-100. This is how a future
/// version can gate risky actions on something better than the raw self-rating:
/// if the model runs 15 points hot, subtract 15 before comparing to a threshold.
pub fn debias(confidence: u8, report: &CalibrationReport) -> u8 {
    if !report.trustworthy {
        return confidence.min(100);
    }
    let adjusted = confidence.min(100) as f64 - report.bias * 100.0;
    adjusted.clamp(0.0, 100.0).round() as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(confidence: u8, correct: bool) -> Prediction {
        Prediction {
            confidence,
            correct,
        }
    }

    #[test]
    fn empty_sample_is_well_formed_and_not_trustworthy() {
        let r = report(&[]);
        assert_eq!(r.sample_size, 0);
        assert!(!r.trustworthy);
        assert!(r.bins.is_empty());
        assert!(r.summary.contains("No rated answers"));
    }

    #[test]
    fn perfect_prediction_scores_zero_on_both_measures() {
        // Always says 100 and is always right; always says 0 and is always wrong.
        let preds: Vec<_> = (0..10)
            .map(|i| {
                if i % 2 == 0 {
                    p(100, true)
                } else {
                    p(0, false)
                }
            })
            .collect();
        let r = report(&preds);
        assert!(r.brier.abs() < 1e-9, "brier should be 0, got {}", r.brier);
        assert!(r.ece.abs() < 1e-9, "ece should be 0, got {}", r.ece);
        assert!(r.bias.abs() < 1e-9);
        assert!(r.trustworthy);
    }

    #[test]
    fn always_saying_fifty_gives_the_reference_brier_of_a_quarter() {
        let preds: Vec<_> = (0..10).map(|i| p(50, i % 2 == 0)).collect();
        let r = report(&preds);
        assert!((r.brier - 0.25).abs() < 1e-9, "got {}", r.brier);
        // Confidence 0.5 and accuracy 0.5: no bias.
        assert!(r.bias.abs() < 1e-9);
    }

    #[test]
    fn confident_and_always_wrong_is_the_worst_possible_brier() {
        let preds: Vec<_> = (0..10).map(|_| p(100, false)).collect();
        let r = report(&preds);
        assert!((r.brier - 1.0).abs() < 1e-9);
        assert!((r.ece - 1.0).abs() < 1e-9);
        assert!((r.bias - 1.0).abs() < 1e-9);
    }

    #[test]
    fn detects_the_overconfidence_the_literature_predicts() {
        // Says 90 every time but is right only 6 in 10.
        let mut preds = vec![p(90, true); 6];
        preds.extend(vec![p(90, false); 4]);
        let r = report(&preds);
        assert!(r.bias > NOTABLE_BIAS, "bias was {}", r.bias);
        assert!((r.bias - 0.3).abs() < 1e-9);
        assert!(r.summary.contains("Overconfident"));
        assert!(r.summary.contains("30 points"));
    }

    #[test]
    fn detects_underconfidence_too() {
        // Says 40 but is right 9 in 10.
        let mut preds = vec![p(40, true); 9];
        preds.push(p(40, false));
        let r = report(&preds);
        assert!(r.bias < -NOTABLE_BIAS, "bias was {}", r.bias);
        assert!(r.summary.contains("Underconfident"));
    }

    #[test]
    fn calls_a_well_matched_sample_calibrated() {
        // 70 confidence, right 7 of 10.
        let mut preds = vec![p(70, true); 7];
        preds.extend(vec![p(70, false); 3]);
        let r = report(&preds);
        assert!(r.bias.abs() < 1e-9);
        assert!(r.summary.contains("Well calibrated"), "got: {}", r.summary);
    }

    #[test]
    fn small_samples_are_flagged_rather_than_trusted() {
        let r = report(&[p(90, true), p(90, false)]);
        assert!(!r.trustworthy);
        assert_eq!(r.sample_size, 2);
        assert!(r.summary.contains("2 rated answers"));
        assert!(r.summary.contains("10 needed"));
        // One answer should read as singular, not "1 rated answers".
        let r1 = report(&[p(90, true)]);
        assert!(
            r1.summary.contains("1 rated answer so far") && !r1.summary.contains("answers"),
            "got: {}",
            r1.summary
        );
    }

    #[test]
    fn bins_cover_only_populated_bands_and_land_in_the_right_one() {
        let preds = vec![p(5, false), p(55, true), p(95, true), p(100, true)];
        let r = report(&preds);
        assert_eq!(r.bins.len(), 3, "three populated bands: 0s, 50s, 90s");
        let lows: Vec<u8> = r.bins.iter().map(|b| b.low).collect();
        assert_eq!(lows, vec![0, 50, 90]);
        // 100 must fall in the top band, not spill into an eleventh.
        let top = r.bins.last().unwrap();
        assert_eq!(top.count, 2);
        assert_eq!(top.high, 100);
    }

    #[test]
    fn bin_accuracy_and_confidence_are_per_band_means() {
        let preds = vec![p(90, true), p(90, false), p(20, false)];
        let r = report(&preds);
        let high = r.bins.iter().find(|b| b.low == 90).unwrap();
        assert!((high.mean_confidence - 0.9).abs() < 1e-9);
        assert!((high.accuracy - 0.5).abs() < 1e-9);
        let low = r.bins.iter().find(|b| b.low == 20).unwrap();
        assert!((low.accuracy - 0.0).abs() < 1e-9);
    }

    #[test]
    fn ece_weights_bands_by_how_many_answers_they_hold() {
        // A big well-calibrated band and one small badly-calibrated band: ECE
        // should stay small because the bad band barely counts.
        let mut preds = vec![p(70, true); 7];
        preds.extend(vec![p(70, false); 3]); // 10 answers, perfectly calibrated
        preds.push(p(100, false)); // 1 answer, maximally wrong
        let r = report(&preds);
        // Only the 1-in-11 bad band contributes: 1/11 * 1.0.
        assert!((r.ece - 1.0 / 11.0).abs() < 1e-9, "got {}", r.ece);
    }

    #[test]
    fn confidence_above_one_hundred_is_clamped_not_trusted() {
        let r = report(&[p(255, true)]);
        assert!((r.mean_confidence - 1.0).abs() < 1e-9);
        assert!(r.bins[0].high == 100);
    }

    #[test]
    fn debias_subtracts_measured_overconfidence() {
        let mut preds = vec![p(90, true); 6];
        preds.extend(vec![p(90, false); 4]); // 30 points hot
        let r = report(&preds);
        // A fresh 90 should be treated as roughly 60.
        assert_eq!(debias(90, &r), 60);
        // Clamping holds at both ends.
        assert_eq!(debias(10, &r), 0);
        assert_eq!(debias(100, &r), 70);
    }

    #[test]
    fn debias_leaves_confidence_alone_until_there_is_evidence() {
        let r = report(&[p(90, false)]);
        assert!(!r.trustworthy);
        assert_eq!(debias(90, &r), 90, "must not adjust on noise");
    }

    fn event(id: u64, kind: &str, payload: serde_json::Value) -> crate::core::eventlog::Event {
        crate::core::eventlog::Event {
            id,
            ts: id as i64,
            kind: kind.into(),
            payload,
        }
    }

    #[test]
    fn pairs_answers_with_their_ratings_from_the_log() {
        let events = vec![
            event(1, "chat.user", serde_json::json!({ "text": "hi" })),
            event(
                2,
                "chat.assistant",
                serde_json::json!({ "msg_id": 20, "confidence": 90 }),
            ),
            event(
                3,
                "chat.assistant",
                serde_json::json!({ "msg_id": 21, "confidence": 40 }),
            ),
            event(
                4,
                RATED_EVENT,
                serde_json::json!({ "msg_id": 20, "helpful": true }),
            ),
            event(
                5,
                RATED_EVENT,
                serde_json::json!({ "msg_id": 21, "helpful": false }),
            ),
        ];
        let preds = pair_from_events(&events);
        assert_eq!(preds, vec![p(90, true), p(40, false)]);
    }

    #[test]
    fn unrated_and_unscored_answers_are_ignored() {
        let events = vec![
            // Answered but never rated.
            event(
                1,
                "chat.assistant",
                serde_json::json!({ "msg_id": 10, "confidence": 80 }),
            ),
            // Rated but the answer had no confidence (e.g. an older reply).
            event(2, "chat.assistant", serde_json::json!({ "msg_id": 11 })),
            event(
                3,
                RATED_EVENT,
                serde_json::json!({ "msg_id": 11, "helpful": true }),
            ),
            // Rated with no matching answer at all.
            event(
                4,
                RATED_EVENT,
                serde_json::json!({ "msg_id": 99, "helpful": true }),
            ),
        ];
        assert!(pair_from_events(&events).is_empty());
    }

    #[test]
    fn changing_a_rating_corrects_rather_than_double_counts() {
        let events = vec![
            event(
                1,
                "chat.assistant",
                serde_json::json!({ "msg_id": 5, "confidence": 70 }),
            ),
            event(
                2,
                RATED_EVENT,
                serde_json::json!({ "msg_id": 5, "helpful": true }),
            ),
            event(
                3,
                RATED_EVENT,
                serde_json::json!({ "msg_id": 5, "helpful": false }),
            ),
        ];
        let preds = pair_from_events(&events);
        assert_eq!(preds, vec![p(70, false)], "the later rating should win");
    }

    #[test]
    fn pairing_is_deterministic_regardless_of_map_ordering() {
        let mut events = Vec::new();
        for i in 0..25u64 {
            events.push(event(
                i * 2,
                "chat.assistant",
                serde_json::json!({ "msg_id": i as i64, "confidence": 50 + i }),
            ));
            events.push(event(
                i * 2 + 1,
                RATED_EVENT,
                serde_json::json!({ "msg_id": i as i64, "helpful": i % 2 == 0 }),
            ));
        }
        let a = pair_from_events(&events);
        let b = pair_from_events(&events);
        assert_eq!(a, b);
        assert_eq!(a.len(), 25);
        // Ascending message id, so the confidences come back in order.
        assert_eq!(a[0].confidence, 50);
        assert_eq!(a[24].confidence, 74);
    }

    #[test]
    fn report_serializes_for_the_ui() {
        let r = report(&[p(80, true), p(60, false)]);
        let json = serde_json::to_string(&r).unwrap();
        for key in ["sample_size", "brier", "ece", "bias", "bins", "summary"] {
            assert!(json.contains(key), "missing {key} in {json}");
        }
    }
}
