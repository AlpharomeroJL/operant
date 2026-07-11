// Mock OAuth server for the X16 broker's e2e tests (Node stdlib only --
// no npm install required). Implements exactly the endpoints in
// contracts/fixtures/oauth/config.json: authorize / token (also handles
// refresh, per that file's endpoint map) / revoke, with:
//   - PKCE S256 enforcement ("pkce": "S256 required; plain rejected")
//   - state and nonce echoed back on the authorize redirect
//   - loopback-only redirect_uri ("redirect": "loopback 127.0.0.1 with
//     ephemeral port only")
//   - refresh rotation ("refresh_rotates": true)
//   - an already-rotated/unknown refresh token returns 400
//     ("revoked_refresh_returns": 400)
//
// Contract-identical to (but independent of) the in-process Rust mock at
// crates/orchestrator/src/oauth/mock_server.rs, which `cargo test
// -p operant-orchestrator` uses instead so that suite never needs Node or
// a subprocess. This server is the standalone deliverable for
// cross-process / browser-driven e2e (see README.md).
//
// Usage:
//   node server.mjs [--port N]
// Prints one machine-parseable line once listening:
//   MOCK_OAUTH_LISTENING port=<port> base_url=http://127.0.0.1:<port>

import { createServer } from 'node:http';
import { randomBytes, createHash } from 'node:crypto';
import { pathToFileURL } from 'node:url';

function base64url(buffer) {
  return buffer.toString('base64').replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
}

function randomToken(byteLen = 24) {
  return base64url(randomBytes(byteLen));
}

// RFC 7636 S256: BASE64URL-ENCODE(SHA256(ASCII(verifier))).
function challengeFor(verifier) {
  return base64url(createHash('sha256').update(verifier, 'ascii').digest());
}

function sendJson(res, status, body) {
  const payload = Buffer.from(JSON.stringify(body));
  res.writeHead(status, {
    'Content-Type': 'application/json',
    'Content-Length': payload.length,
    Connection: 'close',
  });
  res.end(payload);
}

function sendRedirect(res, location) {
  res.writeHead(302, { Location: location, 'Content-Length': 0, Connection: 'close' });
  res.end();
}

function sendNotFound(res) {
  const payload = Buffer.from('not found');
  res.writeHead(404, { 'Content-Length': payload.length, Connection: 'close' });
  res.end(payload);
}

function readBody(req) {
  return new Promise((resolve, reject) => {
    const chunks = [];
    req.on('data', (chunk) => chunks.push(chunk));
    req.on('end', () => resolve(Buffer.concat(chunks).toString('utf8')));
    req.on('error', reject);
  });
}

/**
 * One mock provider instance: its own in-memory state (codes, tokens,
 * request counters) and the `http.Server` bound to it. Every test gets a
 * fresh instance rather than sharing module-level state, so parallel
 * tests never see each other's codes or tokens.
 */
