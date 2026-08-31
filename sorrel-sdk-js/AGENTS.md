# Agent instructions for sorrel-sdk-js

## What this module is

The JavaScript SDK for Sorrel: a small client over the Hub HTTP surface for
Node and browser consumers.

**Status: minimal Hub client shipped.** Typed HTTP client over live `sorrel-hub`
routes. Broader protocol/CLI bindings wait on the embedding surface (see root
`ROADMAP.md`).

## Common checks

```sh
npm ci
npm run check
```

## Workflow

- Keep changes scoped to this package and required workspace consumers.
- Prefer small, reviewable commits.
- Do not commit secrets.
- Coordinate shared contracts through `sorrel-protocol`.
