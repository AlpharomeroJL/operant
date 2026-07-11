// Self-test for server.mjs: drives the mock provider over real loopback
// HTTP (Node's built-in `fetch`), the same way a broker or a browser
// would. No test framework dependency -- prints `ok - <name>` /
// `FAIL - <name>` per check and exits non-zero if anything failed.
//
// Run: node smoke-test.mjs   (or: npm run smoke)

import { randomBytes, createHash } from 'node:crypto';
import { createMockOauthServer } from './server.mjs';

function base64url(buffer) {
  return buffer.toString('base64').replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
}

function verifierAndChallenge() {
  const verifier = base64url(randomBytes(32));
  const challenge = base64url(createHash('sha256').update(verifier, 'ascii').digest());
  return { verifier, challenge };
}

let failures = 0;
function check(name, condition) {
  if (condition) {
    console.log(`ok - ${name}`);
  } else {
    console.error(`FAIL - ${name}`);
    failures += 1;
  }
}

async function main() {
  const server = createMockOauthServer();
  await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
  const { port } = server.address();
  const base = `http://127.0.0.1:${port}`;

  // --- PKCE plain is rejected ---------------------------------------------
  {
    const url = new URL(`${base}/oauth/authorize`);
    url.searchParams.set('response_type', 'code');
    url.searchParams.set('client_id', 'smoke-client');
    url.searchParams.set('redirect_uri', 'http://127.0.0.1:1/callback');
    url.searchParams.set('state', 's1');
    url.searchParams.set('nonce', 'n1');
    url.searchParams.set('code_challenge', 'whatever');
    url.searchParams.set('code_challenge_method', 'plain');
    const res = await fetch(url, { redirect: 'manual' });
    check('authorize rejects code_challenge_method=plain with 400', res.status === 400);
  }

  // --- non-loopback redirect_uri is rejected ------------------------------
  {
    const url = new URL(`${base}/oauth/authorize`);
    url.searchParams.set('response_type', 'code');
    url.searchParams.set('client_id', 'smoke-client');
    url.searchParams.set('redirect_uri', 'https://evil.example/callback');
    url.searchParams.set('state', 's1');
    url.searchParams.set('nonce', 'n1');
    url.searchParams.set('code_challenge', 'whatever');
    url.searchParams.set('code_challenge_method', 'S256');
    const res = await fetch(url, { redirect: 'manual' });
    check('authorize rejects a non-loopback redirect_uri with 400', res.status === 400);
  }

  // --- full flow: authorize -> callback with code -> token -> refresh -> revoke
  const clientId = 'smoke-client';
  const redirectUri = 'http://127.0.0.1:9999/callback'; // never dialed; only echoed back
  const { verifier, challenge } = verifierAndChallenge();
  const state = base64url(randomBytes(8));
  const nonce = base64url(randomBytes(8));

  let code;
  {
    const url = new URL(`${base}/oauth/authorize`);
    url.searchParams.set('response_type', 'code');
    url.searchParams.set('client_id', clientId);
    url.searchParams.set('redirect_uri', redirectUri);
    url.searchParams.set('state', state);
    url.searchParams.set('nonce', nonce);
    url.searchParams.set('code_challenge', challenge);
    url.searchParams.set('code_challenge_method', 'S256');
    const res = await fetch(url, { redirect: 'manual' });
    check('authorize with a valid S256 request redirects (302)', res.status === 302);
    const location = new URL(res.headers.get('location'));
    code = location.searchParams.get('code');
    check('redirect echoes the original state', location.searchParams.get('state') === state);
    check('redirect echoes the original nonce', location.searchParams.get('nonce') === nonce);
    check('redirect carries a code', typeof code === 'string' && code.length > 0);
  }

  let accessToken;
  let refreshToken;
  {
    const body = new URLSearchParams({
      grant_type: 'authorization_code',
      code,
      redirect_uri: redirectUri,
      client_id: clientId,
      code_verifier: verifier,
    });
    const res = await fetch(`${base}/oauth/token`, { method: 'POST', body });
    check('token exchange succeeds (200)', res.status === 200);
    const json = await res.json();
    accessToken = json.access_token;
    refreshToken = json.refresh_token;
    check('token exchange returns an access_token', typeof accessToken === 'string' && accessToken.startsWith('at-'));
    check('token exchange returns a refresh_token', typeof refreshToken === 'string' && refreshToken.startsWith('rt-'));
  }

  // A spent authorization code must not be redeemable twice.
  {
    const body = new URLSearchParams({
      grant_type: 'authorization_code',
      code,
      redirect_uri: redirectUri,
      client_id: clientId,
      code_verifier: verifier,
    });
    const res = await fetch(`${base}/oauth/token`, { method: 'POST', body });
    check('reusing a spent authorization code is rejected (400)', res.status === 400);
  }

  let rotatedRefresh;
  {
    const body = new URLSearchParams({ grant_type: 'refresh_token', refresh_token: refreshToken, client_id: clientId });
    const res = await fetch(`${base}/oauth/token`, { method: 'POST', body });
    check('refresh succeeds (200)', res.status === 200);
    const json = await res.json();
    rotatedRefresh = json.refresh_token;
    check('refresh rotates the access token', json.access_token !== accessToken);
    check('refresh rotates the refresh token', rotatedRefresh !== refreshToken);
  }

  // The pre-rotation refresh token is now dead.
  {
    const body = new URLSearchParams({ grant_type: 'refresh_token', refresh_token: refreshToken, client_id: clientId });
    const res = await fetch(`${base}/oauth/token`, { method: 'POST', body });
    check('a rotated-away refresh token returns 400', res.status === 400);
  }

  {
    const body = new URLSearchParams({ token: rotatedRefresh, token_type_hint: 'refresh_token', client_id: clientId });
    const res = await fetch(`${base}/oauth/revoke`, { method: 'POST', body });
    check('revoke succeeds (200)', res.status === 200);
  }

  {
    const body = new URLSearchParams({ grant_type: 'refresh_token', refresh_token: rotatedRefresh, client_id: clientId });
    const res = await fetch(`${base}/oauth/token`, { method: 'POST', body });
    check('a revoked refresh token returns 400', res.status === 400);
  }

  {
    const body = new URLSearchParams({ token: 'never-issued', client_id: clientId });
    const res = await fetch(`${base}/oauth/revoke`, { method: 'POST', body });
    check('revoking an unknown token is still 200 (idempotent)', res.status === 200);
  }

  await new Promise((resolve) => server.close(resolve));

  console.log(`\n${failures === 0 ? 'PASS' : 'FAIL'} (${failures} failure(s))`);
  // Set the exit code and let the event loop drain naturally rather than
  // forcing `process.exit()` here: a forced exit immediately after
  // `server.close()`'s callback races libuv's own handle teardown on
  // Windows (observed as `Assertion failed: !(handle->flags &
  // UV_HANDLE_CLOSING)`), which is a shutdown-ordering artifact, not a
  // server-logic bug -- every check above already passed or failed on its
  // own merits before this point.
  process.exitCode = failures === 0 ? 0 : 1;
}

main().catch((err) => {
  console.error(err);
  process.exitCode = 1;
});
