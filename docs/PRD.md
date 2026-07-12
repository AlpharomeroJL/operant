# Operant PRD

**Operant**: an open source, local-first agentic desktop assistant. It perceives the screen through accessibility trees and vision, acts through synthesized input and app adapters, plans with a pluggable LLM (local or API key), speaks and listens through a bundled local voice stack, and, critically, compiles every successful run into a deterministic, invariant-gated script that replays without inference.

Owner: Josef Long. License: Apache 2.0. Free and open source. This PRD describes the full product; section 9 defines the launch cut (v1.0.0). Horizon versions (v10, v50) exist to force the architecture wide enough that nothing built at launch is ever rewritten.

---

## 1. Problem

Knowledge workers and solo operators perform thousands of repetitive cross-application workflows: pull data from a PDF into a spreadsheet, reconcile a portal against an inbox, file the same report in three systems. Cloud agent products (Simular, Manus, Operator-class tools) solve this with per-seat pricing, cloud execution, and screen data leaving the machine. Open source agents (UI-TARS Desktop, Open Interpreter, UFO) solve perception and action but re-run expensive, slow, non-deterministic inference on every execution, so reliability decays exactly when task length grows.

Two failures define the gap:

1. **Privacy and cost.** Screenshot-to-cloud agents ship your screen (passwords, PHI, financials) to a vendor and meter every action.
2. **Reliability.** Probabilistic replay means a 40-step workflow with 98 percent per-step accuracy fails 55 percent of the time. Nobody ships that to a customer.

## 2. Product thesis

Exploration should be probabilistic. Execution should be deterministic. Operant lets a model explore a task once (or a few times, with human correction), then freezes the successful trajectory into inspectable, auditable code guarded by invariants. After compilation, the workflow runs in milliseconds per step, offline, forever, at zero inference cost. **The model is a compiler, not a runtime.** This is loop engineering applied to computer use: plan, execute in gated packets, verify against invariants, never let the model free-run in production.

## 3. Users and jobs to be done

| Persona | Job | Operant answer |
|---|---|---|
| Solo operator / freelancer | "Every Monday I copy 30 invoice rows from a portal into QuickBooks" | Teach once by voice or demo, compile, schedule |
| Privacy-constrained professional (legal, medical, fire/life-safety, defense) | "Nothing on my screen can leave this machine" | Fully local model path, air-gap mode, signed audit log |
| Developer / power user | "I want Playwright ergonomics for the whole desktop" | TypeScript DSL and SDK over the same runtime, MCP both directions |
| SMB owner | "I want a digital employee, not a $500/mo seat" | Free OSS, registry of vetted workflows, `operant install` |
| Complete non-coder | "I have never opened a terminal and never will" | Guided onboarding, teach by demonstration or voice, workflows shown as plain-English steps, one-click template installs, zero code visible by default |
| Accessibility user | "I need to drive apps by voice" | Voice-first command palette over the same perception layer |

The non-coder persona is the design bar for every user-facing surface. Developers get power through progressive disclosure; laymen get the default experience. If a screen, message, or flow requires knowing what a DSL, an MCP, or a trajectory is, it fails review.

## 4. Functional requirements

### 4.1 Perception
- FR-P1: Windows UI Automation (UIA) tree of any process, normalized snapshot under 200 ms typical, stable selector chains, snapshot diffing, wait-until-changed.
- FR-P2: OS-agnostic `Perceiver` trait; macOS AX and Linux AT-SPI backends compile from day one (stub-level at launch, tier 1 later).
- FR-P3: Vision grounding fallback via a local VLM endpoint (UI-TARS-class through Ollama/llama.cpp) or API vision model; per-step strategy chosen by the runtime and recorded; every vision-grounded step stores an anchor image so replay uses template matching, never a model.
- FR-P4: Browser perception via CDP (DOM plus accessibility tree); Electron and WebView2 attach.
- FR-P5: On-device OCR and structured extraction from PDFs and images, exposed as an adapter.

### 4.2 Action
- FR-A1: Synthesized keyboard, mouse, clipboard, window management, human-plausible pacing option.
- FR-A2: App adapters where APIs beat GUI: shell/PowerShell, filesystem, browser CDP, Office COM (Excel and Word), email (IMAP/SMTP), OCR/PDF. Resolution order enforced in code: adapter beats UIA beats vision.
- FR-A3: Every action is typed, parameterized, serializable Action IR with a risk class (read, write, destructive). No raw coordinate clicks in compiled output unless vision-grounded with a stored anchor and tolerance.

