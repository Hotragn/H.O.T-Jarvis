//! Microphone capture (§6.4). The I/O half of Voice v1 — the DSP it feeds is
//! pure and lives in `core::audio`.
//!
//! Capture runs on its own thread because a cpal `Stream` is not `Send` on every
//! platform: the thread owns the stream for its whole life, and talks to the
//! rest of the app over channels and a shared buffer. That keeps the Tauri
//! command handlers free of platform quirks and makes "stop" a single message
//! rather than cross-thread stream juggling.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use crate::core::audio;

/// A finished take, still in the device's native format.
#[derive(Debug, Clone)]
pub struct Captured {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
    pub channels: u16,
}

impl Captured {
    /// Conditioned for Whisper: mono, 16 kHz, trimmed, normalized.
    pub fn for_whisper(&self) -> Vec<f32> {
        audio::prepare_for_whisper(&self.samples, self.channels, self.sample_rate)
    }

    pub fn duration_secs(&self) -> f32 {
        let frames = self.samples.len() / self.channels.max(1) as usize;
        frames as f32 / self.sample_rate.max(1) as f32
    }
}

/// Live capture: the owning thread plus the channels used to end it.
struct Active {
    stop: Sender<()>,
    done: Receiver<Result<Captured, String>>,
    handle: thread::JoinHandle<()>,
}

/// Push-to-talk recorder. One take at a time, by design: a second `start` while
/// recording is a bug in the caller, not something to silently queue.
#[derive(Default)]
pub struct MicRecorder {
    active: Mutex<Option<Active>>,
}

impl MicRecorder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_recording(&self) -> bool {
        self.active.lock().map(|a| a.is_some()).unwrap_or(false)
    }

    /// Opens the default input device and starts filling a buffer. Returns the
    /// negotiated rate and channel count so the UI can show what it's hearing.
    pub fn start(&self) -> Result<(u32, u16), String> {
        let mut slot = self.active.lock().map_err(|e| e.to_string())?;
        if slot.is_some() {
            return Err("already recording".into());
        }

        let (stop_tx, stop_rx) = mpsc::channel::<()>();
        let (done_tx, done_rx) = mpsc::channel::<Result<Captured, String>>();
        // The thread reports the negotiated format before it starts streaming so
        // `start` can fail loudly if there is no usable input device.
        let (ready_tx, ready_rx) = mpsc::channel::<Result<(u32, u16), String>>();

        let handle = thread::spawn(move || {
            match capture_loop(&ready_tx, &stop_rx) {
                Ok(captured) => {
                    let _ = done_tx.send(Ok(captured));
                }
                Err(e) => {
                    // If we failed before reporting readiness, `start` is still
                    // waiting on ready_rx — tell it there, not just here.
                    let _ = ready_tx.send(Err(e.clone()));
                    let _ = done_tx.send(Err(e));
                }
            }
        });

        match ready_rx.recv() {
            Ok(Ok(format)) => {
                *slot = Some(Active {
                    stop: stop_tx,
                    done: done_rx,
                    handle,
                });
                Ok(format)
            }
            Ok(Err(e)) => Err(e),
            // The thread died without reporting: surface something useful.
            Err(_) => Err("microphone thread stopped before it started".into()),
        }
    }

    /// Ends the take and hands back the audio.
    pub fn stop(&self) -> Result<Captured, String> {
        let active = {
            let mut slot = self.active.lock().map_err(|e| e.to_string())?;
            slot.take().ok_or("not recording")?
        };
        // Asking the thread to finish; if the channel is already closed the
        // thread has exited on its own and `done` still holds the result.
        let _ = active.stop.send(());
        let result = active
            .done
            .recv()
            .unwrap_or_else(|_| Err("microphone thread ended without a result".into()));
        let _ = active.handle.join();
        result
    }

    /// Drops a take without transcribing it (used when the user cancels).
    pub fn cancel(&self) {
        if let Ok(mut slot) = self.active.lock() {
            if let Some(active) = slot.take() {
                let _ = active.stop.send(());
                let _ = active.handle.join();
            }
        }
    }
}

