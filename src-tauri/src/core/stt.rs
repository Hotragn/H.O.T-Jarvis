//! On-device speech-to-text (§6.4, Voice v1).
//!
//! Voice v0 leaned on the WebView's `SpeechRecognition`, which WebView2 doesn't
//! implement — so on Windows, voice *input* silently didn't exist. This module
//! is the honest fix: the microphone is read in Rust and transcribed by a local
//! Whisper model, so dictation works offline, costs nothing, and no audio ever
//! leaves the machine.
//!
//! Why candle instead of whisper.cpp bindings: those need cmake, a C++ compiler,
//! and LLVM/libclang (with `LIBCLANG_PATH` set) on every contributor's machine.
//! candle needs none of those, which keeps "clone and run" close to true. It is
//! not entirely C-free — candle-core pulls tokenizers with default features,
//! which builds Oniguruma from C, so a plain C compiler is still required — but
//! that is a much lighter ask. candle is slower than whisper.cpp, which is why
//! the default model is the smallest useful one.
//!
//! Everything here is deliberately inference-engine-agnostic: the catalog,
//! cache layout, download bookkeeping, and transcript cleanup are pure logic
//! and unit-tested. The actual model call lives behind the `Transcriber` trait
//! (see `whisper.rs`, compiled only with the `local-whisper` feature).

use std::path::{Path, PathBuf};

/// A downloadable Whisper checkpoint. Quantized GGUF weights keep the download
/// small enough that "free and local" stays a real promise on a laptop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelSpec {
    /// Stable id used in config, events, and the UI.
    pub id: &'static str,
    /// Human label for the picker.
    pub label: &'static str,
    /// Hugging Face repo the files come from.
    pub repo: &'static str,
    /// Quantized weights file within the repo.
    pub weights: &'static str,
    /// Tokenizer file within the repo.
    pub tokenizer: &'static str,
    /// Model config file within the repo.
    pub config: &'static str,
    /// Approximate total download, for an honest "this will fetch ~N MB" prompt.
    pub approx_mb: u32,
    /// English-only checkpoints are smaller and sharper for English dictation.
    pub english_only: bool,
}

/// The catalog. Only checkpoints that actually exist upstream are listed — every
/// entry here was verified to return HTTP 200, because a model picker that
/// offers a 404 is worse than a short list. Sizes are the real content lengths
/// (weights + tokenizer + config), rounded up.
pub const MODELS: &[ModelSpec] = &[
    ModelSpec {
        id: "tiny.en",
        label: "Tiny (English) — fastest, ~43 MB",
        repo: "lmz/candle-whisper",
        weights: "model-tiny-en-q80.gguf",
        tokenizer: "tokenizer-tiny-en.json",
        config: "config-tiny-en.json",
        approx_mb: 43,
        english_only: true,
    },
    ModelSpec {
        id: "tiny",
        label: "Tiny (multilingual) — ~43 MB",
        repo: "lmz/candle-whisper",
        weights: "model-tiny-q80.gguf",
        tokenizer: "tokenizer-tiny.json",
        config: "config-tiny.json",
        approx_mb: 43,
        english_only: false,
    },
];

/// What we reach for when the user hasn't chosen: smallest useful model.
pub const DEFAULT_MODEL: &str = "tiny.en";

pub fn find_model(id: &str) -> Option<&'static ModelSpec> {
    MODELS.iter().find(|m| m.id == id)
}

/// Resolves a requested id to a real spec, falling back to the default rather
/// than failing — a stale config shouldn't break voice entirely.
pub fn resolve_model(id: Option<&str>) -> &'static ModelSpec {
    id.and_then(find_model)
        .or_else(|| find_model(DEFAULT_MODEL))
        .unwrap_or(&MODELS[0])
}

/// Public mirror URL for a file in a Hugging Face repo. No token, no account:
/// these are openly downloadable, which is what keeps this free.
pub fn file_url(spec: &ModelSpec, file: &str) -> String {
    format!("https://huggingface.co/{}/resolve/main/{}", spec.repo, file)
}

/// Where a model's files are cached. Namespaced per model id so switching
/// models doesn't clobber the previous download.
pub fn model_dir(data_dir: &Path, spec: &ModelSpec) -> PathBuf {
    data_dir.join("models").join("whisper").join(spec.id)
}

/// The three files a model needs, as (url, destination) pairs.
pub fn required_files(data_dir: &Path, spec: &ModelSpec) -> Vec<(String, PathBuf)> {
    let dir = model_dir(data_dir, spec);
    [spec.weights, spec.tokenizer, spec.config]
        .iter()
        .map(|f| (file_url(spec, f), dir.join(f)))
        .collect()
}

/// True when every file is present and non-empty. Size is the cheap integrity
/// check: a truncated download leaves a short file, and we write atomically so
/// a partial file never lands at the final path.
pub fn is_downloaded(data_dir: &Path, spec: &ModelSpec) -> bool {
    required_files(data_dir, spec).iter().all(|(_, path)| {
        std::fs::metadata(path)
            .map(|m| m.is_file() && m.len() > 0)
            .unwrap_or(false)
    })
}

