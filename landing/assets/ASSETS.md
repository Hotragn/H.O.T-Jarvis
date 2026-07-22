# Asset manifest — one command per slot

Status as of this PR: **0 assets generated** (Higgsfield balance is 0 credits
on a Plus plan). Every slot below is wired in the page with a designed SVG
poster and lazy/reduced-motion plumbing already working. When credits exist,
generate in this order (keyframes first for narrative slots), drop the encoded
files at the listed paths, and flip `data-has-asset="true"` on the matching
slot in `index.html`.

Encode every clip for web before committing:
`ffmpeg -i raw.mp4 -c:v libaom-av1 -crf 34 -b:v 0 -an slot.webm`
and an MP4 fallback: `ffmpeg -i raw.mp4 -c:v libx264 -crf 24 -movflags +faststart -an slot.mp4`.
Target the actual display size (hero ≤1920w, feature loops ≤1280w). Keep each
loop under ~2.5 MB transferred.

---

## Slot 1 — Hero ambient backdrop  → `assets/video/hero-ambient.{webm,mp4}`
- **Feeling:** calm power. Sits *behind* the live canvas core at low opacity.
- **Model:** Veo 3.1 (`veo3`, atmospheric). 21:9, ~8s, seamless, muted.
- **Prompt:** "A vast, near-black volumetric space. Faint teal-cyan particulate
  drifts slowly like deep-ocean bioluminescence. Subtle horizontal light
  striations, extremely slow parallax, no subject, no text. Calm, premium,
  cinematic, high dynamic range darkness."

## Slot 2 — Flagship: a skill is born  → `assets/video/skill-born.{webm,mp4}`
- **Feeling:** aliveness + earned trust. The one flagship sequence.
- **Keyframes first** (image model, 16:9):
  - start: "A glowing cyan arc-reactor core alone in dark space, concentric
    instrument rings, single point of light at center. Restrained, cinematic."
  - end: "The same core, now with a crisp holographic card locked in beside it
    stamped with a green check, faint connective light traces between them.
    Sense of something proven and complete."
- **Model:** Seedance 2.0 (`seedance_2_0`), 16:9, 12–15s, `mode: std`,
  `resolution: 1080p`, `genre: epic`, start_image + end_image = the keyframes.
- **Motion:** core pulses, emits a card, the card self-tests (scanline), a
  check resolves, everything settles. No literal UI text.

## Slot 3 — Reasoning-memory ambient  → `assets/video/memory.{webm,mp4}`
- **Feeling:** depth. "It learns from its own past."
- **Model:** Veo 3.1, 16:9, ~6s loop, muted.
- **Prompt:** "Scattered points of soft light in dark space slowly drawing
  faint lines between themselves, forming and dissolving constellations.
  Contemplative, deep, teal-cyan on near-black."

## Slot 4 — Confidence gauge motion  → `assets/video/confidence.{webm,mp4}`
- **Feeling:** trust. "It tells you when it's unsure."
- **Model:** Veo 3.1, 16:9, ~5s loop, muted.
- **Prompt:** "A single thin arc of light sweeping to fill a circular gauge,
  hesitating near the end, settling amber then resolving to teal. Precise,
  instrument-like, dark background."

## Slot 5 — Replay & undo (restyled real UI)  → `assets/video/undo.{webm,mp4}`
- **Feeling:** precision, safety. Authentic-but-stylized.
- **Source:** a real screen recording of the app's EVENTS timeline undoing an
  action (capture with the app running).
- **Model:** WAN 2.6 (restyle the capture into the holographic aesthetic —
  do not generate from nothing). Keep it clearly stylized.

---

### Generation is credit-gated
When ready, top up credits (your call), then for each slot call
`mcp__higgsfield__generate_image` for keyframes and
`mcp__higgsfield__generate_video` with the model + params above, poll
`job_status`, download, encode, and place. Draft on MiniMax first to judge
composition cheaply; finish the keepers on the model listed.
