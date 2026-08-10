// Export (copy) the canonical policy conformance manifest and its sidecar
// metadata into a consumer checkout's vendored conformance directory.
//
// This is an OPTIONAL convenience for monorepo-style or side-by-side checkouts.
// Normal package use does not require it; a consumer can also just copy the two
// files by hand. The export keeps the canonical manifest as the single source of
// truth so consumers never hand-edit their vendored copy.
//
// Usage:
//   node scripts/export-conformance.mjs <dest-dir>
//
//   <dest-dir> is the consumer's vendored conformance directory, e.g.
//     ../sorrel-core/tests/conformance
//     ../sorrel-hub/test/conformance
//
// It writes both:
//   <dest-dir>/policy-conformance.json
//   <dest-dir>/policy-conformance.meta.json
//
// The sidecar is regenerated from the manifest first so the two always agree.

import { mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

import { computeMeta } from "./conformance-meta.mjs";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const manifestPath = path.join(root, "conformance", "policy-conformance.json");

async function main() {
  const dest = process.argv[2];
  if (!dest) {
    console.error("usage: node scripts/export-conformance.mjs <dest-dir>");
    process.exit(2);
  }

  const destDir = path.resolve(dest);
  await mkdir(destDir, { recursive: true });

  const manifestRaw = await readFile(manifestPath);
  const meta = await computeMeta();

  const destManifest = path.join(destDir, "policy-conformance.json");
  const destMeta = path.join(destDir, "policy-conformance.meta.json");

  await writeFile(destManifest, manifestRaw);
  await writeFile(destMeta, `${JSON.stringify(meta, null, 2)}\n`);

  console.log(`exported manifest -> ${destManifest}`);
  console.log(`exported sidecar  -> ${destMeta}`);
  console.log(`  manifestVersion: ${meta.manifestVersion}`);
  console.log(`  sha256:          ${meta.sha256}`);
}

await main();