### 4.3 Planning and models
- FR-M1: Pluggable backends across the whole engine market. Local: Ollama, llama.cpp server, LM Studio, vLLM, and any OpenAI-compatible endpoint by base URL. Cloud by API key: Anthropic, OpenAI, Google Gemini, DeepSeek, MiniMax, Moonshot Kimi, Qwen (DashScope), Groq, Mistral, xAI, OpenRouter. Cloud by subscription OAuth (the layman path, no key handling): Sign in with ChatGPT (Codex/ChatGPT plan auth) and Sign in with Claude (Claude plan auth), via PKCE loopback flows with tokens in the OS credential vault and silent refresh. A capability probe classifies every configured backend (vision, tool use, context length) so role assignment degrades gracefully. Zero network calls to any vendor without opt-in.
- FR-M2: Role-split models (planner, grounder, STT, TTS), each independently swappable, with a tested default local stack for 16 GB VRAM consumer GPUs and a VRAM arbitration protocol between sidecars. Default build makes zero network calls except signed update checks (disableable); air-gap mode verified by a CI network test.
- FR-M3: Human-in-the-loop mid-run: pause, redirect in natural language, resume; corrections captured into the trajectory and collapsed by the compiler.

### 4.4 Trajectory compilation (the moat)
- FR-T1: Every run records a trajectory: ordered Action IR, perception digests, grounding strategy, timing, outcomes, corrections.
- FR-T2: One-click compile into a workflow: deterministic TypeScript DSL plus manifest (typed inputs inferred from varying literals, capability requirements, gate bindings, signature block). Workflows are plain files: diffable, signable, shareable.
- FR-T3: Invariant gates (InvariantEval lineage) in both explore and replay: preconditions (right app, right account, expected state), postconditions (counts, sums, file existence), safety invariants (credential fields, payment/delete confirmations). Failure halts and escalates; hard safety invariants are runtime-enforced and cannot be disabled by any workflow file.
- FR-T4: Full drift repair loop: a failed compiled step triggers single-step re-grounding, a proposed patch diff, human approval, and a versioned merge. Workflows heal; they never silently mutate.
- FR-T5: Replay determinism is a tested property: a compiled run makes zero model and zero network calls, asserted in CI.

### 4.5 Voice (bundled)
- FR-V1: Local STT (whisper.cpp/Parakeet class), push-to-talk plus optional wake word.
- FR-V2: Local TTS (Kokoro class) for confirmations, run summaries, and spoken gate escalations.
- FR-V3: Voice is an interface to the same palette and runs, never a separate execution path; lazy-loaded to protect the RAM/VRAM budget.

### 4.6 Orchestration surface
- FR-O1: Tray app, global-hotkey command palette, run viewer with live step stream, intervene/stop, and a visible model ON/OFF indicator distinguishing explore from replay.
- FR-O2: Scheduler: cron, file-watch, window-appears, email-arrives triggers; unattended triggers may launch compiled workflows only.
- FR-O3: MCP both directions: Operant serves compiled workflows as MCP tools, and consumes external MCP servers as adapters.
- FR-O4: CLI (`operant run|compile|dry-run|list|install|bench|doctor|explain`) and stable TS SDK; headless mode.
- FR-O5: Workflow composition v0: a compiled workflow may call another as a step; the effective capability set is the intersection of both grants, enforced at execution time; the plain-English view renders the callee inline as a collapsible group.
- FR-O6: Watch-and-suggest v0 (the v50 seed, opt-in and OFF by default): a local repetition detector over the recorded event stream notices when the user performs a similar manual sequence repeatedly and offers, once, "You have done this 4 times. Want me to learn it?" Accepting starts a normal supervised explore run pre-seeded with the observed steps. Nothing is watched, stored, or suggested unless the user turns it on; the observation buffer is local, redacted per FR-S7, and purgeable in one click.

### 4.7 Safety, permissions, audit
- FR-S1: Capability grants per workflow (apps, paths, network, risk classes) checked at execution time.
- FR-S2: Dry-run interpreter renders the full plan against live perception with zero side effects (filesystem-diff tested).
- FR-S3: Hash-chained append-only audit log with perception digests; JSONL and PDF export.
- FR-S4: Hard invariant: never type into a credential field, never confirm a payment or deletion dialog, without an explicit human approval event. Runtime-enforced, unremovable, regression-tested forever.
- FR-S5: Kill switch: a global panic hotkey (and tray button) that instantly freezes all input synthesis, halts every run, and turns the tray red. Freeze happens at the action-execution layer, below the planner, so no model state can delay it. Latency under 100 ms, tested.
- FR-S6: Undo journal: every write-class action records an inverse where one exists (file create/move/delete to recycle-bin semantics, clipboard restore); "Undo last run" is one click and renders in plain English what will be restored. Actions without a safe inverse (sent email, submitted web form) are labeled irreversible in the plain-English step view BEFORE the run.
- FR-S7: Anchor redaction: stored screenshots and vision anchors auto-redact regions flagged sensitive by the accessibility tree (password fields, credential dialogs) before they touch disk; verified by test.

