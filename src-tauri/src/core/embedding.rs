//! Semantic recall (§ memory tier two): vector similarity over past messages.
//!
//! The design bets on smallness. Embeddings come from the same Ollama the chat
//! already uses (`nomic-embed-text`, a ~270 MB one-time pull, or whatever the
//! user configures); vectors are stored as little-endian f32 BLOBs in the
//! existing SQLite database; search is brute-force cosine in Rust. No FAISS, no
//! sqlite-vec C extension, no index structure — at the scale of a personal
//! chat history (thousands of messages, not millions), scanning every vector
//! is sub-millisecond work and the honest-engineering answer. If someone's
//! history ever outgrows it, an ANN index is a drop-in behind the same
//! functions.
//!
//! Everything in this module is pure: encoding, similarity, ranking. The I/O
//! (Ollama call, SQLite) lives in `router` and `memory` respectively.

/// Similarity floor below which a hit is treated as noise rather than recall.
/// Cosine on modern embedding models puts unrelated text around 0.2-0.4 and
/// clearly related text above 0.6; 0.45 keeps borderline association out of
/// the prompt without demanding near-duplicates.
pub const RECALL_FLOOR: f32 = 0.45;

/// How many recalled messages ride along in a chat prompt. Small on purpose:
/// recall supplements the recent window, it must not drown it.
pub const RECALL_IN_PROMPT: usize = 4;

/// Serializes a vector as little-endian f32 bytes for BLOB storage.
pub fn to_bytes(vector: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(vector.len() * 4);
    for v in vector {
        out.extend_from_slice(&v.to_le_bytes());
    }
    out
}

/// Reads a BLOB back into a vector. Returns None on a torn length rather than
/// guessing: a corrupt embedding should drop out of search, not skew it.
pub fn from_bytes(bytes: &[u8]) -> Option<Vec<f32>> {
    if !bytes.len().is_multiple_of(4) {
        return None;
    }
    Some(
        bytes
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect(),
    )
}

/// Cosine similarity in -1.0..=1.0. Dimension mismatches and zero vectors
/// score 0 (unrelated) instead of erroring: they can only come from mixing
/// embedding models, and search should survive that.
pub fn cosine(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let (mut dot, mut na, mut nb) = (0.0f32, 0.0f32, 0.0f32);
    for (x, y) in a.iter().zip(b) {
        dot += x * y;
        na += x * x;
        nb += y * y;
    }
    if na <= f32::EPSILON || nb <= f32::EPSILON {
        return 0.0;
    }
    dot / (na.sqrt() * nb.sqrt())
}

/// A scored search hit.
#[derive(Debug, Clone, PartialEq)]
pub struct Scored {
    pub id: i64,
    pub score: f32,
}

/// Ranks stored vectors against a query, best first, dropping everything under
/// `floor`. `exclude` removes ids that are already in the prompt anyway (the
/// recent window) — recalling what is already visible is worse than nothing.
pub fn top_k(
    query: &[f32],
    items: &[(i64, Vec<f32>)],
    k: usize,
    floor: f32,
    exclude: &[i64],
) -> Vec<Scored> {
    let mut scored: Vec<Scored> = items
        .iter()
        .filter(|(id, _)| !exclude.contains(id))
        .map(|(id, v)| Scored {
            id: *id,
            score: cosine(query, v),
        })
        .filter(|s| s.score >= floor)
        .collect();
    scored.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| b.id.cmp(&a.id))
    });
    scored.truncate(k);
    scored
}

