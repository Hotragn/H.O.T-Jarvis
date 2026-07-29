---
title: Talk to Jarvis (local speech-to-text)
description: Turn on fully on-device dictation with Whisper, so voice input works offline, costs nothing, and never leaves your machine.
---

Jarvis can listen as well as speak. Speech recognition runs **entirely on your
machine** with a local Whisper model: no account, no API key, no audio upload.

## Why this needs a build flag

Voice output works everywhere out of the box, because every OS ships speech
synthesis. Voice *input* is the harder half:

- The web `SpeechRecognition` API doesn't exist in WebView2, the engine Jarvis
  uses on Windows. So on Windows there was simply no dictation.
- Where that API *does* exist, most browsers implement it by streaming your
  audio to a cloud service. That breaks the promise that nothing leaves your
  machine, so Jarvis prefers the local engine whenever it's available.

The fix is to capture the microphone in Rust and transcribe locally. That pulls
in a pure-Rust ML stack, which adds a few minutes of build time, so it's opt-in
rather than forced on everyone.

## Enable it

Build with the `local-whisper` feature:

```bash frame="terminal"
npm run tauri dev -- --features local-whisper
```

For a release build:

```bash frame="terminal"
npm run tauri build -- --features local-whisper
```

No cmake, no C++ compiler, no LLVM needed — unlike `whisper.cpp` bindings, the
inference here is plain Rust (via [candle](https://github.com/huggingface/candle)),
so `cargo` handles everything.

## First run: one download

The model isn't bundled, because shipping tens of megabytes to people who never
use voice would be rude. The first time you click the mic button it shows a **⇩**
and fetches the model:

- **~43 MB**, one time, from Hugging Face's public mirror (no account needed)
- Cached under your app data directory, namespaced per model
- Written to a `.partial` file and renamed into place, so an interrupted
  download can never be mistaken for a good one

Once it lands, the button becomes a real push-to-talk mic and everything after
that is offline.

## Using it

1. Click the mic (or press it while typing — dictation appends to whatever is
   already in the composer).
2. Speak.
3. Click again to stop. The button shows **…** while Whisper transcribes, then
   drops the text into the composer for you to edit before sending.

If you say nothing intelligible, Jarvis tells you it didn't catch anything
rather than inventing a sentence. That is deliberate: Whisper is known to
hallucinate filler like "thank you" on silence, and putting words in your mouth
is worse than admitting it heard nothing.

## What it does to your audio

Before transcription, the take is conditioned locally:

| Step | Why |
|------|-----|
| Downmix to mono | Whisper takes one channel |
| Resample to 16 kHz | the only rate Whisper accepts |
| Trim silence | drops dead air at both ends |
| Normalize peak | a quiet talker isn't read as silence |
| Energy gate | skips transcription entirely if nobody spoke |

Long dictation is split on Whisper's 30-second window and rejoined, so a long
thought isn't silently cut off.

## Choosing a model

Two checkpoints are available, both quantized to keep the download small:

- `tiny.en` (default) — English only, fastest
- `tiny` — multilingual

The English-only model is sharper for English and is what you get unless you
ask otherwise.

## If it doesn't work

- **Mic button is dim with "not available in this build"** — you're running
  without `--features local-whisper`, and this window has no speech recognizer.
- **"no microphone found"** — check your OS input device; Jarvis uses the system
  default.
- **Nothing transcribes but the mic lights up** — try speaking closer; the
  energy gate treats very quiet takes as silence on purpose.
