# Brand

The H.O.T-Jarvis mark and how to use it.

## The mark

An **aperture core**: three arc segments around a center node, left open with
gaps — an assistant that's listening and focused, not a closed box. One segment
is mint and throws a small **spark**: the assistant growing a new skill. It's the
same instrument language as the app's HUD and the landing hero, distilled to a
glyph that reads at 16px.

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
