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

## Per-slot compositions

All five are authored HyperFrames compositions (canvas 2D + a paused, seek-safe
GSAP timeline) rendered to MP4 — no external model, no capture. Sources live at
`../motion/<slot>/index.html`.

### Slot 1 — Hero ambient backdrop → `video/hero-ambient.mp4`
Feeling: calm power. Sits behind the live canvas core at low opacity. Drifting
cyan/mint particulate on periodic orbits + faint shifting striations over a dark
volumetric field. 1080p, 8s seamless loop.

### Slot 2 — Flagship: a skill is born → `video/skill-born.mp4`
Feeling: aliveness + earned trust. The arc-reactor core wakes, emits a
holographic skill card, a scanline self-tests it, a green "TEST PASSED" resolves,
then everything settles. 1080p, 12s.

### Slot 3 — Reasoning-memory → `video/memory.mp4`
Feeling: depth. Nodes of light draw faint constellations that form and dissolve
on a periodic cycle. 1080p, 8s seamless loop.

### Slot 4 — Confidence gauge → `video/confidence.mp4`
Feeling: trust. A filled 270° gauge ("78") with a slow radar sweep and a
breathing glow. 1080p, 8s seamless loop.

### Slot 5 — Replay & undo → `video/undo.mp4`
Feeling: precision, safety. A timeline spine with a travelling light pulse and a
rotating revert arc on the amber "undo" step. Authored (not a screen capture).
1080p, 8s seamless loop.

## Seamless-loop rule
Every loop's motion is a pure periodic function of timeline time over the clip
duration, so frame(0) == frame(D) and `<video loop>` repeats without a visible
cut. Verify with `hyperframes check` (its anti-static test also confirms the
timeline actually advances under seek).
