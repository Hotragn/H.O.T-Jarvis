//! Local Whisper inference (§6.4), compiled only with the `local-whisper`
//! feature.
//!
//! Greedy decoding, one 30-second window at a time. The upstream candle example
//! adds temperature fallback with log-prob and compression-ratio retries; for
//! push-to-talk dictation of a sentence or two, greedy is the right trade — it
//! is roughly a third of the code, deterministic, and fast enough on CPU. The
//! guards that actually matter for short takes are kept: non-speech suppression,
//! and refusing to emit Whisper's classic silence hallucinations.
//!
//! The mel filterbank is vendored (64 KB) rather than downloaded, so a first run
//! needs exactly one network fetch — the model itself.

use std::path::Path;

use candle_core::{IndexOp, Tensor};
use candle_transformers::models::whisper::{self as m, quantized_model::Whisper};
use candle_transformers::quantized_var_builder::VarBuilder;
use tokenizers::Tokenizer;

use super::stt::{clean_transcript, is_probably_silence, ModelSpec};

/// Precomputed 80 x 201 mel filterbank from the candle project (MIT/Apache-2.0),
/// little-endian f32. Whisper's front-end needs exactly these coefficients.
const MEL_FILTERS: &[u8] = include_bytes!("../../assets/melfilters.bytes");

/// Stop runaway decoding. Whisper's text context is 448; a spoken sentence is
/// far shorter, and a cap keeps a degenerate loop from hanging the app.
const MAX_TOKENS: usize = 224;

pub struct WhisperTranscriber {
    model: Whisper,
    tokenizer: Tokenizer,
    mel_filters: Vec<f32>,
    device: candle_core::Device,
    /// Token ids that must never be emitted as text.
    suppress: Vec<u32>,
    sot: u32,
    eot: u32,
    transcribe: u32,
    no_timestamps: u32,
    language: Option<u32>,
}

impl WhisperTranscriber {
    /// Loads a cached model from disk. CPU only: this has to work on any machine,
    /// and a GPU build would mean per-platform feature juggling for a model this
    /// small.
    pub fn load(model_dir: &Path, spec: &ModelSpec) -> Result<Self, String> {
        let device = candle_core::Device::Cpu;

        let config_path = model_dir.join(spec.config);
        let config: m::Config = serde_json::from_slice(
            &std::fs::read(&config_path)
                .map_err(|e| format!("could not read {}: {e}", config_path.display()))?,
        )
        .map_err(|e| format!("bad model config: {e}"))?;

        let tokenizer_path = model_dir.join(spec.tokenizer);
        let tokenizer = Tokenizer::from_file(&tokenizer_path)
            .map_err(|e| format!("could not read the tokenizer: {e}"))?;

        let weights_path = model_dir.join(spec.weights);
        let vb = VarBuilder::from_gguf(&weights_path, &device)
            .map_err(|e| format!("could not read the model weights: {e}"))?;
        let model =
            Whisper::load(&vb, config).map_err(|e| format!("could not build the model: {e}"))?;

        let token = |t: &str| tokenizer.token_to_id(t);
        let sot = token(m::SOT_TOKEN).ok_or("tokenizer is missing the start token")?;
        let eot = token(m::EOT_TOKEN).ok_or("tokenizer is missing the end token")?;
        let transcribe = token(m::TRANSCRIBE_TOKEN).ok_or("tokenizer is missing <|transcribe|>")?;
        let no_timestamps =
            token(m::NO_TIMESTAMPS_TOKEN).ok_or("tokenizer is missing <|notimestamps|>")?;
        // Multilingual checkpoints need to be told the language; the .en ones
        // must not be, or decoding goes sideways.
        let language = if spec.english_only {
            None
        } else {
            token("<|en|>")
        };

        // Never let the model narrate itself into the transcript.
        let mut suppress: Vec<u32> = m::NO_SPEECH_TOKENS
            .iter()
            .filter_map(|t| token(t))
            .collect();
        suppress.push(sot);
        suppress.push(transcribe);
        suppress.push(no_timestamps);
        if let Some(translate) = token(m::TRANSLATE_TOKEN) {
            suppress.push(translate);
        }

        let mel_filters = MEL_FILTERS
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect();

        Ok(Self {
            model,
            tokenizer,
            mel_filters,
            device,
            suppress,
            sot,
            eot,
            transcribe,
            no_timestamps,
            language,
        })
    }

