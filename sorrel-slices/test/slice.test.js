import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import { execFileSync } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { createSliceManifest, parseImports } from "../src/slice.js";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, "..");
const basicFixture = path.join(__dirname, "fixtures", "basic");

test("parseImports decodes escaped JavaScript module specifiers without executing source", () => {
  const imports = parseImports(String.raw`
    import value from "./dep\u002ejs";
    export { value } from './quo\'ted.js';
    require("./\x64ep.js");
    import("./face\u{1f600}.js");
    require("./back\\slash.js");
    require("./continued\
name.js");
    require('./legacy\144ep.js');
  `);
  assert.deepEqual(imports, [
    { kind: "static", syntax: "import", specifier: "./dep.js" },
    { kind: "static", syntax: "export", specifier: "./quo'ted.js" },
    { kind: "static", syntax: "require", specifier: "./dep.js" },
    { kind: "dynamic", syntax: "import", specifier: "./face😀.js" },
    { kind: "static", syntax: "require", specifier: "./back\\slash.js" },
    { kind: "static", syntax: "require", specifier: "./continuedname.js" },
    { kind: "static", syntax: "require", specifier: "./legacydep.js" }
  ]);
});

test("escaped imports include actual dependency files in the slice closure", () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "sorrel-escaped-import-"));
  try {
    fs.writeFileSync(path.join(root, "index.js"), String.raw`import "./dep\u002ejs";`);
    fs.writeFileSync(path.join(root, "dep.js"), "export const value = 1;\n");
    const manifest = createSliceManifest({ projectRoot: root, entrypoint: "index.js" });
    assert.deepEqual(manifest.includedFiles, ["dep.js", "index.js"]);
    assert.deepEqual(manifest.unresolvedImports, []);
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

test("module strings preserve control/identity escapes and CRLF continuation, skipping malformed literals", () => {
  assert.deepEqual(parseImports('require("./a\\\r\nb.js");'), [
    { kind: "static", syntax: "require", specifier: "./ab.js" }
  ]);
  assert.deepEqual(parseImports(String.raw`require("./\b\f\n\r\t\v\0\z\8\9");`), [
    { kind: "static", syntax: "require", specifier: "./\b\f\n\r\t\v\0z89" }
  ]);
  for (const literal of [String.raw`"./\xZ1"`, String.raw`"./\u12"`, String.raw`"./\u{110000}"`]) {
    assert.deepEqual(parseImports("require(" + literal + '); require("./real");'), [
      { kind: "static", syntax: "require", specifier: "./real" }
    ]);
  }
  assert.deepEqual(parseImports('require("./unclosed'), []);
});

test("parseImports distinguishes module calls from property and identifier lookalikes", () => {
  const source = `
    object.require("./property");
    object?.require("./optional");
    object. /* comment */ require("./commented-property");
    object.import("./import-property");
    $require("./prefixed-name");
    custom$require("./long-prefixed-name");
    require("./real");
    import("./lazy");
  `;
  assert.doesNotThrow(() => new Function(source));
  assert.deepEqual(parseImports(source), [
    { kind: "static", syntax: "require", specifier: "./real" },
    { kind: "dynamic", syntax: "import", specifier: "./lazy" }
  ]);
});

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

test("parseImports ignores import-like text in strings and templates", () => {
  assert.deepEqual(parseImports(`
    const example = 'require("./fake-common")';
    const template = \`import "./fake-static"; export { value } from "./fake-export"; import("./fake-dynamic");\`;
    const incomplete = \`import value from\`;
    import value from "./real";
    /* import "./comment"; */
  `), [{ kind: "static", syntax: "import", specifier: "./real" }]);
});

test("parseImports still detects calls inside template expressions", () => {
  assert.deepEqual(parseImports('const text = `${require("./real")}-${`${import("./lazy")}`}`;'), [
    { kind: "static", syntax: "require", specifier: "./real" },
    { kind: "dynamic", syntax: "import", specifier: "./lazy" }
  ]);
});

function unresolvedKeys(manifest) {
  return manifest.unresolvedImports.map((item) => `${item.from}|${item.specifier}|${item.reason}`);
}


test("parseImports ignores regex braces inside template expressions", () => {
  const sources = [
    'const text = `${ /}/.test("}") ? require("./inside") : "none" }`; require("./outside");',
    'const text = `${ /{/.test("{") ? require("./inside") : "none" }`; require("./outside");',
    'const text = `${ /[{}]/.test("}") ? require("./inside") : "none" }`; require("./outside");'
  ];
  for (const source of sources) {
    assert.doesNotThrow(() => new Function(source));
    assert.deepEqual(parseImports(source), [
      { kind: "static", syntax: "require", specifier: "./inside" },
      { kind: "static", syntax: "require", specifier: "./outside" }
    ]);
  }
});

test("parseImports ignores dependency-like text inside regex literals", () => {
  const source = String.raw`
    const regular = /require("fake-common")/;
    const escapes = /[}/]require\("fake-escaped"\)\/end/g;
    if (condition) /import "fake-static";/.test(text);
    function test() { return /export value from "fake-export";/; }
    require("./real");
  `;
  assert.doesNotThrow(() => new Function(source));
  assert.deepEqual(parseImports(source), [{ kind: "static", syntax: "require", specifier: "./real" }]);
});

test("parseImports distinguishes division from regex literals", () => {
  for (const numerator of ['value', 'value++', 'value--', 'call()', '[value]', '({})',
    '"value"', '`value`', 'object.return', 'object.if(value)', 'of', 'await', 'yield', 'π', '𝒜']) {
    const source = `const quotient = ${numerator} / denominator; require("./real");`;
    assert.doesNotThrow(() => new Function(source));
    assert.deepEqual(parseImports(source), [{ kind: "static", syntax: "require", specifier: "./real" }], source);
  }
  const template = 'const text = `${value / denominator}-${require("./real")}`;';
  assert.doesNotThrow(() => new Function(template));
  assert.deepEqual(parseImports(template), [{ kind: "static", syntax: "require", specifier: "./real" }]);
});
