//! Talking over the assistant (§6.4, Voice v3).
//!
//! Voice v2 closed the microphone while the assistant spoke. That was the right
//! default — an open mic during playback is the single most common way a
//! hands-free loop starts transcribing its own voice and answering itself
//! forever — but it also means you cannot interrupt. Waiting out a wrong answer
//! is exactly the moment you most want to cut in.
//!
//! ## The problem this has to solve honestly
//!
//! Without acoustic echo cancellation, a microphone next to a speaker hears the
//! assistant at least as loudly as it hears the user. Any fixed threshold either
//! triggers on the assistant's own voice (the loop talks to itself) or is set so
//! high that only shouting works.
//!
//! So the detector **calibrates against the echo itself**. For the first stretch
//! of playback it does nothing but measure, building a picture of how loud this
//! room, this speaker, and this volume make the assistant sound in this
//! microphone. After that, only sound clearly above that measured floor, held for
//! long enough to not be a cough or a door, counts as someone talking.
//!
//! This is a real limit, not a solved problem: on a laptop at high volume the
//! echo floor can be high enough that normal speech won't clear it. That is why
//! barge-in is a setting rather than an assumption, and why the calibration is
//! reported so the UI can be honest about it.
//!
//! ## What it deliberately does not do
//!
//! It never transcribes. Detection is energy only — "someone is talking" — and
//! the transcript comes from the normal capture path afterwards. Nothing the
//! assistant says can become a request, because during playback no audio is ever
//! handed to a model.
//!
//! Pure arithmetic over frame loudness: no audio device, no model, no Tauri.

use serde::Serialize;

/// Ignored at the very start of playback: the speaker is ramping up and the OS
/// mixer is settling, so early frames are not representative of anything.
pub const GRACE_MS: u32 = 200;

/// How long to spend measuring the echo before the detector will fire. Long
/// enough to hear actual speech from the assistant rather than a leading pause.
pub const CALIBRATE_MS: u32 = 500;

/// How far above the measured echo floor a sound has to be to count.
///
/// A ratio, not an absolute level, because the whole point is to adapt to the
/// room. Chosen so that speaking normally over a comfortably audible assistant
/// clears it, while the assistant's own peaks — which vary maybe 2x around their
/// average — do not.
pub const MARGIN: f32 = 3.0;

/// How long the sound has to persist. A cough, a chair, or a door is loud and
/// brief; a person starting a sentence is loud and keeps going.
pub const SUSTAIN_MS: u32 = 280;

/// A floor beneath which nothing counts however quiet the echo was. Without it, a
/// silent playback (an empty utterance, a muted speaker) calibrates to near zero
/// and then every rustle is a 3x interruption.
pub const ABSOLUTE_FLOOR: f32 = 0.02;

/// What the detector thinks after the latest frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case", tag = "state")]
pub enum Barge {
    /// Still measuring the echo; will not fire yet.
    Calibrating,
    /// Listening, nothing above the floor.
    Quiet,
    /// Something loud, but not yet held long enough to be a person.
    Maybe,
    /// Someone is talking over the assistant. Stop playback.
    Interrupt,
}

/// Watches frame loudness during playback and decides when to yield.
#[derive(Debug, Clone)]
pub struct Detector {
    elapsed_ms: u32,
    /// Sum and count of frames seen while calibrating, for a mean.
    echo_sum: f32,
    echo_frames: u32,
    /// Loudest frame seen while calibrating. The mean alone underestimates the
    /// echo, because speech is mostly gaps.
    echo_peak: f32,
    loud_ms: u32,
    fired: bool,
}

impl Default for Detector {
    fn default() -> Self {
        Self::new()
    }
}

impl Detector {
    pub fn new() -> Self {
        Self {
            elapsed_ms: 0,
            echo_sum: 0.0,
            echo_frames: 0,
            echo_peak: 0.0,
            loud_ms: 0,
            fired: false,
        }
    }

