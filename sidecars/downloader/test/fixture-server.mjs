// Tiny static file server with HTTP Range support, for exercising the
// downloader against contracts/fixtures/model_download without any real
// network access. Records per-request byte counts so tests can prove a
// resumed download did not re-fetch bytes already on disk.
//
// Named without a "test" prefix on purpose: Node's test runner treats any
// `test-*.mjs` file, or any file at all under a directory literally named
// `test/`, as a test file to execute. This is a fixture helper, not a test;
// see downloader.test.mjs for the actual tests.
import fs from "node:fs";
import http from "node:http";

/**
 * @param {string} filePath - Absolute path to the file to serve at any path (method and URL are ignored; this is a single-fixture server).
 * @param {object} [opts]
 * @param {{ chunkBytes?: number, delayMs?: number }} [opts.throttle] - When
 *   set, the response body is written in `chunkBytes`-sized pieces with a
 *   `delayMs` pause between each, instead of piping the file at full speed.
 *   A 256 KB fixture over loopback can otherwise finish in well under a
 *   millisecond, which leaves no real window for a test to abort "partway".
 *   Throttling trades a slower test for a deterministic one.
 * @returns {Promise<{server: import("node:http").Server, port: number, baseUrl: string, requests: Array<object>, close: () => Promise<void>}>}
 */
export function startRangeServer(filePath, opts = {}) {
  const stat = fs.statSync(filePath);
  const requests = [];
  const throttle = opts.throttle || null;

  const server = http.createServer((req, res) => {
    const record = {
      method: req.method,
      rangeHeader: req.headers.range || null,
      status: null,
      start: 0,
      end: stat.size - 1,
      bytesServed: 0,
    };
    requests.push(record);

    let start = 0;
    let end = stat.size - 1;
    let status = 200;

    if (req.headers.range) {
      const m = /^bytes=(\d+)-(\d*)$/.exec(req.headers.range);
      if (m) {
        start = Number(m[1]);
        end = m[2] ? Number(m[2]) : stat.size - 1;
        status = 206;
      }
    }

    record.status = status;
    record.start = start;
    record.end = end;

    let clientGone = false;
    res.on("error", () => {
      clientGone = true;
      /* client disconnected mid-response (interruption test); nothing to do */
    });
    res.on("close", () => {
      clientGone = true;
    });

    if (start >= stat.size) {
      res.writeHead(416, { "Content-Range": `bytes */${stat.size}` });
      res.end();
      return;
    }

    res.writeHead(status, {
      "Content-Type": "application/octet-stream",
      "Accept-Ranges": "bytes",
      "Content-Length": String(end - start + 1),
      ...(status === 206 ? { "Content-Range": `bytes ${start}-${end}/${stat.size}` } : {}),
    });

    if (!throttle) {
      const stream = fs.createReadStream(filePath, { start, end });
      stream.on("data", (chunk) => {
        record.bytesServed += chunk.length;
      });
      stream.on("error", () => {
        /* stream torn down because the client disconnected; expected during the abort test */
      });
      stream.pipe(res);
      return;
    }

    // Throttled path: read and write fixed-size chunks with a delay between
    // each, so a test has a real, controllable window in which to abort.
    const chunkBytes = throttle.chunkBytes || 8192;
    const delayMs = throttle.delayMs ?? 15;
    const fd = fs.openSync(filePath, "r");
    let offset = start;

    const cleanupFd = () => {
      try {
        fs.closeSync(fd);
      } catch {
        /* already closed */
      }
    };

    const pump = () => {
      if (clientGone || res.destroyed) {
        cleanupFd();
        return;
      }
      if (offset > end) {
        cleanupFd();
        res.end();
        return;
      }
      const want = Math.min(chunkBytes, end - offset + 1);
      const buf = Buffer.alloc(want);
      let read;
      try {
        read = fs.readSync(fd, buf, 0, want, offset);
      } catch (err) {
        cleanupFd();
        res.destroy(err);
        return;
      }
      if (read <= 0) {
        cleanupFd();
        res.end();
        return;
      }
      offset += read;
      record.bytesServed += read;
      const slice = read === want ? buf : buf.subarray(0, read);
      const ok = res.write(slice);
      if (clientGone) {
        cleanupFd();
        return;
      }
      if (ok) {
        setTimeout(pump, delayMs);
      } else {
        res.once("drain", () => setTimeout(pump, delayMs));
      }
    };

    pump();
  });

  return new Promise((resolve, reject) => {
    server.on("error", reject);
    server.listen(0, "127.0.0.1", () => {
      const addr = server.address();
      resolve({
        server,
        port: addr.port,
        baseUrl: `http://127.0.0.1:${addr.port}`,
        requests,
        // server.close() alone only stops accepting new connections; it
        // waits for existing ones to end on their own, which for an idle
        // keep-alive socket means Node's default 5 s keepAliveTimeout. Force
        // every socket closed so teardown between tests is immediate.
        close: () =>
          new Promise((res) => {
            server.close(() => res());
            server.closeAllConnections();
          }),
      });
    });
  });
}
