//! Hands-free conversation (§6.4, Voice v2): the state machine that makes voice
//! work without ever touching the machine.
//!
//! Voice v1 was push-to-talk — one click, one take, one answer. v2 keeps a
//! session open: wake word, request, answer, and then a follow-up window where
//! you can just keep talking. The window is the important part. Requiring the
//! wake phrase before every single sentence is what makes most voice assistants
//! exhausting, and dropping straight back to always-listening is what makes
//! them creepy. A bounded follow-up window is the honest middle.
//!
//! Pure state and timing arithmetic — no audio, no model, no Tauri — so the
//! whole policy is testable without a microphone.

use serde::Serialize;

/// How long after an answer the assistant keeps listening for a follow-up
/// without needing the wake phrase again.
pub const FOLLOW_UP_WINDOW_MS: u32 = 8_000;

/// A take shorter than this after waking is almost always a stray noise, not a
/// request; it re-arms rather than sending an empty prompt to the model.
pub const MIN_REQUEST_MS: u32 = 350;

/// What the session is doing right now. Drives the UI, and the core's own
/// decision about whether audio should be captured at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    /// Hands-free is off. No capture.
    Off,
    /// Listening only for the wake phrase.
    Waiting,
    /// Woken; capturing the request.
    Listening,
    /// Request captured, model working.
    Thinking,
    /// Reading the answer out.
    Speaking,
    /// Answer delivered; a follow-up needs no wake phrase until the window ends.
    FollowUp,
}

impl Phase {
    /// Should the microphone be running in this phase? Speaking is excluded so
    /// the assistant doesn't transcribe its own voice — the single most common
    /// way a hands-free loop talks to itself forever.
    pub fn wants_audio(self) -> bool {
        matches!(self, Phase::Waiting | Phase::Listening | Phase::FollowUp)
    }

    /// Is the wake phrase required to be heard before a request counts?
    pub fn needs_wake(self) -> bool {
        matches!(self, Phase::Waiting)
    }

    /// Should the mic be open purely to *watch loudness*, without transcribing?
    ///
    /// Only while speaking, and deliberately distinct from `wants_audio`: this is
    /// what makes barge-in possible without reintroducing the failure
    /// `wants_audio` exists to prevent. Nothing captured here reaches a model, so
    /// the assistant still cannot hear itself into a loop (see `core::bargein`).
    pub fn wants_barge_monitor(self) -> bool {
        matches!(self, Phase::Speaking)
    }
}

/// What the caller should do next. Returned instead of performed, so the state
/// machine stays pure and the effects live in one obvious place.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum Action {
    /// Nothing to do.
    Idle,
    /// Send this text to the model.
    Ask { text: String },
    /// Say this to the user (a nudge, not a model answer).
    Say { text: String },
    /// The session ended; stop capture.
    Sleep,
}

/// One hands-free session.
#[derive(Debug, Clone)]
pub struct Session {
    phase: Phase,
    wake_phrase: String,
    /// Milliseconds spent in FollowUp, so the window can expire.
    follow_up_ms: u32,
}

impl Session {
    pub fn new(wake_phrase: impl Into<String>) -> Self {
        Self {
            phase: Phase::Off,
            wake_phrase: wake_phrase.into(),
            follow_up_ms: 0,
        }
    }

    pub fn phase(&self) -> Phase {
        self.phase
    }

    pub fn wake_phrase(&self) -> &str {
        &self.wake_phrase
    }

    /// Turns hands-free on: start listening for the wake phrase.
    pub fn start(&mut self) -> Action {
        self.phase = Phase::Waiting;
        self.follow_up_ms = 0;
        Action::Idle
    }

    /// Turns it off from anywhere.
    pub fn stop(&mut self) -> Action {
        self.phase = Phase::Off;
        self.follow_up_ms = 0;
        Action::Sleep
    }

