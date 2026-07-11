# Operant Architecture
Companion to PRD.md. Components map 1:1 to build lanes and packets in campaign/MEGA_PROMPT.md. Everything in this document ships in v1.0.0 unless marked [stub at launch].
---
## 1. System overview

```
                            +-----------------------------------------------------+
                            |                   OPERANT DESKTOP                   |
                            |                                                     |
  +----------+   hotkey /   |  +----------------+       +----------------------+  |
  |  Human   |--voice/text--|->| C13 Shell UI   |------>| C6 Orchestrator      |  |
  +----------+              |  | tray, palette, |       | agent loop, run mgr, |  |
       ^                    |  | run viewer,    |<------| HITL pause/redirect, |  |
       | TTS / gate         |  | model ON/OFF,  |       | model backends       |  |
       | escalations        |  | drift diffs    |       +----------+-----------+  |
       |                    |  +-------+--------+                  |              |
  +----+-----+              |          ^                  plan/act | perceive     |
  | C12 Voice|<-------------|--+       |                           v              |
  | STT, TTS,|--------------|->| Event Bus (C1): typed, versioned pub/sub |       |
  | wake word|   intents    |  +--+------+---------+----------+----------+       |
  +----------+              |     ^      ^         ^          ^                   |
                            |     |      |         |          |                   |
                +-----------+-----+--+ +-+---------+--+ +-----+--------------+    |
                | C11 Scheduler      | | C2/C3/C5     | | C4 ACTION          |    |
                | cron, file, window,| | PERCEPTION   | | input synth,       |    |
                | email triggers ->  | | UIA tree(C2) | | adapters: shell,fs,|    |
                | compiled runs only | | vision (C3)  | | OfficeCOM, email,  |    |
                +--------------------+ | browser (C5) | | OCR/PDF, browser,  |    |
                                       | OCR feed     | | MCP-client tools   |    |
                +--------------------+ +--------------+ +-----+--------------+    |
                | C10 SAFETY         |        ^               |                   |
                | grants, dry-run,   |<-------+---------------+                   |
                | audit hash chain   |                        v                   |
                +---------+----------+          +----------------------+          |
                          ^                     | C7 Trajectory        |          |
                          |                     |    Recorder (SQLite  |          |
                +---------+----------+          |    + blob store)     |          |
                | C9 Invariant gates |          +----------+-----------+          |
                | pre / post / safety|                     |                      |
                | (InvariantEval)    |                     v                      |
                +---------+----------+          +----------------------+          |
                          ^                     | C8 Trajectory        |          |
                          +---------------------|    Compiler -> TS DSL|          |
                                                |    + drift repair    |          |
                                                +----------+-----------+          |
                            +-----------------------------------------------------+
                                |               |                |
                     +----------v---+   +-------v--------+  +----v---------------+
                     | C14 SDK/CLI/ |   | C16 Registry   |  | C15 Release        |
                     | MCP server   |   | signed index,  |  | signed installer,  |
                     +--------------+   | operant install|  | Ed25519 updater,   |
                                        +----------------+  | Pages docs site    |
                     +--------------+   +----------------+  +--------------------+
                     | C17 Benchmark|   | C18 E2E +      |
                     | replay vs    |   | Capture harness|
                     | re-inference |   | (CI + README   |
                     +--------------+   |  shots/GIFs)   |
                                        +----------------+
```

Two execution modes over one runtime:

```
EXPLORE (probabilistic, model in loop)             REPLAY (deterministic, no model)
 perceive -> plan(LLM) -> gate -> act -> record     load workflow -> gate -> act -> gate ...
 slow, costly, supervised                           <150ms/step, free, offline, audited
                    \                                 ^                    |
                     \        C8 compile             /            drift?  v
                      +------------------------------+        re-ground one step (model)
                                                              -> patch diff -> human approve
                                                              -> versioned merge (C8)
```

## 2. Component specifications
### C1. Core runtime and event bus
Rust workspace root crate: typed versioned pub/sub bus, config store, structured logging, sidecar supervisor (spawn, health, watchdog restart, VRAM arbitration broker). Contract: `contracts/bus_events.md`. Everything else is a crate or sidecar speaking this bus.
### C2. Perception: accessibility
`Perceiver` trait (OS-agnostic, defined in core): snapshot, diff, resolve, wait_changed. Windows UIA implementation is tier 1: tree walk via COM, normalized elements (role, name, value, patterns, bounds, runtime id), stable selector chains (automation id > name+role path > ordinal). macOS AX and Linux AT-SPI implementations compile as stubs behind the trait [stub at launch].
### C3. Perception: vision grounding sidecar
Process wrapping a local VLM endpoint (Ollama/llama.cpp, UI-TARS-class grounder) or API vision backend. Input: screenshot region plus target description. Output: coordinates, confidence, and a cropped anchor image stored with the step. Replay never calls the model: anchors resolve by template match with tolerance. Fixture mode answers deterministically so CI needs no GPU.
### C4. Action layer
Executes the Action IR, the single serialization all components speak:

