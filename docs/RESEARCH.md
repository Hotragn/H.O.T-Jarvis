# Research scan → feature proposals

The brief (§ research-to-feature loop) asks for periodic scans of recent
agent research for capabilities relevant to the hero features, proposed as
issues with citations, implemented only after they're in the roadmap, and always
original rather than copied.

This is the log of those scans. Each entry states what the paper found, what it
means for this codebase specifically, **what we already do that the paper
validates**, and what is actually missing. The last part matters: a scan that
only produces a wishlist is a way of avoiding the question of whether the current
design is already right.

---

## Scan 1 — 2026-07-31

Three findings, one per hero feature that has published work bearing on it.

### 1. Consolidated memory degrades below no-memory baseline

**Source:** Zhang, Lin, Wu, Sun, Li, Li, Peng — *Useful Memories Become Faulty
When Continuously Updated by LLMs*, arXiv:2605.12978 (UIUC / Tsinghua, May 2026).
<https://arxiv.org/abs/2605.12978>

**What they found.** Agents that consolidate raw trajectories into reusable
text lessons show an inverted-U: utility rises, then degrades, and ends up
*below* the no-memory baseline. GPT-5.4 failed 54% of ARC-AGI problems it had
previously solved without any memory at all. Episodic-only retention matched or
doubled the accuracy of forced-consolidation systems. Different update schedules
produced qualitatively different memories from identical trajectories. Their
conclusion: treat raw episodes as primary evidence rather than intermediate
artifacts, and gate consolidation explicitly instead of updating after every
interaction.

**What this validates in our design.** More than expected:

- The event log is **append-only**. Episodes are primary evidence and are never
  rewritten by a reflection pass, which is the paper's central recommendation.
- Reflection writes to a **separate** insights store. It cannot corrupt the log
  it read from.
- Merging duplicates **credits a corroboration** rather than rewriting either
  lesson's text. The paper's compounding-error mechanism is "each update rewrites
  the products of earlier updates" — we never rewrite a lesson at all.
- Reflection v2 made forgetting a soft delete, so even a dropped lesson is still
  recoverable evidence.

**What is actually missing.** We cannot *detect* the inverted-U. Nothing measures
whether prompts carrying lessons do better than prompts without them, so if our
lessons started hurting, the app would have no way to notice — which is exactly
the silent failure the paper describes.

**Proposal — lesson utility measurement.** We already have the two halves needed
and have never connected them: `chat_send` logs which insight ids rode along in a
prompt, and `rate_message` logs whether the answer was graded helpful. Joining
those over the event log gives a helpfulness rate per lesson, and a rate for
turns that carried no lessons at all — a baseline. Surface the comparison in the
reflections view, and demote lessons whose presence correlates with worse
outcomes rather than waiting for them to decay.

This is a natural extension of the existing calibration work (`core::calibration`
already rebuilds statistics from the log) and it stays original: the paper offers
no algorithm here, only the diagnosis. Note the honest caveat up front — grading
is sparse and voluntary, so this needs a minimum-evidence gate before it acts,
in the same spirit as the forgetting protection window.

---

### 2. Calibrated confidence does not produce risk-sensitive decisions

**Source:** *Are LLM Decisions Faithful to Verbal Confidence?*, arXiv:2601.07767
(January 2026). <https://arxiv.org/abs/2601.07767>

**What they found.** Models can state their uncertainty reasonably well and then
fail to act on it. Their RiskEval framework varies the penalty for being wrong;
models adjusted neither their stated confidence nor their willingness to abstain.
Even when extreme penalties made frequent abstention mathematically optimal,
models almost never abstained, and utility collapsed. Calibrated confidence
scores alone don't produce trustworthy behaviour.

**What this validates in our design.** Confidence v2 already does the thing the
paper says models can't do for themselves: the decision to clarify instead of
answering is enforced *outside* the model, against a `debias()`-adjusted number
derived from measured calibration. We never ask the model to decide whether it
should abstain.

**What is actually missing.** Our ask threshold is a single constant. The paper's
finding is specifically about *cost sensitivity*, and we do have actions of
sharply different cost: answering in chat is cheap to get wrong, while writing a
note, authoring a skill, or running one is not.

**Proposal — cost-weighted confidence gates.** Give the autonomy `ActionKind`
clearances a required-confidence floor that scales with how hard the action is to
undo, and check adjusted confidence against *that* rather than one global
threshold. This connects two subsystems that currently don't talk: the confidence
machinery knows how reliable a given answer is, and the autonomy classifier
already knows how consequential each action is. Neither currently informs the
other.

---

### 3. Self-authored skill libraries drift silently

**Source:** *Library Drift: Diagnosing and Fixing a Silent Failure Mode in
Self-Evolving LLM Skill Libraries*, arXiv:2605.19576 (May 2026).
<https://arxiv.org/abs/2605.19576>

**What they found.** LLM-authored skills delivered **+0.0pp** over a no-skill
baseline where human-curated skills delivered **+16.2pp**. Libraries accumulate
artifacts without quality gates, retrieval dilutes, stale skills get injected,
and performance drifts below baseline with no error signal. Their fix pairs
outcome-driven retirement (only after enough evidence — they use 100 trials) with
a hard cap on active library size. They also found that **harsh** retirement is
worse than none, dropping performance below baseline. A companion survey they cite
reports lifecycle management is "largely neglected" across 20+ such systems.

**What this validates in our design.** Every skill ships a test, and a skill whose
test doesn't pass is flagged and refused — a quality gate at authoring time that
the survey says most systems lack. Skills are versioned with revert-style
rollback. And the evidence-before-acting principle their ablation vindicates is
already how insight forgetting works: a protection window, corroboration, and a
score floor rather than an aggressive cull.

**What is actually missing.** A passing test proves a skill *runs*, not that it
*helps*. Nothing tracks whether a skill's use correlates with good outcomes,
there's no cap on library size, and nothing retires a skill that has stopped
earning its place.

**Proposal — extend the forgetting model to skills.** `core::forgetting` is
already a tested, general scoring engine: decay, corroboration, use counts, a
protection window, a capacity cap, and a logged reason per drop. Skills need
almost exactly that, with run outcomes in place of corroborations. Reuse the
module rather than writing a second one, and keep the paper's warning about harsh
retirement as the reason the protection window and evidence minimum are not
optional. Retirement should mean deactivated-and-restorable, matching the soft
delete Reflection v2 introduced, never deleted.

---

## Deliberately not proposed

Several of the strongest results in this area need things this project has ruled
out, and recording that is more useful than a longer wishlist:

- **Reinforcement-learning approaches** (`MemRL`, `Skill-R1`) need training
  infrastructure and, realistically, paid compute. Free-forever rules them out.
- **Multi-agent memory coordination** (`MemMA`, `MAGMA`) multiplies model calls
  per interaction, which on a free tier means rate limits instead of features.
- **Latent / in-weight skills** (`LatentSkill`) require weight updates. The whole
  point of a text skill library on a frozen model is that it works without them.

## Status

These three are proposals, not commitments. They're on the roadmap as `ready`
under M1/M3 so they go through the same review as anything else. Nothing here is
implemented yet, and nothing should be copied from the papers — the citations
exist so the *problem* is traceable, while the implementations stay ours.
