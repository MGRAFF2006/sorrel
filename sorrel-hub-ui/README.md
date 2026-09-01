# sorrel-hub-ui

Shared SolidJS Hub product UI for browser, desktop, and mobile shells.

```sh
npm ci
npm run dev    # :5181, proxies /api → Hub
npm run build  # library build
npm run check  # typecheck + tests + production build
```

Hosts call `mountHubApp(element, { platformKind: 'web' })`.

## Product routes

- `/` — project picker for choosing or creating a workspace
- `/projects/:id` — Code-first project page backed by synchronized Sorrel
  trees, snapshot metadata, and README content
- `/projects/:id/work` — proposal-backed lane lifecycle board
- `/projects/:id/reviews` — review queue, discussion, checks, and decisions
- `/projects/:id/sync` — connected repository refs and sync state
- `/inbox` — cross-project queue derived from current proposals, unresolved
  comments, and failed workflows; it is deliberately not the splash screen
- `/orgs/:id` — organization README and projects
- `/profile` — the active principal, authored work, and profile README surface

The UI presents Hub/Core state without creating a parallel permission or issue
model. Project repository ids, repository records, or submitted proposal sync
ids connect the Code page to a synchronized repository.
