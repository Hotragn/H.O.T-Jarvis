//! Microphone capture (§6.4). The I/O half of Voice v1 — the DSP it feeds is
//! pure and lives in `core::audio`.
//!
//! Capture runs on its own thread because a cpal `Stream` is not `Send` on every
//! platform: the thread owns the stream for its whole life, and talks to the
//! rest of the app over channels and a shared buffer. That keeps the Tauri
//! command handlers free of platform quirks and makes "stop" a single message
//! rather than cross-thread stream juggling.

use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;

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
