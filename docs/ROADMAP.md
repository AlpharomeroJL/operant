# Operant Roadmap

Version numbers are release milestones, not semver majors. v1.0.0 is the initial release. Later versions exist both as a real plan and as architectural pressure: nothing in them may require breaking a v1 contract.

---

## v1.0.0: initial release

**Hook: "Explore once with a model. Replay forever without one."**

The launch cut, per PRD section 9. Compiler, gates, and audit are the story; the benchmark is the proof; the registry and cookbook are the on-ramp.

- Perception: Windows UIA (C2), vision grounding with anchor-based model-free replay (C3), browser CDP (C5), OCR/PDF adapter
- Action: input synthesis plus shell, filesystem, Office COM, email, browser, MCP-client adapters (C4)
- Orchestration: explore loop with HITL redirect, Ollama/llama.cpp plus Anthropic/OpenAI backends, mock planner for CI (C6)
- The moat: recorder (C7), compiler to TypeScript DSL with typed inputs and gate bindings, deterministic replay with CI-asserted zero model/network calls, FULL drift repair loop with approved versioned merges (C8)
- Safety: grants, dry-run, hash-chained audit with export, runtime-enforced hard invariants (C9, C10)
- Voice: local STT and TTS, push-to-talk, spoken escalations, lazy load, VRAM arbitration (C12)
- Surfaces: Tauri tray, palette, run viewer with model ON/OFF indicator, drift diff approval UI (C13); CLI with run/compile/dry-run/list/install/bench/doctor; MCP server and client (C14)
- Zero-code layer (C19): onboarding wizard with guided model setup, plain-English workflow view with form-field editing, demo mode, in-app template gallery, human error messages with one-click fixes, doctor, microcopy glossary lint, first-run tour; first-timer path E2E as a release blocker
- Guardian set (C20): sub-100ms kill switch, undo journal with "Undo last run", anchor redaction before disk
- Insight set (C21): time-saved ledger with weekly digest, opt-in watch-and-suggest v0 (OFF by default)
- On-ramps (C22): workflow composition v0 with grant intersection, Playwright importer, `operant explain`, diagnostics bundle, TTS-narrated demo video, open source hygiene kit (CONTRIBUTING, SECURITY, templates, ten seeded good-first-issues); browser playground and Spanish locale if the clock allows
- Scheduler: cron, file, window, email triggers, compiled-only unattended runs (C11)
- Distribution: signed installer, live Ed25519 updater, SBOM, reproducible build doc, deployed GitHub Pages docs site (C15); registry v0 with signed manifests and `operant install` (C16)
- Proof and marketing: benchmark harness with published BENCHMARKS.md (C17); six-asset capture suite, marketing README, 10-workflow doc-tested cookbook, LAUNCH.md with HN post, reframe post, and thread (C18, L15)
- Release: public GitHub repo with topics, v1.0.0 tagged release with installer, checksums, SBOM

## v1.x: the listening releases (weeks 1 to 8 post-launch)

Driven by issue traffic and registry telemetry (opt-in only). Standing candidates: wake word if ledgered out, adapter requests from real workflows, Windows 11 UIA quirks, drift-repair edge cases, benchmark suite expansion with community tasks.

## v2: Python and publishers
**Hook: "Same workflow, your language."** Python DSL emission and runtime bindings; registry publisher tooling (`operant publish` flow hardened, publisher key pinning UX); workflow-of-the-week content engine.

## v3: macOS
**Hook: "Your Mac, same determinism."** AX backend graduates from stub to tier 1 behind the existing Perceiver trait; capture suite re-run on macOS; universal cookbook.

## v4: Teams (first paid tier, core stays free)
**Hook: "One-time price. Your whole team runs the same audited workflows."** Flip the commerce flag: team workflow sharing, org policy packs (centrally enforced grants), audit export packs, priority adapters. Compliance narrative for regulated users (legal, medical, fire/life-safety).

## v5: Trust at scale
**Hook: "Install a workflow like a package, trust it like a signed binary."** Registry web of trust, moderation tooling, reproducible-build attestation for workflows, security review and threat model publication, responsible disclosure program.

## v10 horizon
Linux tier 1, multi-profile machines, workflow composition graduating from v0 (grant intersection) to typed pipelines with data contracts between workflows, watch-and-suggest graduating from repetition detection to intent clustering, local fine-tune loop for the grounder on the user's own anchor corpus, benchmark as a public leaderboard others can submit agents to.

## v50 horizon (architecture pressure)
Operant as the ambient automation runtime of the machine: opt-in observation of repetitive manual work, proactive "I can compile this" proposals, an OS-level standard library of a person's digital labor. Still local, still gated, still auditable, still free at core. The design test since day one: nothing on this line requires breaking a v1 contract.

---

## Sequencing logic
v1 ships the differentiator at full depth (compiler, gates, drift repair, benchmark) rather than breadth-first platform coverage, because the launch story must be the thing nobody else has, provable in the repo. Platforms (v3) and payment (v4) wait for reliability proof and community. The benchmark and the capture suite are standing tracks: regenerated every release, because the claim "deterministic replay" must remain checkable forever.

## Standing tracks (every release)
- BENCHMARKS.md regenerated; replay regression blocks release
- Every ADR-worthy decision becomes a build-in-public post
- Any new action class ships with its invariant and a dry-run test first
- Em-dash grep stays in CI (style is enforced by machine, not memory)
- First-timer path E2E and the microcopy glossary lint run on every release; a feature that cannot pass them in default mode ships behind Advanced or not at all
