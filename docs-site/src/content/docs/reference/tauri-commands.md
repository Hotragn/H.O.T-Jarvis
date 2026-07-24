---
title: Tauri command reference
description: Every IPC command the desktop app exposes — arguments, return shapes, and errors.
---

The UI talks to the Rust core through Tauri commands (`invoke`). This is the
app's real API surface. All commands return an error string on failure.

## Conversation

### `get_status`
Provider and readiness snapshot. Returns:

```ts
{
  providers: { id: string; configured: boolean; reachable: boolean | null; model: string }[];
  ready: boolean;
  onboarding: string | null;   // setup guidance when no provider is available
  message_count: number;
  fact_count: number;
}
```

### `chat_send`
Send a user turn; get the assistant reply.

- **Args:** `{ text: string }`
- **Returns:** `{ content, provider, model, cached: boolean, confidence: number | null }`
- **Errors:** empty message; all providers failed; no provider configured.

### `get_history`
- **Args:** `{ limit?: number }` (default 200)
- **Returns:** `{ id, role, content, created_at }[]` in chronological order.

## Telemetry

### `get_telemetry`
Live machine + app vitals. Returns:

```ts
{ cpu_percent: number; mem_used: number; mem_total: number;
  uptime_secs: number; note_count: number; message_count: number; fact_count: number }
```

The first call reports `0` CPU (a prior sample is needed); it settles by the next poll.

## Skills

| Command | Args | Returns |
|---|---|---|
| `list_skills` | — | `SkillManifest[]` |
| `save_skill` | `{ name, description, code, test }` | `SkillManifest` |
| `author_skill` | `{ request }` | `{ manifest, attempts, passed }` |
| `test_skill` | `{ name }` | `SkillManifest` |
| `run_skill` | `{ name, input }` | `string` (output) — errors if flagged |

`SkillManifest`: `{ name, version, description, created_at, updated_at, test_status }`
where `test_status` is `{ status: "passed" }` or `{ status: "failed", detail }`.
See the [skill manifest reference](/reference/skill-manifest/).

## Notes

| Command | Args | Returns |
|---|---|---|
| `list_notes` | — | `string[]` (slugs) |
| `read_note` | `{ name }` | `string` |
| `save_note` | `{ title, content }` | `string` (slug) |

## Memory, reflection, replay

| Command | Args | Returns |
|---|---|---|
| `get_events` | `{ limit?: number }` | `Event[]` |
| `list_insights` | `{ limit?: number }` | `Insight[]` |
| `reflect_now` | — | `Insight[]` (new lessons) |
| `reflect_if_due` | — | `number \| null` (count, or null if not due) |
| `undo_event` | `{ eventId }` | `string` (human-readable outcome) |
| `replay_audit` | — | `ReplayReport` |
| `export_memory` | — | JSON (messages, facts, insights, events, notes) |
| `wipe_memory` | — | — (clears memory + event log; keeps notes) |

`ReplayReport`: `{ matched, missing_in_db, extra_in_db, deterministic }`. See the
[event log reference](/reference/event-log/).
