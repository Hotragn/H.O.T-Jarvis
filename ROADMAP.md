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
- [x] `done` Skill engine v0: manifest + versioned Rhai skills + "every skill ships a test" harness; failing skills flagged and refused (§5.1)
- [x] `done` Skill engine v1: assistant authors skills on request (LLM writes code + test, engine validates, Reflexion refinement loop, flagged if never passing)
- [ ] `ready` Skill quality: use Ollama structured output (format json) for authoring; consider few-shot per failure class
- [x] `done` Confidence estimate v0: self-rated 0-100 per answer, clarify-instead-of-guess below 40, gauge dial on the core + per-message label (§5.3)
- [ ] `ready` Confidence v1: calibration tracking (predicted vs. outcomes via reflection), gate risky actions on threshold
- [x] `done` Replay & undo v1: undo for chat/notes/skills with inverse state captured at write time; revert-style skill rollbacks; deterministic replay audit (log vs. database drift report); undo buttons + audit in the timeline (§5.4)
- [ ] `ready` Replay v2: step-through session player; undo for more kinds; audit covers notes/skills state too
- [x] `done` Reflection pass v0: event-log digest → distilled lessons stored as insights, injected into chat + authoring prompts; manual "Reflect now" + auto-trigger every 20 messages (§5.2)
- [ ] `ready` Reflection v1: insight scoring/decay + selective forgetting (the open problem)

## M2 — Interface & voice

- [x] `done` Command palette (Ctrl+K) + tab navigation (chat / notes / memory, Ctrl+1-3)
- [x] `done` Live telemetry readouts: CPU sparkline, RAM, uptime, clock, memory counts (real data via sysinfo)
- [x] `done` Memory browser view v0 with export-JSON and wipe controls
- [x] `done` Notes view (create / list / read) over the notes tool
- [x] `done` Skill library view with per-skill test status, create form, run panel
- [x] `done` Reflection browser: dedicated view over the reflection insights (ctrl+6) — filter chips by kind (skill/provider/user/general) with counts, per-kind accented cards showing the lesson, its provenance (which events it came from), and date, plus "Reflect now"
- [x] `done` Animated view transitions: keyed per-tab enter animation (the core stays as the persistent shared element), reduced-motion guarded
- [ ] `ready` Shared-element morph between views (FLIP) as a richer follow-up
- [x] `done` Voice v0: spoken replies via OS voices (free, offline), voice toggle, barge-in, speaking/listening core states, push-to-talk where the platform provides recognition, honest fallback where it doesn't
- [ ] `ready` Voice v1: fully local STT (Whisper on-device) so voice input works inside WebView2; then wake word + VAD + continuous conversation
- [x] `done` System tray + global hotkey + launch-at-login: tray icon with show / start-at-login toggle / quit, left-click toggles the window, closing hides to tray, global Ctrl+Shift+J summons it from anywhere; desktop-only, mobile build unaffected (§6.5)

## M3 — Autonomy

- [ ] `blocked (needs CI + guardrails proven)` Auto mode: scheduler loop over this roadmap with resource caps, kill switch, dry-run gates
- [ ] `ready` Research-to-feature loop: scan agent papers, propose issues

## M4 — Mobile (iOS)

Groundwork planned in [docs/ios/README.md](docs/ios/README.md). Build/submit needs macOS + Xcode + a paid Apple Developer account.

- [x] `done` iOS architecture + App Store readiness plan (Tauri v2 iOS target, inference fork, Review-guideline analysis incl. 2.5.2, privacy manifest, asset specs, Mac build/submit checklist)
- [ ] `blocked (owner decision)` Choose the iOS inference model: companion-to-desktop (recommended) / on-device / cloud tiers
- [ ] `blocked (needs Mac)` `tauri ios init` + signing + Simulator run
- [ ] `blocked (needs $99 enrollment)` App Store Connect record, TestFlight, submission
- [ ] `ready` iOS UI pass: safe-area insets, touch targets, hide desktop-only telemetry; native AVSpeech/SFSpeech voice plugin

## Distribution / front door

- [x] `done` Premium landing page (standalone static site, `landing/`): live-canvas hero, four feature showcases, narrative design, performance-correct lazy video plumbing, designed posters in every slot. ~28 KB video-free baseline.
- [x] `done` Flagship cinematic ("a skill is born") rendered free via HyperFrames (HTML→MP4), 1080p/12s, wired into the hero slot.
- [x] `done` Render the remaining loops (hero ambient, memory, confidence, undo) via HyperFrames — all five 1080p clips live in every slot.
- [x] `done` Deploy the site to GitHub Pages: landing at the root, Astro docs built under `/docs` in one `deploy-pages.yml` deploy. Pages enabled (Source = GitHub Actions). Landing https://hotragn.github.io/H.O.T-Jarvis/ · docs https://hotragn.github.io/H.O.T-Jarvis/docs/
- [ ] `ready` README GIF / hero — the 10-second screen recording (needs owner or screen capture).

## Chores

- [x] `done` Export completeness: events + notes in the memory export; wipe also clears the event log
- [ ] `ready` Note deletion in the notes view

## Backlog / open problems

- Selective forgetting in memory (differentiator, hard)
- Shared-element morph transitions between views
- Cross-platform build matrix + auto-update flow on release tags
- Optional Obsidian-vault connector (one skill, never a requirement)
