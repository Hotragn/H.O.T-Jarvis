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
- [x] `done` Memory: semantic recall — local embeddings (Ollama `nomic-embed-text`, env/default-configurable) stored as f32 BLOBs in the existing SQLite (schema v4), brute-force cosine in Rust (sub-ms at personal-history scale; no FAISS/C extensions). Chat prompts recall up to 4 relevant past messages beyond the recent window (floor 0.45, recent turns excluded); every new turn is indexed; "Build index" backfills old history in batches; meaning-based search box in the memory view. Local-only by design: embeddings never go to cloud providers — no embedding model simply means recall is off, never an error.
- [x] `done` Skill engine v0: manifest + versioned Rhai skills + "every skill ships a test" harness; failing skills flagged and refused (§5.1)
- [x] `done` Skill engine v1: assistant authors skills on request (LLM writes code + test, engine validates, Reflexion refinement loop, flagged if never passing)
- [x] `done` Skill quality: authoring uses structured output — Ollama `format` with the skill JSON schema (constrained decoding, so the model *cannot* emit fences/prose/missing keys), and `response_format: json_object` on the OpenAI-compatible providers. Retries now classify the failure (interpolation / no-run / no-test / not-JSON / broken / test-failed) and carry a targeted counter-example instead of repeating generic rules. Structured calls bypass the response cache.
- [x] `done` Confidence estimate v0: self-rated 0-100 per answer, clarify-instead-of-guess below 40, gauge dial on the core + per-message label (§5.3)
- [x] `done` Confidence v1: calibration tracking — grade any reply (✓/✕) and Jarvis scores its own stated confidence against reality. Brier score + expected calibration error + a reliability diagram per confidence band, and the headline signed bias in plain words ("overconfident by about 30 points"). Rebuilt from the event log, so it stays replayable; `debias()` is ready to gate risky actions on adjusted confidence.
- [x] `done` Confidence v2: measured calibration now acts instead of just displaying. The answering prompt carries a calibration note ("your stated confidence has run ~30 points HIGHER than your accuracy — correct for that"); every reply is re-read through `debias()` and carries raw + calibrated confidence; an answer whose *calibrated* value falls below the ask threshold is flagged in the chat with a verify warning and toned by the honest number. Silent until there's enough evidence — never invents a correction.
- [x] `done` Replay & undo v1: undo for chat/notes/skills with inverse state captured at write time; revert-style skill rollbacks; deterministic replay audit (log vs. database drift report); undo buttons + audit in the timeline (§5.4)
- [x] `done` Replay v2: step-through session player (scrub/play/step through the log while the reconstructed world — messages, notes, skills, lessons — rebuilds beside it; "changes only" skips events that changed nothing), the audit now reconciles notes and skills as well as chat (`audit_state`), and a reflection pass is undoable because it logs the ids of the lessons it created. Note *content* stays deliberately unreplayable: the log records a character count, not the body.
- [x] `done` Reflection pass v0: event-log digest → distilled lessons stored as insights, injected into chat + authoring prompts; manual "Reflect now" + auto-trigger every 20 messages (§5.2)
- [x] `done` Reflection v1: insight scoring, decay, and selective forgetting — per-kind half-lives (a `user` preference outlives a `provider` observation by 12x), corroboration when a later pass re-derives a lesson, use counts, a protection window for new lessons, duplicate merging, and a capacity cap. Prompt injection is now scored rather than "most recent N", and every forget is logged with its reason so it stays auditable.
- [ ] `ready` Reflection v2: replace token-overlap duplicate detection with local embeddings once a model is already on disk (Voice v1 proved the download flow); surface a "forgotten" tab so drops are reversible from the UI

## M2 — Interface & voice

- [x] `done` Command palette (Ctrl+K) + tab navigation (chat / notes / memory, Ctrl+1-3)
- [x] `done` Live telemetry readouts: CPU sparkline, RAM, uptime, clock, memory counts (real data via sysinfo)
- [x] `done` Memory browser view v0 with export-JSON and wipe controls
- [x] `done` Notes view (create / list / read) over the notes tool
- [x] `done` Skill library view with per-skill test status, create form, run panel
- [x] `done` Reflection browser: dedicated view over the reflection insights (ctrl+6) — filter chips by kind (skill/provider/user/general) with counts, per-kind accented cards showing the lesson, its provenance (which events it came from), and date, plus "Reflect now"
- [x] `done` Animated view transitions: keyed per-tab enter animation (the core stays as the persistent shared element), reduced-motion guarded
- [x] `done` Shared-element morph between views (FLIP): one accent bar glides + scales under the active tab on every switch, done with transform only (translateX + scaleX off a 1px base) so it's GPU-cheap and layout-free; snaps on first paint and resize, reduced-motion guarded
- [x] `done` Voice v0: spoken replies via OS voices (free, offline), voice toggle, barge-in, speaking/listening core states, push-to-talk where the platform provides recognition, honest fallback where it doesn't
- [x] `done` Voice v1: fully local STT so voice input works inside WebView2 — mic captured in Rust (cpal), conditioned in `core::audio` (mono/16 kHz/trim/normalize, energy VAD + endpointer), transcribed by quantized Whisper via candle (pure Rust, no cmake/clang). Opt-in `local-whisper` feature, one-time ~43 MB model fetch, mel filterbank vendored. Honest routing: local is preferred over the WebView recognizer because that one uploads audio.
- [ ] `ready` Voice v2: wake word + continuous conversation on top of the existing endpointer; hands-free mode that never needs a click
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
- [x] `done` iOS UI pass: safe-area insets (viewport-fit=cover + env()), 44pt touch targets, 16px inputs (no focus zoom), phone-width responsive layout, hover-only controls always visible on touch, system telemetry hidden on iOS. Native AVSpeech/SFSpeech plugin still future work.
- [x] `done` Inference fork implemented (companion default): runtime provider + custom model settings in-app (settings tab, ctrl+7) — Ollama URL/model, Groq + OpenRouter keys/models — applied without restart, persisted in the DB (env still seeds defaults). tauri.conf.json carries the iOS bundle block (usage strings, min iOS 15, encryption-exempt). App Store listing package in docs/ios/APPSTORE.md.

## Distribution / front door

- [x] `done` Premium landing page (standalone static site, `landing/`): live-canvas hero, four feature showcases, narrative design, performance-correct lazy video plumbing, designed posters in every slot. ~28 KB video-free baseline.
- [x] `done` Flagship cinematic ("a skill is born") rendered free via HyperFrames (HTML→MP4), 1080p/12s, wired into the hero slot.
- [x] `done` Render the remaining loops (hero ambient, memory, confidence, undo) via HyperFrames — all five 1080p clips live in every slot.
- [x] `done` Deploy the site to GitHub Pages: landing at the root, Astro docs built under `/docs` in one `deploy-pages.yml` deploy. Pages enabled (Source = GitHub Actions). Landing https://hotragn.github.io/H.O.T-Jarvis/ · docs https://hotragn.github.io/H.O.T-Jarvis/docs/
- [ ] `ready` README GIF / hero — the 10-second screen recording (needs owner or screen capture).

## Chores

- [x] `done` Export completeness: events + notes in the memory export; wipe also clears the event log
- [x] `done` Note deletion in the notes view — undoable: content captured at delete, restorable from the timeline

## Backlog / open problems

- Cross-platform build matrix + auto-update flow on release tags
- Optional Obsidian-vault connector (one skill, never a requirement)