    /// A completed take was transcribed. This is the heart of the machine.
    ///
    /// `duration_ms` guards against stray noise producing a hallucinated
    /// transcript that then gets sent to the model as if it were a request.
    pub fn heard(&mut self, transcript: &str, duration_ms: u32) -> Action {
        if self.phase == Phase::Off || self.phase == Phase::Speaking {
            return Action::Idle;
        }

        // Ending the session by voice works in any listening phase, and takes
        // priority over being interpreted as a request.
        if crate::core::hotword::is_sleep_request(transcript) {
            return self.stop();
        }

        let normalized = crate::core::hotword::normalize(transcript);
        if normalized.is_empty() || duration_ms < MIN_REQUEST_MS {
            // Nothing usable. Stay where we are rather than asking the model
            // to answer silence.
            return Action::Idle;
        }

        match self.phase {
            Phase::Waiting => {
                match crate::core::hotword::find_wake(transcript, &self.wake_phrase) {
                    Some(hit) if !hit.command.is_empty() => {
                        // "hey jarvis what time is it" — one breath, no round trip.
                        self.phase = Phase::Thinking;
                        Action::Ask { text: hit.command }
                    }
                    Some(_) => {
                        // Addressed with nothing after it: acknowledge and listen.
                        self.phase = Phase::Listening;
                        Action::Say {
                            text: "Listening.".into(),
                        }
                    }
                    // Not addressed. Silence is the correct response to speech that
                    // wasn't meant for us.
                    None => Action::Idle,
                }
            }
            // Already awake: the whole utterance is the request. A wake phrase
            // spoken again here is stripped rather than treated as content.
            Phase::Listening | Phase::FollowUp => {
                let text = match crate::core::hotword::find_wake(transcript, &self.wake_phrase) {
                    Some(hit) if !hit.command.is_empty() => hit.command,
                    Some(_) => return Action::Idle, // bare wake phrase, keep waiting
                    None => normalized,
                };
                self.phase = Phase::Thinking;
                self.follow_up_ms = 0;
                Action::Ask { text }
            }
            _ => Action::Idle,
        }
    }

    /// The model answered. Moves to speaking if the answer will be read aloud.
    pub fn answered(&mut self, will_speak: bool) -> Action {
        if self.phase == Phase::Off {
            return Action::Idle;
        }
        self.phase = if will_speak {
            Phase::Speaking
        } else {
            self.follow_up_ms = 0;
            Phase::FollowUp
        };
        Action::Idle
    }

    /// Speech finished; open the follow-up window.
    pub fn finished_speaking(&mut self) -> Action {
        if self.phase == Phase::Off {
            return Action::Idle;
        }
        self.phase = Phase::FollowUp;
        self.follow_up_ms = 0;
        Action::Idle
    }

    /// The user talked over the answer (Voice v3). Playback stops and the session
    /// goes straight to capturing, because someone who interrupts is already
    /// mid-sentence — making them wait for a prompt or say the wake phrase again
    /// would waste the words they just spoke.
    ///
    /// Only meaningful while speaking. Anywhere else it is a no-op rather than an
    /// error: the detector runs on a separate thread and can report a moment
    /// after playback ended on its own.
    pub fn interrupted(&mut self) -> Action {
        if self.phase != Phase::Speaking {
            return Action::Idle;
        }
        self.phase = Phase::Listening;
        self.follow_up_ms = 0;
        Action::Idle
    }

    /// The model call failed. Return to listening rather than stranding the
    /// session in Thinking forever.
    pub fn failed(&mut self) -> Action {
        if self.phase == Phase::Off {
            return Action::Idle;
        }
        self.phase = Phase::FollowUp;
        self.follow_up_ms = 0;
        Action::Idle
    }

    /// Advances time. Only the follow-up window expires; every other phase is
    /// driven by events, so a long think or a long answer never times out.
    pub fn tick(&mut self, elapsed_ms: u32) -> Action {
        if self.phase != Phase::FollowUp {
            return Action::Idle;
        }
        self.follow_up_ms = self.follow_up_ms.saturating_add(elapsed_ms);
        if self.follow_up_ms >= FOLLOW_UP_WINDOW_MS {
            // Back to needing the wake phrase — not off. Hands-free stays armed.
            self.phase = Phase::Waiting;
            self.follow_up_ms = 0;
        }
        Action::Idle
    }

