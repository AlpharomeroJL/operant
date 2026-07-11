// Voice sidecar (C12) process entry point.
//
// Speaks newline-delimited JSON envelopes on stdout, matching the envelope
// shape in contracts/bus_events.md ({v, seq, ts, topic, payload}), and reads
// newline-delimited JSON commands on stdin. This is this lane's default
// framing for contracts/bus_events.md's "cross-process (sidecars) rides the
// supervisor pipe with the same envelope" - the real C1 supervisor
// (crates/core/src/supervisor.rs) owns final say on wire format when it
// spawns a real Child for this sidecar, so treat this as a documented,
// revisable seam rather than a locked protocol.
//
// Commands (stdin, one JSON object per line):
//   {"cmd":"hold_start"}
//   {"cmd":"audio_chunk","dataBase64":"..."}
//   {"cmd":"hold_end"}
//   {"cmd":"cancel"}
//   {"cmd":"speak","text":"..."}
//   {"cmd":"vram_yield_request","budgetMs":2000}
//   {"cmd":"health_check"}
//
// Unknown commands and malformed lines are ignored; this process never
// crashes on bad input from its pipe. None of the above is required to run
// the mock text-mode round trip proven under test/ - see src/sidecar.js for
// the library surface those tests exercise directly, in-process, without
// going through this stdio framing at all.

import readline from "node:readline";
import { pathToFileURL } from "node:url";
import { createSidecar } from "./sidecar.js";

/**
 * @param {object} [opts]
 * @param {NodeJS.ReadableStream} [opts.input]
 * @param {NodeJS.WritableStream} [opts.output]
 * @param {"mock"|"real"} [opts.providerKind]
 */
export function main({ input = process.stdin, output = process.stdout, providerKind } = {}) {
  const kind = providerKind || (process.env.OPERANT_VOICE_PROVIDER === "real" ? "real" : "mock");
  const sidecar = createSidecar({ providerKind: kind, name: "voice" });

  const emit = (env) => output.write(`${JSON.stringify(env)}\n`);
  sidecar.bus.subscribe("*", emit);

  function handleCommand(msg) {
    switch (msg.cmd) {
      case "hold_start":
        sidecar.pushToTalk.holdStart();
        break;
      case "audio_chunk":
        sidecar.pushToTalk.feed(Buffer.from(msg.dataBase64 || "", "base64"));
        break;
      case "hold_end":
        sidecar.pushToTalk.holdEnd();
        break;
      case "cancel":
        sidecar.pushToTalk.cancel();
        break;
      case "speak":
        sidecar.speak(msg.text || "").catch(() => {});
        break;
      case "vram_yield_request":
        sidecar.vram.requestYield(msg.budgetMs).catch(() => {});
        break;
      case "health_check":
        sidecar.reportHealth(true);
        break;
      default:
        break; // unknown commands never crash the sidecar
    }
  }

  const rl = readline.createInterface({ input, terminal: false });
  rl.on("line", (line) => {
    const trimmed = line.trim();
    if (!trimmed) return;
    try {
      handleCommand(JSON.parse(trimmed));
    } catch {
      // malformed input line: ignored, not fatal
    }
  });

  sidecar.start();

  return sidecar;
}

const isDirectRun = process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href;
if (isDirectRun) {
  main();
}