    /// Transcribes 16 kHz mono audio. Long takes are decoded window by window
    /// and joined, so dictation isn't silently cut at 30 seconds.
    pub fn transcribe(&mut self, pcm: &[f32]) -> Result<String, String> {
        if pcm.is_empty() {
            return Ok(String::new());
        }
        let mut pieces = Vec::new();
        for window in super::audio::window_chunks(pcm, m::SAMPLE_RATE as u32) {
            let text = self.transcribe_window(window)?;
            if !text.is_empty() {
                pieces.push(text);
            }
        }
        let joined = pieces.join(" ");
        let cleaned = clean_transcript(&joined);
        // A silence hallucination is worse than an empty result: it puts words
        // in the user's mouth. Prefer nothing.
        if is_probably_silence(&cleaned) {
            return Ok(String::new());
        }
        Ok(cleaned)
    }

    fn transcribe_window(&mut self, pcm: &[f32]) -> Result<String, String> {
        // Whisper's encoder is fixed-size: pad (or trim) to exactly 30 seconds.
        let mut padded = pcm.to_vec();
        padded.resize(m::N_SAMPLES, 0.0);

        let mel = m::audio::pcm_to_mel(&self.model.config, &padded, &self.mel_filters);
        let n_mels = self.model.config.num_mel_bins;
        let frames = mel.len() / n_mels.max(1);
        let mel = Tensor::from_vec(mel, (1, n_mels, frames), &self.device)
            .map_err(|e| format!("mel tensor: {e}"))?;

        self.model.reset_kv_cache();
        let audio = self
            .model
            .encoder
            .forward(&mel, true)
            .map_err(|e| format!("audio encoder: {e}"))?;

        let mut tokens = vec![self.sot];
        if let Some(lang) = self.language {
            tokens.push(lang);
        }
        tokens.push(self.transcribe);
        tokens.push(self.no_timestamps);
        let prompt_len = tokens.len();

        for step in 0..MAX_TOKENS {
            let input = Tensor::new(tokens.as_slice(), &self.device)
                .and_then(|t| t.unsqueeze(0))
                .map_err(|e| format!("token tensor: {e}"))?;
            // Only the first pass fills the KV cache from scratch.
            let ys = self
                .model
                .decoder
                .forward(&input, &audio, step == 0)
                .map_err(|e| format!("text decoder: {e}"))?;
            let (_, seq_len, _) = ys.dims3().map_err(|e| format!("decoder shape: {e}"))?;
            let logits = self
                .model
                .decoder
                .final_linear(
                    &ys.i((.., seq_len - 1..))
                        .map_err(|e| format!("slice logits: {e}"))?,
                )
                .and_then(|l| l.i(0))
                .and_then(|l| l.i(0))
                .map_err(|e| format!("final linear: {e}"))?;

            let next = self.argmax_allowed(&logits)?;
            if next == self.eot {
                break;
            }
            tokens.push(next);
        }

        let text_tokens: Vec<u32> = tokens[prompt_len..]
            .iter()
            .copied()
            .filter(|t| !self.suppress.contains(t))
            .collect();
        if text_tokens.is_empty() {
            return Ok(String::new());
        }
        self.tokenizer
            .decode(&text_tokens, true)
            .map_err(|e| format!("could not decode tokens: {e}"))
    }

    /// Greedy pick over the vocabulary, with suppressed ids removed from the
    /// running rather than post-filtered, so they can't win the argmax.
    fn argmax_allowed(&self, logits: &Tensor) -> Result<u32, String> {
        let values = logits
            .to_vec1::<f32>()
            .map_err(|e| format!("logits to vec: {e}"))?;
        let mut best = None::<(usize, f32)>;
        for (id, &v) in values.iter().enumerate() {
            if self.suppress.contains(&(id as u32)) {
                continue;
            }
            if best.map(|(_, bv)| v > bv).unwrap_or(true) {
                best = Some((id, v));
            }
        }
        best.map(|(id, _)| id as u32)
            .ok_or_else(|| "no candidate token".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vendored_mel_filterbank_has_whisper_shape() {
        let floats: Vec<f32> = MEL_FILTERS
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect();
        // 80 mel bands over (N_FFT/2 + 1) = 201 frequency bins.
        assert_eq!(floats.len(), 80 * (m::N_FFT / 2 + 1));
        // Filter weights are non-negative and bounded.
        assert!(floats.iter().all(|f| f.is_finite() && *f >= 0.0));
        assert!(floats.iter().any(|f| *f > 0.0), "filterbank is all zeros");
    }

    #[test]
    fn loading_a_missing_model_fails_with_a_readable_message() {
        let dir = tempfile::tempdir().unwrap();
        let spec = super::super::stt::resolve_model(Some("tiny.en"));
        // WhisperTranscriber isn't Debug (it holds model weights), so match
        // rather than unwrap_err().
        match WhisperTranscriber::load(dir.path(), spec) {
            Ok(_) => panic!("loading from an empty directory should fail"),
            Err(err) => assert!(
                err.contains("could not read"),
                "expected a readable load error, got: {err}"
            ),
        }
    }
}
