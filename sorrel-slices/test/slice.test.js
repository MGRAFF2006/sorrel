import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { createSliceManifest, parseImports } from "../src/slice.js";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, "..");
const basicFixture = path.join(__dirname, "fixtures", "basic");

test("creates deterministic dependency closure manifests", () => {
  const manifest = createSliceManifest({
    projectRoot: basicFixture,
    entrypoint: "src/index.ts",
    includePatterns: ["src/**", "package.json", "tsconfig.json"],
    excludePatterns: ["src/excluded.ts"]
  });

  assert.deepEqual(manifest.entrypoints, ["src/index.ts"]);
  assert.deepEqual(manifest.includedFiles, [
    "package.json",
    "src/Widget.tsx",
    "src/common.jsx",
    "src/data.json",
    "src/helper.ts",
    "src/index.ts",
    "src/side-effect.js",
    "src/types.ts",
    "tsconfig.json"
  ]);
  assert.deepEqual(manifest.excludedFiles, [
    {
      path: "shared/math.ts",
      reason: "not_included",
      pattern: undefined
    },
    {
      path: "src/excluded.ts",
      reason: "exclude_pattern",
      pattern: "src/excluded.ts"
    }
  ]);
  assert.deepEqual(unresolvedKeys(manifest), [
    "src/Widget.tsx|./style.css|unsupported_extension",
    "src/helper.ts|./missing|not_found",
    "src/index.ts|./lazy|dynamic_import",
    "src/index.ts|react|external_package",
    "src/side-effect.js|node:path|external_package"
  ]);
  assert.deepEqual(manifest.detectedPackageMetadata, [
    {
      type: "package.json",
      path: "package.json",
      name: "@acme/basic-app",
      version: "1.2.3",
      private: true
    },
    {
      type: "tsconfig.json",
      path: "tsconfig.json"
    }
  ]);
  assert.equal(manifest.suggestedTargetRepoName, "acme-basic-app");
});

test("CLI writes the same manifest shape to stdout", () => {
  const stdout = execFileSync(
    process.execPath,
    [
      path.join(repoRoot, "src", "index.js"),
      "--project-root",
      basicFixture,
      "--entrypoint",
      "src/index.ts",
      "--include",
      "src/**",
      "--include",
      "package.json",
      "--include",
      "tsconfig.json",
      "--exclude",
      "src/excluded.ts"
    ],
    { encoding: "utf8" }
  );
  const manifest = JSON.parse(stdout);

  assert.equal(manifest.kind, "SliceManifest");
  assert.equal(manifest.sourceRoot, ".");
  assert.equal(manifest.suggestedTargetRepoName, "acme-basic-app");
  assert.deepEqual(manifest.entrypoints, ["src/index.ts"]);
});

test("parseImports detects supported static forms and dynamic imports", () => {
  const imports = parseImports(`
    import value from "./value";
    import type { Type } from "./types";
    import "./side-effect";
    export { other } from "./other";
    const common = require("./common");
    const lazy = () => import("./lazy");
  `);

  assert.deepEqual(imports, [
    { kind: "static", syntax: "import", specifier: "./value" },
    { kind: "static", syntax: "import", specifier: "./types" },
    { kind: "static", syntax: "import", specifier: "./side-effect" },
    { kind: "static", syntax: "export", specifier: "./other" },
    { kind: "static", syntax: "require", specifier: "./common" },
    { kind: "dynamic", syntax: "import", specifier: "./lazy" }
  ]);
});

function unresolvedKeys(manifest) {
  return manifest.unresolvedImports.map((item) => `${item.from}|${item.specifier}|${item.reason}`);
}
