# Mock OAuth Server

Standalone mock OAuth provider for the X16 broker's e2e tests. Node stdlib
only (`node:http`, `node:crypto`) -- no `npm install` needed. Implements
exactly the endpoints in `contracts/fixtures/oauth/config.json`:

| Endpoint | Method | Behavior |
| --- | --- | --- |
| `/oauth/authorize` | GET | Validates PKCE (S256 only -- `plain` and any other/missing method get 400) and that `redirect_uri` is loopback (`http://127.0.0.1:*`), mints a one-time code, redirects (302) to `redirect_uri` with `code`, `state`, and `nonce` echoed. |
| `/oauth/token` | POST | `grant_type=authorization_code`: verifies PKCE (`SHA256(code_verifier)` matches the challenge from `authorize`) and issues tokens. `grant_type=refresh_token`: rotates -- issues a new access **and** refresh token, invalidates the old refresh token. An unknown or already-rotated refresh token returns 400. |
| `/oauth/revoke` | POST | Revokes the given `token`. Always 200, including for an unknown token. |

Every response other than a redirect is JSON; every response includes
`Connection: close`.

## Run standalone

```bash
node server.mjs [--port N]
```

Prints one line once listening, so a driving script can read the
ephemeral port:

```
MOCK_OAUTH_LISTENING port=54321 base_url=http://127.0.0.1:54321
```

## Self-test

```bash
node smoke-test.mjs
# or: npm run smoke
```

Drives the full contract over real loopback HTTP (`fetch`): PKCE
plain/non-loopback rejection, the full authorize -> callback with code ->
token -> refresh -> revoke sequence, refresh rotation, code single-use,
and revoked/rotated-token 400s. Prints `ok - <check>` / `FAIL - <check>`
per assertion and exits non-zero on any failure.

## Use from a driving test

`createMockOauthServer()` (exported from `server.mjs`) returns a fresh,
unstarted `http.Server` with its own isolated in-memory state -- call
`.listen(0, '127.0.0.1', cb)` for an ephemeral port and `.close(cb)` when
done, same as any Node HTTP server. Each instance is independent, so
parallel tests never share codes or tokens.

```js
import { createMockOauthServer } from './server.mjs';

const server = createMockOauthServer();
await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
const baseUrl = `http://127.0.0.1:${server.address().port}`;
// ... drive the broker under test against baseUrl ...
await new Promise((resolve) => server.close(resolve));
```

## Relationship to the Rust test suite

`cargo test -p operant-orchestrator` does **not** spawn this server -- it
uses an independent, contract-identical in-process mock
(`crates/orchestrator/src/oauth/mock_server.rs`, Rust, `#[cfg(test)]`
only) so that suite stays hermetic with no Node dependency and no
subprocess/port-coordination flakiness. This server is the standalone
deliverable for cross-process or browser-driven e2e coverage (owned by
X16 per `contracts/fixtures/oauth/config.json`).
