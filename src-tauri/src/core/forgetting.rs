//! Insight scoring, decay, and selective forgetting (§5.2, Reflection v1).
//!
//! Reflection v0 only accumulated. That fails in two directions: the prompt
//! budget fills with near-duplicates, and lessons that were true once ("groq is
//! rate-limiting today") outlive the situation that produced them and start
//! actively misleading the assistant. A memory that cannot forget is not a
//! memory, it is a log.
//!
//! Four signals decide what survives:
//!
//! 1. **Recency** — exponential decay with a half-life, so a lesson has to keep
//!    earning its place rather than being kept because it was written down once.
//! 2. **Corroboration** — a later reflection pass independently re-deriving the
//!    same lesson is the strongest evidence it is real, not a one-off artifact
//!    of a single noisy session. This is where merging duplicates pays off:
//!    instead of two weak copies, one strong lesson.
//! 3. **Use** — how often it has actually been injected into a prompt, as a
//!    proxy for relevance.
//! 4. **Kind** — lessons differ in expected lifetime. What the user prefers is
//!    stable for months; how a provider behaved is often stale within days. So
//!    `user` lessons decay slower than `provider` ones, which is the single
//!    biggest improvement over treating all insights alike.
//!
//! Forgetting is deliberately conservative and auditable: new lessons get a
//! protection window so they can't be culled before they've had a chance to be
//! corroborated, and every drop is reported so it lands in the event log and
//! stays undoable.
//!
//! Pure arithmetic and string comparison — no storage, no Tauri, no model.

/// An insight with the bookkeeping that scoring needs.
#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    pub id: i64,
    pub kind: String,
    pub content: String,
    /// Unix seconds.
    pub created_at: i64,
    /// Times a later reflection re-derived this same lesson.
    pub corroborations: u32,
    /// Times it has been injected into a prompt.
    pub uses: u32,
    /// The lesson's embedding, when one has been computed. `None` is the normal
    /// case, not an error: no embedding model on disk means duplicate detection
    /// falls back to word overlap rather than silently doing nothing.
    pub embedding: Option<Vec<f32>>,
}

/// How many insights we keep. The prompt only ever carries a handful, but the
/// store is also the user's record, so the cap is generous.
pub const CAPACITY: usize = 200;

/// A lesson younger than this is never culled: it hasn't had a chance to be
/// corroborated or used yet, and culling on no evidence is just noise.
pub const PROTECT_SECS: i64 = 3 * 24 * 3600;

/// Below this score an insight is considered spent (once out of protection).
pub const SCORE_FLOOR: f64 = 0.22;

/// Token-overlap ratio above which two lessons are treated as the same lesson.
pub const DUPLICATE_SIMILARITY: f64 = 0.6;

/// Cosine floor for calling two lessons the same by meaning.
///
/// Much higher than the token threshold, and deliberately so: sentence
/// embeddings put *any* two English sentences about the same broad subject in
/// the 0.7-0.85 range, so a threshold that looks strict for word overlap would
/// merge lessons that merely share a topic. 0.92 was chosen against real
/// reflection output: genuine paraphrases land above it, distinct lessons about
/// the same subject land below.
pub const DUPLICATE_COSINE: f64 = 0.92;

const W_CORROBORATION: f64 = 0.9;
const W_USE: f64 = 0.12;

/// Half-life in days, per kind. The differences are the point: a preference is
/// durable, an observation about a remote service usually isn't.
pub fn half_life_days(kind: &str) -> f64 {
    match kind {
        "user" => 120.0, // how you work changes slowly
        "skill" => 45.0, // code lessons age with the code
        "general" => 30.0,
        "provider" => 10.0, // service behaviour is the most perishable
        _ => 30.0,
    }
}

/// Exponential decay: 1.0 when fresh, 0.5 at one half-life, and so on.
pub fn decay_factor(age_secs: i64, half_life_days: f64) -> f64 {
    if half_life_days <= 0.0 {
        return 0.0;
    }
    let age_days = (age_secs.max(0) as f64) / 86_400.0;
    0.5_f64.powf(age_days / half_life_days)
}

