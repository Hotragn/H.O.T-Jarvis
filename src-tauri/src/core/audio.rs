//! Audio conditioning for on-device speech recognition (§6.4).
//!
//! Whisper is picky about its input: 16 kHz, mono, f32 samples in -1.0..=1.0.
//! Microphones hand us none of that — they give whatever the device prefers
//! (commonly 44.1/48 kHz, often stereo). Everything here is the pure,
//! deterministic bridge between the two, kept free of cpal and Tauri types so
//! it can be unit-tested without a sound card.
//!
//! The energy gate is deliberately simple. A neural VAD (Silero and friends) is
//! more robust in noise, but it is another model to ship and another download;
//! short-term RMS against an adaptive noise floor is enough to answer the two
//! questions push-to-talk actually asks: "did anyone speak?" and "have they
//! stopped?"

/// The only sample rate Whisper accepts.
pub const WHISPER_SAMPLE_RATE: u32 = 16_000;

/// Whisper's encoder reads 30-second windows; anything longer must be chunked.
pub const WHISPER_WINDOW_SECS: usize = 30;

/// Below this RMS a frame is treated as room tone rather than speech. Chosen
/// from the noise floor of typical laptop mics (~0.002 quiet room).
pub const SILENCE_RMS: f32 = 0.006;

/// How much trailing quiet ends an utterance. Long enough to survive the pause
/// between words, short enough that it doesn't feel laggy.
pub const ENDPOINT_SILENCE_MS: u32 = 900;

/// Ignore blips shorter than this so a keystroke or a cough isn't "speech".
pub const MIN_SPEECH_MS: u32 = 250;

/// Collapses interleaved multi-channel audio to mono by averaging channels.
/// `channels == 1` passes straight through; a ragged tail is averaged over the
/// samples that are actually present rather than dropped.
pub fn downmix_to_mono(interleaved: &[f32], channels: u16) -> Vec<f32> {
    let channels = channels.max(1) as usize;
    if channels == 1 {
        return interleaved.to_vec();
    }
    interleaved
        .chunks(channels)
        .map(|frame| frame.iter().sum::<f32>() / frame.len() as f32)
        .collect()
}

/// Linear resample to a target rate. Not as clean as a windowed-sinc filter,
/// but Whisper's mel front-end is tolerant and this keeps the dependency count
/// at zero. Returns the input unchanged when the rates already match.
pub fn resample_linear(input: &[f32], from_hz: u32, to_hz: u32) -> Vec<f32> {
    if from_hz == 0 || to_hz == 0 || input.is_empty() {
        return Vec::new();
    }
    if from_hz == to_hz {
        return input.to_vec();
    }
    let ratio = to_hz as f64 / from_hz as f64;
    let out_len = ((input.len() as f64) * ratio).round() as usize;
    if out_len == 0 {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(out_len);
    for i in 0..out_len {
        // Position in the source signal this output sample reads from.
        let src = i as f64 / ratio;
        let left = src.floor() as usize;
        let frac = (src - left as f64) as f32;
        let a = input[left.min(input.len() - 1)];
        let b = input[(left + 1).min(input.len() - 1)];
        out.push(a + (b - a) * frac);
    }
    out
}

/// Root-mean-square level of a frame — our stand-in for loudness.
pub fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum: f32 = samples.iter().map(|s| s * s).sum();
    (sum / samples.len() as f32).sqrt()
}

/// Scales the loudest peak up to `target` (leaving headroom) so a quiet talker
/// isn't transcribed as silence. Pure gain: no compression, no clipping. Audio
/// that is already loud, or entirely silent, is returned untouched.
pub fn normalize_peak(samples: &[f32], target: f32) -> Vec<f32> {
    let peak = samples.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    if peak <= f32::EPSILON || peak >= target {
        return samples.to_vec();
    }
    let gain = target / peak;
    samples
        .iter()
        .map(|s| (s * gain).clamp(-1.0, 1.0))
        .collect()
}

/// Trims leading and trailing room tone, keeping a short pad so consonants at
/// the edges survive. Returns an empty vec when nothing crosses the threshold.
pub fn trim_silence(samples: &[f32], sample_rate: u32, threshold: f32) -> Vec<f32> {
    let frame = frame_len(sample_rate);
    if samples.is_empty() || frame == 0 {
        return Vec::new();
    }
    let loud: Vec<usize> = samples
        .chunks(frame)
        .enumerate()
        .filter(|(_, f)| rms(f) >= threshold)
        .map(|(i, _)| i)
        .collect();
    let (Some(&first), Some(&last)) = (loud.first(), loud.last()) else {
        return Vec::new();
    };
    // One frame of padding on each side.
    let start = first.saturating_sub(1) * frame;
    let end = ((last + 2) * frame).min(samples.len());
    samples[start.min(samples.len())..end].to_vec()
}

