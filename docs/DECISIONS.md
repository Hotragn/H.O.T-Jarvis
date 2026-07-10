# Decision Log

One short entry per meaningful decision. Newest at the top. Cite sources for ideas borrowed from papers or other projects (ideas only — implementations here are original).

## 2026-07-07 — Reasoning-memory v0: the event log is the experience stream

§5.2 implemented as a reflection pass over the existing event log rather than a parallel trace store — the log already records what was tried and what happened (chat outcomes, authoring attempts, skill runs, failures). A pass digests events past a watermark (`reflection.last_event_id` fact), asks the model for at most 3 one-sentence lessons (JSON contract, tolerant parser — live llama3.2 probe revealed it returns a bare object instead of an array, now accepted and regression-tested), stores them in a new `insights` table (schema v2, migration tested against a hand-built v1 database), and rides the freshest lessons into both the chat and skill-authoring system prompts. Periodicity without an autonomous loop: the frontend pings `reflect_if_due` after each turn; it fires only when 20+ new messages accumulated. Watermark advances even on an empty harvest so the same events are never re-digested. Idea credits: ReasoningBank, "Hindsight is 20/20", Reflexion. Scoring/decay and selective forgetting deferred to v1 (tracked in ROADMAP).

## 2026-07-06 — Skill authoring v1: trust the harness, not the model

The assistant now authors skills from a natural-language request: strict JSON contract (name/description/code/test), parse tolerant of fences/chatter, save, run the bundled test, and on failure loop the error back to the model (Reflexion) for up to 3 attempts; a never-passing draft is saved *flagged* and reported honestly. Validated against the owner's live llama3.2 before shipping: the 3B model produces well-formed JSON but buggy Rhai (`${}` interpolation habits, off-by-one tests) — which confirmed the design premise that **correctness must come from executing the test, never from trusting the model**. The system prompt gained a concrete correct example and an explicit anti-`${}` rule after those live probes. Authoring quality scales with the model: local 3B will flag more drafts; a free Groq 70B key one-shots most requests. Follow-up in ROADMAP: Ollama structured output (`format: json`) to guarantee parseability.

## 2026-07-06 — Skill engine v0: Rhai as the skill runtime