export function createMockOauthServer() {
  const codes = new Map(); // code -> { clientId, codeChallenge, redirectUri, used }
  const refreshTokens = new Map(); // refresh_token -> { clientId }
  const accessTokens = new Map(); // access_token -> clientId
  const counters = { authorize: 0, token: 0, revoke: 0 };

  function handleAuthorize(query, res) {
    counters.authorize += 1;
    const get = (k) => query.get(k) ?? '';

    if (get('response_type') !== 'code') {
      return sendJson(res, 400, { error: 'unsupported_response_type' });
    }
    if (get('code_challenge_method') !== 'S256') {
      return sendJson(res, 400, {
        error: 'invalid_request',
        error_description: 'code_challenge_method must be S256',
      });
    }
    if (!get('code_challenge')) {
      return sendJson(res, 400, { error: 'invalid_request', error_description: 'code_challenge required' });
    }
    const redirectUri = get('redirect_uri');
    if (!redirectUri.startsWith('http://127.0.0.1:')) {
      return sendJson(res, 400, { error: 'invalid_request', error_description: 'redirect_uri must be loopback' });
    }
    const clientId = get('client_id');
    if (!clientId) {
      return sendJson(res, 400, { error: 'invalid_client' });
    }

    const code = `code-${randomToken(18)}`;
    codes.set(code, { clientId, codeChallenge: get('code_challenge'), redirectUri, used: false });

    const location = new URL(redirectUri);
    location.searchParams.set('code', code);
    location.searchParams.set('state', get('state'));
    location.searchParams.set('nonce', get('nonce'));
    sendRedirect(res, location.toString());
  }

  function handleAuthorizationCodeGrant(params, res) {
    const code = params.get('code') ?? '';
    const verifier = params.get('code_verifier') ?? '';
    const redirectUri = params.get('redirect_uri') ?? '';
    const clientId = params.get('client_id') ?? '';

    const record = codes.get(code);
    if (!record || record.used || record.clientId !== clientId || record.redirectUri !== redirectUri) {
      return sendJson(res, 400, { error: 'invalid_grant' });
    }
    if (challengeFor(verifier) !== record.codeChallenge) {
      return sendJson(res, 400, { error: 'invalid_grant', error_description: 'PKCE verification failed' });
    }
    record.used = true;

    const accessToken = `at-${randomToken(24)}`;
    const refreshToken = `rt-${randomToken(24)}`;
    accessTokens.set(accessToken, clientId);
    refreshTokens.set(refreshToken, { clientId });

    sendJson(res, 200, {
      access_token: accessToken,
      refresh_token: refreshToken,
      token_type: 'Bearer',
      expires_in: 3600,
      scope: 'model.complete',
    });
  }

  function handleRefreshGrant(params, res) {
    const refreshToken = params.get('refresh_token') ?? '';
    const clientId = params.get('client_id') ?? '';

    const record = refreshTokens.get(refreshToken);
    // "revoked_refresh_returns": 400 -- also covers "never issued".
    if (!record || record.clientId !== clientId) {
      return sendJson(res, 400, { error: 'invalid_grant' });
    }

    // "refresh_rotates": true -- the old refresh token stops working.
    refreshTokens.delete(refreshToken);
    const newAccess = `at-${randomToken(24)}`;
    const newRefresh = `rt-${randomToken(24)}`;
    accessTokens.set(newAccess, clientId);
    refreshTokens.set(newRefresh, { clientId });

    sendJson(res, 200, {
      access_token: newAccess,
      refresh_token: newRefresh,
      token_type: 'Bearer',
      expires_in: 3600,
      scope: 'model.complete',
    });
  }

  function handleToken(body, res) {
    counters.token += 1;
    const params = new URLSearchParams(body);
    const grantType = params.get('grant_type');
    if (grantType === 'authorization_code') return handleAuthorizationCodeGrant(params, res);
    if (grantType === 'refresh_token') return handleRefreshGrant(params, res);
    return sendJson(res, 400, { error: 'unsupported_grant_type' });
  }

  function handleRevoke(body, res) {
    counters.revoke += 1;
    const params = new URLSearchParams(body);
    const token = params.get('token') ?? '';
    refreshTokens.delete(token);
    accessTokens.delete(token);
    // Revoking an unknown token is not an error, per the contract.
    sendJson(res, 200, { revoked: true });
  }

  const server = createServer(async (req, res) => {
    const url = new URL(req.url, 'http://127.0.0.1');
    try {
      if (req.method === 'GET' && url.pathname === '/oauth/authorize') {
        return handleAuthorize(url.searchParams, res);
      }
      if (req.method === 'POST' && url.pathname === '/oauth/token') {
        return handleToken(await readBody(req), res);
      }
      if (req.method === 'POST' && url.pathname === '/oauth/revoke') {
        return handleRevoke(await readBody(req), res);
      }
      return sendNotFound(res);
    } catch (err) {
      sendJson(res, 500, { error: 'server_error', error_description: String(err) });
    }
  });

  server.counters = counters;
  return server;
}

// Standalone entry point: `node server.mjs [--port N]`. Compared via
// `pathToFileURL` (not a hand-rolled `file://` join) so this check is
// correct on Windows, where an absolute path's URL form is
// `file:///D:/...`, not `file://D:/...`.
const isMain = process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href;
if (isMain) {
  const portArgIndex = process.argv.indexOf('--port');
  const requestedPort = portArgIndex !== -1 ? Number(process.argv[portArgIndex + 1]) : 0;
  const server = createMockOauthServer();
  server.listen(requestedPort, '127.0.0.1', () => {
    const { port } = server.address();
    console.log(`MOCK_OAUTH_LISTENING port=${port} base_url=http://127.0.0.1:${port}`);
  });

  process.on('SIGINT', () => server.close(() => process.exit(0)));
  process.on('SIGTERM', () => server.close(() => process.exit(0)));
}