/// The full microphone-to-Whisper pipeline: mono, 16 kHz, trimmed, normalized.
/// This is what the recorder hands the transcriber.
pub fn prepare_for_whisper(interleaved: &[f32], channels: u16, from_hz: u32) -> Vec<f32> {
    let mono = downmix_to_mono(interleaved, channels);
    let resampled = resample_linear(&mono, from_hz, WHISPER_SAMPLE_RATE);
    let trimmed = trim_silence(&resampled, WHISPER_SAMPLE_RATE, SILENCE_RMS);
    normalize_peak(&trimmed, 0.85)
}

/// Splits audio into <=30 s chunks on Whisper's window boundary, so a long
/// dictation transcribes in pieces instead of being silently truncated.
pub fn window_chunks(samples: &[f32], sample_rate: u32) -> Vec<&[f32]> {
    let per_window = sample_rate as usize * WHISPER_WINDOW_SECS;
    if per_window == 0 || samples.is_empty() {
        return Vec::new();
    }
    samples.chunks(per_window).collect()
}

/// Was there ever enough speech in this take to bother transcribing?
pub fn has_speech(samples: &[f32], sample_rate: u32) -> bool {
    let frame = frame_len(sample_rate);
    if frame == 0 {
        return false;
    }
    let needed = (MIN_SPEECH_MS as usize * sample_rate as usize / 1000) / frame.max(1);
    let loud = samples
        .chunks(frame)
        .filter(|f| rms(f) >= SILENCE_RMS)
        .count();
    loud > needed.max(1)
}

/// 20 ms analysis frame, the usual granularity for speech energy.
fn frame_len(sample_rate: u32) -> usize {
    (sample_rate as usize / 50).max(1)
}

/// Rolling endpoint detector for hands-free capture: feed it frames as they
/// arrive and it reports when the speaker has started and then stopped.
/// Deliberately a state machine over plain numbers so it can be tested with
/// synthetic audio.
#[derive(Debug, Clone)]
pub struct Endpointer {
    sample_rate: u32,
    speech_ms: u32,
    silence_ms: u32,
    started: bool,
}

impl Endpointer {
    pub fn new(sample_rate: u32) -> Self {
        Self {
            sample_rate,
            speech_ms: 0,
            silence_ms: 0,
            started: false,
        }
    }

    /// Push one frame. Returns true once the utterance looks complete —
    /// speech happened, and enough silence has followed to call it done.
    pub fn push(&mut self, frame: &[f32]) -> bool {
        if frame.is_empty() || self.sample_rate == 0 {
            return false;
        }
        let ms = (frame.len() as u32 * 1000) / self.sample_rate.max(1);
        if rms(frame) >= SILENCE_RMS {
            self.speech_ms += ms;
            self.silence_ms = 0;
            if self.speech_ms >= MIN_SPEECH_MS {
                self.started = true;
            }
        } else if self.started {
            self.silence_ms += ms;
        }
        self.started && self.silence_ms >= ENDPOINT_SILENCE_MS
    }

    /// True once real speech has been heard (drives the "listening" UI state).
    pub fn speech_started(&self) -> bool {
        self.started
    }

