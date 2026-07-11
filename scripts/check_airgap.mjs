// Air-gap gate placeholder. L6B replaces this with a real test that boots the
// default configuration and asserts zero outbound sockets (except a disableable
// signed update check). Until then this asserts the invariant is declared: the
// replay crate must not depend on the orchestrator crate (which owns network).
import { readFileSync } from "node:fs";

const replayToml = readFileSync("crates/replay/Cargo.toml", "utf8");
if (/operant-orchestrator/.test(replayToml)) {
  console.error("check-airgap: FAILED. crates/replay must not depend on operant-orchestrator.");
  console.error("  Zero-model replay is enforced by the crate graph; this dependency breaks it.");
  process.exit(1);
}
console.log("check-airgap: OK (replay is backend-free by crate graph)");
