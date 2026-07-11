# sidecars/downloader

Resumable, checksum-verified file downloader for the zero-code wizard's
"Download a free brain" (local model) setup path, per `docs/specs/zero-code.md`
and `docs/specs/backends.md`. Owned by lane U2A. Zero dependencies beyond
Node core, so it runs headless with no toolchain install.

Two ways to use it:

- **As a Node library**: `import { downloadFile } from "./index.mjs"`.
- **As a subprocess (sidecar)**: `node cli.mjs --url ... --dest ...`, for a
  non-Node host (the Tauri UI, U2B) to spawn and read newline-delimited JSON
  from. This is the "sidecar protocol" referenced in the lane brief.

## Files

| File | Purpose |
|---|---|
| `index.mjs` | The library: `downloadFile()`, `sha256File()`, `parseSumsFile()`, `DownloadError`. |
| `cli.mjs` | Subprocess entry point wrapping `downloadFile()`. This is what U2B spawns. |
| `wizard_copy.json` | Plain-language copy for the welcome, setup-path, mic-check, and schedule screens. |
| `check_wizard_copy.mjs` | Gate: fails if any string in `wizard_copy.json` uses a glossary internal term or an em dash. |
| `test/downloader.test.mjs` | The bar test. `node test/downloader.test.mjs`. |
| `test/fixture-server.mjs` | A 127.0.0.1 static file server with HTTP Range support, used only by tests. |

## Library API (`index.mjs`)

### `downloadFile(opts) -> Promise<Result>`

```js
import { downloadFile } from "./index.mjs";

const result = await downloadFile({
  url: "https://example.com/model.bin",   // http(s)://, file://, or a bare filesystem path
  dest: "/path/to/model.bin",              // destination file path
  sumsFile: "/path/to/SHA256SUMS",         // looked up by basename(dest); OR:
  sha256: "abc123...",                     // explicit digest, takes priority over sumsFile
  onProgress: (p) => console.log(p),       // { bytesReceived, bytesTotal, percent }
  signal: abortController.signal,          // optional: abort to pause
});
// => { path, bytesWritten, bytesTotal, sha256, resumedFrom, alreadyComplete }
```

Skipping both `sumsFile` and `sha256` disables verification entirely; the
wizard's local-model path must always pass one of them to get the
fail-closed guarantee below.

**Resume semantics.** Every call writes to `<dest>.part` first, then
verifies and renames it to `dest`. If `<dest>.part` already has bytes on
disk (a prior attempt was interrupted), the next call sends
`Range: bytes=<existing size>-` and appends only what is missing. If the
server does not honor the Range header (answers `200` instead of `206`),
the part file is restarted from scratch rather than silently corrupted by
blind appending. If the part file already covers the whole resource (the
server answers `416`, e.g. a process died between finishing the transfer
and finishing verification), no bytes are re-fetched at all; the file goes
straight to the checksum step.

**Fail closed.** If the verified digest does not match, the part file is
deleted and `dest` is never created or left in a stale state. A caller can
never observe a corrupt or half-written file at `dest`: it is either absent
or fully verified.

**Safe to kill at any point, not just `signal.abort()`.** The resume
guarantee is a property of what is actually on disk, not of how the
previous attempt ended. Every received chunk is written to `<dest>.part`
before the next chunk is read, so an ungraceful process kill (Windows
`TerminateProcess`, `SIGKILL`, a crash) leaves a valid, if incomplete,
prefix on disk; the next call resumes from `fs.stat(partPath).size`
regardless of why the previous attempt stopped.

**Error codes** (`err.code` on a thrown `DownloadError`):

| Code | Meaning |
|---|---|
| `BAD_ARGS` | `url` or `dest` missing. |
| `NOT_FOUND` | Local/file:// source, or the `sumsFile` itself, does not exist. |
| `SUMS_ENTRY_MISSING` | `sumsFile` exists but has no entry for `basename(dest)`. |
| `HTTP_ERROR` | Non-2xx/3xx/416 status, too many redirects, or a transport error. |
| `READ_ERROR` / `WRITE_ERROR` | Local filesystem failure reading the source or writing the part file. |
| `ABORTED` | `signal` was aborted mid-transfer. `err.resumedFrom` has the byte offset to resume from (it is also just `fs.stat(partPath).size`). |
| `CHECKSUM_MISMATCH` | Transfer finished but the digest did not match. `err.expected` / `err.actual` carry both digests. |
| `UNKNOWN` | Anything unanticipated; `err.cause` has the original error. |

### `sha256File(path) -> Promise<string>`, `parseSumsFile(text) -> Map<filename, hexDigest>`