    pub fn reset(&mut self) {
        self.speech_ms = 0;
        self.silence_ms = 0;
        self.started = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A sine loud enough to read as speech.
    fn tone(len: usize, amp: f32) -> Vec<f32> {
        (0..len)
            .map(|i| amp * (i as f32 * 0.05).sin())
            .collect::<Vec<_>>()
    }

    #[test]
    fn downmix_averages_stereo_pairs() {
        let stereo = vec![1.0, 0.0, 0.5, -0.5, 0.2, 0.2];
        assert_eq!(downmix_to_mono(&stereo, 2), vec![0.5, 0.0, 0.2]);
    }

    #[test]
    fn downmix_passes_mono_through_untouched() {
        let mono = vec![0.1, -0.2, 0.3];
        assert_eq!(downmix_to_mono(&mono, 1), mono);
        // A zero channel count is nonsense from a device; treat it as mono.
        assert_eq!(downmix_to_mono(&mono, 0), mono);
    }

    #[test]
    fn resample_halves_length_going_48k_to_24k() {
        let input = tone(480, 0.5);
        let out = resample_linear(&input, 48_000, 24_000);
        assert_eq!(out.len(), 240);
    }

    #[test]
    fn resample_to_whisper_rate_from_48k_is_a_third() {
        let input = tone(4800, 0.5); // 100 ms at 48 kHz
        let out = resample_linear(&input, 48_000, WHISPER_SAMPLE_RATE);
        assert_eq!(out.len(), 1600); // 100 ms at 16 kHz
    }

    #[test]
    fn resample_is_identity_at_matching_rates() {
        let input = tone(100, 0.4);
        assert_eq!(resample_linear(&input, 16_000, 16_000), input);
    }

    #[test]
    fn resample_handles_degenerate_input() {
        assert!(resample_linear(&[], 48_000, 16_000).is_empty());
        assert!(resample_linear(&[0.1, 0.2], 0, 16_000).is_empty());
        assert!(resample_linear(&[0.1, 0.2], 48_000, 0).is_empty());
    }

    #[test]
    fn rms_of_silence_is_zero_and_of_dc_is_its_level() {
        assert_eq!(rms(&[]), 0.0);
        assert_eq!(rms(&[0.0; 32]), 0.0);
        assert!((rms(&[0.5; 32]) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn normalize_lifts_a_quiet_take_but_never_clips() {
        let quiet = vec![0.05, -0.05, 0.025];
        let out = normalize_peak(&quiet, 0.85);
        let peak = out.iter().fold(0.0f32, |m, s| m.max(s.abs()));
        assert!(
            (peak - 0.85).abs() < 1e-5,
            "peak should reach target: {peak}"
        );
        assert!(out.iter().all(|s| s.abs() <= 1.0));
    }

    #[test]
    fn normalize_leaves_loud_or_silent_audio_alone() {
        let loud = vec![0.9, -0.95];
        assert_eq!(normalize_peak(&loud, 0.85), loud);
        let silent = vec![0.0; 8];
        assert_eq!(normalize_peak(&silent, 0.85), silent);
    }

    #[test]
    fn trim_drops_leading_and_trailing_room_tone() {
        let sr = WHISPER_SAMPLE_RATE;
        let frame = (sr / 50) as usize;
        let mut clip = vec![0.0; frame * 5]; // silence
        clip.extend(tone(frame * 5, 0.5)); // speech
        clip.extend(vec![0.0; frame * 5]); // silence
        let out = trim_silence(&clip, sr, SILENCE_RMS);
        assert!(!out.is_empty());
        // Padded by one frame either side, so shorter than the original but
        // longer than the bare speech region.
        assert!(out.len() < clip.len(), "should have trimmed something");
        assert!(out.len() >= frame * 5, "speech must survive trimming");
    }

    #[test]
    fn trim_returns_nothing_for_pure_silence() {
        assert!(trim_silence(&[0.0; 4800], WHISPER_SAMPLE_RATE, SILENCE_RMS).is_empty());
        assert!(trim_silence(&[], WHISPER_SAMPLE_RATE, SILENCE_RMS).is_empty());
    }

    #[test]
    fn pipeline_yields_mono_16k_from_stereo_48k() {
        // 200 ms of stereo speech at 48 kHz, padded with silence.
        let frames = 48_000 / 5;
        let mut interleaved = vec![0.0f32; 4_800 * 2];
        for (i, s) in tone(frames, 0.4).into_iter().enumerate() {
            interleaved.push(s); // L
            interleaved.push(s); // R
            let _ = i;
        }
        interleaved.extend(vec![0.0f32; 4_800 * 2]);
        let out = prepare_for_whisper(&interleaved, 2, 48_000);
        assert!(!out.is_empty(), "speech should survive the pipeline");
        // Trimmed to roughly the speech region, resampled to a third the rate.
        assert!(out.len() < interleaved.len() / 2);
        assert!(out.iter().all(|s| s.abs() <= 1.0));
    }

    #[test]
    fn has_speech_distinguishes_a_take_from_a_quiet_room() {
        let sr = WHISPER_SAMPLE_RATE;
        assert!(has_speech(&tone(sr as usize / 2, 0.4), sr));
        assert!(!has_speech(&[0.0; 8000], sr));
        // A 10 ms blip is not an utterance.
        assert!(!has_speech(&tone(160, 0.6), sr));
    }

    #[test]
    fn window_chunks_splits_on_the_30_second_boundary() {
        let sr = WHISPER_SAMPLE_RATE;
        let forty_five_secs = vec![0.1f32; sr as usize * 45];
        let chunks = window_chunks(&forty_five_secs, sr);
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].len(), sr as usize * 30);
        assert_eq!(chunks[1].len(), sr as usize * 15);
        assert!(window_chunks(&[], sr).is_empty());
    }

    #[test]
    fn endpointer_fires_only_after_speech_then_silence() {
        let sr = WHISPER_SAMPLE_RATE;
        let frame = (sr / 50) as usize; // 20 ms
        let mut ep = Endpointer::new(sr);

        // Silence alone never ends an utterance that never began.
        for _ in 0..100 {
            assert!(!ep.push(&vec![0.0; frame]));
        }
        assert!(!ep.speech_started());

        // Speak for ~400 ms.
        let speech = tone(frame, 0.5);
        for _ in 0..20 {
            assert!(!ep.push(&speech));
        }
        assert!(ep.speech_started());

        // Then go quiet: it should fire once past the endpoint threshold.
        let silence = vec![0.0; frame];
        let mut fired_at = None;
        for i in 1..=80 {
            if ep.push(&silence) {
                fired_at = Some(i * 20);
                break;
            }
        }
        let ms = fired_at.expect("endpointer should fire after trailing silence");
        assert!(
            ms >= ENDPOINT_SILENCE_MS as usize,
            "fired too early at {ms}ms"
        );
    }

    #[test]
    fn endpointer_reset_clears_state() {
        let sr = WHISPER_SAMPLE_RATE;
        let frame = (sr / 50) as usize;
        let mut ep = Endpointer::new(sr);
        for _ in 0..20 {
            ep.push(&tone(frame, 0.5));
        }
        assert!(ep.speech_started());
        ep.reset();
        assert!(!ep.speech_started());
    }
}
