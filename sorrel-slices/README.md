# sorrel-slices

Prototype tooling for creating Sorrel slice manifests.

Sorrel slices are shareable extracted subprojects created from one or more
entrypoints. This first prototype focuses on TypeScript/JavaScript dependency
closure and manifest generation. It does not copy files, create repositories,
project permissions, or call external package registries.

## What this prototype does

- Accepts a project root, one or more entrypoints, include globs, and exclude
  globs.
- Parses static ESM imports, static ESM re-exports, and static CommonJS
  `require(...)` calls.
- Follows local relative imports that resolve to `.ts`, `.tsx`, `.js`, `.jsx`,
  or `.json` files.
- Records external, missing, unsupported, outside-root, and dynamic imports as
  unresolved imports.
- Adds nearby `package.json` and `tsconfig.json` files when they are relevant to
  included source files and allowed by the include/exclude patterns.
- Emits a deterministic JSON manifest with sorted paths and no timestamps.

## Install and test

```bash
npm install
npm test
npm run lint
```

The package has no runtime dependencies.

## CLI usage

```bash
node src/index.js \
  --project-root ./test/fixtures/basic \
  --entrypoint src/index.ts \
  --include 'src/**' \
  --include package.json \
  --include tsconfig.json \
  --exclude 'src/**/*.test.ts'
```

Write the manifest to a file:

```bash
node src/index.js \
  --project-root ./my-app \
  --entrypoint src/index.ts \
  --include 'src/**' \
  --include package.json \
  --include tsconfig.json \
  --out slice-manifest.json
```

Options:

- `--project-root`, `-r`: project root that all manifest paths are relative to.
- `--entrypoint`, `-e`: entry file relative to the project root. Repeatable.
- `--include`, `-i`: include glob relative to the project root. Repeatable.
  Defaults to `**/*`.
- `--exclude`, `-x`: exclude glob relative to the project root. Repeatable.
- `--out`, `-o`: output file. Defaults to stdout.

Glob support is intentionally small and deterministic: `*` matches within one
path segment, `**` matches across path segments, and `?` matches one character
within a path segment.

## Library usage

```js
import { createSliceManifest } from "@sorrel/slices";

const manifest = createSliceManifest({
  projectRoot: "/repo/my-app",
  entrypoint: "src/index.ts",
  includePatterns: ["src/**", "package.json", "tsconfig.json"],
  excludePatterns: ["src/**/*.test.ts"]
});

console.log(JSON.stringify(manifest, null, 2));
```

## Manifest shape

```json
{
  "schemaVersion": "sorrel.slices.manifest.v0",
  "kind": "SliceManifest",
  "language": "typescript-javascript",
  "sourceRoot": ".",
  "entrypoints": ["src/index.ts"],
  "includePatterns": ["package.json", "src/**", "tsconfig.json"],
  "excludePatterns": ["src/**/*.test.ts"],
  "includedFiles": ["package.json", "src/index.ts", "tsconfig.json"],
  "excludedFiles": [
    {
      "path": "src/index.test.ts",
      "reason": "exclude_pattern",
      "pattern": "src/**/*.test.ts"
    }
  ],
  "unresolvedImports": [
    {
      "from": "src/index.ts",
      "specifier": "react",
      "reason": "external_package"
    }
  ],
  "detectedPackageMetadata": [
    {
      "type": "package.json",
      "path": "package.json",
      "name": "@example/app",
      "version": "1.0.0",
      "private": true
    },
    {
      "type": "tsconfig.json",
      "path": "tsconfig.json"
    }
  ],
  "suggestedTargetRepoName": "example-app"
}
```

`sourceRoot` is currently `.` because all manifest paths are relative to the
provided project root.

## Current constraints

- No full Git, submodule, or repository extraction.
- No permission projection.
- No registry lookups and no package installation analysis.
- Dynamic imports are listed as unresolved with reason `dynamic_import`.
- Only local relative imports are followed.