/// The temp path a download is streamed to before being moved into place.
pub fn staging_path(final_path: &Path) -> PathBuf {
    let mut name = final_path.file_name().unwrap_or_default().to_os_string();
    name.push(".partial");
    final_path.with_file_name(name)
}

/// How the assistant can currently hear, so the UI can tell the truth instead
/// of showing a mic button that does nothing.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case", tag = "state")]
pub enum SttReadiness {
    /// Built with `local-whisper` and the model is cached: real local dictation.
    Ready { model: String },
    /// Built with `local-whisper` but the model still needs fetching.
    NeedsDownload { model: String, approx_mb: u32 },
    /// Built without the feature — say so plainly rather than pretending.
    NotCompiled,
}

/// Model output is littered with control tokens, bracketed non-speech markers,
/// and duplicated whitespace. This is what turns raw decoder output into
/// something worth putting in the composer.
pub fn clean_transcript(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut depth_angle = 0usize;
    // Strip <|timestamp|> / <|en|> / <|transcribe|> style control tokens.
    let mut chars = raw.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '<' if chars.peek() == Some(&'|') => {
                depth_angle += 1;
            }
            '|' if depth_angle > 0 && chars.peek() == Some(&'>') => {
                chars.next();
                depth_angle -= 1;
            }
            _ if depth_angle > 0 => {}
            _ => out.push(c),
        }
    }

    // Drop non-speech annotations: [BLANK_AUDIO], (music), [ Silence ].
    let mut cleaned = String::with_capacity(out.len());
    let mut skip_depth = 0usize;
    for c in out.chars() {
        match c {
            '[' | '(' => skip_depth += 1,
            ']' | ')' => skip_depth = skip_depth.saturating_sub(1),
            _ if skip_depth > 0 => {}
            _ => cleaned.push(c),
        }
    }

    cleaned.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Joins per-window transcripts from a long take, dropping any window that is
/// only a silence hallucination.
///
/// Filtering per window matters: a filler window concatenated with real speech
/// ("open the notes" + "thank you") produces a string that no longer matches the
/// filler list, so checking only the joined result lets it through.
pub fn join_windows(pieces: &[String]) -> String {
    let kept: Vec<&str> = pieces
        .iter()
        .map(|p| p.trim())
        .filter(|p| !p.is_empty() && !is_probably_silence(p))
        .collect();
    let joined = clean_transcript(&kept.join(" "));
    if is_probably_silence(&joined) {
        return String::new();
    }
    joined
}