/// Captures one utterance and stops on its own, using the tested energy
/// endpointer instead of waiting for a click. This is what makes hands-free
/// possible: push-to-talk needs a human to say when they finished, and a wake
/// phrase can't.
///
/// Returns `Ok(None)` when the window expired with no speech at all, which is
/// the normal case while merely armed and waiting — a silent room must not
/// produce a transcription attempt on every cycle.
///
/// Bounded twice on purpose: `max_wait` caps how long we sit in silence, and
/// `max_utterance` caps a single take, so a stuck-open microphone or a
/// television in the room can never grow an unbounded buffer.
pub fn listen_until_endpoint(
    max_wait: Duration,
    max_utterance: Duration,
) -> Result<Option<Captured>, String> {
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or("no microphone found — check your input device")?;
    let config = device
        .default_input_config()
        .map_err(|e| format!("microphone has no usable format: {e}"))?;
    let sample_rate = config.sample_rate().0;
    let channels = config.channels();

    let buffer = Arc::new(Mutex::new(Vec::<f32>::new()));
    let stream = open_input_stream(&device, config, Arc::clone(&buffer))?;

    // The endpointer works on mono frames at the device rate; downmix as we go
    // so its energy maths sees what a listener would hear.
    let mut endpointer = audio::Endpointer::new(sample_rate);
    let mut consumed = 0usize; // interleaved samples already fed to the endpointer
    let started = std::time::Instant::now();
    let mut ended_cleanly = false;

    loop {
        std::thread::sleep(Duration::from_millis(40));

        let available = {
            let buf = buffer.lock().map_err(|e| e.to_string())?;
            buf.len()
        };
        if available > consumed {
            let chunk: Vec<f32> = {
                let buf = buffer.lock().map_err(|e| e.to_string())?;
                buf[consumed..available].to_vec()
            };
            consumed = available;
            let mono = audio::downmix_to_mono(&chunk, channels);
            if endpointer.push(&mono) {
                ended_cleanly = true;
                break;
            }
        }

        if endpointer.speech_started() {
            if started.elapsed() >= max_utterance {
                // Someone (or something) is talking indefinitely; take what we
                // have rather than growing forever.
                ended_cleanly = true;
                break;
            }
        } else if started.elapsed() >= max_wait {
            // Silence for the whole window: nothing was said.
            break;
        }
    }

    drop(stream);

    if !ended_cleanly && !endpointer.speech_started() {
        return Ok(None);
    }
    let samples = buffer
        .lock()
        .map(|b| b.clone())
        .map_err(|e| e.to_string())?;
    Ok(Some(Captured {
        samples,
        sample_rate,
        channels,
    }))
}

/// Opens and starts an input stream that appends normalized f32 samples to
/// `sink`. Shared by every capture path so a new one can't drift on sample
/// format handling.
fn open_input_stream(
    device: &cpal::Device,
    config: cpal::SupportedStreamConfig,
    sink: Arc<Mutex<Vec<f32>>>,
) -> Result<cpal::Stream, String> {
    let on_error = |e| eprintln!("microphone stream error: {e}");
    let format = config.sample_format();
    let stream = match format {
        cpal::SampleFormat::F32 => device.build_input_stream(
            &config.into(),
            move |data: &[f32], _: &_| append(&sink, data.iter().copied()),
            on_error,
            None,
        ),
        cpal::SampleFormat::I16 => device.build_input_stream(
            &config.into(),
            move |data: &[i16], _: &_| {
                append(&sink, data.iter().map(|s| *s as f32 / i16::MAX as f32))
            },
            on_error,
            None,
        ),
        cpal::SampleFormat::U16 => device.build_input_stream(
            &config.into(),
            move |data: &[u16], _: &_| {
                append(
                    &sink,
                    data.iter()
                        .map(|s| (*s as f32 / u16::MAX as f32) * 2.0 - 1.0),
                )
            },
            on_error,
            None,
        ),
        other => Err(cpal::BuildStreamError::BackendSpecific {
            err: cpal::BackendSpecificError {
                description: format!("unsupported sample format {other:?}"),
            },
        }),
    }
    .map_err(|e| format!("could not open the microphone: {e}"))?;
    stream
        .play()
        .map_err(|e| format!("could not start the microphone: {e}"))?;
    Ok(stream)
}

