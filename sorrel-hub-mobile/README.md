# Sorrel Hub mobile

Native iOS, iPadOS, and Android companion for Sorrel Hub, built with React
Native and Expo.

The app is deliberately a Hub companion: projects, reviews, discussion, and
repository refs work against the current Hub HTTP API. It does not embed the
local Sorrel engine yet. Core policy remains authoritative on the server.

## Experience

- Platform-native tab bars, navigation stacks, back gestures, large titles,
  search bars, and form sheets through Expo Router.
- The tab bar adapts to an iPad sidebar on supported iPadOS versions. Content
  grids and readable widths adapt across iPad and Android tablet layouts.
- Pull to refresh, haptic action feedback, Dynamic Type, dark mode, and system
  accessibility labels.
- Bearer credentials are stored in iOS Keychain / Android Keystore through
  SecureStore. The app never stores raw secret values from Sorrel Vault.

Native tabs are an Expo Router alpha API, pinned to Expo SDK 57. Both platform
exports are part of the package check so upgrades cannot drift silently.

## Run locally

```sh
npm ci
npm start         # scan with Expo Go on a physical device
npm run ios       # macOS + Xcode simulator/device
npm run android   # Android Studio emulator/device
```

Enter the externally reachable base URL of a running Hub, without `/api`.
Production and remote deployments should use HTTPS. Plain HTTP is supported
for a trusted local development network only; Sorrel Hub itself still requires
its explicit insecure-development opt-in before binding beyond loopback.

The connection screen accepts an optional OIDC bearer token. Tokens are never
prefilled after saving. In development auth mode, Settings exposes a clearly
labelled acting-principal selector for testing server-side policy behavior.

## Validate

```sh
npm run check
```

This typechecks and lints the app, runs pure behavior tests, checks Expo SDK and
config compatibility, and produces both iOS and Android JavaScript exports.
Store-signed binaries use `eas.json` and require maintainer-owned Expo, Apple,
and Google credentials.
