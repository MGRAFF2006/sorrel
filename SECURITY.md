# Security policy

## Supported releases

Sorrel is currently an alpha developer preview. Only the latest coordinated
alpha release receives security fixes.

## Reporting a vulnerability

Do not open a public issue for a suspected vulnerability. Use GitHub's private
security-advisory reporting for the affected `MGRAFF2006/sorrel*` repository,
including reproduction steps, affected versions, and expected impact.

## Alpha deployment boundary

The alpha Hub is a development service, not a production security boundary.
Published server images improve reproducibility and runtime isolation; they do
not change that support boundary:

- Keep it bound to localhost unless it is isolated by an authenticated reverse
  proxy and network policy.
- Local bootstrap grants are disabled by default. Enable
  `SORREL_HUB_BOOTSTRAP_GRANTS=1` only for an isolated development environment.
- Acting-principal headers are not cryptographic identity.
- The release Compose file binds published ports to `127.0.0.1` by default and
  deliberately refuses non-loopback development auth without an explicit
  insecure-demo override.
- Do not store production secrets in the local vault prototype or expose raw
  Hub data directories.

See `sorrel-hub/README.md` and `docs/STATUS.md` for current limitations.
