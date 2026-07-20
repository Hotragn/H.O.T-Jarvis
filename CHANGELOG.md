# Changelog

All notable changes to H.O.T-Jarvis. Format loosely follows [Keep a Changelog](https://keepachangelog.com/); versions follow semver.

## [0.1.0] — 2026-07-07

First release: a runnable core with all four hero features, local-first and free forever.

### Added

**App & interface**
- Tauri v2 desktop app with a Jarvis-style instrument HUD: arc-reactor core visualizer reflecting offline/idle/thinking state, corner-bracket framing, dark theme + blueprint-style light theme, all driven by design tokens with a `prefers-reduced-motion` path.
- Five views — chat, skills, notes, memory, events — with tab navigation (Ctrl+1-5) and a Ctrl+K command palette (fuzzy subsequence matching).
- Live telemetry: CPU sparkline, RAM, process uptime, wall clock, and memory/fact/note counts. Real numbers only.

**Model routing (free forever)**
- Router tries local Ollama first, then free cloud tiers (Groq, OpenRouter `:free`) when keys are present; friendly onboarding when nothing is configured.
- Response cache (10 min, hits labeled "cached") and per-provider rate-limit backoff honoring Retry-After; the local model is exempt from both.

**Hero feature 1 — self-evolving skill library**
- Skills are sandboxed Rhai scripts with a manifest and a bundled test; every save runs the test, failing skills are flagged and refuse to run.
- Ask Jarvis to build a skill: the model drafts code + test, the engine proves it, failures loop back for refinement (up to 3 attempts).
- Integer versioning with archived history of every previous version.

**Hero feature 2 — reflective reasoning-memory**
- A reflection pass digests the event log into at most 3 one-sentence lessons, stored durably and injected into future chat and authoring prompts.
- Auto-fires every 20 messages; manual "Reflect now" in the memory view.

**Hero feature 3 — calibrated autonomy**
- Every answer carries a self-rated 0-100 confidence, shown as a gauge dial on the core and a per-message label; below 40 the assistant asks a clarifying question instead of guessing.

**Hero feature 4 — replay & undo**
- Append-only event log of every action, inspectable in a timeline view.
- Undo for chat turns, note saves, and skill changes, using inverse state captured at write time; skill undo is a revert-style rollback that preserves history; every undo is itself logged.
- Replay audit: rebuilds the conversation purely from the log and diffs it against live memory, reporting drift.

**Memory & data control**
- Persistent SQLite store (messages, facts, insights) with versioned migrations tested against old databases.
- One-click JSON export of everything (memory, insights, events, notes); wipe clears memory and the event log, with an honest confirmation.

**Engineering**
- 61 Rust + 26 frontend tests; CI with lint, type-check, tests, secret scan, and build; merged-branch auto-cleanup.

### Known limitations
- No voice, no installers or auto-update, unsigned builds, single-machine memory, no semantic (vector) recall yet.

[0.1.0]: https://github.com/Hotragn/H.O.T-Jarvis/releases/tag/v0.1.0
