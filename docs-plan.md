# Docs site plan — H.O.T-Jarvis

For your approval before I scaffold. This describes the stack, structure, and
component hierarchy for a production-grade documentation site that stands next to
Stripe / Vercel / Tailwind / Supabase docs. Nothing below is built yet.

## Decision: Astro v5 + Starlight (not Docusaurus)

Starlight wins here. It ships, out of the box and accessibly, most of your
"non-negotiable" list — three-column sticky layout, dark-mode-first with toggle,
sidebar with collapsible groups + persisted state, prev/next pagination,
on-page TOC, mobile hamburger, **Pagefind** search, "Edit this page", and
"Last updated" from git history. That means our effort goes into *content and
the custom polish* (feedback widget, code language tabs, OG images, JSON-LD,
analytics, brand), not rebuilding table stakes. Astro's islands give the
100/100 Lighthouse SEO/Accessibility target realistically.

Docusaurus is only better if we need its heavyweight **versioned-docs** system.
We don't yet (pre-1.0, one version). Plan keeps a branch-based versioning
placeholder; if real multi-version docs become a hard requirement later, that's
the one reason to revisit.

## Where it lives

A standalone Astro project at **`docs-site/`** (separate from the existing
`docs/` decision-log folder and from `landing/`). Own `package.json`, so it
never touches the desktop app or the landing page. Deploys independently.

```
docs-site/
├─ astro.config.mjs          # Starlight + sitemap + Tailwind + Pagefind + integrations
├─ package.json
├─ tailwind.config.mjs
├─ public/
│  ├─ robots.txt             # generated-friendly; references sitemap
│  ├─ brand/                 # logo family copied from /brand
│  └─ og/                    # generated OG images land here
├─ src/
│  ├─ content/
│  │  └─ docs/               # Diátaxis quadrants (below)
│  ├─ content.config.ts      # Starlight docs schema + custom frontmatter
│  ├─ styles/tailwind.css    # tokens matching the app/landing (cyan/mint/ink)
│  ├─ components/            # overrides + custom (below)
│  └─ pages/                 # custom Home (splash), OG endpoint
└─ netlify.toml / vercel.json  # edge caching headers
```

## Visual identity

