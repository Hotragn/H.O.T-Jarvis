// Fails the build if any internal link would 404 under the site's base path.
//
// This exists because that bug shipped once and was invisible from the inside:
// every page returned 200, the sidebar worked, only the hand-written links in
// content pointed at the domain root. A subpath deploy turns those into 404s,
// so the whole site looks broken except the external GitHub link.
//
// Usage: node scripts/check-links.mjs [distDir] [base]

import { readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";

const dist = process.argv[2] ?? "dist";
const base = process.argv[3] ?? process.env.DOCS_BASE ?? "/";
const prefix = base.endsWith("/") ? base.slice(0, -1) : base;

function walk(dir) {
  const out = [];
  for (const entry of readdirSync(dir)) {
    const full = join(dir, entry);
    if (statSync(full).isDirectory()) out.push(...walk(full));
    else if (entry.endsWith(".html")) out.push(full);
  }
  return out;
}

const offenders = [];
let checked = 0;

for (const file of walk(dist)) {
  const html = readFileSync(file, "utf8");
  for (const match of html.matchAll(/(?:href|src)="([^"]+)"/g)) {
    const url = match[1];
    // Only root-absolute, same-origin paths can be wrong here.
    if (!url.startsWith("/") || url.startsWith("//")) continue;
    checked += 1;
    if (prefix && !url.startsWith(`${prefix}/`) && url !== prefix) {
      offenders.push({ file, url });
    }
  }
}

console.log(
  `link check: ${checked} root-absolute URLs across ${walk(dist).length} pages, base "${base}"`,
);

if (offenders.length > 0) {
  console.error(`\n${offenders.length} link(s) would 404 under this base:\n`);
  for (const { file, url } of offenders.slice(0, 25)) {
    console.error(`  ${file}\n    -> ${url}`);
  }
  console.error(
    "\nFix: use a relative link, or a root-absolute one in markdown (the rehype\n" +
      "plugin rewrites those). Component props like <LinkCard href> bypass the\n" +
      "plugin, so those must be relative.",
  );
  process.exit(1);
}

console.log("link check: no off-base links.");
