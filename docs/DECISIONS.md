# Decision Log

One short entry per meaningful decision. Newest at the top. Cite sources for ideas borrowed from papers or other projects (ideas only — implementations here are original).

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
