# sorrel-hub-ui

Shared SolidJS Hub product UI for browser, desktop, and mobile shells.

```sh
npm ci
npm run dev    # :5181, proxies /api → Hub
npm run build  # library build
npm run check  # typecheck + tests + production build
```

Hosts call `mountHubApp(element, { platformKind: 'web' })`.
