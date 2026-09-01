# Agent instructions for sorrel-hub-desktop

## What this module is

The Tauri desktop host for the shared `sorrel-hub-ui` product UI. It produces
native installers for Windows, macOS, and Linux; product behavior remains in
`sorrel-hub-ui`.

## Boundaries

- Keep host chrome, native notifications, external-link handling, and HTTP
  transport wiring here.
- Do not fork or duplicate the shared SolidJS UI.
- Do not claim local Core access until the stable embedding surface exists.
- Keep Tauri capabilities least-privilege. The release app may contact only a
  Hub on loopback until authenticated remote-Hub configuration ships.
- Never persist or display secret values.

## Common checks

```sh
npm ci
npm run check
npm run tauri:dev
```

Platform bundles are built in the release workflow on native x64 and ARM64
runners. On Linux, a local Tauri build also needs WebKitGTK 4.1 and the standard
Tauri system packages documented in this package's README.
