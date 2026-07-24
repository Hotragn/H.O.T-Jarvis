---
title: Local-first and free
description: The constitution behind H.O.T-Jarvis — why it costs nothing, keeps your data on your machine, and what that means for its design.
---

H.O.T-Jarvis has a constitution, and two clauses shape almost every design
decision: **free forever at runtime** and **local by default**.

## Free forever

The assistant must cost you nothing to run. Inference goes through a
model-agnostic router with this priority: local models via Ollama first
(unlimited, private), then free cloud tiers (Groq, OpenRouter `:free`) only as a
fallback. **No paid API is ever required**, and the app never nudges you toward
paying. The router respects free-tier limits in code — caching, backoff — so the
project never pressures anyone to upgrade.

This is why, for example, the router exempts the local model from rate-limit
penalties, and why cache hits are labeled rather than silently served: the
cheapest, most private path stays the default and the behavior stays honest.

## Local by default

Your conversations, skills, insights, and event log live in a folder on your
machine. Memory persists across restarts and upgrades, and it's fully
**exportable, importable, and wipeable** — your data, your control. Nothing has
to leave the room. The one exception is explicit: if you add a free cloud key,
requests to that provider necessarily leave your device — the app tells you which
provider answered every time.

## What this rules out

- No telemetry, no accounts, no lock-in.
- No feature that only works behind a paywall.
- No silent data exfiltration — side-effecting actions are gated and logged.

## What it enables

A tool you can run forever without a bill, inspect completely, and trust with
private context — because the private, free path isn't a downgrade, it's the
default. Everything else in these docs is downstream of these two clauses.
