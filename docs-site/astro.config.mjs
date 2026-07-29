// @ts-check
import { defineConfig } from "astro/config";
import starlight from "@astrojs/starlight";
import sitemap from "@astrojs/sitemap";

// The docs ship free on GitHub Pages under the /docs subpath of the project
// site. SITE drives canonical URLs, sitemap, and OG images; BASE is the subpath
// every internal link and asset is prefixed with. Both are env-overridable, so
// a root host (Vercel/Netlify/custom domain) is a two-var build:
//   DOCS_SITE=https://your.domain DOCS_BASE=/  npm run build
const SITE = process.env.DOCS_SITE ?? "https://hotragn.github.io";
const BASE = process.env.DOCS_BASE ?? "/H.O.T-Jarvis/docs";
const REPO = "https://github.com/Hotragn/H.O.T-Jarvis";

// Prefix a root-relative asset path with BASE so favicons/manifest resolve under
// the subpath (Astro handles this for content links, not for manual head tags).
const asset = (p) => `${BASE.replace(/\/$/, "")}/${p.replace(/^\//, "")}`;

// https://astro.build/config
export default defineConfig({
  site: SITE,
  base: BASE,
  integrations: [
    starlight({
      title: "H.O.T-Jarvis",
      description:
        "An open-source, local-first AI assistant that grows its own skills, remembers how it reasons, tells you when it's unsure, and lets you undo anything — running on your machine, for free.",
      logo: {
        src: "./src/assets/logo.svg",
        replacesTitle: false,
      },
      favicon: "/favicon.svg",
      // Cross-platform icons: PNG favicons, iOS home-screen, PWA manifest, theme.
      // Paths run through asset() so they resolve under the /docs base path.
      head: [
        { tag: "link", attrs: { rel: "icon", href: asset("favicon-32.png"), sizes: "32x32", type: "image/png" } },
        { tag: "link", attrs: { rel: "icon", href: asset("favicon-16.png"), sizes: "16x16", type: "image/png" } },
        { tag: "link", attrs: { rel: "apple-touch-icon", href: asset("apple-touch-icon.png") } },
        { tag: "link", attrs: { rel: "manifest", href: asset("site.webmanifest") } },
        { tag: "meta", attrs: { name: "theme-color", content: "#04070d" } },
      ],
      customCss: ["./src/styles/theme.css"],
      social: [{ icon: "github", label: "GitHub", href: REPO }],
      editLink: {
        baseUrl: `${REPO}/edit/main/docs-site/`,
      },
      lastUpdated: true,
      pagination: true,
      // Custom overrides: Footer adds the feedback + community widgets;
      // Head injects JSON-LD, OG defaults, and the (env-gated) analytics slot.
      components: {
        Footer: "./src/components/Footer.astro",
        Head: "./src/components/Head.astro",
      },
      sidebar: [
        {
          label: "Start here",
          items: [
            { label: "Introduction", slug: "index" },
            { label: "Quickstart", slug: "tutorials/quickstart" },
          ],
        },
        {
          label: "Tutorials",
          items: [{ autogenerate: { directory: "tutorials" } }],
        },
        {
          label: "How-to guides",
          items: [{ autogenerate: { directory: "how-to" } }],
        },
        {
          label: "Reference",
          items: [{ autogenerate: { directory: "reference" } }],
        },
        {
          label: "Explanation",
          items: [{ autogenerate: { directory: "explanation" } }],
        },
        {
          label: "Project",
          items: [
            { label: "Contributing", slug: "contributing" },
            { label: "Blog", link: "/blog/" },
          ],
        },
      ],
    }),
    sitemap(),
  ],
});