/// Current worth of a lesson. Evidence lifts it, time pulls it down.
pub fn score(c: &Candidate, now: i64) -> f64 {
    let evidence = 1.0 + c.corroborations as f64 * W_CORROBORATION + c.uses as f64 * W_USE;
    evidence * decay_factor(now - c.created_at, half_life_days(&c.kind))
}

/// Normalized word set, for comparing two lessons by meaning-ish overlap:
/// lowercased, split on non-alphanumerics, stopwords dropped, plurals folded.
///
/// Deliberately crude — no embeddings. An embedding model would handle paraphrase
/// far better, but it is another model to ship and another download, and
/// reflection output is formulaic enough that token overlap catches the
/// duplicates that actually occur. The one refinement that proved necessary was
/// plural folding (see `stem`); without it real paraphrases scored 0.45 and
/// slipped past the duplicate threshold entirely.
fn tokens(text: &str) -> Vec<String> {
    const STOP: &[&str] = &[
        "the", "a", "an", "and", "or", "but", "is", "are", "was", "were", "to", "of", "in", "on",
        "for", "it", "its", "that", "this", "with", "as", "at", "by", "be", "been", "so", "than",
        "then", "when", "if", "not",
    ];
    let mut out: Vec<String> = text
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() > 2 && !STOP.contains(w))
        .map(stem)
        .collect();
    out.sort_unstable();
    out.dedup();
    out
}

/// Crudest useful stemming: fold regular plurals and third-person verbs so
/// "test"/"tests" and "fail"/"fails" count as the same word. Without this,
/// genuine paraphrases of one lesson score well below the duplicate threshold —
/// measured at 0.45 on a real pair, versus 0.78 with it.
fn stem(word: &str) -> String {
    let w = word.trim_end_matches('\'');
    for suffix in ["ies", "es", "s"] {
        if let Some(base) = w.strip_suffix(suffix) {
            // Keep short words intact ("is", "gas") and don't create stubs.
            if base.len() >= 3 {
                return if suffix == "ies" {
                    format!("{base}y")
                } else {
                    base.to_string()
                };
            }
        }
    }
    w.to_string()
}

/// Jaccard overlap of the two token sets, 0.0..=1.0.
pub fn similarity(a: &str, b: &str) -> f64 {
    let (ta, tb) = (tokens(a), tokens(b));
    if ta.is_empty() || tb.is_empty() {
        return 0.0;
    }
    let inter = ta.iter().filter(|w| tb.contains(w)).count() as f64;
    let union = (ta.len() + tb.len()) as f64 - inter;
    if union <= 0.0 {
        return 0.0;
    }
    inter / union
}

/// How two lessons were judged the same, and how strongly.
///
/// Kept separate rather than collapsed to one number because the two scales are
/// not comparable: 0.8 is a strong word overlap and a weak meaning match. A
/// single field would make the logged reason misleading.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case", tag = "by", content = "score")]
pub enum Match {
    /// Compared as embeddings. Catches paraphrase that shares no vocabulary.
    Meaning(f64),
    /// Compared as token sets. The fallback when either side lacks a vector.
    Words(f64),
}

impl Match {
    pub fn score(self) -> f64 {
        match self {
            Match::Meaning(v) | Match::Words(v) => v,
        }
    }

    /// Plain words for the event log, naming the method so a 0.93 meaning match
    /// is never read as a 0.93 word overlap.
    pub fn describe(self) -> String {
        match self {
            Match::Meaning(v) => format!("{:.0}% meaning match", v * 100.0),
            Match::Words(v) => format!("{:.0}% word overlap", v * 100.0),
        }
    }
}

/// Decides whether two lessons are the same lesson.
///
/// Prefers embeddings when both sides have one of the same width, and falls back
/// to word overlap otherwise. Mismatched widths mean the vectors came from
/// different embedding models, and comparing those is meaningless — so that case
/// falls back too rather than producing a confident wrong number.
pub fn duplicate_match(a: &Candidate, b: &Candidate) -> Option<Match> {
    if let (Some(va), Some(vb)) = (&a.embedding, &b.embedding) {
        if !va.is_empty() && va.len() == vb.len() {
            let cos = crate::core::embedding::cosine(va, vb) as f64;
            return (cos >= DUPLICATE_COSINE).then_some(Match::Meaning(cos));
        }
    }
    let sim = similarity(&a.content, &b.content);
    (sim >= DUPLICATE_SIMILARITY).then_some(Match::Words(sim))
}