    /// The level a sound must exceed, once calibration is done.
    ///
    /// Blends the mean and the peak of the measured echo. The mean alone is far
    /// too low, because speech is mostly silence between words and calibrating on
    /// that would make every syllable of the assistant's own voice look like an
    /// interruption. The peak alone is too high, because one loud consonant would
    /// set the bar for the whole utterance.
    pub fn threshold(&self) -> Option<f32> {
        if self.echo_frames == 0 {
            return None;
        }
        let mean = self.echo_sum / self.echo_frames as f32;
        let reference = (mean + self.echo_peak) / 2.0;
        Some((reference * MARGIN).max(ABSOLUTE_FLOOR))
    }

    /// What the detector measured the assistant's own loudness to be. Exposed so
    /// the UI can say why barge-in isn't working, rather than just failing.
    pub fn echo_level(&self) -> Option<f32> {
        (self.echo_frames > 0).then(|| self.echo_sum / self.echo_frames as f32)
    }

    /// Feeds one frame of captured audio loudness.
    ///
    /// Fires at most once per playback: after `Interrupt` the caller is expected
    /// to stop speaking, and a detector that kept firing would produce a stream
    /// of duplicate interruptions from the tail of the same sentence.
    pub fn frame(&mut self, rms: f32, frame_ms: u32) -> Barge {
        if self.fired {
            return Barge::Interrupt;
        }
        self.elapsed_ms = self.elapsed_ms.saturating_add(frame_ms);

        // The speaker is still ramping; measuring here would skew the floor low.
        if self.elapsed_ms <= GRACE_MS {
            return Barge::Calibrating;
        }

        if self.elapsed_ms <= GRACE_MS + CALIBRATE_MS {
            self.echo_sum += rms;
            self.echo_frames += 1;
            self.echo_peak = self.echo_peak.max(rms);
            return Barge::Calibrating;
        }

        let Some(threshold) = self.threshold() else {
            // No frames were measured (frames longer than the whole calibration
            // window). Refusing to fire is the safe answer: a false interruption
            // caused by the assistant's own voice is worse than no barge-in.
            return Barge::Quiet;
        };

        if rms < threshold {
            // One quiet frame ends a candidate. Someone genuinely talking does not
            // drop below the assistant's own level mid-word.
            self.loud_ms = 0;
            return Barge::Quiet;
        }

        self.loud_ms = self.loud_ms.saturating_add(frame_ms);
        if self.loud_ms >= SUSTAIN_MS {
            self.fired = true;
            Barge::Interrupt
        } else {
            Barge::Maybe
        }
    }

