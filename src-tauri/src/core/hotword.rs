//! Wake-phrase detection (§6.4, Voice v2).
//!
//! No second model. A dedicated wake-word engine (openWakeWord, Porcupine)
//! would be more power-efficient, but it means another download, another
//! licence to check, and another thing that can be missing — against a
//! free-forever local app whose whole promise is "clone and run". Instead:
//! the energy VAD already knows when someone is speaking, so only those
//! moments go to the Whisper model that is already loaded, and we look for the
//! phrase in the transcript.
//!
//! That makes tolerant matching the real engineering problem. Whisper renders
//! "hey jarvis" as "Hey, Jarvis!", "hey jarvis.", "hay jarvis", "hey jarvus" —
//! so an exact `contains` misses most real activations, while a too-loose match
//! wakes on any sentence containing "is".
//!
//! The line is drawn at one edit per word. That covers the mis-hearings that
//! actually occur (jarvus, jervis, harvis, jarvi — all one edit) and
//! deliberately does not cover "travis", which measures three edits away.
//! Allowing three edits on a six-letter word starts matching unrelated words,
//! and a wake phrase that fires on ordinary speech is worse than one that
//! occasionally needs repeating. Everything here is pure and tested against
//! those real shapes.

/// Default phrase. Two words on purpose: single-word wake terms fire on
/// ordinary speech far too often.
pub const DEFAULT_WAKE_PHRASE: &str = "hey jarvis";

/// Per-word edit distance allowed when matching. 1 catches the common
/// mis-hearings (jarvis/travis, hey/hay) without matching unrelated words.
const MAX_WORD_DISTANCE: usize = 1;

/// Words shorter than this must match exactly. At 3 letters one edit is still
/// discriminating, because *every* word in the phrase has to match: "hay
/// jarvis" should wake, "he is nice" should not. At 2 letters it isn't — one
/// edit there matches almost anything.
const FUZZY_MIN_LEN: usize = 3;

