# sorrel-sdk-js

Experimental, minimal JavaScript client for the Sorrel Hub HTTP API.

Talks to a real `sorrel-hub` process — no mocks. Mirrors Hub routes
(`/healthz`, `/projects`, `/admin/*`, sync refs) without inventing permission
logic.

This alpha is an HTTP client for the current Hub API. It is not a stable Sorrel
embedding SDK and does not yet provide broad Core, protocol, or CLI bindings.
Expect the API and supported routes to change before a stable release.

## Usage

```js
import { HubClient } from '@sorrel/sdk-js';

const hub = new HubClient({ baseUrl: 'http://127.0.0.1:3000' });
await hub.health();
await hub.listProjects();
await hub.listSyncRepos();
```

Remote OIDC deployments can pass an access token. The client sends it as a
Bearer credential and never includes it in request errors:

```js
const hub = new HubClient({ baseUrl, accessToken });
await hub.session();
```

TypeScript declarations are included with the package.

## Checks

```sh
npm run check
```
