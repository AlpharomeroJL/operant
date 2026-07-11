# Operant campaign checkpoint

Human-readable ledger. Machine truth is `campaign/state.json` + `campaign/merged/*.ok`.
See `campaign/RESUME.md` for the one-move continue procedure.

## Status

- Phase: **Wave 1 in progress** (core spine complete)
- Merged (10 lanes + PHASE0): C1A, B1A, D1A, X10, S1A, L6A, L1A, L5A, L3A, M1A.
  The full deterministic + safety foundation is real and tested: ir, core (bus/config/
  supervisor), action (executor/synth/adapters), recorder (SQLite WAL/blobs/GC), gates
  (evaluator), safety (grants/dry-run/audit/FR-S4). Plus cookbook, bench renderer, docs
  site, community kit, soak runner, launch drafts.
- In flight: L2A (perception-uia), L4A (model-backends), L8A (compiler + replay, the moat).
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

## Resume drill

- [ ] Not yet run. Scheduled after the first Wave 1 merges (validates frontier reconciliation).
