# Landing page — narrative design

The story this page tells, and the generated moments that serve it. Written
before any asset exists so every clip has a job. If a slot can't name the
feeling it carries, it doesn't get generated.

## Why a landing page (not in-app video)

H.O.T-Jarvis the app is local-first, offline-capable, 60fps, free forever —
its constitution says "premium means powerful, not bloated." Cinematic video
belongs on the public front door, not inside the assistant. This page is a
standalone static site (no framework, no build) so it can never add a byte to
the app or touch its offline guarantees. It loads instantly; that *is* the
premium signal.

## The feeling arc

Someone lands knowing nothing. In three seconds they should feel **calm
power** — this thing is alive, precise, and already running. Then, as they
scroll, the feeling should travel from *capability* to *trust*:

1. **It's alive** (hero) — a living core, breathing. Restraint, not spectacle.
2. **It grows** — skills that write and test themselves. Wonder.
3. **It remembers how it reasons** — learns from its own outcomes. Depth.
4. **It admits doubt** — a confidence gauge that asks instead of guessing. Trust.
5. **You can undo anything** — a time machine over every action. Safety.
6. **It's yours, and it's free** — local, private, undoable. Ownership, warmth.

The motion vocabulary tightens as you descend: ambient and volumetric at the
hero (calm presence), exact and instrumented at the features (precision when it
acts). That mirrors the product itself — quiet until it does something, then
surgical.

## Model mapping (matched to the job, not one default)

Only one flagship sequence. Everything else is ambient or restyled-real. No
recurring character is needed (no face), so **Kling is intentionally unused**.

Model choices reflect what's actually in the owner's **7-Day Unlimited** pass
(confirmed 2026-07-07). Veo is *not* in it, so the atmospheric loops use
**Kling 3.0 at 1080p** — higher-res than Veo would have given us here anyway.

| # | Slot | Feeling | Model | Why this model | Spec |
|---|------|---------|-------|----------------|------|
| — | Hero core | calm power | **none — live `<canvas>`** | Zero asset, zero bytes, genuinely premium and interactive. The real arc-reactor. | runs now |
| 1 | Hero ambient backdrop | calm power | **Kling 3.0** (1080p) | Best atmospheric loop in the unlimited set | 16:9, ~10s seamless, muted, behind the canvas |
| 2 | Flagship: a skill is born | aliveness + earned trust | **Seedance 2.0** (720p, 15s) | Highest-fidelity multi-shot; this is *the* cinematic moment | 16:9, 15s, start/end keyframes first, `genre: epic` |
| 3 | Reasoning-memory ambient | depth | **Kling 3.0** (1080p) | Atmospheric: drifting light forming constellations | 16:9, ~10s loop, muted |
| 4 | Confidence gauge motion | trust | **Kling 3.0** (1080p) | Atmospheric arc filling with light | 16:9, ~10s loop, muted |
| 5 | Replay & undo — restyled real UI | precision | **Wan 2.7** (1080p) | Restyles an actual screen recording of the undo timeline into the holographic aesthetic — authentic but clearly stylized | from real capture |
| — | Keyframes (slots 1–4) | — | **Nano Banana Pro** (2K) | Precise composition and control for start/end frames | 16:9 stills |

Generate keyframes first, then feed them as start/end frames to the video
model — far more control than a freeform prompt. Never ship a
watermarked/free-tier render on this page.

## Keyframe-first rule

For narrative slots (1, 2), generate start + end **images** first and pass them
as `start_image` / `end_image` references to the video model. A single freeform
prompt gives far less control over how a shot begins and ends. Exact prompts
live in [assets/ASSETS.md](assets/ASSETS.md).

## Authenticity

Every generated asset is stylistic brand motion — abstract holographic
visuals, not fabricated product footage or testimonials. The only "real
product" shown is a genuine screen recording (slot 5), and even that is openly
restyled. No invented customers, endorsements, or claims. This keeps the page
clear of the rules governing real testimonials, and keeps it honest.

## Performance contract

- WebM/AV1 first `<source>`, MP4 fallback second, poster always set.
- Below-the-fold video is lazy-loaded via IntersectionObserver; nothing heavy
  auto-plays until in view.
- `prefers-reduced-motion` → the designed poster, never the video.
- Every reserved slot ships a real designed SVG poster now, so the page is
  fully premium before a single clip exists, and the poster is also the
  reduced-motion and load fallback.
