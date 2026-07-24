// Generate the cross-platform raster icon set from the brand SVGs.
// Run from a dir where `sharp` resolves (e.g. `cd docs-site && node ../scripts/generate-icons.mjs`).
//
// Sources (repo root /brand):
//   icon.svg         — rounded dark square mark  → favicons, PWA "any"
//   icon-square.svg  — full-bleed opaque square  → apple-touch, PWA maskable, iOS 1024
//
// Outputs are written to landing/, docs-site/public/, and brand/.
import { readFileSync, mkdirSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";
import { createRequire } from "node:module";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
// sharp lives in docs-site/node_modules; resolve it from there.
const require = createRequire(resolve(root, "docs-site/package.json"));
const sharp = require("sharp");
const rounded = readFileSync(resolve(root, "brand/icon.svg"));
const square = readFileSync(resolve(root, "brand/icon-square.svg"));

const png = (svg, size, out) =>
  sharp(svg, { density: 384 })
    .resize(size, size, { fit: "contain", background: { r: 0, g: 0, b: 0, alpha: 0 } })
    .png()
    .toFile(out)
    .then(() => console.log(`  ${out}  (${size}px)`));

// Web icon set, written to a site root dir.
async function webSet(dir) {
  mkdirSync(dir, { recursive: true });
  await png(rounded, 16, `${dir}/favicon-16.png`);
  await png(rounded, 32, `${dir}/favicon-32.png`);
  await png(rounded, 192, `${dir}/icon-192.png`);
  await png(rounded, 512, `${dir}/icon-512.png`);
  await png(square, 180, `${dir}/apple-touch-icon.png`); // iOS home screen (opaque)
  await png(square, 512, `${dir}/icon-maskable-512.png`); // PWA maskable (safe-zone padded)
}

console.log("landing/:");
await webSet(resolve(root, "landing"));
console.log("docs-site/public/:");
await webSet(resolve(root, "docs-site/public"));
console.log("brand/ (iOS AppIcon master):");
await png(square, 1024, resolve(root, "brand/app-icon-1024.png"));
console.log("done.");
