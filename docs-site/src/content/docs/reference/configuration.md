---
title: Configuration
description: Environment variables, data location, the kill switch, and free-tier routing behavior.
---

All configuration is optional. With nothing set, the app starts and explains how
to finish setup. Keys are read from the environment or a gitignored `.env`.

## Environment variables

| Variable | Default | Purpose |
|---|---|---|
| `OLLAMA_BASE_URL` | `http://localhost:11434` | Local model endpoint |
| `OLLAMA_MODEL` | `llama3.2` | Local model name |
| `GROQ_API_KEY` | — | Enables the Groq free-tier fallback |
| `GROQ_MODEL` | `llama-3.3-70b-versatile` | Groq model |
| `OPENROUTER_API_KEY` | — | Enables the OpenRouter fallback |
| `OPENROUTER_MODEL` | `meta-llama/llama-3.3-70b-instruct:free` | OpenRouter model |
| `JARVIS_DATA_DIR` | OS app-data dir | Where memory, events, notes, and skills live |
| `JARVIS_STOP` | — | Kill switch — set to halt any autonomous loop |

## Routing order

The router always tries **Ollama first** (local, private, unlimited), then each
configured cloud provider in order. If nothing is reachable, it returns a
friendly onboarding message rather than an error.

## Free-tier hygiene

- **Response cache** — identical requests are served from a 10-minute cache
  (deduping double-sends and retries). Cache hits are labeled `cached` in the
  reply footer, never served silently.
- **Rate-limit backoff** — a cloud provider that returns 429/503 goes on an
  exponential cooldown (30s → 10min, honoring `Retry-After`) and is skipped until
  it clears. The local model is exempt — it's unlimited.

## Data directory layout

```
<data_dir>/
├─ jarvis.sqlite3     # messages, facts, insights (WAL mode)
├─ events.jsonl       # append-only action log
├─ notes/             # <slug>.md
└─ skills/            # <slug>/{manifest.json, skill.rhai, test.rhai, history/}
```

The store is exportable and wipeable from the Memory tab — see
[Export or wipe your memory](/how-to/export-and-wipe-memory/).
