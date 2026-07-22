# Asset manifest — generate these with your 7-Day Unlimited

Status: **0 assets generated.** Verified 2026-07-07 that the 7-Day Unlimited
pass covers the **Higgsfield web app** but NOT MCP/API generation — the MCP
path bills against workspace credits (0 here), so a keyframe preflighted at 2
credits and submits failed. So generate these **in the web app** (where your
pass applies, at no extra cost), then drop the files in and flip the slot on.

Every slot is already wired in the page with a designed SVG poster and
lazy/reduced-motion plumbing. To light one up: save the clip to the listed
path and set `data-has-asset="true"` on that slot in `index.html`.

## Models mapped to your actual unlimited lineup
(Veo is not in your pass, so the ambient loops use **Kling 3.0 1080p**, which
is higher-res anyway. Keyframes use **Nano Banana Pro** — 2K, precise.)

| Slot | Model (in your unlimited) | Output |
|------|---------------------------|--------|
| keyframes | Nano Banana Pro | 2K stills, 16:9 |
| 1 hero ambient | Kling 3.0 (1080p, 10s) | seamless loop |
| 2 flagship | Seedance 2.0 (720p, 15s) | start+end keyframes |
| 3 memory | Kling 3.0 (1080p, 10s) | loop |
| 4 confidence | Kling 3.0 (1080p, 10s) | loop |
| 5 undo restyle | Wan 2.7 (1080p, 10s) | restyle a real capture |

## Web-app workflow
1. In the Higgsfield web app, generate each slot with the model + prompt below.
   For slots 1–4 generate the two **keyframes first** (Nano Banana Pro), then
   feed them as start/end frames to the video model — far more control.
2. Download the MP4. Name it `<slot>.mp4` (e.g. `skill-born.mp4`) and drop it in
   `assets/video/`.
3. In `index.html`, set `data-has-asset="true"` on that slot.
4. (Optional, later) once `ffmpeg` is installed, also make a WebM/AV1:
   `ffmpeg -i skill-born.mp4 -c:v libaom-av1 -crf 34 -b:v 0 -an skill-born.webm`
   MP4 alone plays in every browser; WebM is just a smaller-bytes optimization.
   Keep each loop under ~2.5 MB transferred.

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
