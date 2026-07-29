# Deploying the docs

The site is a static Astro build (`npm run build` → `dist/`). Two env vars drive
where it can live:

- `DOCS_SITE` — the origin for canonical URLs, sitemap, and OG images.
- `DOCS_BASE` — the path prefix every asset and link is built under.

Both have defaults for the live GitHub Pages home, so a plain `npm run build`
produces the Pages build.

## GitHub Pages (live, free, default)

The docs ship as part of the repo's existing Pages site, under the `/docs`
subpath, alongside the landing page at the root. This is wired in
`.github/workflows/deploy-pages.yml`: on every push to `main` that touches
`docs-site/`, it builds the docs with `DOCS_BASE=/H.O.T-Jarvis/docs`, drops the
output into `_site/docs`, and deploys the whole site.

- Landing: <https://hotragn.github.io/H.O.T-Jarvis/>
- Docs: <https://hotragn.github.io/H.O.T-Jarvis/docs/>

One-time setup (already done): repo Settings → Pages → Build and deployment →
Source = "GitHub Actions". Nothing else is required, and it costs nothing.

## A root host instead (Vercel / Netlify / custom domain)

To serve the docs at the root of their own domain, build with the two vars set
to that domain and an empty base:

```bash frame="terminal"
cd docs-site
DOCS_SITE=https://your.domain DOCS_BASE=/ npm run build
```

- **Vercel:** import `Hotragn/H.O.T-Jarvis`, set **Root Directory** to
  `docs-site`, and add `DOCS_SITE` / `DOCS_BASE` as environment variables.
  `vercel.json` supplies the cache headers.
- **Netlify:** import the repo, set **Base directory** to `docs-site`;
  `netlify.toml` supplies the build command, publish dir, and cache headers. Add
  the same two env vars.

After the first root deploy, update `public/robots.txt`'s `Sitemap:` line to the
new domain.

## Analytics (optional)

Set `PUBLIC_GA4_ID` in the host's environment variables to switch on Google
Analytics 4. Left unset, the analytics slot is a no-op.

## Caching

`vercel.json` / `netlify.toml` set `immutable` long-cache headers on hashed build
assets (`/_astro/*`) and fonts, so repeat visits are instant. HTML stays
revalidated so content updates ship immediately. (GitHub Pages applies its own
sensible caching.)
