// @ts-check
import { defineConfig } from "astro/config";
import starlight from "@astrojs/starlight";
import sitemap from "@astrojs/sitemap";

// Swap SITE to your production domain (Vercel/Netlify) — drives canonical URLs,
// sitemap, and OG image URLs.
const SITE = "https://hot-jarvis.vercel.app";
const REPO = "https://github.com/Hotragn/H.O.T-Jarvis";

// https://astro.build/config
export default defineConfig({
  site: SITE,
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
      head: [
        { tag: "link", attrs: { rel: "icon", href: "/favicon-32.png", sizes: "32x32", type: "image/png" } },
        { tag: "link", attrs: { rel: "icon", href: "/favicon-16.png", sizes: "16x16", type: "image/png" } },
        { tag: "link", attrs: { rel: "apple-touch-icon", href: "/apple-touch-icon.png" } },
        { tag: "link", attrs: { rel: "manifest", href: "/site.webmanifest" } },
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
