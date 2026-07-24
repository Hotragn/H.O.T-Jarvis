# Deploying the docs

The site is a static Astro build (`npm run build` → `dist/`). Config for Vercel
(`vercel.json`) and Netlify (`netlify.toml`) is already in this folder.

## Vercel (recommended, ~2 minutes)

Because the docs live in a subdirectory of the repo, the one setting that matters
is the **Root Directory**.

1. Go to [vercel.com/new](https://vercel.com/new) and import
   `Hotragn/H.O.T-Jarvis`.
2. Set **Root Directory** to `docs-site`. Vercel auto-detects Astro; leave the
   build command (`npm run build`) and output (`dist`) as detected.
3. Deploy. Vercel then auto-deploys every push to `main` that touches `docs-site`.

CLI alternative (needs `vercel login` once):

```bash frame="terminal"
cd docs-site
vercel --prod        # first run: link project, set root dir when prompted
```

### After the first deploy

Set your real domain in `astro.config.mjs` (`const SITE = ...`) so canonical
URLs, the sitemap, and OG image URLs are correct, then redeploy. Update
`public/robots.txt`'s `Sitemap:` line to match.

## Analytics (optional)

Set `PUBLIC_GA4_ID` in the Vercel project's Environment Variables to switch on
Google Analytics 4. Left unset, the analytics slot is a no-op.

## Netlify alternative

Import the repo, set **Base directory** to `docs-site`; `netlify.toml` supplies
the build command, publish dir, and cache headers.

## Caching

`vercel.json` / `netlify.toml` set `immutable` long-cache headers on hashed build
assets (`/_astro/*`) and fonts, so repeat visits are instant. HTML stays
revalidated so content updates ship immediately.
