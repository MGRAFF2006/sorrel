# Agent instructions for sorrel-slices

## What this module is

The TypeScript/JavaScript slice manifest generator: it detects the dependency
closure from an entrypoint (imports, `package.json`, `tsconfig`, local workspace
deps, tests, assets) and produces a portable `Slice` manifest used to extract a
shareable subproject. Prototype stage.

## Stack and conventions

- TypeScript/JavaScript, Node. Keep dependencies minimal.
- Output should carry permission/secret-schema references (not values) and link
  the slice back to its source, per the protocol `Slice` shape.

## Core boundary

- Carry over permission/reviewer/secret-schema references, never raw secret
  values, when projecting a slice.

## Common checks

```sh
npm test
npm run lint        # if present
```

## Workflow

- Keep changes scoped to this repository.
- Prefer small, reviewable commits.
- Do not commit secrets.
- Coordinate shared contracts through `sorrel-protocol`.
