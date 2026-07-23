# Asset manifest — rendered locally with HyperFrames (free, HTML→MP4)

Pivot (2026-07-07): dropped Higgsfield (its 7-Day Unlimited pass covers the web
app but not MCP/API generation, which bills 0-balance workspace credits). The
assets are now authored as **HTML/CSS/GSAP compositions and rendered to MP4 with
HyperFrames** — free, deterministic, on this machine (Playwright Chromium +
ffmpeg). Sources live under `../motion/<slot>/`; render with
`npx hyperframes render` and copy the MP4 into `video/`.

Every slot is wired in the page with a designed SVG poster + lazy/reduced-motion
plumbing. To light one up: render, save `<slot>.mp4` to `assets/video/`, set
`data-has-asset="true"` on that slot in `index.html`.

## Status

All five slots are now rendered and wired (each a seamless 8s loop except the
12s flagship). Total video weight ~2.5 MB, lazy-loaded per slot.

| Slot | Feeling | Source | Status |
|------|---------|--------|--------|
| 2 flagship — a skill is born | aliveness + trust | `../motion/skill-born/` | ✅ 1080p, 12s, 1.16 MB |
| 1 hero ambient | calm power | `../motion/hero-ambient/` | ✅ 1080p, 8s loop, 531 KB |
| 3 memory | depth | `../motion/memory/` | ✅ 1080p, 8s loop, 284 KB |
| 4 confidence | trust | `../motion/confidence/` | ✅ 1080p, 8s loop, 422 KB |
| 5 undo | precision | `../motion/undo/` | ✅ 1080p, 8s loop, 133 KB (authored, not a capture) |

## Render workflow (per slot)
1. `npx hyperframes init landing/motion/<slot> --non-interactive --example=blank`
2. Author `index.html`: a paused GSAP timeline on `window.__timelines["main"]`,
   canvas/DOM driven by timeline time (seek-safe, deterministic). Vendor GSAP
   locally (no CDN). Use `monospace`/`Roboto` (renderer-bundled fonts) and
   transforms only (`x/y/scale/opacity`), never layout props like `top`.
3. `npx hyperframes check` → fix lint (motion/fonts/contrast) → `render`.
4. Copy `renders/video.mp4` → `assets/video/<slot>.mp4`; flip `data-has-asset`.
5. (Optional) smaller WebM/AV1: `ffmpeg -i <slot>.mp4 -c:v libaom-av1 -crf 34 -b:v 0 -an <slot>.webm`
   and set `data-webm="true"` on the slot. MP4 alone plays everywhere.

Keep each clip within its budget (hero/flagship ≤ ~2 MB, loops ≤ ~1 MB).

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
