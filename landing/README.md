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
- `assets/video/` — where generated clips land (empty until generated).
- [STORY.md](STORY.md) — the narrative design: the feeling arc and which model serves which moment.
- [assets/ASSETS.md](assets/ASSETS.md) — per-slot generation manifest with exact prompts.

## Adding a generated clip (when credits exist)

1. Generate per [assets/ASSETS.md](assets/ASSETS.md) (keyframes first for narrative slots).
2. Encode for web (AV1/WebM + MP4 fallback, sized to display) and drop both at `assets/video/<slot>.{webm,mp4}`.
3. In `index.html`, set `data-has-asset="true"` on that slot. Done — `reveal.js` lazy-loads it into view, keeps the poster until it can play, and still shows the poster under reduced motion.

## Deploy

Any static host. For GitHub Pages, serve this `landing/` directory. No build
required.
