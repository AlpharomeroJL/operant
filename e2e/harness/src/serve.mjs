// Minimal static HTTP server for contracts/fixtures/webapp (index.html,
// drift.html). No framework, no build step: Playwright's webServer config
// shells out to this so the browser-driven capture tests have something
// real to load over http:// (file:// breaks localStorage semantics that
// the fixture app relies on).
//
// Usage:
//   node serve.mjs [--port N]
// Prints one machine-parseable line once listening:
//   HARNESS_SERVE_LISTENING port=<port> root=<dir>

import { createServer } from 'node:http';
import { readFile, stat } from 'node:fs/promises';
import { extname, join, normalize, sep } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = fileURLToPath(new URL('.', import.meta.url));
export const FIXTURE_ROOT = normalize(join(here, '..', '..', '..', 'contracts', 'fixtures', 'webapp'));

const MIME = {
  '.html': 'text/html; charset=utf-8',
  '.js': 'text/javascript; charset=utf-8',
  '.css': 'text/css; charset=utf-8',
  '.json': 'application/json; charset=utf-8',
  '.png': 'image/png',
  '.svg': 'image/svg+xml',
};

export function createFixtureServer(root = FIXTURE_ROOT) {
  return createServer(async (req, res) => {
    try {
      const url = new URL(req.url, 'http://localhost');
      let relPath = decodeURIComponent(url.pathname);
      if (relPath === '/') relPath = '/index.html';
      // Reject path traversal outside the fixture root.
      const target = normalize(join(root, relPath));
      if (!target.startsWith(normalize(root) + sep) && target !== normalize(root)) {
        res.writeHead(403).end('forbidden');
        return;
      }
      const info = await stat(target).catch(() => null);
      if (!info || !info.isFile()) {
        res.writeHead(404).end('not found');
        return;
      }
      const body = await readFile(target);
      const type = MIME[extname(target)] || 'application/octet-stream';
      res.writeHead(200, { 'Content-Type': type, 'Content-Length': body.length });
      res.end(body);
    } catch (err) {
      res.writeHead(500).end(String(err && err.message ? err.message : err));
    }
  });
}

function parsePort(argv) {
  const idx = argv.indexOf('--port');
  if (idx !== -1 && argv[idx + 1]) return Number(argv[idx + 1]);
  return Number(process.env.PORT) || 4173;
}

async function main() {
  const port = parsePort(process.argv.slice(2));
  const server = createFixtureServer();
  await new Promise((resolve) => server.listen(port, '127.0.0.1', resolve));
  console.log(`HARNESS_SERVE_LISTENING port=${port} root=${FIXTURE_ROOT}`);
}

const isDirectRun = process.argv[1] && fileURLToPath(import.meta.url) === normalize(process.argv[1]);
if (isDirectRun) {
  main().catch((err) => {
    console.error('serve.mjs failed:', err);
    process.exit(1);
  });
}
