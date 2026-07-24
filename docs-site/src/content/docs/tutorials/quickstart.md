---
title: Quickstart
description: Install H.O.T-Jarvis, give it a brain, run it, and confirm it remembers you across a restart.
---

By the end of this page you'll have H.O.T-Jarvis running on your machine, holding
a real conversation through a local model, and remembering it after you quit and
reopen the app. It costs nothing.

## Prerequisites

- **Node.js 20+** and **Rust (stable)** — the app is Tauri v2 (Rust core, web UI).
- On Linux, the [Tauri system dependencies](https://tauri.app/start/prerequisites/).
- Either **Ollama** (recommended, local and unlimited) or a free cloud key.

## 1. Give it something to think with

The assistant needs a model. The private, unlimited, free option is a local one:

```bash frame="terminal" title="install a local model"
# install Ollama from https://ollama.com, then:
ollama pull llama3.2
```

No local model? You can use a free cloud tier instead — see
[Add a free cloud key](/how-to/add-a-free-cloud-key/). With neither, the app
still launches and shows you exactly how to finish setup.

## 2. Run the app

```bash frame="terminal" title="from the repo root"
npm install
npm run tauri dev
```

The first launch compiles the Rust core, so give it a few minutes. After that,
startup is fast.

## 3. Say hello

Type a message in the composer and send it. The arc-reactor core moves from
**standby** to **online** once a model is reachable, and the reply's footer shows
which provider and model answered, plus a confidence score.

## 4. Confirm it remembers

This is the part most "assistants" skip. Tell it something about yourself, then:

1. Quit the app completely.
2. Reopen it.
3. Open the **Memory** tab.

Your conversation is still there. Memory is a local SQLite database that survives
restarts, upgrades, and reboots — and it's yours to
[export or wipe](/how-to/export-and-wipe-memory/) whenever you want.

## Next

- [Author your first skill](/tutorials/your-first-skill/) — watch the assistant
  write and test a new ability.
- [The four hero features](/explanation/hero-features/) — what makes this more
  than a chat window.