    /// Whether this detector has already yielded to the user.
    pub fn fired(&self) -> bool {
        self.fired
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FRAME: u32 = 20; // ms per frame, matching a typical capture buffer

    /// Runs `count` frames at one level and returns the last verdict.
    fn feed(d: &mut Detector, rms: f32, ms: u32) -> Barge {
        let mut last = Barge::Quiet;
        let mut left = ms;
        while left > 0 {
            last = d.frame(rms, FRAME);
            left = left.saturating_sub(FRAME);
        }
        last
    }

    /// Plays the assistant through the calibration window at `echo`.
    fn calibrated(echo: f32) -> Detector {
        let mut d = Detector::new();
        feed(&mut d, echo, GRACE_MS + CALIBRATE_MS + FRAME);
        d
    }

    #[test]
    fn the_assistants_own_voice_never_counts_as_an_interruption() {
        // The failure that matters. If the echo alone fires the detector, the
        // assistant interrupts itself and hands-free becomes unusable.
        let mut d = calibrated(0.12);
        // Keep playing at the same level, with the natural variation of speech.
        for level in [0.12, 0.05, 0.18, 0.02, 0.15, 0.2, 0.09, 0.13] {
            let verdict = feed(&mut d, level, 400);
            assert_ne!(
                verdict,
                Barge::Interrupt,
                "fired on its own echo at {level}"
            );
        }
        assert!(!d.fired());
    }

    #[test]
    fn someone_talking_over_it_is_caught() {
        let mut d = calibrated(0.10);
        let threshold = d.threshold().unwrap();
        let verdict = feed(&mut d, threshold * 1.4, SUSTAIN_MS + FRAME);
        assert_eq!(verdict, Barge::Interrupt);
    }

    #[test]
    fn a_brief_noise_is_not_an_interruption() {
        // A cough or a door is loud and short. Yielding to it would stop the
        // answer for nothing, which is worse than not having barge-in.
        let mut d = calibrated(0.10);
        let loud = d.threshold().unwrap() * 2.0;
        assert_eq!(feed(&mut d, loud, SUSTAIN_MS - FRAME * 2), Barge::Maybe);
        assert_eq!(feed(&mut d, 0.01, 100), Barge::Quiet, "and it resets");
        assert!(!d.fired());
    }

    #[test]
    fn a_gap_resets_the_sustain_counter() {
        // Otherwise two unrelated thumps a second apart would add up to a person.
        let mut d = calibrated(0.10);
        let loud = d.threshold().unwrap() * 2.0;
        feed(&mut d, loud, SUSTAIN_MS - FRAME * 2);
        feed(&mut d, 0.001, FRAME);
        assert_eq!(
            feed(&mut d, loud, SUSTAIN_MS - FRAME * 2),
            Barge::Maybe,
            "the earlier loud stretch must not still be counting"
        );
    }

    #[test]
    fn nothing_fires_during_the_grace_and_calibration_windows() {
        // Shouting at the very start would otherwise cut off the first word, and
        // the echo floor would be measured from the shout.
        let mut d = Detector::new();
        assert_eq!(feed(&mut d, 5.0, GRACE_MS), Barge::Calibrating);
        assert_eq!(feed(&mut d, 5.0, CALIBRATE_MS), Barge::Calibrating);
        assert!(!d.fired());
    }

    #[test]
    fn a_loud_room_raises_the_bar_instead_of_breaking() {
        // The adaptive part: on speakers the echo is loud, so the threshold has
        // to rise with it. A fixed threshold is what makes naive barge-in fire
        // constantly on a laptop.
        let quiet = calibrated(0.05).threshold().unwrap();
        let loud = calibrated(0.30).threshold().unwrap();
        assert!(loud > quiet * 3.0, "quiet {quiet}, loud {loud}");
    }

    #[test]
    fn silence_does_not_calibrate_the_bar_to_nothing() {
        // A muted speaker or an empty utterance measures ~0 echo. Scaling that by
        // the margin gives ~0, and then every rustle is an interruption.
        let d = calibrated(0.0);
        assert_eq!(d.threshold(), Some(ABSOLUTE_FLOOR));
        let mut d = calibrated(0.0);
        assert_ne!(
            feed(&mut d, ABSOLUTE_FLOOR * 0.5, 1_000),
            Barge::Interrupt,
            "quiet room noise must not fire"
        );
    }

    #[test]
    fn it_fires_at_most_once_per_answer() {
        // The tail of the same sentence would otherwise produce a stream of
        // duplicate interruptions.
        let mut d = calibrated(0.10);
        let loud = d.threshold().unwrap() * 2.0;
        assert_eq!(feed(&mut d, loud, SUSTAIN_MS + FRAME), Barge::Interrupt);
        assert_eq!(feed(&mut d, 0.0, 1_000), Barge::Interrupt, "stays fired");
        assert!(d.fired());
    }

    #[test]
    fn frames_longer_than_the_calibration_window_refuse_rather_than_guess() {
        // A pathological buffer size skips calibration entirely. Firing on an
        // unmeasured room would mean interrupting on the assistant's own voice.
        let mut d = Detector::new();
        // One frame overshoots grace *and* calibration, so nothing is ever
        // measured and the detector reports Quiet forever rather than guessing.
        assert_eq!(
            d.frame(0.1, 10_000),
            Barge::Quiet,
            "no measurement, no fire"
        );
        assert_eq!(d.frame(9.0, 10_000), Barge::Quiet, "still refuses");
        assert!(!d.fired());
        assert_eq!(d.threshold(), None);
    }

    #[test]
    fn the_measured_echo_is_reported_for_the_ui() {
        // Barge-in that silently doesn't work is worse than none; the UI needs to
        // be able to say the room is too loud.
        assert_eq!(Detector::new().echo_level(), None);
        let level = calibrated(0.2).echo_level().unwrap();
        assert!((level - 0.2).abs() < 0.001, "measured {level}");
    }
}
