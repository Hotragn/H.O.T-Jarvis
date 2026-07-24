---
title: The four hero features
description: Why H.O.T-Jarvis is built around skills, reasoning-memory, calibrated confidence, and replay — and the research each draws on.
---

Most assistants are a chat window over a model. H.O.T-Jarvis is built around four
capabilities that compound into something you can trust and that improves with use.

## 1. Self-evolving skill library

The assistant authors, tests, versions, and reuses its own skills. The key idea
is that **correctness comes from executing a test, not from trusting the model**:
every skill ships a bundled test, and one that can't pass is flagged and refused.
Capabilities compound across sessions.

*Grounded in:* the Voyager skill-library idea, MUSE-Autoskill, and Reflexion.

## 2. Reflective reasoning-memory

Beyond remembering *facts*, the assistant remembers *how it reasoned* — what it
tried, what worked, what failed. A periodic reflection pass digests the event log
into short lessons that ride along in future prompts, so it improves from its own
experience.

*Grounded in:* ReasoningBank, "Hindsight is 20/20", and Mem0's selective memory.

## 3. Calibrated autonomy

Before an answer stands, the model rates its own probability of being right.
Below a threshold it asks a clarifying question instead of guessing. Verbalized
confidence is imperfectly calibrated — small models are overconfident — so the
app **surfaces** the number (a gauge on the core, a per-message label) rather
than trusting it blindly. It's a trust signal and a safety rail, not a gate.

*Grounded in:* "Agentic Uncertainty Reveals Agentic Overconfidence."

## 4. Replay & undo

Every action is recorded with enough state to reverse it. You can undo chat
turns, note saves, and skill changes, and run a **replay audit** that rebuilds
the conversation from the log and proves it matches live memory. A replay you
can't verify is a story, not a record — so the audit reports drift instead of
hiding it.

*Grounded in:* the determinism-faithfulness / replayable-agent literature.

---

These aren't independent gimmicks. Skills generate events; events feed
reflection; reflection improves skills; confidence gates risky answers; and the
log makes all of it inspectable and reversible. Together they're the product's
identity: an assistant that grows, learns, admits doubt, and never does anything
you can't take back.