### 4.8 Zero-code experience
- FR-U1: Onboarding wizard: detects hardware, offers three paths in plain language (download a free local model with a progress bar and disk/VRAM guidance, paste an API key with a link to get one, or start in demo mode), then walks the user through teaching, compiling, and running a starter workflow. Target: first compiled workflow in under 15 minutes for a non-technical user. Wizard media and sample flows are verified audible/visible in CI (the near-silent-wizard failure class is a known trap).
- FR-U2: Plain-English workflow view: every compiled workflow renders as numbered human steps ("Open Chrome", "Click Sign in", "Type the invoice number into the Amount box") with editable parameters as form fields. The TypeScript view exists behind an Advanced toggle; a layman can teach, inspect, edit inputs, approve drift patches, and schedule without ever seeing code.
- FR-U3: Human error handling: every user-facing failure states what happened, why, and one suggested action in plain language, with a one-click fix when the fix is automatable. `operant doctor` runs self-diagnostics (permissions, model reachability, disk, updater) and is surfaced as a "Check my setup" button, not just a CLI verb.
- FR-U4: Demo mode: a sandboxed sample workflow runnable before granting any capability, so the first experience is watching Operant work, not configuring it.
- FR-U5: In-app template gallery backed by the registry: browse, pick, review grants written in plain language ("This workflow can read files in Downloads and control Chrome"), approve, run. Install is a click, never a command.
- FR-U6: Progressive disclosure: the default UI contains no jargon (no DSL, MCP, trajectory, grounding). Advanced mode unlocks the developer surface. Copy is governed by a microcopy glossary enforced in review.
- FR-U7: Learnability over time: an in-app tour on first run, contextual hints that retire themselves after use, and drift-repair prompts phrased as "The Save button moved. Want me to update the workflow?" with a preview.
- FR-U8: Time-saved ledger: Operant estimates minutes saved per workflow (explore duration versus replay duration, times runs) and surfaces it plainly ("Operant saved you 3.2 hours this week") in the tray, the library, and an optional weekly digest notification. This is the layman's dashboard and the screenshot people share.
- FR-U9: Backup and portability: one-click export/import of all workflows, grants, and settings as a single file; restore onto a fresh machine reproduces the setup.
- FR-U10: The app itself is accessible: full screen-reader labeling, keyboard-only navigation, and contrast compliance. A product built on accessibility trees exposes a first-class tree of its own; CI checks it.
- FR-U11: `operant explain <file>` renders any workflow file as plain-English steps in the terminal and the UI, using the same renderer as FR-U2, so a shared or registry workflow can be understood before it is trusted.

### 4.9 Distribution, registry, proof
- FR-D1: Signed single-source NSIS installer, Ed25519-signed auto-update manifests (endpoint live in release builds, CI-checked), reproducible build recipe, SBOM.
- FR-D2: Registry v0: git-backed index of signed workflow manifests; `operant install <name>` fetches, verifies signature, and grants run only after local approval; unsigned workflows execute in dry-run only.
- FR-D3: Benchmark harness and published BENCHMARKS.md: compiled replay versus model re-inference on the same task suite, success rate and per-step latency, regenerated each release. The benchmark is a marketing asset and a regression gate.
- FR-D4: Cookbook of at least 10 real workflows, every code sample executed by a doc-test script; docs site deployed to GitHub Pages.
- FR-D5: Importers: `operant import playwright <test.spec.ts>` converts a Playwright test into a workflow skeleton (web steps mapped to Action IR, TODO markers where desktop context is needed), giving developers a zero-cost migration on-ramp.
- FR-D6: The demo video is narrated by Operant's own local TTS voice, stated in the video: the marketing dogfoods the product.
- FR-D7: Open source hygiene at launch: CONTRIBUTING, SECURITY.md with disclosure policy, code of conduct, issue and PR templates, and ten seeded good-first-issues so the repo is contributable on day one.
- FR-D8: Browser playground (stretch, first ledger cut): the deterministic replay executor compiled to WASM drives the fixture web app inside the docs site, so a visitor can watch teach-and-replay in the browser before downloading anything.
- FR-D9: i18n scaffold: UI strings extracted to a locale catalog with one non-English locale (Spanish) as proof; the microcopy glossary becomes per-locale.
- FR-D10: Diagnostics bundle: one click produces a redacted zip (logs, doctor report, versions) sized for a GitHub issue attachment.

