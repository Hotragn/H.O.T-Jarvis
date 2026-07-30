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
- **15 minutes** minimum between cycles, so the heartbeat can't become a busy loop
- **2 minutes** of you being idle before an unattended cycle starts

Caps are checked *before* an action runs, not detected afterwards, so a limit is
never exceeded. Every cycle is logged with its usage and the reason it stopped.

## Dry run first

The panel always shows what the next cycle *would* do and why, before it does
anything — including the actions it's deferring to you. A cycle you can't
preview isn't one you can trust.

## The heartbeat

Once armed, a background loop wakes every few seconds and asks one question:
may a cycle run right now? If any gate says no, nothing happens and the loop
goes back to sleep.

Waking often and doing almost nothing is deliberate. The alternative is a loop
that sleeps for 15 minutes at a time, which makes disarming look broken: you'd
press the button and watch a cycle start anyway. The poll interval is a quarter
of the cycle gap, clamped to 5 to 60 seconds, so pressing stop feels immediate
while the actual work stays rate-limited.

The loop never decides anything itself. The interval, every gate, and the plan
all come from the same tested policy code that the manual **Run one cycle**
button uses. There is no second, looser path into autonomous work.

### It waits while you're using the app

Reflection and indexing both make model calls. Doing that while you're
mid-conversation makes Jarvis feel slow for no visible reason, so a cycle only
starts after you've been idle for a couple of minutes. Deferring costs nothing:
none of this work is urgent.

If several gates are closed at once, the panel reports the *hardest* one. Being
told "you stopped it" is more useful than "wait 90 seconds" when the STOP file
is the real reason.

### Only one cycle at a time

The cycle gap is recorded when a cycle *finishes*, so while one is mid-flight the
rate limit still reads as satisfied. Without something else, pressing **Run one
cycle** during a heartbeat cycle would start a second one that passed every gate
and, between them, spent twice the caps.

A latch prevents that: whichever started first holds it, and the other is told a
cycle is already running. It releases when the cycle ends, including if the cycle
fails partway.

### You can see it beating

The panel shows what the heartbeat did on its last wake-up: held (and why),
checked with nothing to do, or ran with a count of actions. A background loop
you can't observe is indistinguishable from one that's broken.

You can still trigger a cycle yourself from the panel at any time.
