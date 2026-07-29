# App Store package — name, positioning, and listing copy

Everything App Store Connect asks for, decided in advance so the Mac session is
mechanical. Written from a marketing-strategy view: who this is for, why they'd
pick it, and the exact copy to paste.

## The audience, honestly

Three groups actually download an app like this:

1. **Privacy-first tinkerers** — people who run Ollama, self-host, use Obsidian,
   read Hacker News. They convert on "nothing leaves your machine" and "no
   subscription". They are also the reviewers and word-of-mouth engine.
2. **AI-curious pragmatists** — tried ChatGPT, dislike paying $20/month for
   light use. They convert on "free forever" but need the first-run to work
   without knowing what an "endpoint" is. The app must degrade gracefully and
   the onboarding copy carries this group.
3. **Students / builders** — want an assistant they can extend. The skill
   engine is the hook; "it writes and tests its own abilities" is the line that
   makes them screenshot it.

The wedge is that every big assistant is a subscription with your data in the
cloud. This is the anti-product: local, inspectable, undoable, free. Marketing
should never blur that — it is the only claim the incumbents can't copy.

## Name

App Store display names are limited to 30 characters and searched heavily.

- **Primary: `H.O.T Jarvis`** — keeps brand continuity with the repo, landing
  page, and docs. "Jarvis" carries built-in search demand (people literally
  search "jarvis ai"). Note: "JARVIS" as a Marvel character is MCA/Disney IP;
  a plain word "Jarvis" used for an assistant persona has many precedents on
  the store, but if Review or legal caution wins, use the fallback.
- **Fallback (clean-IP): `HOT Assistant — Local AI`** or rebrand wholesale to
  **`Reactor: Local AI Assistant`** (the logo is literally a reactor; strong
  mark-name fit and zero IP contact).

**Subtitle (30 chars max):** `Local, private, free AI` (23 chars).

## One-paragraph pitch (used everywhere)

> The AI assistant that's actually yours. H.O.T Jarvis thinks on your own
> machine, learns from its own mistakes, tells you when it's unsure, and lets
> you undo anything it does. No account. No subscription. Nothing leaves your
> network.

## App Store description (paste-ready)

```
The assistant that grows its own skills — private, free, and yours.

H.O.T Jarvis is a different kind of AI assistant. Instead of renting
intelligence from a cloud, it thinks on hardware you own: pair it with your
own computer over your home network, and every conversation stays inside
your four walls.

WHAT MAKES IT DIFFERENT

• It grows its own skills. Ask for a new ability and Jarvis writes it, tests
  it, and only keeps it when the test passes. Skills that can't prove
  themselves refuse to run.

• It remembers how it reasons. Jarvis re-reads its own activity log and keeps
  short lessons about what worked. Lessons that stop being true fade out —
  it forgets on purpose, the way memory should.

• It tells you when it's unsure. Every answer carries the assistant's own
  confidence, and the app tracks whether that confidence was honest, so you
  learn exactly how much to trust it.

• You can undo anything. Every action is recorded with enough state to
  reverse it. Nothing is permanent unless you want it to be.

PRIVATE BY ARCHITECTURE, NOT BY POLICY

Your conversations, notes, and the assistant's memory live on your device.
Pair with your own computer (running the free desktop app) and even the AI's
thinking happens on your own hardware. Optional free cloud fallback is
clearly labelled and off by default.

FREE MEANS FREE

No account. No subscription. No trial that expires. The project is open
source (Apache-2.0) and built in the open.

H.O.T Jarvis for iPhone is the companion to the free desktop app for
Windows and macOS.
```

**Promotional text (170 chars, editable without review):**
`Your AI, on your hardware. Grows its own skills, admits when it's unsure, and lets you undo anything. No account, no subscription, nothing leaves your network.`

**Keywords (100 chars, comma-separated, no spaces wasted):**
`local ai,private ai,offline ai,assistant,ollama,jarvis,chat,no subscription,self hosted,voice`

**Category:** Productivity (primary), Utilities (secondary).
**Age rating:** 4+ (the model runs locally/on your own server; nothing user-generated is shared).

## Screenshots that sell (capture on the Mac, 6.7" + 6.5")

1. Chat with the arc-reactor core mid-answer — caption "Thinks on your hardware".
2. Skill library with a passing test — "It writes and tests its own abilities".
3. Reflections view with lessons + calibration panel — "It knows when to doubt itself".
4. Timeline with an undo — "Nothing is permanent unless you want it to be".
5. Settings view pointing at a desktop — "Pair with your own computer".

Dark theme only in screenshots; it's the brand.

## Launch audience plan (zero budget)

- **Day 0:** Show HN post ("H.O.T Jarvis — a local-first AI assistant that
  grows its own skills"), r/LocalLLaMA and r/selfhosted posts. These
  communities are audience #1 and tolerate honest, technical writeups only —
  lead with the forgetting/calibration engineering, not marketing language.
- **Landing page** already live; add the App Store badge when approved.
- **The README is the ad** for audience #3 — it already shows the architecture.
- **One honest demo video** (HyperFrames pipeline already exists) showing a
  skill being born; post as the HN comment, not the submission.
- No paid ads, no growth hacks; the free-forever promise is the marketing.

## App Review notes (paste into the Review Notes field)

> H.O.T Jarvis is the iOS companion to an open-source desktop assistant
> (github.com/Hotragn/H.O.T-Jarvis). Inference runs either on the user's own
> computer over their local network (user-configured, Bonjour-free, plain
> HTTP endpoint the user types in) or via an optional user-supplied API key.
> The app's "skills" are short scripts in Rhai, a sandboxed embedded
> interpreter, comparable to spreadsheet formulas or Shortcuts: they are
> authored locally at the user's request, cannot modify the app, and are
> never downloaded from a server. The app functions on first launch with no
> external services (memory, notes, and settings work offline).