Reuse the product's design tokens (ink `#04070d`, cyan `#38c6f4`, mint
`#56dfb4`) so docs, landing, and app read as one brand. Typeface: **Geist Sans**
+ **Geist Mono** (self-hosted via Fontsource — no external font CDN, keeps
Lighthouse + privacy clean). The new logo family from `/brand` supplies the nav
mark, favicon, and OG template. Dark mode default; light theme is the same
tokens inverted (matches the app's blueprint light theme).

## Content — Diátaxis

```
src/content/docs/
├─ index.mdx                     # Home / splash (hero, feature grid, code demo, trusted-by)
├─ tutorials/                    # learning-oriented, step-by-step
│  ├─ quickstart.md              # install Ollama → run → first conversation that persists
│  └─ your-first-skill.md        # ask Jarvis to author a skill, watch it self-test
├─ how-to/                       # problem-oriented recipes
│  ├─ add-a-free-cloud-key.md    # Groq / OpenRouter fallback
│  ├─ author-and-run-skills.md
│  ├─ export-and-wipe-memory.md
│  └─ deploy-the-desktop-app.md  # (build/run; ties to the Tauri story)
├─ reference/                    # information-oriented specs
│  ├─ tauri-commands.md          # every IPC command: args, returns, errors
│  ├─ event-log.md               # event kinds + payload schema
│  ├─ skill-manifest.md          # manifest fields, test contract, statuses
│  └─ configuration.md           # .env keys, data dir, kill switch
├─ explanation/                  # understanding-oriented background
│  ├─ architecture.md            # router / memory / skills / orchestrator
│  ├─ the-four-hero-features.md  # why skills/reasoning-memory/confidence/replay
│  └─ local-first-and-free.md    # the constitution, threat model, privacy
└─ contributing.md               # PRs, code style, running tests
```

A **blog** (`src/content/docs/blog/` via Starlight's blog pattern or a simple
collection): launch post — "H.O.T-Jarvis 0.1.0: an assistant that grows its own
skills." Real content, drawn from the decision log — no lorem.

The four API-Reference sample requirements map to `reference/tauri-commands.md`
(request/response examples, error cases) since this app's "API" is its IPC
surface, which is the honest thing to document.

## Feature → implementation map

| Requirement | How |
|---|---|
| 3-column sticky layout, dark-first, mobile menu | Starlight built-in |
| Collapsible sidebar groups + persisted state | Starlight built-in (localStorage) |
| Prev/Next pagination, breadcrumbs, on-page TOC | Starlight built-in (breadcrumbs enabled via config) |
| Search | **Pagefind** (Starlight default) — static, free |
| Syntax highlighting | Shiki (Starlight default), high-contrast theme tuned to our tokens |
| Code language tabs (npm/yarn/pnpm, JS/Py/cURL) | Starlight `<Tabs>`/`<TabItem>` + a small `<PkgManager>` wrapper |
| Copy button + toast | Starlight copy button; add a toast micro-interaction override |
| Terminal window styling | Custom `<Terminal>` component (macOS-dot chrome) |
| Edit this page | Starlight `editLink.baseUrl` → GitHub |
| Last updated (git) | Starlight `lastUpdated: true` |
| "Was this helpful?" → GitHub issue on No | Custom `<Feedback>` in the `<Footer>` override; No → prefilled issue link |
| Community CTA | Custom `<CommunityCTA>` (GitHub Discussions/Issues; Discord slot) |
| Versioning | Branch-based placeholder + a documented pattern (Starlight sidebar version switch stub) |
| sitemap.xml | `@astrojs/sitemap` |
| robots.txt | `public/robots.txt` referencing the sitemap |
| OG images | `astro-og-canvas` (or Satori) endpoint → per-page cards from the brand template |
| JSON-LD (`SoftwareApplication` / `TechArticle`) | `<head>` injection in a `<Head>` override, per page type |
| GA4 / Vercel Analytics | Env-gated `<Analytics>` in `<Head>` override (`PUBLIC_GA4_ID`); no-op when unset |
| Fade-in page transitions | Astro View Transitions (Starlight-compatible), reduced-motion respected |
| 100/100 Lighthouse A11y+SEO | Static output, semantic Starlight, self-hosted fonts, alt text, color-contrast audit |

Custom components to build (the ~20% Starlight doesn't give free):
`Terminal`, `PkgManager` (tabbed install commands), `Feedback`, `CommunityCTA`,
`Analytics`, `Head` (JSON-LD + OG + GA4), `Footer` override (adds Feedback +
CTA), and the custom **Home** splash. Everything else is Starlight config.

## SEO & metadata

- `site` set in `astro.config.mjs` → canonical URLs + sitemap.
- Per-page OG cards generated at build from a branded template (mark + title).
- JSON-LD: `SoftwareApplication` on Home, `TechArticle` on doc pages.
- `robots.txt` allows all, points at `/sitemap-index.xml`.

## Deployment

Primary: **Vercel** (`vercel.json` with long-cache immutable headers for
`/_astro/*` and assets, short cache + revalidate for HTML). Netlify parity via
`netlify.toml`. Static output (`output: 'static'`), so it also hosts anywhere.
The landing page stays on GitHub Pages; the docs get their own Vercel project so
the two deploy independently.

## Build order (once approved)

1. Scaffold `docs-site/` (Starlight), wire Tailwind + tokens + Geist + brand.
2. Config pass: site URL, sidebar (Diátaxis), edit-link, last-updated,
   breadcrumbs, Pagefind, sitemap, robots, View Transitions.
3. Custom components: `Head` (JSON-LD/OG/GA4), `Terminal`, `PkgManager`,
   `Feedback`, `CommunityCTA`, `Footer` override.
4. Content: write all pages above with real, accurate content from the codebase
   and decision log (active voice, no fluff).
5. Home splash + launch blog post.
6. Verify: `astro build`, Pagefind index, Lighthouse (A11y/SEO), link check;
   report scores.
7. Deploy config (Vercel/Netlify) + a GitHub Actions build check.

## Open questions for you

1. **Domain?** e.g. `docs.hotjarvis.dev` vs. a Vercel subdomain. Affects `site`,
   OG URLs, canonical.
2. **Community link** — Discord invite / GitHub Discussions? (CTA target.)
3. **Analytics** — GA4 (give me the measurement ID as `PUBLIC_GA4_ID`, or leave
   the slot empty) or Vercel Analytics?
4. **Blog** — keep it in the docs site, or is a launch post enough for now?

Approve and I'll build it in the order above and open a PR.