/// Lowercase, strip punctuation, collapse whitespace. The shape everything else
/// works on.
///
/// Apostrophes are *dropped* rather than turned into spaces, so contractions
/// collapse the way a listener hears them: "that's all" becomes "thats all",
/// not "that s all". Splitting them silently broke sleep-phrase matching.
pub fn normalize(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .filter(|c| !matches!(c, '\'' | '\u{2019}'))
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Levenshtein distance, capped early: we only ever care about "within 1".
fn distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    if a.is_empty() {
        return b.len();
    }
    if b.is_empty() {
        return a.len();
    }
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0usize; b.len() + 1];
    for (i, ca) in a.iter().enumerate() {
        cur[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let cost = if ca == cb { 0 } else { 1 };
            cur[j + 1] = (prev[j] + cost).min(prev[j + 1] + 1).min(cur[j] + 1);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

/// Do two words match, allowing for a mis-hearing?
fn word_matches(heard: &str, want: &str) -> bool {
    if heard == want {
        return true;
    }
    // Short words must be exact, or "is" matches "it", "in", "if"...
    if want.len() < FUZZY_MIN_LEN || heard.len() < FUZZY_MIN_LEN {
        return false;
    }
    // A big length gap is a different word, not a mis-hearing.
    if heard.len().abs_diff(want.len()) > MAX_WORD_DISTANCE {
        return false;
    }
    distance(heard, want) <= MAX_WORD_DISTANCE
}

/// Where the wake phrase was found, and what followed it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WakeMatch {
    /// Index of the first phrase word in the normalized transcript.
    pub start: usize,
    /// Index just past the last phrase word.
    pub end: usize,
    /// Anything spoken after the phrase, already normalized. Usually the actual
    /// request ("hey jarvis what time is it"), which is how hands-free avoids a
    /// second round trip.
    pub command: String,
}

/// Looks for the phrase anywhere in the transcript.
///
/// Anywhere, not just at the start: Whisper regularly prefixes filler ("Um, hey
/// Jarvis...") and requiring a prefix match loses those activations.
pub fn find_wake(transcript: &str, phrase: &str) -> Option<WakeMatch> {
    let heard: Vec<String> = normalize(transcript)
        .split_whitespace()
        .map(str::to_string)
        .collect();
    let want: Vec<String> = normalize(phrase)
        .split_whitespace()
        .map(str::to_string)
        .collect();
    if want.is_empty() || heard.len() < want.len() {
        return None;
    }
    for start in 0..=(heard.len() - want.len()) {
        let hit = want
            .iter()
            .enumerate()
            .all(|(i, w)| word_matches(&heard[start + i], w));
        if hit {
            let end = start + want.len();
            return Some(WakeMatch {
                start,
                end,
                command: heard[end..].join(" "),
            });
        }
    }
    None
}

/// Was the assistant addressed, ignoring where in the sentence?
pub fn is_wake(transcript: &str, phrase: &str) -> bool {
    find_wake(transcript, phrase).is_some()
}

/// Phrases that end a hands-free session by voice, so stopping never requires
/// reaching for the machine — the entire point of hands-free.
const SLEEP_PHRASES: &[&str] = &[
    "stop listening",
    "go to sleep",
    "never mind",
    "nevermind",
    "that is all",
    "thats all",
    "thank you jarvis",
];

/// True when the speaker asked to end the session.
pub fn is_sleep_request(transcript: &str) -> bool {
    let heard = normalize(transcript);
    if heard.is_empty() {
        return false;
    }
    SLEEP_PHRASES.iter().any(|p| heard.contains(p))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_strips_punctuation_case_and_spacing() {
        assert_eq!(normalize("  Hey,   JARVIS!  "), "hey jarvis");
        // Contractions collapse rather than leaving a stray "s".
        assert_eq!(normalize("Hey-Jarvis... what's up?"), "hey jarvis whats up");
        assert_eq!(
            normalize("that\u{2019}s all"),
            "thats all",
            "curly apostrophes too — Whisper emits them"
        );
        assert_eq!(normalize(""), "");
        assert_eq!(normalize("!!!"), "");
    }

    #[test]
    fn matches_the_phrase_however_whisper_punctuates_it() {
        // These are the shapes a real transcript takes; an exact `contains`
        // would miss every one of them.
        for said in [
            "hey jarvis",
            "Hey Jarvis",
            "Hey, Jarvis!",
            "hey jarvis.",
            "HEY JARVIS",
        ] {
            assert!(
                is_wake(said, DEFAULT_WAKE_PHRASE),
                "should wake on {said:?}"
            );
        }
    }

    #[test]
    fn tolerates_a_single_mis_heard_letter() {
        // Every one of these is exactly one edit from the phrase — measured,
        // not assumed.
        for said in [
            "hey jarvus",
            "hey jervis",
            "hey harvis",
            "hey jarvi",
            "hey jarviss",
            "hay jarvis",
        ] {
            assert!(
                is_wake(said, DEFAULT_WAKE_PHRASE),
                "one edit should still wake: {said:?}"
            );
        }
    }

    #[test]
    fn does_not_stretch_to_a_three_edit_mis_hearing() {
        // "travis" is 3 edits from "jarvis". Accepting that much drift would
        // start matching unrelated words, so this is a deliberate limit rather
        // than an oversight: the phrase occasionally needs repeating, and never
        // fires on ordinary speech.
        assert_eq!(distance("travis", "jarvis"), 3);
        assert!(!is_wake("hey travis", DEFAULT_WAKE_PHRASE));
    }

    #[test]
    fn does_not_wake_on_unrelated_speech() {
        for said in [
            "what time is it",
            "the car is fast",
            "hey there",
            "jarvis",     // one word alone is not the phrase
            "hey",        // nor the other
            "he is nice", // near-miss on short words
            "save this note",
        ] {
            assert!(
                !is_wake(said, DEFAULT_WAKE_PHRASE),
                "must not wake on {said:?}"
            );
        }
    }

    #[test]
    fn short_words_are_matched_exactly() {
        // With a 2-letter wake word, fuzzy matching would fire constantly.
        assert!(is_wake("ok jarvis", "ok jarvis"));
        assert!(!is_wake("oh jarvis", "ok jarvis"), "no fuzz on short words");
    }

    #[test]
    fn a_large_length_gap_is_a_different_word() {
        assert!(!is_wake("hey j", DEFAULT_WAKE_PHRASE));
        assert!(!is_wake("hey jarvissimo", DEFAULT_WAKE_PHRASE));
    }

    #[test]
    fn finds_the_phrase_mid_sentence_and_returns_the_command() {
        // Whisper prefixes filler constantly; requiring a prefix match would
        // throw away real activations.
        let m = find_wake("um, hey Jarvis, what time is it?", DEFAULT_WAKE_PHRASE).unwrap();
        assert_eq!(m.start, 1);
        assert_eq!(m.end, 3);
        assert_eq!(
            m.command, "what time is it",
            "the request after the phrase is the command"
        );
    }

    #[test]
    fn a_bare_wake_phrase_yields_an_empty_command() {
        let m = find_wake("hey jarvis", DEFAULT_WAKE_PHRASE).unwrap();
        assert_eq!(m.command, "", "nothing followed, so wait for the request");
    }

    #[test]
    fn a_custom_phrase_works_and_the_default_then_does_not() {
        assert!(is_wake("computer open the notes", "computer"));
        assert!(!is_wake("hey jarvis open the notes", "computer"));
        // An empty phrase must never match, or hands-free wakes on everything.
        assert!(!is_wake("anything at all", ""));
        assert!(!is_wake("anything at all", "   "));
    }

    #[test]
    fn sleep_requests_are_recognised_however_phrased() {
        for said in [
            "stop listening",
            "Stop listening.",
            "okay, go to sleep",
            "never mind",
            "that's all",
            "thank you Jarvis",
        ] {
            assert!(is_sleep_request(said), "should end the session: {said:?}");
        }
    }

    #[test]
    fn ordinary_speech_does_not_end_the_session() {
        for said in [
            "what time is it",
            "stop the timer",
            "listening to music",
            "",
        ] {
            assert!(!is_sleep_request(said), "must keep listening: {said:?}");
        }
    }

    #[test]
    fn distance_is_symmetric_and_zero_for_equal() {
        assert_eq!(distance("jarvis", "jarvis"), 0);
        assert_eq!(distance("jarvis", "travis"), distance("travis", "jarvis"));
        assert_eq!(distance("", "abc"), 3);
        assert_eq!(distance("abc", ""), 3);
    }
}