## 5. Non-functional requirements

- NFR-1 Latency: compiled step under 150 ms median on the accessibility path; exploratory step under 3 s API, under 15 s local on reference hardware (RTX 4060 Ti 16 GB).
- NFR-2 Reliability: compiled replay at or above 99 percent on unchanged UI; 100 percent halt (never wrong-action) on gate failure; a 30-minute scheduled-workflow soak passes before any release.
- NFR-3 Privacy: default build makes zero network calls except update checks; air-gap mode CI-verified.
- NFR-4 Footprint: idle under 300 MB RAM with no models loaded; sidecars lazy.
- NFR-5 Security: least-privilege process model, per-workflow sandboxing, secrets only in the OS credential vault, planner sees slot references never values.
- NFR-6 Testability: every subsystem behind a versioned contract with fixtures; E2E harness drives victim apps (Notepad, Explorer, a fixture web app) headlessly in CI; the same harness captures README screenshots and GIFs.
- NFR-7 Usability: a scripted "first-timer path" E2E (install, wizard, demo mode, teach starter task, compile, run, schedule) passes in CI touching only default-mode UI, zero code visible, zero terminal required. Time budget asserted at 15 minutes of simulated interaction.

## 6. Non-goals

- No cloud execution environment; local machine or user-owned VM only.
- No autonomous free-running mode without gates, ever. This is the brand.
- No user-to-user payments in core; the registry is free sharing.
- No mobile control.

## 7. Success metrics

North star: compiled workflow executions per week. Activation: percent of new users compiling a first workflow within 48 hours (target 40), measured separately for users who never open Advanced mode (the number that actually matters). Reliability: gate-halt rate versus silent-failure rate (silent target zero) plus the published benchmark trend. Community: registry installs and workflows published per month; stars as top-of-funnel proxy.

## 8. Competitive posture

Compose with the perception giants (UI-TARS-class grounders are a supported backend, not a rival) and differentiate on the compilation and invariant layer, Windows-native depth, offline voice, radical local privacy, and the published determinism benchmark. Against Simular: the same neuro-symbolic thesis, open source, local, free. Distribution runs through the founder's build-in-public engine; every ADR is a post, the benchmark is the launch story.

## 9. Launch cut: v1.0.0

Everything above EXCEPT: macOS/Linux perception implementations (traits compile, backends stubbed), Python DSL emission (TS only at launch), wake word if squeezed (push-to-talk always), commerce (free product; the Pro/teams stack is a flagged skeleton), registry moderation tooling (signing and local approval only). Explicitly IN for v1.0.0: the entire zero-code layer (onboarding wizard, plain-English workflow view, demo mode, template gallery, human error handling, doctor, tour), the guardian set (kill switch, undo journal, anchor redaction), the insight set (time-saved ledger, opt-in watch-and-suggest v0), workflow composition v0, the Playwright importer, full drift repair loop, benchmark harness with published numbers, registry v0 with `operant install` and in-app installs, 10-workflow cookbook, deployed docs site, MCP both directions, voice, vision fallback with anchor replay, all adapters in FR-A2, signed installer with live updater, the open source hygiene kit, the TTS-narrated demo video, and the README capture suite. The first-timer path E2E is a release blocker equal in rank to the safety suite. The browser playground and i18n scaffold ride along if the clock allows (they lead the ledger).

## 10. Horizon markers (architecture pressure only)

v10: macOS/Linux tier 1, Python DSL, registry web of trust, org policy packs, team sharing. v50: Operant as the ambient automation runtime of the machine: every repetitive act observed (opt-in), proposed as a compilable workflow, the OS-level standard library of a person's digital labor, still local, still gated, still auditable. Nothing in v50 requires breaking a v1 contract; that is the design test.

## 11. Key risks

| Risk | Mitigation |
|---|---|
| UI drift breaks compiled workflows | FR-T4 full repair loop plus anchor redundancy (selector, role+name, ordinal, vision anchor per step) |
| Local model quality insufficient for exploration | API-key path is first-class; local is a privacy option, not a purity test |
| Safety incident | Runtime-enforced hard invariants, dry-run default for new and installed workflows, signed audit |
| ByteDance or Microsoft ships the compiler layer | Speed, community registry, the audit/compliance story for regulated users, published benchmark |
| Solo-maintainer bus factor | Contract boundaries, ADR discipline, CI gates so contributors land safely |
