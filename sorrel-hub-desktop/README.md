# sorrel-hub-desktop

Native Tauri host for the shared Sorrel Hub product UI. The same SolidJS UI is
used by `sorrel-hub-web`; this package owns only desktop transport and native
integration.

Release builds are produced for:

- Windows x64 and ARM64
- macOS Intel and Apple Silicon
- Linux x64 and ARM64

## Run locally

Start a development Hub in another terminal:

```sh
cd ../sorrel-hub
SORREL_HUB_BOOTSTRAP_GRANTS=1 npm start
```

Then start the desktop shell:

```sh
npm ci
npm run tauri:dev
```

The app connects to `http://127.0.0.1:3000`. Its Tauri capability allows HTTP
only to loopback Hub addresses. Authenticated remote-Hub selection will widen
that scope deliberately when the production login flow exists.

On Arch Linux, a local Tauri build requires the WebKitGTK and app-indicator
development libraries provided by `webkit2gtk-4.1`, `libappindicator-gtk3`,
and their normal build dependencies. Release CI installs the corresponding
packages on its Linux runners. If `linuxdeploy` cannot strip an AppImage on
Arch, build with `NO_STRIP=true npm run tauri:build`.

## Validate

```sh
npm run check
cargo check --manifest-path src-tauri/Cargo.toml
```

The second command requires the platform-native Tauri system libraries. The
cross-platform bundle builds run in the release workflow on native runners.

## Current boundary

This first desktop host is a GUI companion for a running local Hub. It does not
embed `sorrel-core` or replace the `sorrel` CLI. Local workspace access remains
off until Sorrel's versioned embedding API is stable, so the app does not
create a second private VCS contract.
