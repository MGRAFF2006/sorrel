import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import fs from "node:fs";
import { tmpdir } from "node:os";
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

test("rejects entrypoints that escape the project root through symlinks", (t) => {
  const { projectRoot, outside } = temporaryProject(t);
  fs.writeFileSync(path.join(outside, "index.ts"), "export const value = 1;\n");
  fs.symlinkSync(path.join(outside, "index.ts"), path.join(projectRoot, "linked.ts"));
  fs.symlinkSync(outside, path.join(projectRoot, "linked-dir"), "dir");

  for (const entrypoint of ["linked.ts", "linked-dir/index.ts", "../outside/index.ts"]) {
    assert.throws(() => createSliceManifest({ projectRoot, entrypoint }), /outside project root/);
  }
});

test("reports imports through outside-root symlinks without reading their targets", (t) => {
  const { projectRoot, outside } = temporaryProject(t);
  fs.writeFileSync(path.join(outside, "index.ts"), 'import "./private-dependency";\n');
  fs.symlinkSync(path.join(outside, "index.ts"), path.join(projectRoot, "linked.ts"));
  fs.symlinkSync(outside, path.join(projectRoot, "linked-dir"), "dir");
  fs.writeFileSync(path.join(projectRoot, "index.ts"), [
    'import "./linked";',
    'import "./linked.ts";',
    'import "./linked-dir";',
    'import "./linked-dir/index";',
    'import "../outside/index";'
  ].join("\n"));

  const manifest = createSliceManifest({ projectRoot, entrypoint: "index.ts" });
  assert.deepEqual(manifest.includedFiles, ["index.ts"]);
  assert.deepEqual(unresolvedKeys(manifest), [
    "index.ts|../outside/index|outside_project_root",
    "index.ts|./linked|outside_project_root",
    "index.ts|./linked-dir|outside_project_root",
    "index.ts|./linked-dir/index|outside_project_root",
    "index.ts|./linked.ts|outside_project_root"
  ]);
});

test("rejects outside-root package metadata symlinks", (t) => {
  const { projectRoot, outside } = temporaryProject(t);
  fs.writeFileSync(path.join(projectRoot, "index.ts"), "export const value = 1;\n");

  for (const fileName of ["package.json", "tsconfig.json"]) {
    // Invalid JSON proves the target is rejected before parsing its contents.
    fs.writeFileSync(path.join(outside, fileName), "private metadata");
    const link = path.join(projectRoot, fileName);
    fs.symlinkSync(path.join(outside, fileName), link);
    assert.throws(
      () => createSliceManifest({ projectRoot, entrypoint: "index.ts" }),
      /outside project root/
    );
    fs.unlinkSync(link);
  }
});

test("accepts internal symlinks, symlinked project roots, and dot-prefixed paths", (t) => {
  const { projectRoot } = temporaryProject(t);
  fs.mkdirSync(path.join(projectRoot, "..shared"));
  fs.writeFileSync(path.join(projectRoot, "..shared", "helper.ts"), "export const value = 1;\n");
  fs.symlinkSync(path.join(projectRoot, "..shared"), path.join(projectRoot, "alias"), "dir");
  fs.writeFileSync(path.join(projectRoot, "..entry.ts"), 'import "./alias/helper";\n');
  const rootLink = path.join(path.dirname(projectRoot), "project-link");
  fs.symlinkSync(projectRoot, rootLink, "dir");

  const manifest = createSliceManifest({ projectRoot: rootLink, entrypoint: "..entry.ts" });
  assert.deepEqual(manifest.includedFiles, ["..entry.ts", "alias/helper.ts"]);
  assert.deepEqual(manifest.unresolvedImports, []);
  const direct = createSliceManifest({ projectRoot, entrypoint: "..shared/helper.ts" });
  assert.deepEqual(direct.includedFiles, ["..shared/helper.ts"]);
});

function temporaryProject(t) {
  const directory = fs.mkdtempSync(path.join(tmpdir(), "sorrel-slices-boundaries-"));
  t.after(() => fs.rmSync(directory, { recursive: true, force: true }));
  const projectRoot = path.join(directory, "project");
  const outside = path.join(directory, "outside");
  fs.mkdirSync(projectRoot);
  fs.mkdirSync(outside);
  return { projectRoot, outside };
}

test("preserves literal POSIX backslashes without reading normalized outside paths", { skip: path.sep !== "/" }, (t) => {
  const { projectRoot, outside } = temporaryProject(t);
  const fileName = "..\\outside\\secret.ts";
  fs.writeFileSync(path.join(projectRoot, "index.ts"), `import "./${fileName}";\n`);
  fs.writeFileSync(path.join(projectRoot, fileName), "export const harmless = true;\n");
  fs.writeFileSync(path.join(outside, "secret.ts"), 'import "sensitive-external-package-name";\n');

  const manifest = createSliceManifest({ projectRoot, entrypoint: "index.ts" });
  assert.deepEqual(manifest.includedFiles, [fileName, "index.ts"]);
  assert.deepEqual(manifest.unresolvedImports, []);

  const direct = createSliceManifest({ projectRoot, entrypoint: fileName });
  assert.deepEqual(direct.entrypoints, [fileName]);
  assert.deepEqual(direct.includedFiles, [fileName]);
  assert.deepEqual(direct.unresolvedImports, []);
});

test("preserves literal POSIX backslashes in metadata directories", { skip: path.sep !== "/" }, (t) => {
  const { projectRoot, outside } = temporaryProject(t);
  const directoryName = "..\\outside";
  const directory = path.join(projectRoot, directoryName);
  fs.mkdirSync(directory);
  fs.writeFileSync(path.join(projectRoot, "index.ts"), `import "./${directoryName}/index.ts";\n`);
  fs.writeFileSync(path.join(directory, "index.ts"), "export const harmless = true;\n");
  fs.writeFileSync(path.join(directory, "package.json"), '{"name":"inside-project"}');
  fs.writeFileSync(path.join(directory, "tsconfig.json"), "{}");
  fs.writeFileSync(path.join(outside, "index.ts"), 'import "sensitive-external-package-name";\n');
  fs.writeFileSync(path.join(outside, "package.json"), "private metadata");

  const manifest = createSliceManifest({ projectRoot, entrypoint: "index.ts" });
  assert.deepEqual(manifest.includedFiles, [
    `${directoryName}/index.ts`,
    `${directoryName}/package.json`,
    `${directoryName}/tsconfig.json`,
    "index.ts"
  ]);
  assert.deepEqual(manifest.unresolvedImports, []);
  assert.equal(manifest.detectedPackageMetadata[0].name, "inside-project");
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
