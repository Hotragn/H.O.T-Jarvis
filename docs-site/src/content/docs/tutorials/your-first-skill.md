---
title: Author your first skill
description: Ask H.O.T-Jarvis to build a new skill, watch it write code and a test, and run it — the self-evolving skill library in action.
---

The skill library is the heart of H.O.T-Jarvis: the assistant writes its own new
abilities, tests them, and reuses them. This tutorial takes you through creating
one from a plain request.

## 1. Open the Skills tab

Press <kbd>Ctrl</kbd>+<kbd>2</kbd> (or use the <kbd>Ctrl</kbd>+<kbd>K</kbd>
command palette and jump to **Skills**). You'll see the library — empty at first.

## 2. Ask for a skill

In the authoring box at the top, describe an ability in plain language:

> count the words in the input

Click **Author**. Behind the scenes, the assistant:

1. Writes a small script (`fn run(input)`) **and** a test (`fn test()`).
2. Runs the test in a sandbox.
3. If the test fails, feeds the error back to the model and tries again (up to
   three attempts).

## 3. Read the result

When the test passes, the skill lands in your library with a **✓ passed** badge.
If the model never produces a working version, the skill is saved but **flagged**
and refuses to run — H.O.T-Jarvis never uses a skill it can't prove works.

:::note
On a small local model (like llama3.2) some drafts get flagged — that's the
harness doing its job, not a failure. A larger free model (e.g. Groq's 70B)
one-shots most requests. Either way, nothing untested runs.
:::

## 4. Run it

Select the skill, type an input, and press **Run**. You'll get the output, and
the run is recorded in the [event log](/reference/event-log/).

## 5. See it improve itself

Every save, test, and run is logged. After enough activity, the
[reflection pass](/explanation/hero-features/#2-reflective-reasoning-memory)
distills lessons ("re-test after a failure", "avoid this pattern") that ride
along in future authoring — so the library gets better at making skills over time.

## What just happened

You created a versioned, sandboxed, self-tested ability from one sentence. The
full contract is in the [skill manifest reference](/reference/skill-manifest/).
