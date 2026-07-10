# H.O.T-Jarvis

> **The assistant that grows its own new skills, remembers you and how it reasons, tells you when it's unsure, and lets you replay and undo anything it does — running locally, for free.**

H.O.T-Jarvis is an open-source, local-first personal AI assistant shipped as a real desktop app (Tauri v2). It costs nothing to run: inference goes through a model-agnostic router that prefers local models via Ollama and falls back to free cloud tiers (Groq, OpenRouter `:free`). No paid API is ever required.

**Status: early bootstrap.** The thin runnable core exists; the hero features below are being built in the open. Nothing on this page is described as done unless it is.

## What works today

- **Desktop app shell** (Tauri v2 + React): a Jarvis-style HUD with an arc-reactor core visualizer, dark/light themes, tab navigation (chat / skills / notes / memory / events), a Ctrl+K command palette, and live telemetry (CPU, RAM, uptime — real numbers only).
- **Model router** — tries Ollama first (local, private, unlimited), then free cloud tiers (Groq, OpenRouter `:free`) if you add a key. Response caching and rate-limit backoff keep it polite to free tiers. With nothing configured, the app still starts and shows you exactly how to get free inference in one step.
- **Persistent memory** — conversation history, profile facts, and learned insights in SQLite with versioned migrations. Restart the app; it still remembers. Export everything as one JSON file, or wipe it — your data, your control.
- **Self-evolving skill library** — ask Jarvis to build a skill and it writes sandboxed code *plus a test*, proves it by running the test, and refines on failure. Failing skills are flagged and refuse to run. Versioned with history.
- **Reflective reasoning-memory** — Jarvis periodically re-reads its own action log and keeps short lessons about what worked and what failed; the freshest lessons ride along in future prompts.
- **Confidence meter** — every answer carries the model's self-rated probability of being right, shown as a gauge on the core; below the threshold it asks a clarifying question instead of guessing. Surfaced honestly, not blindly trusted.
- **Append-only event log** — every action is recorded and inspectable in a timeline view (the seed of replay & undo).
- **One built-in tool** — local notes (saved inside the app's own data folder, never elsewhere).

## Quickstart

```bash
# 1. Free local inference (recommended): install https://ollama.com then
ollama pull llama3.2

# 2. Or a free cloud key: copy .env.example to .env and add a Groq or OpenRouter key
cp .env.example .env

# 3. Run the app
npm install
npm run tauri dev
```

Prerequisites: [Node.js](https://nodejs.org) 20+, [Rust](https://rustup.rs) stable, and on Linux the [Tauri system deps](https://tauri.app/start/prerequisites/).

## Free model options

| Provider | Cost | Notes |
|---|---|---|
| Ollama (local) | Free, unlimited | Default. Private — nothing leaves your machine |
| Groq | Free tier | Fast; get a key at console.groq.com |
| OpenRouter `:free` models | Free tier | Many models; key at openrouter.ai |

The router respects free-tier rate limits (backoff and fallback) so the app never pressures you to pay.

## Hero features (tracked in [ROADMAP.md](ROADMAP.md))

1. **Self-evolving skill library** *(shipped, v1)* — the assistant authors, tests, versions, and reuses its own skills; watch the library grow in the SKILLS tab.
2. **Reflective reasoning-memory** *(shipped, v0)* — it stores lessons from its own outcomes, not just facts, and improves from experience.
3. **Calibrated autonomy** *(shipped, v0)* — a visible confidence gauge; below a threshold it asks you instead of guessing.
4. **Replay & undo** *(in progress)* — the append-only event log and timeline view exist; deterministic replay and undo controls are next.

## Honest limitations

- No voice yet; no wake word; no auto-update; unsigned builds.
- The chat loop is minimal: one tool, no sub-agents, no skill engine yet.
- Free cloud tiers rate-limit; local Ollama quality depends on the model you pull and your hardware.

## Contributing & security

See [CONTRIBUTING.md](CONTRIBUTING.md) and [SECURITY.md](SECURITY.md). Licensed under [Apache-2.0](LICENSE).
