# Agent-bridge protocol (P0b proof harness)

This is the operator's driving reference for the `AgentBridgeBackend`
(`crates/orchestrator/src/backends/agent_bridge.rs`), the filesystem
`ModelBackend` that lets an operator (a human, or an agent such as Claude
driving the desktop) act as the planner "brain" for a real `operant explore`
teach run. It is compiled ONLY behind the opt-in `dev-agent-bridge` cargo
feature and is never in a default or release build.

The backend replaces a model endpoint with a directory of request/response
JSON files. Each planner turn is one numbered round. You read the request the
loop wrote, decide the next move, and write the response it waits for.

## 1. Setup

Build the CLI with the feature and point it at a rendezvous directory:

```powershell
$env:CARGO_TARGET_DIR = 'D:\dev\operant-target-p0b'
$env:OPERANT_AGENT_BRIDGE_DIR = 'D:\dev\operant\bridge'   # any writable dir
mkdir $env:OPERANT_AGENT_BRIDGE_DIR -Force

cargo run -p operant-cli --features real-uia,real-input,dev-agent-bridge -- `
  explore --goal "Write an invoice note in Notepad and save it" `
          --window-process notepad.exe `
          --out D:\dev\operant\out\teach1
```

(For a dry, desktop-free rehearsal of the wiring, drop `real-uia,real-input`
so perception and input stay mock; the bridge protocol is identical.)

`OPERANT_AGENT_BRIDGE_DIR` must be set or the run fails immediately with a
`config_error`.

## 2. The round loop

For each planner turn `N` (a per-instance counter starting at 1):

1. The backend writes `<dir>/req-<N>.json` (atomically: temp file + rename).
2. The backend prints one line to STDOUT and flushes it:
   `AGENT_BRIDGE_AWAIT <N>`. This is your cue that round `N` is waiting.
3. You read `req-<N>.json`, decide the next action(s), and write
   `<dir>/resp-<N>.json`. **Write it atomically** (write a temp file, then
   rename it onto `resp-<N>.json`) so the backend never reads a half-written
   file. A leading UTF-8 BOM is tolerated (Windows PowerShell's `Set-Content
   -Encoding utf8` emits one), so you do not have to strip it.
4. The backend polls every 500ms, parses your `resp-<N>.json` as a JSON array
   of backend events, and streams them to the loop. If your array does not end
   in a terminal event it appends a `done` for you.

If no response appears within 20 minutes the round returns a single terminal
`error` with `error_id: "agent_bridge_timeout"` and the run halts. A response
file that is present but does not parse returns `error_id:
"agent_bridge_bad_response"`.

### Request file: `req-<N>.json`

```json
{ "seq": 1, "prompt": "Goal: Write an invoice note in Notepad and save it\n\n<element digest>\nSteps taken so far:\n- ..." }
```

- `seq` is the round number `N`.
- `prompt` is the loop's `CompletionRequest.concat_text()`: the literal string
  `"Goal: {goal}\n\n{digest.to_prompt_text()}"` followed, from round 2 on, by a
  `Steps taken so far:` history block. The digest is a plain-text summary of the
  perception snapshot's element tree (the planner never sees pixels). Read it to
  choose selectors that exist on screen right now.

### Response file: `resp-<N>.json`

A JSON array of backend events. Field names and casing are the wire contract
(`crates/orchestrator/src/backends/types.rs`, `#[serde(tag = "event",
rename_all = "snake_case")]`):

| event | fields | meaning |
|---|---|---|
| `tool_call` | `id` (string), `name` (string), `arguments` (object) | one planner tool call |
| `text_delta` | `text` (string) | optional narration; the loop ignores it for planning |
| `done` | `usage` (object; `{ "input_tokens": 0, "output_tokens": 0 }`) | terminal; end the turn |
| `error` | `error_id`, `message`, `retryable` | terminal; halts the run |

How the explore loop reads the array
(`crates/orchestrator/src/explore/mod.rs`):

- Any `tool_call` whose `name` is `"done"` ends the run (the goal is reached).
- Any other `tool_call` is a proposed step: its `arguments` object is parsed as
  one **Action IR** object (section 3) and executed. Use `name:
  "propose_action"` for these (the loop offers exactly the two tools
  `propose_action` and `done`).
- An array with no `tool_call` at all (e.g. `[]`) is an implicit `done`.
- A `tool_call` batch (several `propose_action` in one response) is allowed:
  the loop executes them one at a time, re-perceiving between each. Because the
  screen changes as you act, prefer **one action per round** and react to the
  next request; batch only steps you are certain of.

Do NOT propose an `assert` action here: the explore executor refuses to
dispatch `assert` (it is a gate predicate, evaluated by the gate engine, not an
action), so proposing one halts the run. Postconditions are derived at compile
time, not proposed in the loop.

## 3. Action IR JSON (the `arguments` of a `propose_action`)

Mirrors `contracts/action_ir.schema.json` and `crates/ir/src/action.rs`. The
`arguments` object of a non-`done` tool call must deserialize as this shape.

Required: `id`, `kind`, `risk_class`, `grounding` (`v` defaults to `1`).

| field | type | notes |
|---|---|---|
| `v` | integer | schema version, `1`. Optional (defaults to 1). |
| `id` | string | unique step id; a ULID is recommended, any non-empty string works. |
| `kind` | enum | `click` \| `type` \| `key` \| `scroll` \| `drag` \| `wait` \| `assert` \| `adapter_call`. |
| `intent` | string | plain-English step intent; the compiler emits it as the step comment. Optional but recommended. |
| `target` | object | where the action lands (below). Omit for global `key` and for `adapter_call`/`wait`. |
| `params` | object | kind-specific (below). |
| `pace` | enum | `instant` (default) \| `human`. |
| `risk_class` | enum | `read` \| `write` \| `destructive`. |
| `irreversible` | bool | default `false`; `true` for send/submit/side-effectful shell. |
| `grounding` | enum | `uia` \| `vision` \| `adapter`. |
| `timeout_ms` | integer | default `5000`. |
| `retry` | object | `{ "attempts": 2, "backoff_ms": 250 }` by default. |

`target`:

| field | type | notes |
|---|---|---|
| `window` | object | `{ "process": "notepad.exe", "title_pattern": "<anchored regex>" }`. |
| `selectors` | array | ordered most-stable-first; replay tries them in order (below). |
| `anchor` | object | `{ "img_hash": "<blake3 hex>", "tolerance": 0.85 }` (vision fallback). |
| `coords_last_known` | object | `{ "x": 700.0, "y": 514.0, "monitor": "MON1", "dpi_scale": 1.0 }`. |

`selectors[]` variants (tagged by `kind`):

- `{ "kind": "automation_id", "value": "RichEditD2DPT" }` (most stable).
- `{ "kind": "name_role_path", "path": [ { "role": "window", "name": "Untitled - Notepad" }, { "role": "document", "name": "Text editor" } ] }`.
- `{ "kind": "ordinal_path", "path": [ { "role": "window", "ordinal": 0 }, { "role": "document", "ordinal": 0 } ] }`.
- `{ "kind": "css", "value": "#save" }` (browser adapter only).

`params` by `kind`:

- `type`: `{ "text": "..." }` or `{ "input_ref": "<workflow input name>" }`.
- `key`: `{ "combo": "ctrl+s" }`.
- `scroll`: `{ "direction": "down", "amount": 3 }`.
- `drag`: `{ "to": <target> }`.
- `adapter_call`: `{ "namespace": "fs", "verb": "...", "args": { ... } }`.

Selector guidance: for a `click`, the loop resolves the target against LIVE
perception at execute time, so a selector must match an element actually on
screen (read the digest in the prompt). Prefer `automation_id`, then a short
`name_role_path`. Include `coords_last_known` when you know it: a headless
(mock) replay clicks that cached point; a real (`real-uia`) replay re-resolves
the selectors live and only falls back to the coordinate.

## 4. Worked response examples

Each block below is a complete `resp-<N>.json` you could write.

### 4a. Click by `name_role_path` selector

```json
[
  {
    "event": "tool_call",
    "id": "1",
    "name": "propose_action",
    "arguments": {
      "id": "01JADCLICK0000000000000001",
      "kind": "click",
      "intent": "Click the text editor",
      "target": {
        "window": { "process": "notepad.exe" },
        "selectors": [
          { "kind": "name_role_path", "path": [
            { "role": "window", "name": "Untitled - Notepad" },
            { "role": "document", "name": "Text editor" }
          ] }
        ],
        "coords_last_known": { "x": 700.0, "y": 514.0, "monitor": "MON1", "dpi_scale": 1.0 }
      },
      "risk_class": "read",
      "grounding": "uia"
    }
  }
]
```

### 4b. Type text

```json
[
  {
    "event": "tool_call",
    "id": "1",
    "name": "propose_action",
    "arguments": {
      "id": "01JADTYPE00000000000000002",
      "kind": "type",
      "intent": "Type the invoice note",
      "target": { "window": { "process": "notepad.exe" } },
      "params": { "text": "Invoice 2026-07-11 total $142.50" },
      "risk_class": "write",
      "grounding": "uia"
    }
  }
]
```

### 4c. Key combo

```json
[
  {
    "event": "tool_call",
    "id": "1",
    "name": "propose_action",
    "arguments": {
      "id": "01JADKEY000000000000000003",
      "kind": "key",
      "intent": "Save the file",
      "target": { "window": { "process": "notepad.exe" } },
      "params": { "combo": "ctrl+s" },
      "pace": "instant",
      "risk_class": "write",
      "grounding": "uia"
    }
  }
]
```

### 4d. Done (goal reached)

```json
[
  { "event": "tool_call", "id": "1", "name": "done", "arguments": {} }
]
```

### 4e. One response, whole task as a batch (optional)

A single `resp-1.json` can carry the entire task followed by `done`; the loop
executes each `propose_action` in order, re-perceiving between them:

```json
[
  { "event": "tool_call", "id": "1", "name": "propose_action", "arguments": { "id": "s1", "kind": "click", "target": { "window": { "process": "notepad.exe" }, "selectors": [ { "kind": "automation_id", "value": "RichEditD2DPT" } ], "coords_last_known": { "x": 700.0, "y": 514.0, "monitor": "MON1" } }, "risk_class": "read", "grounding": "uia" } },
  { "event": "tool_call", "id": "2", "name": "propose_action", "arguments": { "id": "s2", "kind": "type", "target": { "window": { "process": "notepad.exe" } }, "params": { "text": "Invoice 2026-07-11 total $142.50" }, "risk_class": "write", "grounding": "uia" } },
  { "event": "tool_call", "id": "3", "name": "propose_action", "arguments": { "id": "s3", "kind": "key", "target": { "window": { "process": "notepad.exe" } }, "params": { "combo": "ctrl+s" }, "risk_class": "write", "grounding": "uia" } },
  { "event": "tool_call", "id": "4", "name": "done", "arguments": {} }
]
```

## 5. After the run

When the loop finishes (your `done`, or an implicit done), `operant explore`
writes `<out>/trajectory.json` (plus `<out>/recorder.sqlite3`). Compile and
replay it:

```powershell
operant compile D:\dev\operant\out\teach1\trajectory.json D:\dev\operant\out\teach1\compiled
operant run D:\dev\operant\out\teach1\compiled\compiled.json
```

Explore once with a planner; replay forever without one.
