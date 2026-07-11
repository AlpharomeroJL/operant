# Operant campaign checkpoint

Human-readable ledger. Machine truth is `campaign/state.json` + `campaign/merged/*.ok`.
See `campaign/RESUME.md` for the one-move continue procedure.

## Status

- Phase: **Wave 1 done; Wave 2 + backlog in progress** (27 lanes + PHASE0 merged)
- Merged: the full deterministic + safety + moat foundation is real and tested:
  ir, core, action (+native adapters fs/email/OCR/Office), recorder (+undo journal +backup),
  gates, safety (+audit PDF export), compiler + model-free replay (+composition), perception
  (fixture + UIA), model backends (16-provider quirk table + mock transport), scheduler,
  registry (client verify + signed index repo), voice sidecar, doctor (+diagnostics bundle),
  model downloader, Tauri UI foundation, cookbook, bench renderer, docs site + guides,
  community kit, soak runner, launch drafts.
- In flight: L7A (explore loop), X4 (anchor-redaction), X1 (kill-switch).
- Not yet dispatched (remaining): E1A/E1B/E1C (e2e + first-timer), L9A (browser-adapter),
  L4B (backend-integration), L12A (shell-complete), L13A (cli-sdk-mcp), L14A (release),
  L8B (drift-repair), L7B (registry-surface), L9B (bench-suite), L10B (soak-run), U1B, U2B,
  U2C, U3B, U4A (renderer), U5A, C1B, M1C, V1-V5, X3, X5, X8, X9, X11, X12, X14, X15, X16.
- Repos: `AlpharomeroJL/operant` (public), `AlpharomeroJL/operant-registry` (public). SSH remotes.
- Build tree: `D:/dev/operant`  (off OneDrive; `CARGO_TARGET_DIR=D:/dev/operant-target`)
- Identity: all commits `Josef Long <Josefdean@protonmail.com>`, zero AI attribution
- `main` is green on `just ci` after every merge.

## Standing decisions (durable)

- NO GitHub Actions / workflows. CI is on-device (`just ci`) only. Docs site deploys from
  the `gh-pages` branch (subtree push), not Actions. See RESUME.md.
- Remotes are SSH (OAuth token lacks `workflow` scope).
- Heavy platform deps (`windows` crate) are added per-crate behind a feature (`real-input`
  in action, expected `real-uia` in perception) so the default workspace build stays lean
  and CI-green headless; the fixture/mock path is always available.

## Phase 0 deliverables (done)

- docs/: PRD, ARCHITECTURE, ROADMAP, 15 feature specs
- contracts/: action_ir, perception_snapshot, workflow_manifest, registry_manifest,
  bench_result schemas; bus_events + model_backend prose; microcopy_glossary; fixtures
  (notepad snapshot, trajectory, compiled workflow, gates, drift pair, web app, credential
  form, IMAP dump, model download + checksum, signed registry manifest + keypair, OAuth config)
- crates/: ir (real shared types + fixture round-trip tests), core (bus + Perceiver trait +
  config), plus 14 compiling skeleton crates with a structurally backend-free replay crate
- justfile + scripts: build, test, check-json, check-emdash, check-microcopy, check-airgap
- .claude/: 7 operant-* skills, 3 tiered lane agents
- campaign/: state.json (64 packets), gen_state.mjs, frontier.mjs, RESUME.md, this file

## Fix-at-gate log

- FG-001 (ir): removed `Eq` derive from `Anchor` and `Target` (contain f64). Scaffold bug.
- FG-002 (ci): defined em-dash sentinels by code point so the checker does not match itself.
- FG-003 (S1A): relocated a stray root `package.json` into `e2e/soak/` with relative paths.
- FG-004 (contract): documented the new `config.changed` topic in `bus_events.md` (L1A followup).

## Packet ledger

See `campaign/state.json`. Waves 1-4 plus backlog X1-X16. Depth-first: the core crates
(core, action, recorder, safety+gates, compiler+replay, scheduler, registry, bench) go
green before UI/marketing/registry breadth merges to main.

## Known integration issues (flagged by E1B golden path; fix before "live-perfect")

- KI-1 (replay selector resolution): `ExploreLoop` records the proposed action but does not
  backfill `target.coords_last_known` with the point perception actually resolved, and the
  replay executor currently leans on `coords_last_known` rather than re-resolving the selector
  chain against a live perceiver (action.md wants fresh resolution at execution time). A real
  (non-scripted) planner using selectors alone would compile a click replay cannot resolve on
  live Windows. Fix: either enrich the recorded step in L7A, or have replay resolve selectors
  via a `Perceiver` at run time (a perceiver is not a model backend, so this keeps replay
  model-free). Fixture/mock path is green; live path needs this.
- KI-2 (correction field mismatch): explore records `human_correction: {instruction, at_seq}`
  but the compiler collapses on `human_correction.supersedes_seq`. A live HITL redirect will
  not collapse the superseded step the way the hand-authored fixture does. Fix: reconcile the
  field name across L7A explore/control and the compiler normalize pass.

## Resume drill

- [ ] Not yet run. Scheduled after the first Wave 1 merges (validates frontier reconciliation).
