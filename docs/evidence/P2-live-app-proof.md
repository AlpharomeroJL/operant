# P2 live-app proof: the shipped app teaches a real task over the sidecar

This is the evidence record for driving the **whole assembled app** (not the CLI)
through a real teach, on a real Windows desktop, with the Opus-4.8 dev
agent-bridge as the planner brain. It is the app-level companion to
`docs/evidence/P0-live-engine.md` (which proved the engine and CLI). It also
records the real "first contact with reality" defects the drive surfaced, with
root causes and file:line, so they can be fixed rather than hidden.

Build under test: `operant-shell` (Tauri v2) launched with `cargo tauri dev`,
spawning the core sidecar `D:\dev\operant-target\debug\operant.exe` built with
`--features real-uia,real-input,real-transport,dev-agent-bridge`, env
`OPERANT_AGENT_BRIDGE_DIR=D:\dev\app-proof\bridge`. Branch `redesign` at
`c095ccb`.

## What was proven (live, through the real GUI)

1. **Real capability handshake.** The shell's `core_call get_capabilities`
   returned `real_uia=true, real_input=true, real_vision=false,
   mock_planner_only=false, transport_kind=stdio, version=1.0.0`. The app rendered
   the real UI (not the capability-block screen, not Demo).

2. **The GUI fires a real teach through the sidecar.** Typing a goal into the
   command palette (Ctrl+K) and pressing Enter drove
   `handlePaletteCommit -> coreCommands.startExplore(goal)` (main.ts:778) ->
   `invoke("core_call", {cmd:"start_explore"})` -> the sidecar's serve loop ->
   `ExploreLoop` -> `AgentBridgeBackend`. Confirmed by `req-1.json` appearing in
   the bridge dir about 2s after Enter, carrying the goal and a live UIA
   perception digest. Nothing canned: the Demo path never writes a bridge
   request (verified separately with an empty bridge dir).

3. **The teach changed the real desktop.** As the brain I answered the bridge
   rounds with Action IR (select-all, type the token `APPTEACH-9F2`, select-all,
   copy). Ground truth: the Windows clipboard, seeded beforehand with
   `SENTINEL-BEFORE-COPY`, read back exactly **`APPTEACH-9F2`** after the run.
   The token typed cleanly (no garbling; the per-code-unit pacing fix from P0b
   held). All five recorded steps report `outcome:"ok"`.

4. **The run is recorded in the flight recorder.** `core_call list_runs`
   returned the run; `core_call get_run` returned `run_18c1b5a36aac81d4_0`,
   `mode:"explore"`, `status:"completed"`, goal
   "Type APPTEACH-9F2 in Notepad and copy it to the clipboard", with all five
   steps and their full Action IR (kind, params, target, retry, grounding,
   snapshot digests).

5. **Compile through the app core.** `core_call compile_run
   {run_id:run_18c1b5a36aac81d4_0}` produced the workflow
   `notepad-appteach-9f2-copy-clipboard` (5 steps, v1.0.0) and wrote real
   artifacts (`manifest.json`, `workflow.ts`, `compiled.json`) under the core's
   data dir.

6. **Replay is model-free, and the determinism gates actively guard it.**
   `core_call start_replay` runs `Replayer` (real synthesizer + live UIA
   re-resolution), never the `ExploreLoop`; zero bridge requests are written
   during a replay attempt (confirmed: the bridge dir did not grow). The gate
   engine is not a rubber stamp: the replay attempt returned
   `"precondition gate #0 failed; halting before any step ran"` and refused to
   run because the compiled precondition did not match the live screen. See
   finding 1 for why the precondition was wrong, and P0-live-engine.md for a
   passing model-free replay (5/5, CLI).

Raw artifacts (the bridge req/resp rounds) are kept locally at
`D:\dev\app-proof\bridge` and deliberately NOT committed: the perception digest
in each request enumerates the operator's live desktop contents, which must not
land in a public repo.

## Findings (real defects surfaced by driving it for real)

### 1. The app never tells the core which app to automate (target-window stub)
`readForegroundWindowProcess` is a hardcoded stub returning
`DEV_FOREGROUND_WINDOW = "explorer.exe"` (ui/src/bus/commands.ts:87, :94), and
the **real** command path wires that same stub
(`foregroundWindow: readForegroundWindowProcess`, ui/src/main.ts:320). So every
GUI teach sends `window_process:"explorer.exe"`; the core's `spawn_explore`
passes it into `ExploreLoop::new(.., window_process)` (cli/src/commands/serve.rs:623)
and faithfully perceives the desktop shell (Program Manager), not the target app.
That is why the perception digest was the desktop in every round, and why the
compiled precondition gate encoded the desktop and then failed at replay.
Note this is a design gap, not a one-line fix: when a user types in Operant's
palette, Operant itself is the foreground window, so "use the foreground window"
cannot be the whole answer. The teach flow needs a real way to choose the target
app (name it, pick it, or infer from the goal).

### 2. Explore-run progress does not reach the webview flight recorder
The run recorded correctly (queryable via list_runs/get_run) but the Runs screen
showed "Nothing has run yet". The live `operant://bus` run/step events for an
explore run started via `start_explore` did not surface in the webview
`runViewer`. Request/response screens (dashboard metrics, run list) work; the
live-stream binding for explore runs does not.

### 3. Focus-from-background is best-effort and loses the race from a sidecar
`focus_with_attach_workaround` returned ok but did not actually win the
foreground from the spawned sidecar process: the first `ctrl+a` landed on the
desktop, not Notepad. The teach only drove Notepad after the target was
foregrounded out-of-band. An autonomous replay has no such helper, so reliable
focus-from-background is the real blocker for hands-off GUI replay. Extends the
P0b foreground-lock work and P0b-follow (#46).

### 4. The dev agent-bridge writes to stdout, which is the IPC channel
`AgentBridgeBackend` prints `AGENT_BRIDGE_AWAIT <N>` to stdout; under the sidecar
that stdout is the NDJSON transport, so the shell logged
"skipping unparseable frame from core". Harmless (the shell tolerates and skips
non-JSON lines) and dev-only (the bridge never ships), but the dev bridge should
write diagnostics to stderr, not stdout.

### 5. git_sha is "unknown" in the dev build
`get_capabilities` reported `git_sha:"unknown"`. Provenance should stamp the real
commit in release builds; worth confirming the release path sets it.

## How to reproduce
Launch the app as above with the bridge dir set, then drive the webview over the
WebView2 devtools endpoint (launch with
`WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222`): set
`localStorage["operant.wizard.completed"]="1"` and reload to skip onboarding,
Ctrl+K, type a Notepad goal, Enter, then answer the `req-<N>.json` rounds with
Action IR per `docs/evidence/agent-bridge-protocol.md`. The agent-bridge is a
dev-only feature and is never in a release build.
