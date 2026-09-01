# sorrel-hub-ui

Shared SolidJS Hub product UI for the browser host, the Tauri desktop app, and
future mobile shells.

```sh
npm ci
npm run dev    # :5181, proxies /api → Hub
npm run build  # library build
npm run check  # typecheck + tests + production build
```

Browser hosts call `mountHubApp(element, { platformKind: 'web' })`. Native hosts
also inject an `apiBase`, scoped `fetch` implementation, and desktop platform
adapters; see `sorrel-hub-desktop`.
