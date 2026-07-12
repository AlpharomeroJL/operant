# Phase 2 integration playbook (orchestrator working notes)

Status: working notes for integrating the 16 Phase 2 lanes into `redesign`.
Not user-facing. Delete after Phase 2 integrates + verifies.

## Lane status (2026-07-12)

MERGED: B14-uninstaller (518b69c), B15-buildmatrix (1b923c1).
HELD, green in worktree, ready to merge: B3-realclient (d3d9aa4), B4-runviewer
(815d789), B5-library (ef74adc), B6-dashboard (4a221c0), B7-palette (a8147cc),
B8-wizard (ddf730a), B9-settings (2cdbcdc), B10-undo (5eb9d44), B11-killswitch
(c1af099), B12-doctor (2fc2279), B13-scheduler (7ceb332), B16-teach (067f302).
STILL RUNNING: B1-serve (core --serve loop), B2-supervisor (shell sidecar).

## Merge order

1. **B1-serve** then **B2-supervisor** (the bridge). B1 conflicts with the
   already-merged B15 in `cli/src/commands/mod.rs` + `main.rs` (both add a verb):
   union both arms (`serve` + `capabilities`).
2. **B3-realclient** (UI foundation: real client + capability blocking screen +
   Demo confinement; restructures `main.ts` client selection; extracts
   `matchesTopic` into `mockClient.ts`).
3. The screen lanes B4..B13, B16. Then reconcile (below). Then consolidated
   `just verify` (cargo clean -p operant-action first, FG-005).

## Shared-file conflicts to resolve

- **ui/src/main.ts** - every UI lane adds wiring. Most are additive (B12 appended
  an EOF block deliberately; others thread one option/port). Union-merge; ensure
  ONE shared client instance (see command layer) not N.
- **ui/src/bus/mockClient.ts** - B3 extracted `matchesTopic(prefix,topic)`; B4
  added an optional `command()` method + a listener `sidecar` arg. Keep both.
- **ui/src/bus/types.ts** - B4 added `RunStepThumb`/`EvtSidecar`. Keep.
- **ui/src/dashboard/state.ts (+view/strings)** - B6 (get_metrics/list_runs +
  list_triggers up-next) AND B13 (deleted UP_NEXT_FIXTURE, honest up-next) both
  edited it. Both honest; keep the union: B6's real metrics/runs + B13's honest
  not_implemented up-next (drop the fixture).
- **ui/src/library/state.ts** - B5 (list_workflows/start_replay/explain) AND B13
  (schedule -> upsert_trigger honest) both edited. Keep both; one honest schedule.
- **teach/command client** - B7 (CoreCommands: startExplore/runSavedWorkflow/
  dryRun/listWorkflows), B16 (TeachClient: startExplore/compileRun), B5
  (CommandClient.request), plus B6/B8/B9/B10/B11/B12 each have their own
  injectable port. COLLAPSE to ONE command layer (below). B7 reroutes palette
  submit; B16 owns compile + save-as-workflow (no second compile button).

## Command layer unification (the main reconciliation)

Every lane invented an injectable seam for UI->core commands/queries, each
mock-backed off-Tauri and meant to be real invoke-backed in Tauri. Unify:
- Base: `CommandClient.request<T>(cmd, args)` (B5) mirrors the contract `res`
  frame - make it THE request/response primitive, backed by B2's `core_call`
  Tauri command (`invoke("core_call",{cmd,args})`).
- Event side: B3's `createRealClient` (publish-routes command-topics to core_call
  + listens for `operant://bus` evts) is the real `BusClient`. Add a `command()`
  method for B4, delegating to the same core_call.
- The typed seams (CoreCommands, TeachClient, UndoCommands, PanicClient,
  DashboardSource, SchedulerCommands, settings liveStore, doctor invoke) each get
  a real impl that calls `CommandClient.request(<contract cmd>, args)`. Wire the
  real ones in `main.ts` when `isTauri()` (B3 gates on capabilities), mock/omit
  otherwise.

## Command-NAME reconciliation to contracts/ipc.md 5b

UI lanes used `docs/specs/ipc-bridge.md` names; the frozen `contracts/ipc.md`
req/res cmd names win. Known: running a saved workflow UI-side `run_saved_workflow`
-> contract `start_replay`. Audit each seam's cmd string against contracts/ipc.md
section 5 when wiring the real `CommandClient`; B1's serve loop is the source of
truth for accepted cmd names.

## Not-yet-implemented commands (surface honestly, no fakes)

probe_backend, delete_workflow, list_triggers, upsert_trigger -> core returns
`not_implemented`; lanes already show honest empty/unavailable states. Keep.

## After integration

- Consolidated `just verify` green (golden + airgap must stay green).
- `just check-release-artifact` (B15) on a real-feature build.
- LIVE end-to-end proof: build the app with the release feature set (real-uia,
  real-input,real-transport), spawn the core sidecar, and drive teach -> watch ->
  compile -> replay -> undo through the real bridge on the desktop. Expect
  P0b-style real issues; that is the "make it work at the end" commitment.
- Then reconcile KNOWN_ISSUES honestly (replay live is proven now; undo screen
  exists; settings wired) and proceed to Phase 3 (C1-C6).
