# Landing page

The public front door for H.O.T-Jarvis. A standalone static site — no
framework, no build step — kept entirely separate from the desktop app so it
can never affect the app's bundle size or offline guarantees. Fast load is the
premium signal.

## Preview locally

```bash
cd landing
python -m http.server 8080
# open http://localhost:8080
```

## What's here

- `index.html` — one premium page: live-canvas hero + four feature showcases + close.
- `core.js` — the arc-reactor hero, a vanilla port of the app's ArcCore. Zero assets, pauses when off-screen, static frame under reduced motion.
- `reveal.js` — scroll reveals and lazy, reduced-motion-aware video loading.
- `styles.css` — the instrument design language (dark steel, hairlines, one glow).
- `assets/posters/*.svg` — designed art filling every reserved video slot. These are the current experience *and* the poster / reduced-motion / load fallback.
- `assets/video/` — rendered clips. `skill-born.mp4` (the flagship) is rendered; others land here as they're produced.
- `motion/` — HyperFrames source projects (HTML/CSS/GSAP) that render the clips to MP4, free and locally. See `motion/skill-born/index.html` for the flagship composition.
- [STORY.md](STORY.md) — the narrative design: the feeling arc and which model serves which moment.
- [assets/ASSETS.md](assets/ASSETS.md) — per-slot generation manifest with exact prompts.

## Adding a generated clip (free, via HyperFrames)

Clips are HTML/CSS/GSAP compositions rendered to MP4 with HyperFrames — no paid
service. Full per-slot recipe in [assets/ASSETS.md](assets/ASSETS.md). In short:

1. Author/edit the composition under `motion/<slot>/index.html`.
2. `cd motion/<slot> && npx hyperframes check && npx hyperframes render . -q draft -o ./renders/video.mp4`
   (needs `ffmpeg` on PATH; the render also uses a headless Chromium).
3. Copy `renders/video.mp4` → `assets/video/<slot>.mp4` and set `data-has-asset="true"` on that slot in `index.html`.
   `reveal.js` lazy-loads it into view, keeps the poster until it can play, and still shows the poster under reduced motion.

## Deploy

Any static host. For GitHub Pages, serve this `landing/` directory. No build
required.
