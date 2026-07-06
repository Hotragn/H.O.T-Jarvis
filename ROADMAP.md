# Roadmap

Machine-readable task queue. Status: `ready` | `in-progress` | `done` | `blocked`. Autonomous cycles pick the highest-value `ready` task, top to bottom within a milestone.

## M0 — Bootstrap (thin runnable core)

- [x] `done` Repo hygiene: license, gitignore, env example, docs, CI
- [x] `done` Tauri v2 + React shell that launches on Windows
- [x] `done` Model router: Ollama + Groq + OpenRouter `:free`, fallback + friendly no-provider onboarding
- [x] `done` Persistent memory v0: SQLite (messages + kv profile) surviving restart, with migrations
- [x] `done` Built-in tool v0: local notes in app data dir
- [x] `done` Bare HUD: design tokens, dark/light themes, animated waveform, chat view
- [ ] `ready` Verify `npm run tauri dev` end-to-end on Windows with a live Ollama model; record README GIF
- [ ] `ready` Response caching + per-provider backoff in the router (free-tier hygiene)

## M1 — Hero feature foundations

- [ ] `ready` Event log v0: append-only JSONL of every action with enough state to replay (§5.4 groundwork)
- [ ] `ready` Memory: local vector store for semantic recall (FAISS/Chroma/Qdrant equivalent that's Rust-friendly, e.g. sqlite-vec)
- [ ] `ready` Skill engine v0: skill manifest format + loader + "every skill ships a test" harness (§5.1)
- [ ] `ready` Confidence estimate v0: pre-action self-rating surfaced in the HUD (§5.3)
- [ ] `blocked (needs event log)` Replay timeline UI v0 (§5.4)
- [ ] `ready` Reflection pass v0: periodic summarization of reasoning traces into memory (§5.2)

## M2 — Interface & voice

- [ ] `ready` Command palette (Ctrl+K) + client-side routing with animated transitions
- [ ] `ready` Skill library view with per-skill test status
- [ ] `ready` Memory & reflection browser view
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