Skills (§5.1) are Rhai scripts, not Python/WASM/JS: Rhai is pure Rust (no system deps, compiles in CI unchanged), sandboxed **by construction** (scripts see only language built-ins — no fs, no network, nothing we don't register), and hard-capped per execution (200k operations, bounded call depth / string / collection sizes) so a runaway loop terminates instead of hanging the assistant — there's a test proving it. Contract: `fn run(input)` plus a bundled `fn test()`; the test runs on every save and re-save; a failing skill is *flagged and refused at run time*, never used blindly (Reflexion-style refinement loop). Versions are integers; the previous source is archived to `history/v<N>/` on every update — cheap provenance for the future replay/undo story. Idea credits: Voyager's skill library, MUSE-Autoskill, Reflexion. v1 (LLM authors skills from chat) is next in ROADMAP; v0 deliberately ships the engine + manual authoring UI so the harness is proven before the model writes code into it.

## 2026-07-06 — Router hardening: cache + cooldown, local model exempt

Free-tier hygiene (§2) as pure clock-injected bookkeeping in `core/reliability.rs`: a TTL+capacity response cache (dedupes identical requests — double-sends and retries — 10 min TTL, 64 entries) and a per-provider cooldown tracker (exponential 30s→10min backoff, honors Retry-After) that the router consults before calling cloud providers. Deliberate asymmetry: **Ollama is exempt from both penalties** — local inference is unlimited and private, so penalizing it only hurts the user. Cache hits are marked `cached: true` and shown in the reply meta, because silently serving stale answers would violate the honesty principle. Provider call errors became structured (`CallError::Http{status, retry_after}`) so backoff decisions don't parse error strings.

## 2026-07-06 — Event log v0: JSONL, append-only, tolerant reader

Chose plain JSONL over SQLite for the event log even though SQLite is already in the app: an append-only text file is trivially greppable, corruption-isolated per line (the reader skips bad lines instead of failing — tested with a simulated torn write), and matches the replay literature's framing of an immutable event stream. Ids are monotonic and resume across restarts. Logging is best-effort by design (`log_event` swallows errors): the log must never take the assistant down. Full chat text is logged because deterministic replay (§5.4) needs it; the file lives in the same local app-data dir as memory and is covered by the same export/wipe story (wiring the log into export is a follow-up). The EVENTS tab is read-only on purpose — no replay/undo buttons until the engine exists.

## 2026-07-06 — Multi-view HUD: tabs + palette, honest telemetry only

Added tab navigation (chat / notes / memory, Ctrl+1-3) and a Ctrl+K command palette (subsequence-scored filtering, pure + unit-tested) instead of pulling in a router dependency — three views don't justify react-router yet; revisit when deep-linkable views land (§6.3). The reference wallpapers are full of gauges, so the rule for live data is: **only real numbers** — CPU/RAM/uptime come from a new `get_telemetry` command backed by `sysinfo`, plus actual message/fact/note counts and a wall clock. No invented readouts. New views surface the backend that already existed (notes tool, memory export/wipe) rather than faking future features (skill library, replay) — those stay in the roadmap until their engines exist.

## 2026-07-06 — HUD visual language: instrument panel, one glow

Owner supplied film-Jarvis reference imagery (wallpapercave.com/jarvis-wallpapers). Reading the references closely: they are *restrained* — mostly monochrome steel hairlines and tiny uppercase mono labels, with exactly one bright element (the circular core). Adopted as a hard rule: glow is reserved for the arc-reactor core (canvas `ArcCore`, the app's signature element — its offline/idle/thinking states are the primary trust signal) and everything else stays hairlines and type. Light theme reinterprets the same instrument as a blueprint on paper rather than a dimmed dark theme. System fonts only (offline-first, no font downloads). Original implementation; imagery used as mood reference only.

## 2026-07-05 — Bootstrap stack: Tauri v2 + React/TS + Rust core

- **Desktop shell: Tauri v2** (default per the project brief): light installers, low idle RAM, Rust backend suits the future low-latency voice pipeline. Electron kept as a documented fallback only.
- **Frontend: React 19 + TypeScript + Vite.** Mainstream ecosystem, works with Framer Motion/Motion One for the §6.2 motion system later. Design tokens as CSS variables from day one, themed via `data-theme` on the root element.
- **Core logic in Rust, Tauri-independent.** `src-tauri/src/core/` (router, memory, tools) has no Tauri types so every module unit-tests without a webview. Tauri commands in `lib.rs` are a thin adapter layer.
- **Memory v0: SQLite via `rusqlite` (bundled)** — durable, zero-dependency install, schema-migration table from the start so upgrades never lose data. Vector recall deferred to M1 (candidate: `sqlite-vec` to stay single-store). Data dir: OS app-data dir, overridable with `JARVIS_DATA_DIR` (used by tests and dev).
- **Router v0 providers: Ollama → Groq → OpenRouter `:free`.** Ollama first because local is unlimited and private. Providers implement one trait; the router walks them in priority order and returns a structured "no provider" onboarding message rather than an error when nothing is configured. Gemini/Cerebras adapters deferred until the trait proves itself.
- **First built-in tool: local notes** scoped to the app data dir — real utility, zero external side effects, exercises the tool interface without needing the approval-gate machinery yet.
- **Licensing:** Apache-2.0 (already in repo from initial commit).
- **CI on ubuntu-latest** with Tauri system deps for the Rust job (fastest runners); cross-platform packaging matrix deferred to release milestones per brief §8.
- **Prompt/agent files are gitignored** (`.claude/`, `CLAUDE.md`, `docs/agentbrief*`) at the owner's request — continuity files stay local to this machine.

Idea credits for planned hero features (tracked in ROADMAP): Voyager skill library; MUSE-Autoskill; Reflexion; ReasoningBank; "Hindsight is 20/20" agent memory; Mem0 selective memory; "Agentic Uncertainty Reveals Agentic Overconfidence"; 2026 replayable-agent/determinism literature. To be re-read before each respective implementation.
