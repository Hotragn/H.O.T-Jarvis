---
title: Auto mode (autonomous maintenance)
description: Let Jarvis look after its own memory on a schedule — with a deny-by-default allowlist, resource caps, and a kill switch that always wins.
---

Auto mode lets Jarvis do its own housekeeping without being asked. The
interesting part isn't the loop, it's the **guardrails** — an autonomous
assistant is only worth having if you can predict and stop it.

Find it in **Settings → auto mode**.

## What it will do unattended

Only bounded self-maintenance, all of it either read-only or reversible:

| Action | Why it's safe |
|--------|---------------|
| Replay audit | read-only; checks the log still reproduces reality |
| Test skills | read-only; re-runs each skill's own bundled test |
| Index memory | purely additive; makes old messages searchable |
| Reflect | additive, and undoable from the timeline |
| Tidy lessons | logged with a reason per drop, and undoable |

## What it will never do on its own

- **Needs your approval** (planned and shown, never performed): writing notes,
  authoring a skill, running a skill.
- **Forbidden outright**, approval or not: wiping memory, deleting notes,
  changing settings, anything touching the network or files outside its sandbox.

The line is drawn at *creating or running content*, not at "is it reversible".
Skill authoring is sandboxed and undoable, but a loop that writes and executes
code with nobody asking is a different thing from you requesting a skill.

New capabilities are **Forbidden by default**: nothing becomes automatic until
someone deliberately classifies it. Forgetting to think about safety produces a
refusal, not an incident.

## Stopping it

Three independent ways, and the strongest one works even if the app is wedged:

1. **The STOP file** — the panel shows its exact path. Create it by hand and the
   loop halts immediately. This overrides *everything*, including "armed": an
   emergency brake that can be overridden isn't one.
2. **`JARVIS_AUTONOMY=off`** in the environment. Anything that isn't clearly
   on (`on`, `1`, `true`, `yes`, `enabled`) halts the loop — a misspelling
   should never enable autonomy.
3. **Disarm** in the panel.

The stop file is also re-checked **between every action**, so a cycle already in
flight can be interrupted.

## Resource caps

Deliberately conservative, and configurable:

- **3 actions** per cycle
- **6 tool calls** per cycle (the thing that actually costs time and free-tier limit)
- **120 seconds** wall clock
- **15 minutes** minimum between cycles, so a heartbeat can't become a busy loop

Caps are checked *before* an action runs, not detected afterwards, so a limit is
never exceeded. Every cycle is logged with its usage and the reason it stopped.

## Dry run first

The panel always shows what the next cycle *would* do and why, before it does
anything — including the actions it's deferring to you. A cycle you can't
preview isn't one you can trust.

Today a cycle is **triggered by you** from the panel. An unattended background
heartbeat is the next step; the policy engine and every gate it needs are
already in place and tested.
