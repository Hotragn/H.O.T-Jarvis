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