/// Formats recalled messages as the system-prompt section the model sees.
/// Content is clipped so one long old message can't eat the context budget.
pub fn recall_prompt_section(hits: &[(String, String)]) -> String {
    if hits.is_empty() {
        return String::new();
    }
    let mut out = String::from(
        "\n\nRelevant moments from earlier conversations (recalled by similarity; \
         use them only when they genuinely apply):",
    );
    for (role, content) in hits {
        let clipped: String = content.chars().take(280).collect();
        let ellipsis = if content.chars().count() > 280 {
            "…"
        } else {
            ""
        };
        out.push_str(&format!("\n- ({role}) {clipped}{ellipsis}"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_roundtrip_exactly() {
        let v = vec![0.0f32, 1.0, -1.0, 0.5, f32::MIN_POSITIVE, 12345.678];
        assert_eq!(from_bytes(&to_bytes(&v)).unwrap(), v);
        assert_eq!(from_bytes(&[]).unwrap(), Vec::<f32>::new());
    }

    #[test]
    fn torn_blobs_are_rejected_not_guessed() {
        assert!(from_bytes(&[1, 2, 3]).is_none());
        assert!(from_bytes(&[1, 2, 3, 4, 5]).is_none());
    }

    #[test]
    fn cosine_matches_hand_computed_cases() {
        assert!((cosine(&[1.0, 0.0], &[1.0, 0.0]) - 1.0).abs() < 1e-6);
        assert!((cosine(&[1.0, 0.0], &[0.0, 1.0])).abs() < 1e-6);
        assert!((cosine(&[1.0, 0.0], &[-1.0, 0.0]) + 1.0).abs() < 1e-6);
        // 45 degrees.
        assert!((cosine(&[1.0, 0.0], &[1.0, 1.0]) - std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-6);
    }

    #[test]
    fn cosine_survives_bad_input_instead_of_erroring() {
        assert_eq!(cosine(&[1.0, 2.0], &[1.0]), 0.0, "dimension mismatch");
        assert_eq!(cosine(&[], &[]), 0.0, "empty");
        assert_eq!(cosine(&[0.0, 0.0], &[1.0, 1.0]), 0.0, "zero vector");
    }

    #[test]
    fn top_k_ranks_best_first_and_respects_floor_and_limit() {
        let items = vec![
            (1, vec![1.0, 0.0]),  // identical -> 1.0
            (2, vec![1.0, 1.0]),  // ~0.707
            (3, vec![0.0, 1.0]),  // 0.0 -> under floor
            (4, vec![-1.0, 0.0]), // -1.0 -> under floor
        ];
        let hits = top_k(&[1.0, 0.0], &items, 10, RECALL_FLOOR, &[]);
        assert_eq!(hits.iter().map(|h| h.id).collect::<Vec<_>>(), vec![1, 2]);
        assert!(hits[0].score > hits[1].score);

        let limited = top_k(&[1.0, 0.0], &items, 1, RECALL_FLOOR, &[]);
        assert_eq!(limited.len(), 1);
        assert_eq!(limited[0].id, 1);
    }

    #[test]
    fn top_k_excludes_whats_already_in_the_prompt() {
        let items = vec![(1, vec![1.0, 0.0]), (2, vec![1.0, 0.1])];
        let hits = top_k(&[1.0, 0.0], &items, 10, 0.0, &[1]);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].id, 2, "the excluded id must not be recalled");
    }

    #[test]
    fn top_k_is_deterministic_on_ties() {
        // Two identical vectors: newer id (higher) wins the tie, consistently.
        let items = vec![(1, vec![1.0, 0.0]), (2, vec![1.0, 0.0])];
        let a = top_k(&[1.0, 0.0], &items, 2, 0.0, &[]);
        let b = top_k(&[1.0, 0.0], &items, 2, 0.0, &[]);
        assert_eq!(a, b);
        assert_eq!(a[0].id, 2);
    }

    #[test]
    fn prompt_section_is_empty_for_no_hits_and_clips_long_content() {
        assert_eq!(recall_prompt_section(&[]), "");
        let long = "x".repeat(500);
        let section = recall_prompt_section(&[("user".into(), long)]);
        assert!(section.contains("Relevant moments"));
        assert!(section.contains('…'), "long content should be clipped");
        // 280 chars + ellipsis, not 500.
        assert!(section.len() < 450);
    }
}