    /// Remaining follow-up time, for a UI countdown.
    pub fn follow_up_remaining_ms(&self) -> u32 {
        if self.phase != Phase::FollowUp {
            return 0;
        }
        FOLLOW_UP_WINDOW_MS.saturating_sub(self.follow_up_ms)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WAKE: &str = "hey jarvis";
    const LONG: u32 = 1_200; // a take long enough to be a real request

    fn awake() -> Session {
        let mut s = Session::new(WAKE);
        s.start();
        s
    }

    #[test]
    fn starts_off_and_captures_nothing() {
        let s = Session::new(WAKE);
        assert_eq!(s.phase(), Phase::Off);
        assert!(!s.phase().wants_audio());
    }

    #[test]
    fn waiting_listens_but_ignores_speech_not_addressed_to_it() {
        let mut s = awake();
        assert_eq!(s.phase(), Phase::Waiting);
        assert!(s.phase().wants_audio());
        assert!(s.phase().needs_wake());
        assert_eq!(s.heard("what a nice day", LONG), Action::Idle);
        assert_eq!(s.phase(), Phase::Waiting, "still waiting to be addressed");
    }

    #[test]
    fn wake_plus_request_in_one_breath_asks_immediately() {
        // The behaviour that makes hands-free feel fast: no acknowledge round trip.
        let mut s = awake();
        assert_eq!(
            s.heard("hey jarvis what time is it", LONG),
            Action::Ask {
                text: "what time is it".into()
            }
        );
        assert_eq!(s.phase(), Phase::Thinking);
    }

    #[test]
    fn a_bare_wake_phrase_acknowledges_and_then_takes_the_request() {
        let mut s = awake();
        assert_eq!(
            s.heard("hey jarvis", LONG),
            Action::Say {
                text: "Listening.".into()
            }
        );
        assert_eq!(s.phase(), Phase::Listening);
        // Now the next utterance is the request, no wake phrase needed.
        assert!(!s.phase().needs_wake());
        assert_eq!(
            s.heard("save a note about the telescope", LONG),
            Action::Ask {
                text: "save a note about the telescope".into()
            }
        );
    }

    #[test]
    fn the_follow_up_window_needs_no_wake_phrase() {
        let mut s = awake();
        s.heard("hey jarvis what time is it", LONG);
        s.answered(true);
        assert_eq!(s.phase(), Phase::Speaking);
        s.finished_speaking();
        assert_eq!(s.phase(), Phase::FollowUp);
        assert!(!s.phase().needs_wake(), "that's the point of the window");
        assert_eq!(
            s.heard("and what about tomorrow", LONG),
            Action::Ask {
                text: "and what about tomorrow".into()
            }
        );
    }

    #[test]
    fn the_follow_up_window_expires_back_to_needing_the_wake_phrase() {
        let mut s = awake();
        s.heard("hey jarvis hello", LONG);
        s.answered(false);
        assert_eq!(s.phase(), Phase::FollowUp);
        assert_eq!(s.follow_up_remaining_ms(), FOLLOW_UP_WINDOW_MS);

        s.tick(FOLLOW_UP_WINDOW_MS / 2);
        assert_eq!(s.phase(), Phase::FollowUp, "still inside the window");
        assert_eq!(s.follow_up_remaining_ms(), FOLLOW_UP_WINDOW_MS / 2);

        s.tick(FOLLOW_UP_WINDOW_MS / 2);
        assert_eq!(
            s.phase(),
            Phase::Waiting,
            "expires to armed-and-waiting, not off"
        );
        assert!(s.phase().wants_audio(), "hands-free is still on");
        assert_eq!(s.follow_up_remaining_ms(), 0);
    }

    #[test]
    fn the_mic_is_off_while_the_assistant_is_speaking() {
        // Otherwise it transcribes its own voice and talks to itself forever.
        let mut s = awake();
        s.heard("hey jarvis hello", LONG);
        s.answered(true);
        assert_eq!(s.phase(), Phase::Speaking);
        assert!(!s.phase().wants_audio());
        // And anything "heard" in that phase is ignored outright.
        assert_eq!(s.heard("hey jarvis stop", LONG), Action::Idle);
        assert_eq!(s.phase(), Phase::Speaking);
    }

    #[test]
    fn stray_noise_does_not_become_a_request() {
        let mut s = awake();
        s.heard("hey jarvis", LONG);
        assert_eq!(s.phase(), Phase::Listening);
        // A very short take, or an empty transcript, must not reach the model.
        assert_eq!(s.heard("thank you", 80), Action::Idle);
        assert_eq!(s.heard("", LONG), Action::Idle);
        assert_eq!(s.heard("   ...   ", LONG), Action::Idle);
        assert_eq!(
            s.phase(),
            Phase::Listening,
            "still waiting for a real request"
        );
    }

    #[test]
    fn a_sleep_request_ends_the_session_from_any_listening_phase() {
        for setup in [Phase::Waiting, Phase::Listening, Phase::FollowUp] {
            let mut s = awake();
            match setup {
                Phase::Listening => {
                    s.heard("hey jarvis", LONG);
                }
                Phase::FollowUp => {
                    s.heard("hey jarvis hi", LONG);
                    s.answered(false);
                }
                _ => {}
            }
            assert_eq!(s.phase(), setup);
            assert_eq!(s.heard("stop listening", LONG), Action::Sleep);
            assert_eq!(s.phase(), Phase::Off);
            assert!(!s.phase().wants_audio());
        }
    }

    #[test]
    fn a_sleep_request_beats_being_read_as_a_request() {
        let mut s = awake();
        s.heard("hey jarvis hi", LONG);
        s.answered(false);
        // In FollowUp this would otherwise be sent to the model as a question.
        assert_eq!(s.heard("never mind", LONG), Action::Sleep);
    }

    #[test]
    fn a_failed_model_call_does_not_strand_the_session() {
        let mut s = awake();
        s.heard("hey jarvis hello", LONG);
        assert_eq!(s.phase(), Phase::Thinking);
        s.failed();
        assert_eq!(s.phase(), Phase::FollowUp, "recovers instead of hanging");
        assert!(s.phase().wants_audio());
    }

    #[test]
    fn repeating_the_wake_phrase_while_awake_is_stripped_not_asked() {
        let mut s = awake();
        s.heard("hey jarvis", LONG);
        // People do this. The phrase must not end up in the prompt.
        assert_eq!(
            s.heard("hey jarvis what is the time", LONG),
            Action::Ask {
                text: "what is the time".into()
            }
        );
    }

    #[test]
    fn stopping_works_from_every_phase_and_is_idempotent() {
        let mut s = awake();
        assert_eq!(s.stop(), Action::Sleep);
        assert_eq!(s.phase(), Phase::Off);
        assert_eq!(s.stop(), Action::Sleep, "stopping twice is harmless");
        // Nothing is processed while off.
        assert_eq!(s.heard("hey jarvis hello", LONG), Action::Idle);
        assert_eq!(s.tick(100_000), Action::Idle);
    }

    #[test]
    fn a_custom_wake_phrase_is_respected() {
        let mut s = Session::new("computer");
        s.start();
        assert_eq!(
            s.heard("computer open the notes", LONG),
            Action::Ask {
                text: "open the notes".into()
            }
        );
        assert_eq!(s.wake_phrase(), "computer");
    }

    #[test]
    fn phases_serialize_in_snake_case_for_the_ui() {
        assert_eq!(
            serde_json::to_string(&Phase::FollowUp).unwrap(),
            "\"follow_up\""
        );
        let json = serde_json::to_string(&Action::Ask { text: "hi".into() }).unwrap();
        assert!(json.contains("\"action\":\"ask\""), "got {json}");
    }
    // --- barge-in (Voice v3) ---

    #[test]
    fn the_mic_watches_loudness_while_speaking_but_never_transcribes() {
        // The two must stay separate. If `wants_audio` ever includes Speaking,
        // the assistant transcribes itself and answers its own voice forever.
        assert!(
            !Phase::Speaking.wants_audio(),
            "never capture for transcription"
        );
        assert!(
            Phase::Speaking.wants_barge_monitor(),
            "but do watch loudness"
        );
        for phase in [
            Phase::Off,
            Phase::Waiting,
            Phase::Listening,
            Phase::Thinking,
            Phase::FollowUp,
        ] {
            assert!(
                !phase.wants_barge_monitor(),
                "{phase:?} has no playback to interrupt"
            );
        }
    }

    #[test]
    fn interrupting_goes_straight_to_capturing() {
        // Someone who talks over the answer is already mid-sentence. Sending them
        // back to Waiting would throw away the words they just said.
        let mut s = Session::new(WAKE);
        s.start();
        s.heard("hey jarvis what time is it", LONG);
        s.answered(true);
        assert_eq!(s.phase(), Phase::Speaking);
        assert_eq!(s.interrupted(), Action::Idle);
        assert_eq!(s.phase(), Phase::Listening);
        assert!(!s.phase().needs_wake(), "no wake phrase needed to continue");
        assert!(s.phase().wants_audio(), "and the mic is capturing again");
    }

    #[test]
    fn a_request_spoken_over_the_answer_reaches_the_model() {
        // End to end: interrupt, then the take that follows is a real request.
        let mut s = Session::new(WAKE);
        s.start();
        s.heard("hey jarvis", LONG);
        s.heard("what time is it", LONG);
        s.answered(true);
        s.interrupted();
        assert_eq!(
            s.heard("no, tomorrow", LONG),
            Action::Ask {
                text: "no tomorrow".into()
            }
        );
    }

    #[test]
    fn a_late_interrupt_after_playback_ended_is_harmless() {
        // The detector runs on its own thread and can report just after speech
        // finished. That must not drag a settled session backwards.
        let mut s = Session::new(WAKE);
        s.start();
        s.heard("hey jarvis what time is it", LONG);
        s.answered(true);
        s.finished_speaking();
        assert_eq!(s.phase(), Phase::FollowUp);
        assert_eq!(s.interrupted(), Action::Idle);
        assert_eq!(s.phase(), Phase::FollowUp, "unchanged");

        // And it cannot wake a session that is off.
        let mut off = Session::new(WAKE);
        assert_eq!(off.interrupted(), Action::Idle);
        assert_eq!(off.phase(), Phase::Off);
    }

    #[test]
    fn interrupting_resets_the_follow_up_clock() {
        // Otherwise time spent speaking would eat into the window the user gets
        // after the interruption.
        let mut s = Session::new(WAKE);
        s.start();
        s.heard("hey jarvis hello", LONG);
        s.answered(true);
        s.interrupted();
        s.answered(false);
        assert_eq!(s.follow_up_remaining_ms(), FOLLOW_UP_WINDOW_MS);
    }
}