/// One duplicate pair: the established lesson to keep, and the copy to drop.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Merge {
    /// The older lesson, which absorbs a corroboration.
    pub keep_id: i64,
    /// The newer near-duplicate, which is dropped.
    pub drop_id: i64,
    /// How the two were judged the same, and how strongly.
    pub matched: Match,
}

/// What a maintenance pass would do. Returned rather than applied so the caller
/// can log it and the user can undo it.
#[derive(Debug, Clone, PartialEq, serde::Serialize, Default)]
pub struct ForgetPlan {
    /// Near-duplicates to collapse into their established twin.
    pub merges: Vec<Merge>,
    /// Ids to drop: spent, or squeezed out by the capacity cap.
    pub forget: Vec<i64>,
    /// Human-readable reason per forgotten id, for the event log.
    pub reasons: Vec<(i64, String)>,
    /// How many survive.
    pub kept: usize,
}

/// Decides what to merge and what to forget.
///
/// Order matters: merging first means a lesson that was independently re-derived
/// gets its corroboration credit *before* scoring decides whether it lives. Doing
/// it the other way round would cull lessons for being duplicated, which is the
/// opposite of what duplication tells us.
pub fn plan(candidates: &[Candidate], now: i64, capacity: usize) -> ForgetPlan {
    let mut plan = ForgetPlan::default();
    if candidates.is_empty() {
        return plan;
    }

    // Oldest first, so the established copy of a duplicate pair is the keeper.
    let mut ordered: Vec<&Candidate> = candidates.iter().collect();
    ordered.sort_by_key(|c| (c.created_at, c.id));

    let mut dropped: Vec<i64> = Vec::new();
    let mut extra_corroboration: std::collections::HashMap<i64, u32> =
        std::collections::HashMap::new();

    for (i, later) in ordered.iter().enumerate() {
        if dropped.contains(&later.id) {
            continue;
        }
        for earlier in ordered.iter().take(i) {
            if dropped.contains(&earlier.id) || earlier.kind != later.kind {
                continue;
            }
            if let Some(matched) = duplicate_match(earlier, later) {
                plan.merges.push(Merge {
                    keep_id: earlier.id,
                    drop_id: later.id,
                    matched,
                });
                *extra_corroboration.entry(earlier.id).or_insert(0) += 1;
                dropped.push(later.id);
                plan.reasons.push((
                    later.id,
                    format!(
                        "merged into #{} (same lesson, {})",
                        earlier.id,
                        matched.describe()
                    ),
                ));
                break;
            }
        }
    }

    // Score the survivors, crediting corroboration earned during the merge pass.
    let mut scored: Vec<(f64, &Candidate)> = ordered
        .iter()
        .filter(|c| !dropped.contains(&c.id))
        .map(|c| {
            let bonus = extra_corroboration.get(&c.id).copied().unwrap_or(0);
            let credited = Candidate {
                corroborations: c.corroborations + bonus,
                ..(*c).clone()
            };
            (score(&credited, now), *c)
        })
        .collect();

    // Spent lessons: past the protection window and below the floor.
    let mut spent = Vec::new();
    scored.retain(|(s, c)| {
        let protected = now - c.created_at < PROTECT_SECS;
        if !protected && *s < SCORE_FLOOR {
            spent.push((c.id, *s));
            false
        } else {
            true
        }
    });
    for (id, s) in spent {
        dropped.push(id);
        plan.reasons
            .push((id, format!("faded (score {s:.2} below {SCORE_FLOOR:.2})")));
    }

    // Capacity: keep the strongest, but never cull inside the protection window.
    if scored.len() > capacity {
        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        let overflow: Vec<(f64, &Candidate)> = scored.split_off(capacity);
        for (s, c) in overflow {
            if now - c.created_at < PROTECT_SECS {
                scored.push((s, c)); // too new to judge; keep it
                continue;
            }
            dropped.push(c.id);
            plan.reasons.push((
                c.id,
                format!("over capacity ({capacity}), weakest at {s:.2}"),
            ));
        }
    }

    plan.kept = candidates.len() - dropped.len();
    plan.forget = dropped;
    plan
}