/// Whisper hallucinates filler on silence — a bare "thank you" or "you" from an
/// empty room is the classic case. Treat a transcript this thin as nothing said.
pub fn is_probably_silence(text: &str) -> bool {
    let t = text.trim().trim_matches(|c: char| !c.is_alphanumeric());
    if t.is_empty() {
        return true;
    }
    const FILLER: &[&str] = &[
        "you",
        "thank you",
        "thanks for watching",
        "thank you for watching",
        "bye",
        "okay",
        "ok",
        "um",
        "uh",
        "hmm",
        "mm",
    ];
    let lower = t.to_lowercase();
    FILLER.contains(&lower.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn catalog_ids_are_unique_and_findable() {
        for m in MODELS {
            assert_eq!(find_model(m.id).map(|f| f.id), Some(m.id));
        }
        let mut ids: Vec<_> = MODELS.iter().map(|m| m.id).collect();
        ids.sort_unstable();
        let before = ids.len();
        ids.dedup();
        assert_eq!(before, ids.len(), "duplicate model ids in the catalog");
    }

    #[test]
    fn default_model_exists_in_the_catalog() {
        assert!(find_model(DEFAULT_MODEL).is_some());
    }

    #[test]
    fn resolve_falls_back_instead_of_failing() {
        assert_eq!(resolve_model(Some("tiny")).id, "tiny");
        // Unknown or absent ids degrade to the default rather than breaking voice.
        assert_eq!(resolve_model(Some("does-not-exist")).id, DEFAULT_MODEL);
        assert_eq!(resolve_model(None).id, DEFAULT_MODEL);
    }

    #[test]
    fn urls_point_at_the_public_hugging_face_mirror() {
        let spec = resolve_model(Some("tiny.en"));
        let url = file_url(spec, spec.weights);
        assert_eq!(
            url,
            "https://huggingface.co/lmz/candle-whisper/resolve/main/model-tiny-en-q80.gguf"
        );
        // No credentials embedded — free means no account.
        assert!(!url.contains('@') && !url.contains("token"));
    }

    #[test]
    fn model_files_live_under_a_per_model_directory() {
        let data = PathBuf::from("/data");
        let english = resolve_model(Some("tiny.en"));
        let multilingual = resolve_model(Some("tiny"));
        assert!(model_dir(&data, english).ends_with("models/whisper/tiny.en"));
        // Switching models must not collide — note both use the same weights
        // *filename* pattern, so only the directory keeps them apart.
        assert_ne!(model_dir(&data, english), model_dir(&data, multilingual));
        assert_eq!(required_files(&data, english).len(), 3);
    }

    #[test]
    fn staging_path_is_a_sibling_partial_file() {
        let final_path = PathBuf::from("/data/models/whisper/tiny.en/model.gguf");
        let staged = staging_path(&final_path);
        assert_eq!(staged.file_name().unwrap(), "model.gguf.partial");
        // Same directory, so the move into place is a rename, never a copy.
        assert_eq!(staged.parent(), final_path.parent());
    }

    #[test]
    fn missing_files_are_not_reported_as_downloaded() {
        let dir = tempfile::tempdir().unwrap();
        let spec = resolve_model(Some("tiny.en"));
        assert!(!is_downloaded(dir.path(), spec));
    }

    #[test]
    fn all_three_present_and_non_empty_counts_as_downloaded() {
        let dir = tempfile::tempdir().unwrap();
        let spec = resolve_model(Some("tiny.en"));
        for (_, path) in required_files(dir.path(), spec) {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, b"x").unwrap();
        }
        assert!(is_downloaded(dir.path(), spec));
    }

    #[test]
    fn an_empty_file_does_not_count_as_downloaded() {
        let dir = tempfile::tempdir().unwrap();
        let spec = resolve_model(Some("tiny.en"));
        let files = required_files(dir.path(), spec);
        for (i, (_, path)) in files.iter().enumerate() {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            // Leave the last one truncated, as an interrupted download would.
            std::fs::write(
                path,
                if i == files.len() - 1 {
                    &b""[..]
                } else {
                    &b"x"[..]
                },
            )
            .unwrap();
        }
        assert!(!is_downloaded(dir.path(), spec));
    }

    #[test]
    fn cleanup_strips_control_tokens() {
        assert_eq!(
            clean_transcript("<|startoftranscript|><|en|><|transcribe|> hello there<|endoftext|>"),
            "hello there"
        );
        assert_eq!(
            clean_transcript("<|0.00|> open the notes <|2.50|>"),
            "open the notes"
        );
    }

    #[test]
    fn cleanup_drops_non_speech_annotations() {
        assert_eq!(clean_transcript("[BLANK_AUDIO]"), "");
        assert_eq!(
            clean_transcript("(music) remind me later"),
            "remind me later"
        );
        assert_eq!(
            clean_transcript("[ Silence ] what time is it"),
            "what time is it"
        );
    }

    #[test]
    fn cleanup_collapses_whitespace_and_trims() {
        assert_eq!(
            clean_transcript("  write   a  note \n\n now  "),
            "write a note now"
        );
        assert_eq!(clean_transcript(""), "");
    }

    #[test]
    fn cleanup_preserves_ordinary_punctuation() {
        assert_eq!(
            clean_transcript("<|en|> What's the weather? It's cold."),
            "What's the weather? It's cold."
        );
    }

    #[test]
    fn silence_filler_is_recognised_as_nothing_said() {
        for s in [
            "",
            "   ",
            "you",
            "Thank you.",
            "thanks for watching",
            "[BLANK_AUDIO]",
        ] {
            let cleaned = clean_transcript(s);
            assert!(
                is_probably_silence(&cleaned),
                "{s:?} -> {cleaned:?} should read as silence"
            );
        }
    }

    #[test]
    fn joining_windows_drops_a_hallucinated_tail() {
        // The exact regression: a trailing silence window hallucinates "thank you",
        // which would concatenate into "open the notes thank you" and slip past a
        // check applied only to the joined string.
        let pieces = vec!["open the notes".to_string(), "thank you".to_string()];
        assert_eq!(join_windows(&pieces), "open the notes");
    }

    #[test]
    fn joining_windows_keeps_every_real_window() {
        let pieces = vec![
            "remind me to call".to_string(),
            "the dentist tomorrow".to_string(),
        ];
        assert_eq!(
            join_windows(&pieces),
            "remind me to call the dentist tomorrow"
        );
    }

    #[test]
    fn joining_only_filler_windows_yields_nothing() {
        let pieces = vec!["thank you".to_string(), "you".to_string(), "".to_string()];
        assert_eq!(join_windows(&pieces), "");
        assert_eq!(join_windows(&[]), "");
    }

    #[test]
    fn joining_windows_cleans_control_tokens_and_whitespace() {
        let pieces = vec![
            "<|0.00|> save this  note".to_string(),
            "[BLANK_AUDIO]".to_string(),
        ];
        assert_eq!(join_windows(&pieces), "save this note");
    }

    #[test]
    fn real_speech_is_not_mistaken_for_silence() {
        for s in [
            "open the skill library",
            "what did I ask you yesterday",
            "thank you for saving that note",
        ] {
            assert!(!is_probably_silence(s), "{s:?} should be treated as speech");
        }
    }

    #[test]
    fn readiness_serializes_with_a_tagged_state() {
        let json = serde_json::to_string(&SttReadiness::NotCompiled).unwrap();
        assert_eq!(json, r#"{"state":"not_compiled"}"#);
        let json = serde_json::to_string(&SttReadiness::Ready {
            model: "tiny.en".into(),
        })
        .unwrap();
        assert!(json.contains(r#""state":"ready""#) && json.contains("tiny.en"));
    }
}
