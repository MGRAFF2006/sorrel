// Generate or verify the policy conformance sidecar metadata file.
//
// The canonical manifest is `conformance/policy-conformance.json`. Consumers
// vendor a copy of it. To detect drift without a package/release path, we also
// publish a tiny sidecar metadata file next to the manifest:
//
//   conformance/policy-conformance.meta.json
//
// It records the manifest `id` (treated as the manifest version), its
// `schemaVersion`, and a SHA-256 over the exact manifest bytes. Consumers vendor
// the sidecar alongside the manifest and assert that the SHA-256 of their
// vendored manifest matches the sidecar. That makes drift a test failure.
//
// Usage:
//   node scripts/conformance-meta.mjs           # write the sidecar
//   node scripts/conformance-meta.mjs --check    # verify the sidecar is current
//
// The `--check` mode exits non-zero if the sidecar is missing or stale, so it is
// safe to run in CI / `npm test`.

import { createHash } from "node:crypto";
import { readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const manifestPath = path.join(root, "conformance", "policy-conformance.json");
const metaPath = path.join(root, "conformance", "policy-conformance.meta.json");

export function sha256(buffer) {
  return createHash("sha256").update(buffer).digest("hex");
}

export async function computeMeta() {
  const raw = await readFile(manifestPath);
  const manifest = JSON.parse(raw.toString("utf8"));
  return {
    kind: "PolicyConformanceMeta",
    description:
      "Sidecar metadata for conformance/policy-conformance.json. Consumers vendor this file alongside the manifest and assert the SHA-256 matches so drift is a test failure. Regenerate with `npm run sync:meta`.",
    manifestFile: "policy-conformance.json",
    manifestVersion: manifest.id,
    schemaVersion: manifest.schemaVersion,
    sha256: sha256(raw),
  };
}

function serialize(meta) {
  return `${JSON.stringify(meta, null, 2)}\n`;
}

async function main() {
  const check = process.argv.includes("--check");
  const meta = await computeMeta();

  if (!check) {
    await writeFile(metaPath, serialize(meta));
    console.log(`wrote ${path.relative(root, metaPath)}`);
    console.log(`  manifestVersion: ${meta.manifestVersion}`);
    console.log(`  schemaVersion:   ${meta.schemaVersion}`);
    console.log(`  sha256:          ${meta.sha256}`);
    return;
  }

  let existing;
  try {
    existing = JSON.parse(await readFile(metaPath, "utf8"));
  } catch (error) {
    if (error && error.code === "ENOENT") {
      console.error(
        `not ok ${path.relative(root, metaPath)} is missing; run \`npm run sync:meta\``,
      );
      process.exit(1);
    }
    throw error;
  }

  const mismatches = [];
  for (const field of ["manifestVersion", "schemaVersion", "sha256"]) {
    if (existing[field] !== meta[field]) {
      mismatches.push(`${field}: sidecar=${existing[field]} expected=${meta[field]}`);
    }
  }

  if (mismatches.length > 0) {
    console.error(`not ok ${path.relative(root, metaPath)} is stale:`);
    for (const line of mismatches) {
      console.error(`  ${line}`);
    }
    console.error("run `npm run sync:meta` to regenerate it");
    process.exit(1);
  }

  console.log(`ok ${path.relative(root, metaPath)} is current (sha256 ${meta.sha256})`);
}

if (import.meta.url === `file://${process.argv[1]}`) {
  await main();
}
