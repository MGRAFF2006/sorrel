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

Hosts call `mountHubApp(element, { platformKind: 'web' })`.
