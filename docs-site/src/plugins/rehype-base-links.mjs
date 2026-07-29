/**
 * Rewrites root-absolute internal links so they respect the site's `base`.
 *
 * Why this exists: Astro applies `base` to its own asset and component URLs, but
 * NOT to hand-written hrefs inside markdown. A link written as
 * `/reference/configuration/` therefore resolves to the domain root and 404s
 * whenever the site is served from a subpath — which is exactly what happened
 * when the docs moved to `<domain>/H.O.T-Jarvis/docs/`. Every internal link on
 * the site broke while external ones kept working.
 *
 * Fixing it here rather than in the content keeps the markdown portable: the same
 * files build correctly at a subpath or at a domain root, with no base baked in.
 *
 * Skipped: external URLs, protocol-relative URLs, anchors, mailto/tel, and
 * anything already carrying the base.
 */
export function rehypeBaseLinks(base = "/") {
  const prefix = base.endsWith("/") ? base.slice(0, -1) : base;

  return function transformer(tree) {
    visit(tree, (node) => {
      if (node.type !== "element") return;
      const attr = node.tagName === "a" ? "href" : null;
      if (!attr) return;
      const value = node.properties?.[attr];
      if (typeof value !== "string" || value.length === 0) return;

      // Only touch root-absolute paths: "/foo". Leave "//cdn", "#x", "https://".
      if (!value.startsWith("/") || value.startsWith("//")) return;
      if (prefix && (value === prefix || value.startsWith(`${prefix}/`))) return;

      node.properties[attr] = `${prefix}${value}`;
    });
  };
}

/** Minimal depth-first walk, so this needs no unist-util-visit dependency. */
function visit(node, fn) {
  fn(node);
  const children = node.children;
  if (!Array.isArray(children)) return;
  for (const child of children) visit(child, fn);
}
