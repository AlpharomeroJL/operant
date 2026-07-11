// A mocked local-model downloader standing in for the real one
// (sidecars/downloader, owned by lane U2A). Same seam pattern as
// ui/src/bus/mockClient.ts (mocks the transport this lane does not own) and
// ui/src/settings/mockStore.ts (mocks persistence): sidecars/downloader/cli.mjs
// is a Node subprocess a Tauri host spawns (see the sidecar protocol section
// of sidecars/downloader/README.md), and ui/src has no process-spawning access of its own
// (that is ui/src-tauri's job, out of this lane's owned path). This
// in-process, timer-driven simulation emits the exact same envelope shape
// and topic vocabulary the real cli.mjs does over stdout, so swapping this
// for a real spawned-subprocess reader later is a same-shape transport
// change, not a rendering change, the same promise every other mock in this
// codebase makes.
//
// Pure and DOM-free: runs under plain `node --test`, same split as every
// other module in ui/src.

export type DownloadTopic =
  | "download.started"
  | "download.progress"
  | "download.paused"
  | "download.completed"
  | "download.failed";

export interface DownloadEnvelope {
  v: 1;
  seq: number;
  ts: string;
  topic: DownloadTopic;
  payload: Record<string, unknown>;
}

export type DownloadErrorCode = "CHECKSUM_MISMATCH" | "HTTP_ERROR" | "NOT_FOUND";

export interface StartDownloadOptions {
  url?: string;
  dest?: string;
  /** Total simulated byte count. Defaults to a small fixture-sized number. */
  totalBytes?: number;
  /** How many progress ticks to split the transfer into. */
  ticks?: number;
  /** Delay between ticks in ms. Tests pass something tiny. */
  tickMs?: number;
  /** When set, the transfer fails at this tick (1-based) instead of completing. */
  failAt?: number;
  failCode?: DownloadErrorCode;
  onEvent: (event: DownloadEnvelope) => void;
}

export interface DownloadHandle {
  pause(): void;
  resume(): void;
  cancel(): void;
}

/**
 * Starts a simulated download. Mirrors the real sidecar's resume semantics
 * (sidecars/downloader/README.md "Resume semantics"): pausing keeps whatever
 * has already "arrived" and resuming continues from that byte offset rather
 * than restarting, and the 100%/final update is never throttled away.
 */
export function startDownload(opts: StartDownloadOptions): DownloadHandle {
  const totalBytes = opts.totalBytes ?? 2_000_000;
  const ticks = Math.max(1, opts.ticks ?? 10);
  const tickMs = opts.tickMs ?? 150;
  const bytesPerTick = Math.ceil(totalBytes / ticks);

  let seq = 0;
  let bytesReceived = 0;
  let tick = 0;
  let cancelled = false;
  let timer: ReturnType<typeof setTimeout> | null = null;

  function emit(topic: DownloadTopic, payload: Record<string, unknown>): void {
    if (cancelled) return;
    seq += 1;
    opts.onEvent({ v: 1, seq, ts: new Date().toISOString(), topic, payload });
  }

  function scheduleNext(): void {
    timer = setTimeout(step, tickMs);
  }

  function step(): void {
    if (cancelled) return;
    tick += 1;

    if (opts.failAt && tick === opts.failAt) {
      const code = opts.failCode ?? "CHECKSUM_MISMATCH";
      emit("download.failed", { code, message: "the download did not finish", resumedFrom: bytesReceived });
      return;
    }

    bytesReceived = Math.min(totalBytes, bytesReceived + bytesPerTick);
    const percent = Math.round((bytesReceived / totalBytes) * 10000) / 100;
    emit("download.progress", { bytesReceived, bytesTotal: totalBytes, percent });

    if (bytesReceived >= totalBytes) {
      emit("download.completed", {
        path: opts.dest ?? "model.bin",
        bytesWritten: bytesReceived,
        bytesTotal: totalBytes,
        sha256: "0".repeat(64),
        resumedFrom: 0,
        alreadyComplete: false,
      });
      return;
    }

    scheduleNext();
  }

  emit("download.started", {
    url: opts.url ?? "https://example.com/model.bin",
    dest: opts.dest ?? "model.bin",
    sums: null,
    sha256: null,
    resumedFrom: 0,
  });
  scheduleNext();

  return {
    pause(): void {
      if (timer) {
        clearTimeout(timer);
        timer = null;
      }
      emit("download.paused", { resumedFrom: bytesReceived });
    },
    resume(): void {
      if (cancelled || timer) return;
      emit("download.started", {
        url: opts.url ?? "https://example.com/model.bin",
        dest: opts.dest ?? "model.bin",
        sums: null,
        sha256: null,
        resumedFrom: bytesReceived,
      });
      scheduleNext();
    },
    cancel(): void {
      cancelled = true;
      if (timer) clearTimeout(timer);
      timer = null;
    },
  };
}

// ---- "works on this PC" checks --------------------------------------------
//
// Stand-ins for the real doctor-style checks docs/specs/zero-code.md calls
// out: a works-on-this-PC check from the VRAM probe, plus a doctor check for
// VRAM headroom on configured sidecars. crates/doctor owns the real probe,
// out of this lane's owned path (crates are off limits per the lane brief).
// These are plain, deterministic functions a caller can feed real numbers
// into later without changing the wizard's own logic.

export type CompatibilityLevel = "ok" | "slow" | "fail";

export interface CompatibilityCheckResult {
  level: CompatibilityLevel;
}

/** Below minMb: fails outright. Below slowMb: still runs, just slowly. */
export function probeCompatibility(availableMb: number, minMb = 4000, slowMb = 6000): CompatibilityCheckResult {
  if (availableMb < minMb) return { level: "fail" };
  if (availableMb < slowMb) return { level: "slow" };
  return { level: "ok" };
}

export interface DiskCheckResult {
  ok: boolean;
  neededBytes: number;
  freeBytes: number;
  /** How much more space is required, not the model's total size: "free up {needed}" (wizard_copy.json) means the shortfall, not a re-statement of the download size. Zero when ok. */
  shortfallBytes: number;
}

export function checkDiskSpace(freeBytes: number, neededBytes: number): DiskCheckResult {
  return { ok: freeBytes >= neededBytes, neededBytes, freeBytes, shortfallBytes: Math.max(0, neededBytes - freeBytes) };
}

/** 1_500_000_000 -> "1.5 GB". Small and dependency-free; only handles the sizes a model download realistically needs. */
export function formatBytes(bytes: number): string {
  const gb = bytes / 1_000_000_000;
  if (gb >= 1) return `${Math.round(gb * 10) / 10} GB`;
  const mb = bytes / 1_000_000;
  return `${Math.round(mb)} MB`;
}