```
{ v, id, kind: click|type|key|scroll|drag|wait|assert|adapter_call,
  target: { selector_chain?, anchor?: {img_hash, tolerance}, coords_last_known? },
  params, pace, risk_class: read|write|destructive,
  grounding: uia|vision|adapter, timeout, retry }
```

Input synthesis (SendInput), clipboard, window management. Adapters register as typed `adapter_call` providers: shell/PowerShell, filesystem, Office COM (Excel, Word), email (IMAP/SMTP), OCR/PDF extraction, browser (C5), and external MCP tools (via C14 client). Resolution order enforced in code: adapter beats UIA beats vision.
### C5. Browser adapter
CDP attach to Chrome/Edge/Electron/WebView2. DOM plus accessibility tree emitted as Perception Snapshots; DOM actions emitted as Action IR, so web steps record, compile, and replay identically to native steps.
### C6. Orchestrator and model backends
Explore loop: goal -> perceive -> element digest (never raw pixels to the planner) -> plan -> propose action batch -> safety gate -> execute -> observe -> record -> repeat, with pause/redirect/resume as bus events. Backend trait with roles (planner, grounder) and a provider matrix behind one abstraction:
- **Local**: Ollama, llama.cpp server, LM Studio, vLLM, generic OpenAI-compatible base URL.
- **API key**: Anthropic, OpenAI, Google Gemini, DeepSeek, MiniMax, Moonshot Kimi, Qwen (DashScope), Groq, Mistral, xAI, OpenRouter. Most of these are OpenAI-compatible dialects; implement one compatible client with per-provider quirk tables (auth header shape, streaming format, vision payload format) rather than N clients.
- **Subscription OAuth broker**: Sign in with ChatGPT (Codex/ChatGPT plan) and Sign in with Claude (Claude plan), PKCE loopback flow on 127.0.0.1, tokens in the OS credential vault only, silent refresh, revocation handled as a doctor finding with a re-auth card. This is the layman path: the wizard offers it before API keys.
- **Capability probe**: on configure, classify the backend (vision yes/no, tool use, context length, streaming) and store the profile; role assignment (planner vs grounder) validates against the profile and degrades gracefully with a plain-language explanation instead of a runtime surprise.
Mock planner backend ships for CI and the capture harness.
### C7. Trajectory recorder
SQLite (WAL) plus content-addressed blob store. Records every Action IR, snapshot digest, grounding decision, timing, outcome, human correction. Crash-safe append; refcounted blob GC. This is both the compiler input and the audit substrate.
### C8. Trajectory compiler and drift repair
The moat. Pipeline: normalize (dedupe retries, collapse corrections into the corrected path) -> parameterize (varying or date/amount/path-shaped literals become typed inputs) -> selectorize (stablest chain per step plus anchor redundancy) -> insert waits and postcondition asserts -> emit TypeScript DSL file plus manifest (inputs schema, capabilities, gate bindings, signature block). Deterministic replay executor: zero model calls, zero network, CI-asserted. Drift repair, full loop at launch: failed step -> single-step re-ground -> patch diff file -> human approval in UI or CLI -> versioned merge with changelog entry.
### C9. Invariant gates
InvariantEval-lineage engine. Pre, post, and safety gates evaluated in explore and replay. Failure policy: halt-and-escalate (UI plus voice), or one bounded re-ground if the workflow opts in. Hard safety invariants (credential fields, payment/delete confirmations) live in the runtime, not in workflow files, and cannot be disabled by any manifest.
### C10. Safety, permissions, audit
Capability grants per workflow (apps, paths, network, risk classes) checked at execution time. Dry-run interpreter with zero side effects (filesystem-diff tested). Hash-chained append-only audit log, JSONL and PDF export. Secrets in the OS credential vault only; the planner receives slot references, never values.
### C11. Scheduler and triggers
Cron, file-watch, window-appears, email-arrives. Serialized run queue with priorities; parallelism only across disjoint capability scopes. Unattended triggers launch compiled workflows only, enforced in code.
### C12. Voice sidecar
Local STT (whisper.cpp class) and TTS (Kokoro class) behind a process boundary. Push-to-talk always; wake word optional [ledger candidate]. Emits intents to the palette, renders confirmations, summaries, and gate escalations. Lazy-loaded; yields VRAM to the vision sidecar via the C1 broker.
### C13. Shell UI
Tauri v2: tray, global-hotkey command palette, run viewer streaming bus events with intervene/stop, an explicit model ON/OFF indicator (explore vs replay; the capture harness needs it for the money shot), workflow library, grant prompts, drift patch diff viewer with approve/reject. Two UI modes: default (zero jargon, driven by C19) and Advanced (developer surface). Default mode is the design bar; Advanced is the escape hatch.
### C19. Zero-code layer
The layman gap-closer, built over the same runtime with no privileged APIs:
- **Onboarding wizard**: hardware detection (VRAM, disk), three plain-language setup paths (guided local model download with progress, API key paste with a get-a-key link, demo mode), then a hand-held teach/compile/run of a starter workflow. Wizard media verified audible and visible by an automated check (the near-silent-wizard failure class from prior shipping experience is regression-tested here).
- **Plain-English renderer**: compiles the workflow manifest and DSL AST into numbered human steps with parameter form fields. Bidirectional for parameters only: form edits rewrite manifest inputs; step logic changes route through re-teach or drift repair, never hand-edited prose. The renderer is also used for grant descriptions ("This workflow can read files in Downloads and control Chrome") and drift prompts ("The Save button moved. Update the workflow?").
- **Demo mode**: a sandboxed sample workflow against the fixture web app, runnable with zero grants, so the first minute is watching it work.
- **Template gallery**: in-app registry browser; install is pick, read plain-language grants, approve.
- **Doctor**: self-diagnostics (permissions, model reachability, disk, updater, audio devices) as both `operant doctor` and a "Check my setup" button, each finding paired with a one-click fix where automatable.
- **Microcopy system**: a single glossary file mapping internal terms to user-facing language (trajectory -> recording, compile -> save as workflow, grounding -> finding things on screen); UI strings lint against it in CI so jargon cannot leak into default mode.
- **Tour and hints**: first-run tour, contextual hints that retire after first successful use.
### C20. Guardian set
Trust hardware for a desktop agent, all implemented below the planner so no model state can interfere:
- **Kill switch**: global panic hotkey plus tray button; freezes input synthesis at the action-execution layer and halts all runs in under 100 ms (latency-tested). Tray turns red; recovery is an explicit human resume.
- **Undo journal**: write-class actions record inverses where they exist (file ops via recycle-bin semantics, clipboard restore) into an `undo_journal` table keyed by run; "Undo last run" replays inverses in reverse order and narrates what it restored in plain English. Irreversible actions (sent email, submitted forms) are labeled in the step view before execution.
- **Anchor redaction**: a redaction pass between capture and the blob store; regions the accessibility tree flags sensitive (password fields, credential dialogs) are blacked out before any pixel touches disk. Tested against a fixture credential form.
### C21. Insight set
- **Time-saved ledger**: per-workflow metrics (explore duration versus replay p50, times run count) rolled into tray copy, library badges, and an optional weekly digest notification. Stored in `metrics`; rendered only in plain language.
- **Watch-and-suggest v0**: opt-in, OFF by default. A local repetition detector (n-gram matching over normalized Action IR digests of the user's own manual events) proposes, once per detected pattern, learning a repeated sequence; acceptance seeds a supervised explore run pre-filled with the observed steps. The observation buffer is local, redacted by C20, size-capped, and purgeable in one click. This is the v50 seed shipped at minimum honest scope.
### C22. On-ramps and outreach machinery
- **Playwright importer**: parses a Playwright spec, maps web actions to Action IR, emits a workflow skeleton with TODO markers for desktop context.
- **Composition**: an `adapter_call` kind targeting another workflow; the executor intersects both grant sets at runtime and the plain-English renderer inlines the callee as a collapsible group.
- **Narrated demo pipeline**: assembles capture footage plus a script rendered through the product's own TTS sidecar into the launch video.
- **Browser playground** [stretch, leads the ledger]: the replay executor compiled to WASM driving the fixture web app inside the docs site.
- **Diagnostics bundle**: redacted zip (logs, doctor report, versions) sized for an issue attachment.
### C14. SDK, CLI, MCP (both directions)
TypeScript SDK (the DSL runtime, semver from day one). CLI: `operant run|compile|dry-run|list|install|bench`. MCP server exposes compiled workflows as tools to any MCP client; MCP client registers external MCP servers as C4 adapters.
### C15. Release, updater, docs site
Signed single-source NSIS installer (never dual-source; ADR-0194 lineage). Ed25519-signed update manifests, endpoint live in release builds with a CI assertion (ADR-0193 lineage). Reproducible build doc, SBOM. Docs site built from `docs/` and deployed to GitHub Pages in the campaign. Commerce (Stripe, license keys, Resend) exists as a feature-flagged worker skeleton only; the product is free.
### C16. Registry v0
Git-backed index repo of signed workflow manifests. `operant install <name>`: fetch, verify Ed25519 signature against the publisher key, stage with grants displayed, require local approval; unsigned or unverified workflows run in dry-run only. Publishing: `operant publish` produces a signed manifest PR. Moderation and web of trust are post-launch.
### C17. Benchmark harness
`operant bench` runs a task suite (the fixture tasks plus cookbook workflows) two ways: compiled replay and model re-inference (mock and, when configured, real backends). Emits BENCHMARKS.md: success rate, per-step latency, total tokens/cost. Regenerated each release; regression-gates the replay path; doubles as the launch story's proof artifact.
### C18. E2E and capture harness
One harness, two jobs. CI job: drives victim apps (Notepad, Explorer, fixture web app served locally) through install -> teach -> compile -> schedule -> replay -> audit-verify -> uninstall, headless where possible. Capture job: same flows against the built UI, recording video via Playwright plus OS screenshot fallback, converting to GIF (ffmpeg two-pass palettegen, max width 800, under 8 MB, 12 fps) for the six README asset slots.
## 3. Data model (SQLite core)

```
runs(id, goal, mode[explore|replay|dry], started, ended, status, model_config_json)
steps(id, run_id, seq, action_ir_json, grounding, snapshot_digest, outcome, ms)
artifacts(hash, kind[anchor|screenshot|export], path)          -- content addressed
workflows(id, name, version, dsl_path, manifest_json, signature, source_run_id)
workflow_versions(workflow_id, version, diff_path, approved_by, ts)   -- drift merges
grants(workflow_id, capability, scope, approved_at, approver)
gates(id, workflow_id, step_ref, kind[pre|post|safety], expr, on_fail)
audit(seq, ts, actor, event_json, prev_hash, hash)             -- hash chained
triggers(id, workflow_id, kind, spec_json, enabled)
registry_keys(publisher, pubkey, trust[local|pinned])
bench_runs(id, suite, mode, success_rate, p50_step_ms, tokens, ts)
undo_journal(run_id, seq, inverse_action_ir_json, applied, ts)
metrics(workflow_id, runs, explore_ms, replay_p50_ms, minutes_saved_est, week)
suggestions(id, pattern_digest, occurrences, status[offered|accepted|dismissed], ts)
locales(key, locale, string)
```

## 4. Key trade-offs (decided)
| Decision | Chosen | Rejected | Why |
|---|---|---|---|
| Core language | Rust core, TS SDK/UI | All-Python | Latency, single-binary distribution, existing Rust competence; Python DSL emission post-launch |
| Perception default | Accessibility first | Vision first | 10 to 25x faster, private, precise; vision only where trees fail; anchors make even vision steps replay model-free |
| Model strategy | Bring-your-own | Train/ship own model | Composition beats competition against ByteDance-scale grounders |
| Compiled artifact | Readable TS DSL files | Opaque recordings | Inspectable, diffable, signable; auditability is the product |
| UI framework | Tauri v2 | Electron | Footprint NFR, prior migration experience |
| Platform order | Windows deep first | Cross-platform day one | Windows underserved in OSS agents; traits keep the door open |
| License | Apache 2.0 | AGPL | Distribution is the moat; registry network effects live out-of-repo |
| Proof strategy | Published benchmark in-repo | Claims in README | The determinism story must be checkable or it is hype |
| Voice | Bundled lazy sidecar | Core dependency | Founder requirement without taxing the 16 GB VRAM budget at idle |
| Primary UI mode | Zero-code default, Advanced toggle | Dev-first UI with a simple mode bolted on | The non-coder persona is the growth market; developers tolerate a toggle, laymen do not tolerate jargon |
| Workflow editing | Parameters via forms; logic via re-teach or drift repair | Freeform editing of steps in prose or code by default | Prose-to-action editing is a correctness trap; the safe zero-code edit surface is inputs, scheduling, and approvals |
## 5. Reliability engineering
Single-machine product: scale means long-running stability. Watchdog restarts for sidecars, WAL everywhere, blob GC by refcount, serialized run queue, a 30-minute scheduled-workflow soak in the release gate, and the first-timer path E2E (install, wizard, demo, teach, compile, run, schedule, all in default mode, zero code, zero terminal) as a release blocker equal to the safety suite. The reliability budget concentrates on the replay path: gate coverage, anchor redundancy scoring, drift-repair correctness, and the benchmark as a permanent regression harness. Monitoring is local: run health view, gate-failure trends per workflow, optional opt-in anonymized reliability beacon (off by default).
## 6. Revisit as it grows
macOS/Linux backend implementations behind the existing trait; Python DSL emission; registry web of trust and moderation; org policy packs and team sharing (the flagged commerce skeleton); multi-machine orchestration only if pulled by users, since it doubles the security surface.
