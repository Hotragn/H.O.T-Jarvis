---
title: Author, test, and run skills
description: Create skills by hand or by asking the assistant, re-test them, run them, and roll them back.
---

Skills are self-contained abilities: a manifest, a Rhai script (`fn run(input)`),
and a bundled test (`fn test()`). This guide covers the everyday operations.

## Ask the assistant to build one

In the **Skills** tab, type a request in the authoring box and click **Author**.
The model writes the code and test; the engine proves it. See
[Author your first skill](/tutorials/your-first-skill/) for the full walkthrough.

## Write one by hand

Click **+ new** and fill in the two editors:

```rust title="skill.rhai"
fn run(input) {
  input.to_upper()
}
```

```rust title="test.rhai"
fn test() {
  run("hi") == "HI"
}
```

Click **Save & test**. The bundled test runs immediately; a passing skill is
usable, a failing one is flagged.

## Re-test after changes

Select a skill and click **Re-run test**. Status is re-evaluated and persisted —
useful after editing, or to confirm a flagged skill now works.

## Run a skill

Select a passing skill, enter an input, and press **Run**. Flagged skills refuse
to run until their test passes.

## Roll a skill back

Updating a skill bumps its version and archives the previous source. From the
[event log](/reference/event-log/) you can **undo** a skill change: the previous
version is restored as a *new* version (a revert, never a rewrite), preserving
history. A first version with nothing to revert to is deleted.

## What's guaranteed

- Skills run in a **sandbox** — no filesystem, no network, only language
  built-ins — under a hard operation cap, so a runaway loop terminates.
- A skill is **never used unless its test passes**.

Contract details: [Skill manifest reference](/reference/skill-manifest/).
