# Operant

[![build](https://img.shields.io/badge/build-local%20CI-blue)](CONTRIBUTING.md#building-and-testing)
[![license](https://img.shields.io/badge/license-Apache%202.0-blue)](LICENSE)
[![platform](https://img.shields.io/badge/platform-Windows-0078D6)](docs/ARCHITECTURE.md)
[![local-first](https://img.shields.io/badge/local--first-yes-brightgreen)](docs/PRD.md)
[![benchmark](https://img.shields.io/badge/benchmark-run%205%2F5-brightgreen)](BENCHMARKS.md)

**Teach your computer once. It does it forever.**
*No code. No cloud. Free.*

Operant is a free, open source desktop app for Windows that learns a task by
watching you do it once, then does that task again on its own.

<!-- CAPTURE SLOT (V1 recapture pass): new home dashboard screenshot, not yet
captured. Should show the "time saved this week" line, the recent-weeks
sparkline, the upcoming scheduled runs, and the recent runs list. -->
![Screenshot of the Operant home dashboard: a line reading how much time it saved this week, a small sparkline of the last few weeks, a list of upcoming scheduled runs, and a list of recent runs each with a status dot and a one-line result.](assets/hero-dashboard.png)

[Download the installer](https://github.com/AlpharomeroJL/operant/releases) | [Watch the 90-second demo](#watch-demo) | [Star the repo](https://github.com/AlpharomeroJL/operant) | [Browse the template gallery](https://github.com/AlpharomeroJL/operant-registry) | [See the cookbook](cookbook/README.md)

### Teach it

Show it a task once, by demonstration or by voice, and it remembers exactly how
to do it again.

<a id="watch-demo"></a>

<!-- CAPTURE SLOT (V1 recapture pass): not a new shot. Real footage for this
exact moment already exists under a different filename elsewhere in assets/;
copy or rename it to this filename rather than reshooting. See the M1 packet
handoff notes for the source file. -->
![Animated run viewer stepping through a live teaching run with the model indicator reading "Thinking live": steps to click Downloads, click Invoice.pdf, copy, and paste appear one at a time and turn green.](assets/feature-teach.gif)

### Trust it

A kill switch stops everything in under a tenth of a second, every run can be
undone, and once it has learned a task it does that task again on your own
machine with nothing sent anywhere.

![Animated kill switch demo: a run is mid-step (Click Downloads, Click Invoice.pdf) with the model indicator on, then the panic hotkey fires and the run freezes, the run viewer reads "Stopped, needs you", and the tray icon turns red with an "Operant stopped" notification.](assets/12-killswitch.gif)

### Forget it

Put it on a schedule or run it again with one click, and it just happens
without you.

<!-- CAPTURE SLOT (V1 recapture pass): not yet captured. Should show running a
saved workflow with one click from the library, then setting that workflow to
run automatically on a schedule. -->
![Animated demo of running a saved workflow with one click from the workflow library, then setting that workflow to run automatically on a schedule.](assets/feature-forget.gif)

## Get started

1. Download the installer.
2. Sign in, or pick a free engine.
3. Teach it your first task.

Building from source instead of installing: see [CONTRIBUTING.md](CONTRIBUTING.md).

---

## How it works

Everything below is for people who want the technical detail: the reasoning,
the benchmark, the architecture, and how Operant compares to other projects.

The model is a compiler, not a runtime. Exploration should be probabilistic.
Execution should not be. At 98 percent per-step model accuracy, a 40-step workflow
fails about 55 percent of the time in production: probabilistic execution does not
survive multiplication. So Operant explores a task with a model once, or a few times
with correction, then freezes the successful run into a typed, readable file guarded
by invariant gates. Replay after that makes zero model calls and zero network calls,
both asserted in CI, not just promised.

### Kill switch and undo

Two things hold no matter what Operant is doing.

**One key stops everything, instantly.** The kill switch runs at the action layer,
below the planner, so no model decision can delay it. CI holds the freeze under
100 ms.

![Animated kill switch demo: a run is mid-step (Click Downloads, Click Invoice.pdf) with the model indicator on, then the panic hotkey fires and the run freezes, the run viewer reads "Stopped, needs you", and the tray icon turns red with an "Operant stopped" notification.](assets/12-killswitch.gif)

**Every run can be undone.** Write actions record an inverse before they run, so
"Undo last run" is a real replay of real inverses, narrated in plain English.
Anything without a safe inverse, like a sent email, is labeled irreversible before
you run it, not after.

![Placeholder graphic labeled "Placeholder, not a real capture" stating that this asset will show a finished run, an "Undo last run" action, and the restored files narrated in plain English, and noting this screen does not exist in ui/src yet (see LAUNCH.md's Capture TODOs).](assets/10-undo.gif)

Operant also keeps score. ![Screenshot of a tray notification reading "Your weekly time saved: Saved about 192 minutes this week" with a Dismiss button.](assets/11-timesaved.png) is the tray showing estimated
time saved this week, the screenshot people actually share. If it saves you time,
[star the repo](https://github.com/AlpharomeroJL/operant).

### Explore once, replay forever

![Animated run viewer stepping through a live teaching run with the model indicator reading "Thinking live": steps to click Downloads, click Invoice.pdf, copy, and paste appear one at a time and turn green.](assets/02-explore.gif) is a live teach run with the model indicator on.
![Animated run viewer replaying the same copy-invoice-total task with the model indicator reading "Running from memory, no thinking needed": the same four steps as 02-explore.gif complete almost instantly, ending on Done.](assets/04-replay.gif) is the same task run again with the model indicator off. Same
result, second run instant.

```text
EXPLORE (probabilistic, model in loop)             REPLAY (deterministic, no model)
 perceive -> plan(LLM) -> gate -> act -> record     load workflow -> gate -> act -> gate ...
 slow, costly, supervised                           under 150ms/step, free, offline, audited
                    \                                 ^                    |
                     \        compile                /            drift?  v
                      +------------------------------+        re-ground one step (model)
                                                              -> patch diff -> human approve
                                                              -> versioned merge
```

When a compiled step fails because the screen changed, Operant re-grounds that one
step, proposes a patch diff, and waits for a human approval before merging a new
version. The workflow heals. It never silently mutates.

### The benchmark

Across all three benchmark tasks, compiled replay succeeds 5 out of 5 runs with zero
model calls. Re-inferring the same tasks at every step also succeeds 5 out of 5, but
costs 15 to 25 model calls and 2700 to 4500 tokens per task. Full numbers, from
[BENCHMARKS.md](BENCHMARKS.md), regenerated each release:

| Task | Mode | Success | p50/step | p95/step | Model calls | Tokens |
|---|---|---|---|---|---|---|
| drift_repaired | replay | 5/5 | 0ms | 0ms | 0 | 0 |
| drift_repaired | re-infer (mock) | 5/5 | 6ms | 6ms | 15 | 2700 |
| notepad | replay | 5/5 | 1ms | 1ms | 0 | 0 |
| notepad | re-infer (mock) | 5/5 | 7ms | 7ms | 25 | 4500 |
| web | replay | 5/5 | 0ms | 0ms | 0 | 0 |
| web | re-infer (mock) | 5/5 | 6ms | 6ms | 25 | 4500 |

![Table image of the Operant benchmark headline from BENCHMARKS.md, comparing compiled replay (near-zero latency, zero model calls) against re-inferring every step (higher latency, dozens of model calls and tokens) across three tasks.](assets/07-bench.png)

Replay wins at zero model calls by construction, not by configuration: the replay
executor links against a backend-free crate, so a model call during replay is not a
setting that could be flipped on by accident, it is a compile-time impossibility. The
re-infer (mock) numbers reuse recorded latencies from the actual replay to simulate
agent-at-every-step cost without hitting a real backend, stated plainly in
BENCHMARKS.md's own methods section. Full methodology:
[BENCHMARKS.md](BENCHMARKS.md) and [docs/specs/bench.md](docs/specs/bench.md).

### What a compiled workflow looks like

This is the actual compiler fixture, unedited, from
[`contracts/fixtures/workflow_notepad/workflow.ts`](contracts/fixtures/workflow_notepad/workflow.ts):

```typescript
// Compiled by Operant from run 01JZFIXTURERUN0000000000
// Goal: Write an invoice note in Notepad and save it
// This file is the canonical compiler OUTPUT shape: declarative, one step per
// statement, plain-English intent on every step, zero model calls at replay.
import { defineWorkflow, step, input } from "@operant/sdk";

export default defineWorkflow({
  name: "notepad-invoice-note",
  version: "1.0.0",
  description: "Writes a dated invoice note into Notepad and saves it.",
  inputs: {
    invoice_date: input.date({ default: "2026-07-11", label: "Invoice date" }),
    amount: input.currency({ default: "142.50", label: "Amount" }),
  },
  steps: [
    // 1. Click the text editor
    step.click({
      intent: "Click the text editor",
      window: { process: "notepad.exe", titlePattern: ".* - Notepad" },
      selectors: [
        { kind: "automation_id", value: "RichEditD2DPT" },
        { kind: "name_role_path", path: [{ role: "window", name: "Untitled - Notepad" }, { role: "document", name: "Text editor" }] },
        { kind: "ordinal_path", path: [{ role: "window", ordinal: 0 }, { role: "document", ordinal: 0 }] },
      ],
      risk: "read",
    }),
    // 2. Type the invoice note
    step.type({
      intent: "Type the invoice note",
      window: { process: "notepad.exe", titlePattern: ".* - Notepad" },
      selectors: [
        { kind: "automation_id", value: "RichEditD2DPT" },
        { kind: "name_role_path", path: [{ role: "window", name: "Untitled - Notepad" }, { role: "document", name: "Text editor" }] },
        { kind: "ordinal_path", path: [{ role: "window", ordinal: 0 }, { role: "document", ordinal: 0 }] },
      ],
      text: "Invoice {invoice_date} total ${amount}",
      risk: "write",
    }),
    // 3. Wait for the screen to update
    step.wait({
      intent: "Wait for the screen to update",
      scope: { window: { process: "notepad.exe", titlePattern: ".* - Notepad" } },
      timeoutMs: 5000,
    }),
    // 4. Save the file
    step.key({
      intent: "Save the file",
      window: { process: "notepad.exe", titlePattern: ".* - Notepad" },
      combo: "ctrl+s",
      risk: "write",
    }),
    // 5. Wait for the screen to update
    step.wait({
      intent: "Wait for the screen to update",
      scope: { window: { process: "notepad.exe", titlePattern: ".* - Notepad" } },
      timeoutMs: 5000,
    }),
    // 6. Check that the note was written
    step.assert({
      intent: "Check that the note was written",
      window: { process: "notepad.exe", titlePattern: ".* - Notepad" },
      expr: {
        op: "matches",
        query: { kind: "snapshot_element_value", role: "document", name: "Text editor" },
        regex: "^Invoice \\d{4}-\\d{2}-\\d{2} total \\$\\d+\\.\\d{2}$",
      },
    }),
  ],
});
```

Every step carries a plain-English `intent` string. That is not a comment for
developers only: it is the same text the plain-English workflow view shows a
non-coder, from the same file, with no separate copy that could drift from the real
logic.

### Architecture

Two execution modes over one runtime:

- **Perception**: Windows accessibility tree (UIA) first, CDP for browsers, OCR for
  PDFs and images, a vision-grounding fallback for anything else. Every vision step
  stores an anchor image, so replay resolves by template match, never by calling a
  model.
- **Action**: a typed, serializable Action IR for every step, with a risk class of
  read, write, or destructive. Adapters (shell, filesystem, Office COM, email,
  browser, MCP) win over raw accessibility actions, which win over vision.
- **Compiler and drift repair**: normalizes a recorded run, turns varying literals
  into typed inputs, scores selectors for stability, and emits a TypeScript file plus
  a signed manifest. A failed replay step re-grounds itself, proposes a patch, and
  waits for human approval before merging.
- **Invariant gates**: precondition, postcondition, and safety checks run in both
  explore and replay. Hard safety invariants (credential fields, payment or delete
  confirmations) live in the runtime, not in workflow files, and no workflow can turn
  them off.
- **Safety and audit**: capability grants per workflow, a dry-run mode with zero side
  effects, and a hash-chained append-only audit log with JSON and PDF export.
- **Orchestrator and models**: bring your own backend, 17 named options across local
  runners, API keys, and sign-in-with-subscription, or run fully offline.
- **Voice**: local speech in and out, lazy-loaded so it does not sit in memory until
  used.
- **Shell UI**: tray, global-hotkey command palette, run viewer with a model on/off
  indicator, and a plain-English workflow view with a code toggle for anyone who
  wants it.

Full component specs and the data model: [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

### Registry

`operant install <name>` fetches a workflow manifest from a git-backed index,
verifies its Ed25519 signature against a publisher key, shows the grants it needs in
plain language, and installs only after approval. Unsigned or unverified workflows
still install, but run in dry-run only until you explicitly promote them after
reading the steps.

Registry: [github.com/AlpharomeroJL/operant-registry](https://github.com/AlpharomeroJL/operant-registry).

### MCP, CLI, and SDK

MCP runs both directions. Operant serves every compiled workflow as an MCP tool
(`workflow_<slug>`, schema taken straight from the manifest's own inputs), and it
consumes external MCP servers as adapters your workflows can call. A TypeScript SDK
sits on the same file format as the CLI:

`operant run|compile|dry-run|list|install|bench|doctor|explain`

Details: [docs/specs/mcp.md](docs/specs/mcp.md).

## How Operant compares

Checked against each project's own public documentation. "Not documented" means no
public evidence either way was found, not a claim that the feature is absent;
corrections are welcome as an issue. Last checked: 2026-07-11.

| | Operant | Simular | UI-TARS Desktop | Open Interpreter | UFO |
|---|---|---|---|---|---|
| Deterministic, model-free replay | Yes, CI-asserted | No, cloud execution | No, re-infers every step | No, re-infers every step | No, re-infers every step |
| Works fully offline after teaching | Yes | No | Partial | Partial | Partial |
| Self-heals on UI drift, human-approved | Yes | Not documented | Not documented | Not documented | Not documented |
| Full audit trail, hash-chained, exportable | Yes | Not documented | Not documented | Not documented | Not documented |
| No-code path for non-developers | Yes, wizard plus plain-English steps | Yes | No, developer tool | No, developer tool | No, research framework |
| One-click undo | Yes | Not documented | No | No | No |
| Kill switch under 100ms | Yes, latency-tested | Not documented | No | No | No |
| Price | Free, Apache 2.0 | Paid, per-seat | Free, open source | Free, open source | Free, research license |
| Raw exploration-time model quality | Conceding: dedicated grounding teams are likely ahead here | n/a | n/a | n/a | n/a |
| Mobile device control | Conceding: not at v1.0.0 | Not documented | No | No | No |

This table is kept in sync with the one in LAUNCH.md.

## License and links

- License: [Apache 2.0](LICENSE)
- Contributing: [CONTRIBUTING.md](CONTRIBUTING.md)
- Security policy: [SECURITY.md](SECURITY.md)
- Docs: [alpharomerojl.github.io/operant](https://alpharomerojl.github.io/operant/)
- Registry: [github.com/AlpharomeroJL/operant-registry](https://github.com/AlpharomeroJL/operant-registry)
- Repo: [github.com/AlpharomeroJL/operant](https://github.com/AlpharomeroJL/operant)
