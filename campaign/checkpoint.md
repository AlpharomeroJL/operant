# Operant campaign checkpoint

Human-readable ledger. Machine truth is `campaign/state.json` + `campaign/merged/*.ok`.
See `campaign/RESUME.md` for the one-move continue procedure.

## Status

- Phase: **Phase 0 complete -> Wave 1 dispatch**
- Repos: `AlpharomeroJL/operant` (public), `AlpharomeroJL/operant-registry` (public)
- Build tree: `D:/dev/operant`  (off OneDrive; `CARGO_TARGET_DIR=D:/dev/operant-target`)
- Identity: all commits `Josef Long <Josefdean@protonmail.com>`, zero AI attribution
- Foundation: 16-crate Rust workspace compiles and tests green; contracts + fixtures frozen

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

## Packet ledger

See `campaign/state.json`. Waves 1-4 plus backlog X1-X16. Depth-first: the core crates
(core, action, recorder, safety+gates, compiler+replay, scheduler, registry, bench) go
green before UI/marketing/registry breadth merges to main.

## Resume drill

- [ ] Not yet run. Scheduled after the first Wave 1 merges (validates frontier reconciliation).