/// The freshest, strongest lessons to ride along in a prompt. Scoring here is
/// what makes injection selective rather than "the most recent N".
pub fn top_for_prompt(candidates: &[Candidate], now: i64, limit: usize) -> Vec<&Candidate> {
    let mut scored: Vec<(f64, &Candidate)> =
        candidates.iter().map(|c| (score(c, now), c)).collect();
    scored.sort_by(|a, b| {
        b.0.partial_cmp(&a.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| b.1.created_at.cmp(&a.1.created_at))
    });
    scored.into_iter().take(limit).map(|(_, c)| c).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: i64 = 86_400;
    const NOW: i64 = 1_800_000_000;

    fn c(id: i64, kind: &str, content: &str, age_days: i64) -> Candidate {
        Candidate {
            id,
            kind: kind.into(),
            content: content.into(),
            created_at: NOW - age_days * DAY,
            corroborations: 0,
            uses: 0,
            embedding: None,
        }
    }

    #[test]
    fn decay_halves_at_one_half_life() {
        assert!((decay_factor(0, 30.0) - 1.0).abs() < 1e-9);
        assert!((decay_factor(30 * DAY, 30.0) - 0.5).abs() < 1e-9);
        assert!((decay_factor(60 * DAY, 30.0) - 0.25).abs() < 1e-9);
        // Degenerate half-life must not produce NaN or infinity.
        assert_eq!(decay_factor(DAY, 0.0), 0.0);
        assert!(decay_factor(-DAY, 30.0).is_finite());
    }

    #[test]
    fn perishable_kinds_decay_faster_than_durable_ones() {
        // The core design claim: a provider observation should fade long before
        // a lesson about how the user works.
        assert!(half_life_days("provider") < half_life_days("skill"));
        assert!(half_life_days("skill") < half_life_days("user"));
        let age = 30 * DAY;
        let provider = score(&c(1, "provider", "groq rate limited", 30), NOW);
        let user = score(&c(2, "user", "prefers short answers", 30), NOW);
        assert!(user > provider * 2.0, "user {user} vs provider {provider}");
        let _ = age;
    }

    #[test]
    fn corroboration_outweighs_a_single_fresh_lesson() {
        let fresh = c(1, "general", "something new", 0);
        let mut established = c(2, "general", "something proven", 20);
        established.corroborations = 3;
        assert!(
            score(&established, NOW) > score(&fresh, NOW),
            "three-times-confirmed should beat brand new"
        );
    }

    #[test]
    fn use_counts_lift_a_lesson_but_less_than_corroboration() {
        let mut used = c(1, "general", "x", 10);
        used.uses = 5;
        let mut corroborated = c(2, "general", "x", 10);
        corroborated.corroborations = 5;
        assert!(score(&corroborated, NOW) > score(&used, NOW));
        assert!(score(&used, NOW) > score(&c(3, "general", "x", 10), NOW));
    }

    #[test]
    fn stemming_folds_regular_plurals_without_mangling_short_words() {
        assert_eq!(stem("tests"), "test");
        assert_eq!(stem("fails"), "fail");
        assert_eq!(stem("skills"), "skill");
        assert_eq!(stem("policies"), "policy");
        // Must not shave real words down to stubs.
        assert_eq!(stem("gas"), "gas");
        assert_eq!(stem("is"), "is");
        assert_eq!(
            stem("class"),
            "clas",
            "acceptable: consistent for both sides"
        );
        assert_eq!(stem("note"), "note");
    }

    #[test]
    fn similarity_recognises_the_same_lesson_reworded() {
        let a = "avoid string interpolation in rhai skills, it fails the test";
        let b = "rhai skills should avoid string interpolation because tests fail";
        assert!(
            similarity(a, b) >= DUPLICATE_SIMILARITY,
            "got {}",
            similarity(a, b)
        );
    }

    #[test]
    fn similarity_separates_genuinely_different_lessons() {
        let a = "groq rate limits burst chats, back off to ollama";
        let b = "the user prefers short answers with details on request";
        assert!(
            similarity(a, b) < DUPLICATE_SIMILARITY,
            "got {}",
            similarity(a, b)
        );
        assert_eq!(similarity("", "anything"), 0.0);
        assert_eq!(similarity("the a an", "of to in"), 0.0, "stopwords only");
    }

    #[test]
    fn similarity_is_symmetric_and_self_identical() {
        let a = "skills that touch notes must validate the title";
        let b = "validate the title before writing a note from a skill";
        assert!((similarity(a, b) - similarity(b, a)).abs() < 1e-12);
        assert!((similarity(a, a) - 1.0).abs() < 1e-12);
    }

    #[test]
    fn duplicates_merge_into_the_established_lesson() {
        // Same lesson twice: the older one survives and absorbs the credit.
        let items = vec![
            c(
                1,
                "skill",
                "avoid string interpolation in rhai, tests fail",
                20,
            ),
            c(
                2,
                "skill",
                "rhai string interpolation fails tests, avoid it",
                2,
            ),
        ];
        let p = plan(&items, NOW, CAPACITY);
        assert_eq!(p.merges.len(), 1);
        assert_eq!(p.merges[0].keep_id, 1, "older is the keeper");
        assert_eq!(p.merges[0].drop_id, 2);
        assert_eq!(p.forget, vec![2]);
        assert_eq!(p.kept, 1);
    }

    #[test]
    fn different_kinds_are_never_merged_even_if_worded_alike() {
        let items = vec![
            c(1, "skill", "the notes tool needs a title", 20),
            c(2, "user", "the notes tool needs a title", 2),
        ];
        let p = plan(&items, NOW, CAPACITY);
        assert!(
            p.merges.is_empty(),
            "cross-kind merge would confuse categories"
        );
        assert_eq!(p.kept, 2);
    }

    #[test]
    fn a_stale_perishable_lesson_is_forgotten() {
        // A provider note from 100 days ago is long past useful: 10-day
        // half-life puts it far under the floor.
        let items = vec![c(1, "provider", "groq is rate limiting today", 100)];
        let p = plan(&items, NOW, CAPACITY);
        assert_eq!(p.forget, vec![1]);
        assert!(p.reasons[0].1.contains("faded"));
        assert_eq!(p.kept, 0);
    }

    #[test]
    fn a_durable_lesson_of_the_same_age_survives() {
        // Same age, different kind: this is the whole point of per-kind decay.
        let items = vec![c(1, "user", "prefers the conclusion first", 100)];
        let p = plan(&items, NOW, CAPACITY);
        assert!(
            p.forget.is_empty(),
            "user lessons should outlive provider ones"
        );
        assert_eq!(p.kept, 1);
    }

    #[test]
    fn new_lessons_are_protected_from_being_culled() {
        // Brand new and unproven, but inside the protection window, so it must
        // survive even though its score is unremarkable.
        let items = vec![c(1, "provider", "something just observed", 0)];
        let p = plan(&items, NOW, CAPACITY);
        assert!(p.forget.is_empty());

        // And protection holds against the capacity cap too.
        let mut many: Vec<Candidate> = (0..5)
            .map(|i| {
                let mut old = c(100 + i, "user", &format!("established lesson {i}"), 5);
                old.corroborations = 4;
                old
            })
            .collect();
        many.push(c(1, "provider", "brand new and weak", 0));
        let p = plan(&many, NOW, 5);
        assert!(
            !p.forget.contains(&1),
            "a lesson too new to judge must not be squeezed out by capacity"
        );
    }

    #[test]
    fn capacity_drops_the_weakest_established_lessons_first() {
        // Content must be genuinely distinct, or the merge pass collapses these
        // before capacity ever gets a look — which is correct behaviour, just
        // not what this test is about.
        const STRONG: [&str; 3] = [
            "prefers the conclusion before the detail",
            "works late, keep the dark theme default",
            "wants links checked before they are shared",
        ];
        const WEAK: [&str; 3] = [
            "once asked about timezone handling",
            "mentioned a preference for metric units",
            "opened the notes view during onboarding",
        ];
        let mut items = Vec::new();
        // Strong: recent and corroborated.
        for (i, text) in STRONG.iter().enumerate() {
            let mut s = c(i as i64, "user", text, 10);
            s.corroborations = 5;
            items.push(s);
        }
        // Weak: old, uncorroborated, but still above the floor for `user`.
        for (i, text) in WEAK.iter().enumerate() {
            items.push(c(3 + i as i64, "user", text, 60));
        }
        let p = plan(&items, NOW, 3);
        assert!(
            p.merges.is_empty(),
            "these lessons are distinct; none should merge"
        );
        assert_eq!(p.kept, 3);
        // The three survivors should be the corroborated ones.
        for weak in 3..6 {
            assert!(p.forget.contains(&weak), "weak {weak} should be dropped");
        }
        for strong in 0..3 {
            assert!(
                !p.forget.contains(&strong),
                "strong {strong} should survive"
            );
        }
        assert!(p.reasons.iter().any(|(_, r)| r.contains("over capacity")));
    }

    #[test]
    fn an_empty_store_produces_an_empty_plan() {
        let p = plan(&[], NOW, CAPACITY);
        assert_eq!(p, ForgetPlan::default());
        assert_eq!(p.kept, 0);
    }

    #[test]
    fn plan_is_deterministic() {
        let items = vec![
            c(1, "skill", "avoid interpolation in rhai tests", 20),
            c(2, "skill", "rhai interpolation breaks tests avoid", 2),
            c(3, "provider", "groq rate limits bursts", 90),
            c(4, "user", "prefers short answers", 40),
        ];
        let a = plan(&items, NOW, CAPACITY);
        let b = plan(&items, NOW, CAPACITY);
        assert_eq!(a, b);
    }

    #[test]
    fn every_forgotten_id_carries_a_reason() {
        let items = vec![
            c(1, "provider", "stale provider note", 120),
            c(2, "skill", "avoid interpolation in rhai tests", 30),
            c(3, "skill", "rhai interpolation breaks tests avoid", 4),
        ];
        let p = plan(&items, NOW, CAPACITY);
        assert!(!p.forget.is_empty());
        for id in &p.forget {
            assert!(
                p.reasons.iter().any(|(rid, _)| rid == id),
                "id {id} forgotten with no reason — the event log needs one"
            );
        }
    }

    #[test]
    fn prompt_selection_prefers_strong_lessons_over_merely_recent_ones() {
        let fresh_weak = c(1, "provider", "just saw a timeout", 0);
        let mut proven = c(2, "user", "prefers the conclusion first", 30);
        proven.corroborations = 4;
        // Bound to a local: top_for_prompt borrows from the slice.
        let items = [fresh_weak, proven];
        let picked = top_for_prompt(&items, NOW, 1);
        assert_eq!(picked.len(), 1);
        assert_eq!(
            picked[0].id, 2,
            "a proven lesson should outrank a fresh guess"
        );
    }

    #[test]
    fn prompt_selection_respects_the_limit_and_handles_empty() {
        let items: Vec<Candidate> = (0..10)
            .map(|i| c(i, "general", &format!("l{i}"), i))
            .collect();
        assert_eq!(top_for_prompt(&items, NOW, 3).len(), 3);
        assert_eq!(top_for_prompt(&items, NOW, 0).len(), 0);
        assert!(top_for_prompt(&[], NOW, 5).is_empty());
    }

    #[test]
    fn plan_serializes_for_the_ui() {
        let items = vec![c(1, "provider", "stale", 200)];
        let json = serde_json::to_string(&plan(&items, NOW, CAPACITY)).unwrap();
        for key in ["merges", "forget", "reasons", "kept"] {
            assert!(json.contains(key), "missing {key}");
        }
    }
    // --- duplicate detection by meaning (Reflection v2) ---

    /// A unit vector pointing mostly along `axis`, tilted by `tilt`. Lets a test
    /// place two lessons at a chosen cosine without needing a real model.
    fn vec_at(axis: usize, tilt: f32) -> Vec<f32> {
        let mut v = [0.0f32; 8];
        v[axis] = 1.0;
        v[(axis + 1) % 8] = tilt;
        let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        v.iter().map(|x| x / norm).collect()
    }

    fn with_vector(id: i64, content: &str, vector: Option<Vec<f32>>) -> Candidate {
        Candidate {
            id,
            kind: "skill".into(),
            content: content.into(),
            created_at: NOW,
            corroborations: 0,
            uses: 0,
            embedding: vector,
        }
    }

    #[test]
    fn paraphrase_with_no_shared_words_is_caught_by_meaning() {
        // The whole reason for embeddings: these two say the same thing and share
        // almost no vocabulary, so token overlap cannot see it.
        let a = with_vector(
            1,
            "rhai skills break on string interpolation",
            Some(vec_at(0, 0.0)),
        );
        let b = with_vector(2, "avoid ${} inside generated code", Some(vec_at(0, 0.05)));
        assert!(
            similarity(&a.content, &b.content) < DUPLICATE_SIMILARITY,
            "words alone should miss this"
        );
        match duplicate_match(&a, &b) {
            Some(Match::Meaning(v)) => assert!(v >= DUPLICATE_COSINE, "cosine {v}"),
            other => panic!("expected a meaning match, got {other:?}"),
        }
    }

    #[test]
    fn same_topic_different_lesson_is_not_a_duplicate() {
        // The failure mode embeddings introduce: two lessons about skills are
        // *related* without being the same, and merging them loses one.
        let a = with_vector(1, "skill tests catch broken skills", Some(vec_at(0, 0.0)));
        let b = with_vector(2, "skills should be versioned", Some(vec_at(0, 0.7)));
        let cos = crate::core::embedding::cosine(
            a.embedding.as_ref().unwrap(),
            b.embedding.as_ref().unwrap(),
        );
        assert!(cos > 0.7, "these are genuinely related: {cos}");
        assert!(cos < DUPLICATE_COSINE as f32, "but not the same: {cos}");
        assert_eq!(duplicate_match(&a, &b), None);
    }

    #[test]
    fn a_missing_vector_falls_back_to_words_rather_than_giving_up() {
        // No embedding model on disk is the normal case, not an error.
        let a = with_vector(1, "skill tests catch broken skills", None);
        let b = with_vector(2, "skill test catches a broken skill", None);
        match duplicate_match(&a, &b) {
            Some(Match::Words(v)) => assert!(v >= DUPLICATE_SIMILARITY, "overlap {v}"),
            other => panic!("expected a word match, got {other:?}"),
        }
        // One side missing is still a fallback, not a half-comparison.
        let c = with_vector(3, "skill tests catch broken skills", Some(vec_at(0, 0.0)));
        assert!(matches!(duplicate_match(&c, &b), Some(Match::Words(_))));
    }

    #[test]
    fn vectors_of_different_widths_are_never_compared() {
        // Different embedding models produce different widths. Comparing them
        // yields a confident wrong number, so it must fall back instead.
        let a = with_vector(1, "one lesson", Some(vec![1.0, 0.0]));
        let b = with_vector(2, "another lesson entirely", Some(vec_at(0, 0.0)));
        assert!(!matches!(duplicate_match(&a, &b), Some(Match::Meaning(_))));
    }

    #[test]
    fn the_logged_reason_names_the_method_it_used() {
        // 0.93 word overlap and 0.93 cosine mean very different things; a reason
        // line that hides which one was used is misleading.
        assert!(Match::Meaning(0.93).describe().contains("meaning"));
        assert!(Match::Words(0.93).describe().contains("word"));
        assert_ne!(
            Match::Meaning(0.93).describe(),
            Match::Words(0.93).describe()
        );
    }

    #[test]
    fn a_meaning_duplicate_is_merged_and_credits_the_keeper() {
        // End to end through plan(): the paraphrase is dropped, the original
        // absorbs the corroboration, and the reason says how it was judged.
        let mut older = with_vector(1, "rhai breaks on interpolation", Some(vec_at(0, 0.0)));
        older.created_at = NOW - 10 * DAY;
        let newer = with_vector(2, "never use ${} in generated code", Some(vec_at(0, 0.05)));
        let plan = plan(&[older, newer], NOW, CAPACITY);
        assert_eq!(plan.merges.len(), 1, "{plan:?}");
        assert_eq!(plan.merges[0].keep_id, 1, "the older copy survives");
        assert_eq!(plan.merges[0].drop_id, 2);
        assert!(matches!(plan.merges[0].matched, Match::Meaning(_)));
        let reason = &plan.reasons.iter().find(|(id, _)| *id == 2).unwrap().1;
        assert!(reason.contains("meaning match"), "{reason}");
    }
}