/// Set to stop an in-flight barge-in watch early.
///
/// The watcher holds the microphone, and playback usually ends *before* its
/// length estimate runs out. Without this it would keep the mic for the
/// remaining seconds of its budget, and the follow-up capture that should start
/// immediately after the answer would find the device busy.
static BARGE_CANCEL: AtomicBool = AtomicBool::new(false);

/// Asks an in-flight `watch_for_barge` to return now. Safe to call when nothing
/// is watching.
pub fn cancel_barge_watch() {
    BARGE_CANCEL.store(true, Ordering::Release);
}

/// Watches the microphone during playback and returns as soon as someone talks
/// over the assistant (Voice v3).
///
/// Deliberately returns loudness, never audio: nothing captured here is kept or
/// transcribed, so the assistant cannot hear its own voice into a request. That
/// is the property `Phase::wants_audio` protects, and this path must not weaken
/// it — see `core::bargein` for how the echo is measured rather than assumed.
///
/// `Ok(Some(echo_level))` means the user interrupted, and carries what the
/// assistant's own loudness measured, so the UI can explain a room that is too
/// loud for barge-in to work. `Ok(None)` means playback ran its course, the
/// budget expired, or the caller cancelled.
///
/// The caller is expected to start this once playback has actually begun and to
/// `cancel_barge_watch` when it ends; see `BARGE_CANCEL`.
pub fn watch_for_barge(max: Duration) -> Result<Option<f32>, String> {
    const POLL: Duration = Duration::from_millis(20);

    // Clear any cancel left over from a previous answer. Safe here because the
    // caller starts the watch from the speech `onstart` handler, so a cancel for
    // *this* playback cannot have been issued yet.
    BARGE_CANCEL.store(false, Ordering::Release);

    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or("no microphone found — check your input device")?;
    let config = device
        .default_input_config()
        .map_err(|e| format!("microphone has no usable format: {e}"))?;
    let sample_rate = config.sample_rate().0;
    let channels = config.channels();

    let buffer = Arc::new(Mutex::new(Vec::<f32>::new()));
    let stream = open_input_stream(&device, config, Arc::clone(&buffer))?;

    let mut detector = crate::core::bargein::Detector::new();
    let started = std::time::Instant::now();
    let mut interrupted = false;

    while started.elapsed() < max {
        std::thread::sleep(POLL);

        // Checked every poll, so the microphone is released within ~20ms of
        // playback ending rather than at the end of the length estimate.
        if BARGE_CANCEL.swap(false, Ordering::AcqRel) {
            break;
        }

        // Drain rather than index-and-clear: the capture callback keeps appending
        // while we work, so clearing the whole buffer would silently throw away
        // frames that arrived mid-loop. Draining takes exactly what was read.
        //
        // Draining at all is the point: nothing captured here is kept. Holding on
        // to it would make this a recording of the assistant talking, which is
        // precisely what this path exists to avoid.
        let chunk: Vec<f32> = {
            let mut buf = buffer.lock().map_err(|e| e.to_string())?;
            if buf.is_empty() {
                continue;
            }
            buf.drain(..).collect()
        };

        let mono = audio::downmix_to_mono(&chunk, channels);
        let frame_ms = ((mono.len() as f32 / sample_rate as f32) * 1000.0).round() as u32;
        if frame_ms == 0 {
            continue;
        }
        if detector.frame(audio::rms(&mono), frame_ms) == crate::core::bargein::Barge::Interrupt {
            interrupted = true;
            break;
        }
    }

    drop(stream);
    if let Ok(mut buf) = buffer.lock() {
        buf.clear();
    }
    Ok(interrupted.then(|| detector.echo_level().unwrap_or(0.0)))
}

/// Name of the default input device, for the UI. `None` when there isn't one.
pub fn default_input_name() -> Option<String> {
    cpal::default_host().default_input_device()?.name().ok()
}

