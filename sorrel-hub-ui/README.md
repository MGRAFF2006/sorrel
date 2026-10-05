# sorrel-hub-ui

Shared SolidJS Hub product UI for browser and desktop shells. The native mobile
companion lives in `sorrel-hub-mobile` and shares Hub API/SDK contracts instead
of mounting this DOM UI.

```sh
npm ci
npm run dev    # :5181, proxies /api → Hub
npm run build  # library build
npm run check  # typecheck + tests + production build
```

Browser hosts call `mountHubApp(element, { platformKind: 'web' })`. Desktop
hosts also inject an `apiBase`, scoped `fetch` implementation, and platform
adapters; see `sorrel-hub-desktop`.

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

The repository page provides CLI commands populated with the current Hub URL
and project id. `lane submit` pushes and associates the current workspace;
already pushed repositories can be linked explicitly. Project creation offers
known organizations and a local-workspace default. Review creation selects
repository refs and captures their snapshot ids for stable comparisons.

Files open in a numbered UTF-8 preview; binary files, files over 512 KiB, and
excess lines receive bounded previews or explicit explanations. Submitted
reviews compare the recorded source and target snapshots with a changed-file
list and before/after text panes. Missing snapshots show an unavailable state
instead of inventing a baseline. Review status actions only update metadata:
“Mark as merged” follows a workspace merge and push, and closed reviews remain
separate from merged work. Failed mutations show an explicit retry; creation
and comments preserve drafts and prevent duplicate submissions. Native modal
dialogs provide keyboard focus and Escape dismissal.
