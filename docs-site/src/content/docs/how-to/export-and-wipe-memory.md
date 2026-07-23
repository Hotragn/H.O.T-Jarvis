---
title: Export or wipe your memory
description: Take everything H.O.T-Jarvis knows as one JSON file, or erase it — your data, your control.
---

Everything the assistant remembers lives on your machine. You can take it with
you or erase it at any time.

## Export everything

Open the **Memory** tab and click **Export JSON**. You get a single file
containing:

- **Messages** — the full conversation history.
- **Facts** — the key-value profile the assistant keeps about you.
- **Insights** — the lessons distilled by the reflection pass.
- **Events** — the complete action log.
- **Notes** — every local note.

That's the entirety of what H.O.T-Jarvis knows, in a portable, human-readable
format.

## Wipe it

Click **Wipe…** and confirm. This erases remembered messages, facts, insights,
and the event log. **Notes are kept** — they're documents you authored, not
memory, so deleting them stays a separate, explicit action.

:::caution
A wipe cannot be undone. Export first if you might want the data back.
:::

## Where the data lives

By default, in your OS application-data directory. Override the location with the
`JARVIS_DATA_DIR` environment variable (see
[Configuration](/reference/configuration/)). Because it's a plain folder with a
SQLite database, JSONL event log, and note files, you can also back it up
yourself.