/// Owns the stream for the duration of one take.
fn capture_loop(
    ready: &Sender<Result<(u32, u16), String>>,
    stop: &Receiver<()>,
) -> Result<Captured, String> {
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or("no microphone found — check your input device")?;
    let config = device
        .default_input_config()
        .map_err(|e| format!("microphone has no usable format: {e}"))?;

    let sample_rate = config.sample_rate().0;
    let channels = config.channels();
    let buffer = Arc::new(Mutex::new(Vec::<f32>::new()));
    let sink = Arc::clone(&buffer);
    let on_error = |e| eprintln!("microphone stream error: {e}");

    // Devices hand us whatever format they like; normalize to f32 on the way in.
    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => device.build_input_stream(
            &config.into(),
            move |data: &[f32], _: &_| append(&sink, data.iter().copied()),
            on_error,
            None,
        ),
        cpal::SampleFormat::I16 => device.build_input_stream(
            &config.into(),
            move |data: &[i16], _: &_| {
                append(&sink, data.iter().map(|s| *s as f32 / i16::MAX as f32))
            },
            on_error,
            None,
        ),
        cpal::SampleFormat::U16 => device.build_input_stream(
            &config.into(),
            move |data: &[u16], _: &_| {
                append(
                    &sink,
                    data.iter()
                        .map(|s| (*s as f32 / u16::MAX as f32) * 2.0 - 1.0),
                )
            },
            on_error,
            None,
        ),
        other => Err(cpal::BuildStreamError::BackendSpecific {
            err: cpal::BackendSpecificError {
                description: format!("unsupported sample format {other:?}"),
            },
        }),
    }
    .map_err(|e| format!("could not open the microphone: {e}"))?;

    stream
        .play()
        .map_err(|e| format!("could not start the microphone: {e}"))?;
    let _ = ready.send(Ok((sample_rate, channels)));

    // Block until asked to stop (or the sender is dropped).
    let _ = stop.recv();
    drop(stream); // stop the device before reading the buffer

    let samples = buffer
        .lock()
        .map(|b| b.clone())
        .map_err(|e| e.to_string())?;
    Ok(Captured {
        samples,
        sample_rate,
        channels,
    })
}

/// Appends converted samples, never poisoning the audio callback on a lock error
/// (dropping a frame is far better than panicking inside the driver thread).
fn append(sink: &Arc<Mutex<Vec<f32>>>, samples: impl Iterator<Item = f32>) {
    if let Ok(mut buf) = sink.lock() {
        buf.extend(samples);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duration_accounts_for_channel_interleaving() {
        // 16 000 stereo frames at 16 kHz is one second, not two.
        let captured = Captured {
            samples: vec![0.0; 16_000 * 2],
            sample_rate: 16_000,
            channels: 2,
        };
        assert!((captured.duration_secs() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn duration_is_safe_on_degenerate_metadata() {
        let captured = Captured {
            samples: vec![0.0; 10],
            sample_rate: 0,
            channels: 0,
        };
        // Must not divide by zero.
        assert!(captured.duration_secs().is_finite());
    }

    #[test]
    fn a_fresh_recorder_is_idle_and_refuses_to_stop() {
        let rec = MicRecorder::new();
        assert!(!rec.is_recording());
        assert!(rec.stop().is_err(), "stopping when idle should error");
        // Cancelling when idle is a no-op, not a panic.
        rec.cancel();
    }

    #[test]
    fn conditioning_a_take_produces_whisper_ready_audio() {
        // Loud stereo tone at 48 kHz, padded with silence either side.
        let mut samples = vec![0.0f32; 4_800 * 2];
        for i in 0..48_000 {
            let s = 0.4 * (i as f32 * 0.05).sin();
            samples.push(s);
            samples.push(s);
        }
        samples.extend(vec![0.0f32; 4_800 * 2]);
        let captured = Captured {
            samples,
            sample_rate: 48_000,
            channels: 2,
        };
        let prepared = captured.for_whisper();
        assert!(!prepared.is_empty());
        assert!(prepared.iter().all(|s| s.abs() <= 1.0));
        // Mono at a third the rate, so far fewer samples than we started with.
        assert!(prepared.len() < captured.samples.len() / 2);
    }
}
