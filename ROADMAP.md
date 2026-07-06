# Roadmap

Machine-readable task queue. Status: `ready` | `in-progress` | `done` | `blocked`. Autonomous cycles pick the highest-value `ready` task, top to bottom within a milestone.

## M0 — Bootstrap (thin runnable core)

- [x] `done` Repo hygiene: license, gitignore, env example, docs, CI
- [x] `done` Tauri v2 + React shell that launches on Windows
- [x] `done` Model router: Ollama + Groq + OpenRouter `:free`, fallback + friendly no-provider onboarding
- [x] `done` Persistent memory v0: SQLite (messages + kv profile) surviving restart, with migrations
- [x] `done` Built-in tool v0: local notes in app data dir
- [x] `done` Bare HUD: design tokens, dark/light themes, animated waveform, chat view
- [x] `done` Verify `npm run tauri dev` end-to-end on Windows with a live Ollama model (owner-verified 2026-07-06); README GIF still `ready`
- [x] `done` Response caching + per-provider backoff in the router (free-tier hygiene)

## M1 — Hero feature foundations

- [x] `done` Event log v0: append-only JSONL of every action (chat, notes, wipes, startups) + read-only timeline tab (§5.4 groundwork)
- [ ] `ready` Replay v1: deterministic re-run of a session from the event log; undo for reversible actions (§5.4)
- [ ] `ready` Memory: local vector store for semantic recall (FAISS/Chroma/Qdrant equivalent that's Rust-friendly, e.g. sqlite-vec)
- [ ] `ready` Skill engine v0: skill manifest format + loader + "every skill ships a test" harness (§5.1)
- [ ] `ready` Confidence estimate v0: pre-action self-rating surfaced in the HUD (§5.3)
- [ ] `blocked (needs event log)` Replay timeline UI v0 (§5.4)
- [ ] `ready` Reflection pass v0: periodic summarization of reasoning traces into memory (§5.2)

## M2 — Interface & voice

- [x] `done` Command palette (Ctrl+K) + tab navigation (chat / notes / memory, Ctrl+1-3)
- [x] `done` Live telemetry readouts: CPU sparkline, RAM, uptime, clock, memory counts (real data via sysinfo)
- [x] `done` Memory browser view v0 with export-JSON and wipe controls
- [x] `done` Notes view (create / list / read) over the notes tool
- [ ] `ready` Skill library view with per-skill test status (needs skill engine, M1)
- [ ] `ready` Reflection browser (needs reasoning-memory, M1)
- [ ] `ready` Animated shared-element transitions between views
- [ ] `ready` Voice v0: local STT (whisper.cpp/faster-whisper) + local TTS (Piper/Kokoro), optional and gracefully degrading
- [ ] `ready` System tray + global hotkey + launch-at-login

## M3 — Autonomy

- [ ] `blocked (needs CI + guardrails proven)` Auto mode: scheduler loop over this roadmap with resource caps, kill switch, dry-run gates
- [ ] `ready` Research-to-feature loop: scan agent papers, propose issues

## Backlog / open problems

- Selective forgetting in memory (differentiator, hard)
- Shared-element morph transitions between views
- Cross-platform build matrix + auto-update flow on release tags
- Optional Obsidian-vault connector (one skill, never a requirement)
