---
title: Architecture
description: How the pieces fit — the desktop shell, the Tauri-independent Rust core, and the modules inside it.
---

H.O.T-Jarvis is a Tauri v2 desktop app: a Rust core with a web UI. The guiding
rule is that **all real logic lives in `src-tauri/src/core/`, independent of
Tauri**, so every module unit-tests without a webview. The command layer
(`lib.rs`) is a thin adapter between IPC and the core.

## The shell

- **Tauri v2** — light installer, low idle RAM, native WebView. Chosen over
  Electron for weight and over a web app because the assistant is a real desktop
  presence.
- **React + TypeScript + Vite** — the HUD: a live arc-reactor core, tabbed views
  (chat / skills / notes / memory / events), a command palette, and live
  telemetry.

## Core modules

| Module | Responsibility |
|---|---|
| `router` | One interface over Ollama + free cloud tiers; fallback, response cache, rate-limit backoff |
| `memory` | SQLite store (messages, facts, insights) with versioned migrations; export + wipe |
| `skills` | Sandboxed Rhai skill engine — save, test, version, run, roll back |
| `authoring` | Prompt + parse layer for the assistant writing its own skills |
| `reflection` | Digests the event log into durable insights |
| `confidence` | Extracts the self-rated confidence marker from replies |
| `eventlog` | Append-only JSONL of every action |
| `replay` | Rebuilds state from the log and audits it against the database |
| `reliability` | Clock-injected cache + cooldown bookkeeping (pure, tested) |
| `tools` | Built-in tools — currently local notes, sandboxed to the data dir |

## Data flow of a message

1. UI calls `chat_send`.
2. The core stores the user turn, gathers recent messages + the freshest
   insights, and appends the confidence instruction to the system prompt.
3. The router picks a provider (local first) and calls it.
4. The reply's confidence marker is extracted; the turn is stored; a
   `chat.assistant` event is logged.
5. Periodically, a reflection pass turns recent events into lessons that feed
   step 2 next time.

## Why this shape

Keeping the core Tauri-free makes it fast to test and portable — the same core
is what a future mobile target would reuse. Keeping every action in an
append-only log is what makes replay and undo possible. And routing local-first
is what keeps the whole thing free and private.
