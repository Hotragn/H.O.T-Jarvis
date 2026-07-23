---
title: Event log
description: The append-only action log — event kinds, payloads, and how replay & undo use it.
---

Every action the assistant takes is appended as one JSON line to
`events.jsonl` in the app data directory. The log is the foundation of replay
and undo.

## Event shape

```ts
{ id: number; ts: number; kind: string; payload: object }
```

Ids are monotonic and resume across restarts. The reader skips corrupt lines, so
a torn write can't poison history.

## Event kinds

| Kind | Payload | Emitted when |
|---|---|---|
| `app.started` | `{ version }` | the app launches |
| `chat.user` | `{ text, msg_id }` | you send a message |
| `chat.assistant` | `{ text, provider, model, duration_ms, confidence, msg_id }` | a reply is produced |
| `chat.failed` | `{ error }` | all providers failed |
| `note.saved` | `{ slug, chars, previous }` | a note is saved (`previous` enables undo) |
| `skill.saved` / `skill.authored` | `{ name, version, test_status, ... }` | a skill is saved or authored |
| `skill.tested` | `{ name, test_status }` | a skill is re-tested |
| `skill.run` | `{ name, ok, error? }` | a skill runs |
| `memory.reflected` | `{ insights, events_digested }` | a reflection pass runs |
| `memory.wiped` | `{}` | memory is wiped |
| `undo.chat` / `undo.note` / `undo.skill` | `{ undoes, ... }` | an action is reversed |
| `replay.audited` | `{ matched, missing_in_db, extra_in_db, deterministic }` | a replay audit runs |

## Undo

Undo reads the inverse state captured **at write time** — `note.saved` carries
the previous content; chat events carry their memory row id. Every undo appends
its own `undo.*` event: the timeline is never rewritten, even about reversals.
Wipes and reflections are deliberately irreversible.

## Replay audit

The audit rebuilds the conversation from the log alone (honoring undos and
wipes) and diffs it, order-preserving, against the live database.
**Deterministic** means the two agree exactly; any drift is reported as
`missing_in_db` / `extra_in_db` rather than hidden.
