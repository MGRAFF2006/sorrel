# Agent instructions for sorrel-hub-mobile

## What this module is

The native React Native / Expo companion for Sorrel Hub. One codebase targets
iPhone, iPad, Android phones, and Android tablets.

## Boundaries

- Use Expo Router native stacks and native tabs for platform navigation.
- Keep layouts adaptive; every primary screen must remain usable at phone and
  tablet widths, portrait and landscape.
- Talk to Hub through `@sorrel/sdk-js`. Do not duplicate Core authorization or
  claim on-device VCS access before an embedding surface ships.
- Store bearer credentials only with `expo-secure-store`. Never log, display,
  fixture, or commit them.
- Keep developer acting principals visibly marked as development-only. A
  production AuthAdapter session always wins at the Hub.
- `expo-router/unstable-native-tabs` is intentionally pinned to the Expo SDK;
  validate iOS and Android exports before upgrading it.

## Common checks

```sh
npm ci
npm run check
```

Use `npm run ios` or `npm run android` for device development. Native store
builds use the profiles in `eas.json` and require the maintainer's Expo and
store credentials; do not add those credentials to the repository.
