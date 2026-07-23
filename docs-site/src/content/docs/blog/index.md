---
title: "H.O.T-Jarvis 0.1.0: an assistant that grows its own skills"
description: The first release — a local-first, free-forever AI assistant with a self-evolving skill library, reflective memory, calibrated confidence, and full replay & undo.
lastUpdated: 2026-07-07
---

Today marks the first release of **H.O.T-Jarvis 0.1.0** — an open-source,
local-first personal AI assistant that runs on your machine, for free, and gets
more capable the more you use it.

## Why another "Jarvis"

The name is everywhere, attached mostly to voice clones that open websites and
read the weather. We wanted something with a sharper identity: not a demo, but an
assistant you can actually trust with real context, run forever without a bill,
and inspect completely.

## What's in 0.1.0

All four hero features are real and tested, not roadmap items:

- **Self-evolving skill library.** Ask for an ability; it writes the code *and* a
  test, proves the test passes, and refines on failure. Untested skills are
  flagged and refused.
- **Reflective reasoning-memory.** It re-reads its own action log and keeps
  lessons about what worked and what didn't — then applies them.
- **Calibrated confidence.** Every answer carries a self-rating; below a
  threshold it asks instead of guessing.
- **Replay & undo.** Every action is recorded and reversible, with an audit that
  proves the log reproduces memory exactly.

Underneath: a Tauri v2 desktop app, a Tauri-independent Rust core, a local-first
model router (Ollama → free cloud fallback), persistent SQLite memory, spoken
replies through your OS voices, and a Jarvis-style HUD. Around 90 tests,
CI-gated, Apache-2.0.

## Free and private, on purpose

Inference runs locally by default; no paid API is ever required, and your data
stays in a folder you control — [exportable and wipeable](/how-to/export-and-wipe-memory/)
at any time. See [Local-first and free](/explanation/local-first-and-free/) for
the reasoning.

## Try it

The [quickstart](/tutorials/quickstart/) takes about ten minutes: install a free
local model, run the app, and confirm it remembers you across a restart. Then
[author your first skill](/tutorials/your-first-skill/) and watch it test itself.

Feedback and contributions are welcome on
[GitHub](https://github.com/Hotragn/H.O.T-Jarvis).
