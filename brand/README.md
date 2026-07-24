# Brand

The H.O.T-Jarvis mark and how to use it.

## The mark

**The aperture.** A Reuleaux triangle — a curved-triangle of constant width,
where each edge is a circular arc centered on the opposite vertex — with a single
round eye punched from the middle. One shape, one idea: an assistant that is
always attentive. The curved triangle reads as focus and forward motion (apex
up, stable); the open eye reads as perception.

Design discipline, the way the best marks are built:

- **Geometric and exact.** The silhouette is a real Reuleaux triangle
  (circumradius 34, side 58.89 on a 100-unit grid), not a hand-drawn curve, so it
  is reproducible and balanced. The eye is punched with `fill-rule: evenodd`, so
  the glyph is a single path.
- **One idea, one colour.** No gradients required, no second colour, no text in
  the mark. It works in a single ink (`logomark-mono.svg` uses `currentColor`).
- **Legible at 16px.** A solid silhouette with one negative-space cut holds up as
  a favicon, where thin rings collapse. The ring thickens toward the three
  vertices and thins along the edges, giving it an optical, lens-like quality.

The cyan gradient (`#7fe7ff → #2aa8e0`) is the default finish; mint stays a
system colour for "ready / passed" states, not part of the logo itself.

## Files

| File | Use |
|------|-----|
| `logomark.svg` | The glyph on transparent — in-app, alongside text |
| `icon.svg` | Glyph on a dark rounded square — the SVG favicon source |
| `icon-square.svg` | Full-bleed opaque square (no rounding, safe margin) — master for raster app icons |
| `logomark-mono.svg` | Single-colour (`currentColor`) — stamps, print, one-tone contexts |
| `logo-lockup.svg` | Horizontal mark + wordmark — headers, README, docs nav |
| `app-icon-1024.png` | 1024×1024 opaque — the iOS AppIcon master (drop into Xcode) |

## Cross-platform raster set (web + mobile)

`../scripts/generate-icons.mjs` rasterizes the SVGs (via `sharp`) into the icons
every platform needs, written to `landing/` and `docs-site/public/`:

| Output | Purpose |
|--------|---------|
| `favicon-16.png`, `favicon-32.png` | PNG favicons (fallback to the SVG) |
| `apple-touch-icon.png` (180) | iOS / iPadOS home-screen icon |
| `icon-192.png`, `icon-512.png` | PWA / Android "any" icons |
| `icon-maskable-512.png` | PWA maskable (safe-zone padded) |
| `brand/app-icon-1024.png` | native iOS AppIcon master |

Regenerate after changing a source SVG:

```bash frame="terminal"
node scripts/generate-icons.mjs
```

Both sites reference these via `<link rel="apple-touch-icon">`, PNG `rel="icon"`
entries, and a `site.webmanifest`. No `favicon.ico` is shipped — the SVG + PNG
set covers every current browser without the legacy bloat.

## Palette

| Token | Hex | Use |
|-------|-----|-----|
| Ink | `#04070d` | Background |
| Cyan | `#38c6f4` (→ `#7fe7ff` / `#2aa8e0` gradient) | Primary mark, accents |
| Mint | `#56dfb4` | The growth spark, "passed/ready" states |
| Text | `#eaf3fb` / `#8aa0b6` | Wordmark bright / muted |

## Wordmark

`H.O.T` set bold, `-JARVIS` lighter, in Geist / Inter, ~2px tracking. The lockup
SVG references those webfonts; embed them (or outline the text) before using the
lockup where the fonts aren't loaded.

## Don't

- Recolor the mark outside the palette, add shadows/bevels, or rotate it.
- Crowd it — keep clear space of at least the center-node diameter on all sides.
- Stretch the lockup non-proportionally.
