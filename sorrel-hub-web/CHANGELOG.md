# Changelog

Notable changes to the Sorrel Hub web companion are documented here.

## [0.1.0-alpha.1] - 2026-07-30

### Added

- Framework-free Projects, Reviews, Administration, and Sync views over the
  Sorrel Hub API.
- Project, proposal, review-comment, and workflow-status mutation flows.
- Acting-principal headers on every mutation, consistent with the Sorrel
  JavaScript SDK.
- Development server and `/api/*` proxy for local and container use.

### Alpha limitations

- This is a development-only companion to a compatible `sorrel-hub`; it has no
  login, SSO, or production authentication.
- Mutations use the development principal `user:local`; the acting-principal
  header is not proof of identity.
- Sync is read-only in the browser.
- Review UX is intentionally minimal, with no merge queue or conflict editor.
