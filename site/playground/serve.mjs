// Minimal static HTTP server for this directory (site/playground itself:
// index.html, playground.js/css, pkg/, fixtures/). No framework, no build
// step. Playwright's webServer config shells out to this so the playground
// test loads over http:// (the wasm module's own fetches, and the fixture
// app's localStorage use, both need a real origin, not file://).
//
// Usage:
//   node serve.mjs [--port N]
// Prints one machine-parseable line once listening:
//   PLAYGROUND_SERVE_LISTENING port=<port> root=<dir>

import { createServer } from "node:http";
import { readFile, stat } from "node:fs/promises";
import { extname, join, normalize, sep } from "node:path";
import { fileURLToPath } from "node:url";

// `join(..., ".")` normalizes away the trailing separator
// `fileURLToPath` leaves on a directory URL, so the containment check
// below (`root + sep` prefix) does not double up the separator and reject
// every path.
const ROOT = normalize(join(fileURLToPath(new URL(".", import.meta.url)), "."));

const MIME = {
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".mjs": "text/javascript; charset=utf-8",
  ".css": "text/css; charset=utf-8",
  ".json": "application/json; charset=utf-8",
  ".wasm": "application/wasm",
  ".ts": "text/plain; charset=utf-8",
};

export function createPlaygroundServer(root = ROOT) {
  return createServer(async (req, res) => {
    try {
      const url = new URL(req.url, "http://localhost");
      let relPath = decodeURIComponent(url.pathname);
      if (relPath === "/") relPath = "/index.html";
      const target = normalize(join(root, relPath));
      if (!target.startsWith(normalize(root) + sep) && target !== normalize(root)) {
        res.writeHead(403).end("forbidden");
        return;
      }
      const info = await stat(target).catch(() => null);
      if (!info || !info.isFile()) {
        res.writeHead(404).end("not found");
        return;
      }
      const body = await readFile(target);
      const type = MIME[extname(target)] || "application/octet-stream";
      res.writeHead(200, { "Content-Type": type, "Content-Length": body.length });
      res.end(body);
    } catch (err) {
      res.writeHead(500).end(String(err && err.message ? err.message : err));
    }
  });
}

function parsePort(argv) {
  const idx = argv.indexOf("--port");
  if (idx !== -1 && argv[idx + 1]) return Number(argv[idx + 1]);
  return Number(process.env.PORT) || 4174;
}

async function main() {
  const port = parsePort(process.argv.slice(2));
  const server = createPlaygroundServer();
  await new Promise((resolve) => server.listen(port, "127.0.0.1", resolve));
  console.log(`PLAYGROUND_SERVE_LISTENING port=${port} root=${ROOT}`);
}

const isDirectRun =
  process.argv[1] && fileURLToPath(import.meta.url) === normalize(process.argv[1]);
if (isDirectRun) {
  main().catch((err) => {
    console.error("serve.mjs failed:", err);
    process.exit(1);
  });
}