Small exported helpers `cli.mjs` and the tests both use; useful to a caller
that wants to pre-check a file without downloading.

## Sidecar protocol (`cli.mjs`)

This is the contract for U2B (or any non-Node host) to wire the wizard UI
to a real download.

### Invocation

```
node cli.mjs --url <url> --dest <path> [--sums <file> | --sha256 <hex>] [options]
```

| Flag | Required | Meaning |
|---|---|---|
| `--url` | yes | `http://`, `https://`, `file://`, or a bare filesystem path. |
| `--dest` | yes | Destination file path. |
| `--sums` | one of these two | Path to a SHA256SUMS-style file. |
| `--sha256` | | Expected digest directly; wins over `--sums` if both given. |
| `--part-suffix` | | In-progress file suffix. Default `.part`. |
| `--progress-interval-ms` | | Minimum ms between `download.progress` events. Default `100`. |
| `--help`, `-h` | | Print usage to stdout and exit 0. |

One process, one download. There is no interactive stdin protocol: `--help`
aside, `stdin` is not read.

### stdout: newline-delimited JSON, one envelope per line

```json
{"v":1,"seq":0,"ts":"2026-07-11T12:00:00.000Z","topic":"download.started","payload":{"url":"...","dest":"...","sums":"...","sha256":null,"resumedFrom":0}}
{"v":1,"seq":1,"ts":"2026-07-11T12:00:00.050Z","topic":"download.progress","payload":{"bytesReceived":81920,"bytesTotal":262144,"percent":31.25}}
{"v":1,"seq":2,"ts":"2026-07-11T12:00:00.400Z","topic":"download.completed","payload":{"path":"...","bytesWritten":262144,"bytesTotal":262144,"sha256":"...","resumedFrom":0,"alreadyComplete":false}}
```

The envelope (`v`, `seq`, `ts`, `topic`, `payload`) is the same shape as
`contracts/bus_events.md`, so a host that already parses that envelope
elsewhere can reuse the same deserializer. The `download.*` topic family
below is this sidecar's own local vocabulary; it is **not yet** part of
`contracts/bus_events.md` (see Follow-ups). Do not rely on any topic other
than the ones listed here.

| Topic | When | Payload |
|---|---|---|
| `download.started` | Once, before any network activity. | `{ url, dest, sums, sha256, resumedFrom }` |
| `download.progress` | Repeatedly while transferring, throttled to `--progress-interval-ms` (the 100%/final update is never throttled away). | `{ bytesReceived, bytesTotal, percent }`, same shape as `onProgress` above. |
| `download.completed` | Once, on success. Last line, exit 0. | The `downloadFile()` result: `{ path, bytesWritten, bytesTotal, sha256, resumedFrom, alreadyComplete }` |
| `download.failed` | Once, on any non-abort failure. Last line, exit 1. | `{ code, message, resumedFrom }`, `code` is a `DownloadError` code from the table above. |
| `download.paused` | Once, only when `SIGINT`/`SIGTERM` triggered the stop (see below). Last line, exit 130. | `{ resumedFrom }` |

`stderr` carries only human-readable usage diagnostics (missing/bad flags)
and is never part of the protocol; a host can log it verbatim or ignore it.

### Exit codes

| Code | Meaning |
|---|---|
| 0 | `download.completed` emitted. The file at `--dest` is verified. |
| 1 | `download.failed` emitted. `--dest` does not exist (fail closed). |
| 2 | Bad invocation (missing `--url`/`--dest`, or an unparsable flag). No NDJSON emitted; message on stderr only. |
| 130 | Interrupted by `SIGINT`/`SIGTERM` before finishing. `--dest` does not exist yet, but `<dest>.part` has whatever was already received. |

### Pausing and resuming a download

There is no in-process pause command. A host pauses by stopping the child
process, and resumes by starting a new one with the same `--dest` (and,
for `--sums`-based verification, the fixture/model directory unchanged):

1. **Pause**: kill the child however the host normally kills a child
   process (on Windows this is ordinarily an unconditional terminate; there
   is no graceful shutdown window to rely on). `cli.mjs` also listens for
   `SIGINT`/`SIGTERM` and will emit a clean `download.paused` plus exit 130
   when the host and platform support delivering them, but nothing about
   correctness depends on that handler running.
2. **Resume**: spawn `cli.mjs` again with the same `--url --dest --sums`.
   It re-checks `<dest>.part`, issues a Range request for only the missing
   bytes when the server supports it, and re-verifies the digest before
   ever producing `--dest`. This is exactly what the "Resume" button on the
   local-model wizard card should do: relaunch the same command.
