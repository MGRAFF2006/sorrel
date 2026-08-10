# Agent instructions for sorrel-sdk-js

## What this module is

The planned TypeScript/JavaScript SDK for Sorrel: typed client bindings over the
protocol object shapes and the CLI/Hub surfaces, for Node and browser consumers.

**Status: minimal Hub client shipped.** Typed HTTP client over live `sorrel-hub`
routes. Broader protocol/CLI bindings wait on the embedding surface (see root
`ROADMAP.md`).

## Common checks

```sh
npm test
npm run lint
```

## Workflow

- Keep changes scoped to this repository.
- Prefer small, reviewable commits.
- Do not commit secrets.
- Coordinate shared contracts through `sorrel-protocol`.
