---
title: Lessons and forgetting
description: How Jarvis distils lessons from its own event log, decides which ones still earn their place, and lets you put back anything it dropped.
---

Jarvis re-reads its own event log and keeps short lessons about what worked and
what failed. Those lessons ride along in later prompts, which is what makes the
assistant get sharper the more you use it.

Find them in **reflections** (ctrl+6).

## Why lessons fade

A memory that only ever grows is a memory that gets worse. Old observations stop
being true, the prompt has finite room, and a lesson that was a guess shouldn't
outlive one that has been confirmed a dozen times.

So each lesson carries a score, and the score decays. How fast depends on what
kind of thing it is:

| Kind | Half-life | Why |
|------|-----------|-----|
| `user` | 120 days | how you work changes slowly |
| `skill` | 45 days | code lessons age with the code |
| `general` | 30 days | usually context-bound |
| `provider` | 10 days | a model being slow today says little about next month |

Three things push back against decay: **corroboration** (a later pass
independently reached the same conclusion), **use** (it has actually been
injected into prompts), and a **protection window** so a brand new lesson is
never culled before it has had a chance to prove itself.

## Duplicate lessons become evidence

Reflection is repetitive by nature: run it twice over overlapping activity and it
will reach the same conclusion twice. Storing that twice would be worse than
useless, because it would crowd out everything else.

Instead the newer copy is merged into the older one, and the older one earns a
corroboration. Duplication is treated as evidence, not as noise, which is the
opposite of what a naive dedupe would do.

Two lessons are judged the same in one of two ways:

- **By meaning**, when both have embeddings. This catches paraphrase that shares
  almost no vocabulary — "rhai skills break on string interpolation" and "avoid
  `${}` inside generated code" are the same lesson, and no amount of word
  counting will tell you that.
- **By word overlap**, as the fallback. No embedding model on disk is the normal
  case, not an error, so detection degrades rather than switching off.

The event log names which method was used. This matters more than it sounds:
0.93 word overlap and 0.93 cosine similarity mean very different things, and a
reason line that hid the difference would be misleading.

The meaning threshold is much stricter than the word one, deliberately. Sentence
embeddings put *any* two English sentences about the same broad subject in the
0.7–0.85 range, so a threshold that looks strict for word overlap would happily
merge two distinct lessons that merely share a topic.

Lessons are embedded when they're created, and **Build index** in the memory view
backfills older ones. Until a lesson has a vector, it's compared by words.

## Nothing is deleted

When a lesson fades or gets merged, it is **not** removed. It moves to the
**forgotten** tab, and every drop is recorded in the event log with the reason
that drove it.

That's a deliberate limit on the app's authority. Deciding what matters to you is
a guess, and a decay curve is a crude one. So the drop is reversible: open
**forgotten**, press **Restore**, and the lesson counts as live again.

The forgotten list survives restarts, and a memory wipe clears it along with
everything else — a wipe that left forgotten lessons on disk would break the one
promise it makes.

## Running it yourself

- **Reflect now** runs a pass immediately instead of waiting for the automatic
  trigger.
- **Tidy up** runs the merge-and-fade pass and reports exactly what went, and how
  many lessons it kept.

Auto mode does both of these unattended, within its caps. See
[Auto mode](/how-to/auto-mode/).