3. **Cancel** (not pause): kill the child, then delete `<dest>.part`
   yourself. `cli.mjs` does not delete partial files on an external kill,
   since that is what makes pause-then-resume possible.

## `wizard_copy.json` and `check_wizard_copy.mjs`

`wizard_copy.json` holds the plain-language copy for the screens this lane
owns: welcome, the four thinking-power setup-path cards (Sign in with
ChatGPT, Sign in with Claude, Download a free brain, I have an access key)
plus the "Just show me a demo" link, the microphone check, and the
schedule screen, per `docs/specs/zero-code.md`. It follows the
`operant-ux` rule set: no jargon, and every error entry is a `what` / `why`
/ `action` triple (one sentence each), matching the error-catalog shape in
`docs/specs/zero-code.md`.

`contracts/microcopy_glossary.json` is the append-only glossary of internal
terms mapped to their user-facing replacement (e.g. `API key` to
`access key`, `VRAM` to `graphics memory`). `check_wizard_copy.mjs` walks
every string value in `wizard_copy.json` and fails if any glossary internal
term appears (word-boundary, case-insensitive, mirroring
`scripts/microcopy_lint.mjs`) or if an em dash / horizontal bar appears.
Run it with:

```
node check_wizard_copy.mjs
```

`scripts/microcopy_lint.mjs` (the repo-wide `just check-microcopy` gate)
only scans `ui/src` and `ui/src-tauri/locales` today, so it does not see
this file; `check_wizard_copy.mjs` is the interim, self-contained gate for
this directory (see Follow-ups).

## Testing

```
node test/downloader.test.mjs
```

also runs via `npm test` (`node --test test/downloader.test.mjs`). No
external network access: `test/fixture-server.mjs` starts a real
127.0.0.1 HTTP server (a fresh ephemeral port per test) serving
`contracts/fixtures/model_download/model.bin`, with HTTP Range support and
per-request byte-served tracking. Covers:

1. **Fresh download**: progress events well-formed and monotonic, output
   byte-identical to the fixture, digest matches `SHA256SUMS`.
2. **Resumable interruption** (the bar test): start a download against a
   throttled server, abort partway via `AbortController` once a real
   partial file is on disk, restart with a fresh `downloadFile()` call, and
   assert the restart's `Range` header starts exactly at the bytes already
   on disk, the server's own per-request byte counter shows pass 2 served
   only the missing tail (never the already-downloaded prefix), and the
   final file is complete and checksum-valid.
3. **Checksum mismatch fails closed**: a server serving corrupted bytes
   under a filename the real `SHA256SUMS` has a (different, correct) entry
   for. Asserts `dest` is never created and the bad part file is deleted.
4. **Resume from an already-complete-but-unverified part file**: simulates
   a crash between finishing the transfer and finishing verification.
   Asserts zero bytes are re-fetched.
5. **Bare-path and `file://` sources**: the non-HTTP transfer path resumes
   by byte offset the same way, and the idempotent already-verified
   short circuit works for a `file://` URL too.
6. **`cli.mjs` protocol, success and failure**: spawns the real subprocess,
   parses its stdout as NDJSON, and checks the envelope shape, exit code,
   and final file against the same fixture.

## Decisions

- **Envelope shape borrowed from `contracts/bus_events.md`, topics are
  not.** `download.*` is this sidecar's own vocabulary so a future lane can
  fold it into the real bus without a shape change; see Follow-ups.
- **Pause is process-kill, not a stdin command.** Simpler, and the
  correctness of resuming does not depend on how the previous process
  died, so there is nothing a richer in-process pause protocol would add
  for this sidecar. If a future need for lower-latency pause/resume inside
  a single long-lived process shows up, revisit.
- **`downloadFile()` re-hashes an existing `dest` on every call when a
  digest is available**, so a second call after a fully successful run is
  a fast verify-and-return rather than a silent no-op. For a small fixture
  this is free; for a multi-gigabyte model this is a real (if one-time,
  disk-bound) cost worth knowing about before wiring a UI that might call
  it speculatively on every screen render.

## Follow-ups

- `download.*` is not yet a documented topic family in
  `contracts/bus_events.md`. Whoever wires the real bus for this sidecar
  (U2B, or a dedicated contracts change) should add it there; the payload
  shapes above are meant to be copied in directly.
- `scripts/microcopy_lint.mjs` does not scan `sidecars/`. Either add a scan
  root there for wizard copy files repo-wide, or treat each sidecar's own
  local check script (this one included) as the permanent gate.
- No bandwidth cap: a real multi-gigabyte model download has no
  server-independent throttle. Not needed for correctness, but worth
  knowing if a low-bandwidth wizard experience becomes a requirement.
