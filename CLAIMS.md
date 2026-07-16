# CLAIMS.md: marketing is a test suite

Every capability sentence Operant publishes must cite a passing test id or an
evidence doc. This file is the ledger that pairs each published claim with its
proof. The contract is deliberately harsh:

- A claim that cannot cite a real, passing test or a real evidence doc is
  **deleted**, never softened. We do not ship copy we cannot prove.
- A citation is **never fabricated**. Every id below was confirmed to exist in
  the named file by grep. If backing does not exist, the claim goes in the
  `## Unbacked` section and the copy gets cut or earned, not dressed up.
- `just claims` runs `release/scripts/check-claims.mjs`, which parses this file,
  fails if any citation is dangling, and fails if the `## Unbacked` section
  still lists any claim. A red `just claims` means: cut the copy, or land the
  test that backs it.

## How to read this file

- The final column of every row in `## Backed claims` holds the machine-checked
  citations, each wrapped in backticks. Two citation shapes are checked:
  - `path/to/file.rs::test_or_title` - the file must exist AND the text after
    `::` must appear verbatim in it (a test fn name, or a UI test title).
  - `path/to/file` or `just <recipe>` - the file (or recipe) must exist.
- The "Where it is published" column is a human pointer (file and approximate
  line). It is not machine-checked, so line drift never breaks the gate.
- Sources of published copy audited here: `README.md`, `site/index.html` (the
  live landing page; `dist/site/index.html` is its generated copy), and
  `LAUNCH.md` (Show HN, launch thread, Product Hunt copy).

## Backed claims

| # | Claim (quoted from published copy) | Where it is published | Verified citations (machine-checked) |
|---|---|---|---|
| 1 | "Describe a task once... a model works the task out live on your screen, and Operant freezes that run into a workflow it can repeat on its own" (shipped teach path is model-driven describe-it, not record-by-doing; the recorder that watches you is roadmap) | README.md:11,21; site/index.html:29,42 | `e2e/golden-path/tests/golden_path.rs::golden_path_explores_compiles_and_replays_with_zero_model_calls`, `crates/orchestrator/tests/explore_loop.rs::explore_loop_completes_a_scripted_notepad_task_recording_and_gating_every_step`, `docs/roadmap/demonstration-capture.md` |
| 2 | "local speech in and out, lazy-loaded so it does not sit in memory until used" | README.md:248; site/index.html:138 | `sidecars/voice/test/roundtrip.test.js::text-mode round trip: STT -> intent -> TTS with the mock provider`, `crates/core/src/config.rs`, `docs/specs/voice.md` |
| 3 | "Replay after that makes zero model calls and zero network calls, both asserted in CI, not just promised" | README.md:71; site/index.html:92 | `e2e/golden-path/tests/golden_path.rs::golden_path_explores_compiles_and_replays_with_zero_model_calls`, `crates/replay/tests/replay_notepad.rs::replay_performs_zero_network_operations`, `just check-airgap` |
| 3b | "Deterministic, model-free replay: Yes, CI-asserted" | README.md:285; site/index.html:160 | `crates/replay/src/lib.rs::replay_crate_is_backend_free`, `crates/replay/tests/replay_notepad.rs::replay_reproduces_click_type_save_and_passes_postcondition` |
| 4 | "a model call during replay is not a setting that could be flipped on by accident, it is a compile-time impossibility" | README.md:135; site/index.html:127 | `crates/replay/src/lib.rs::replay_crate_is_backend_free`, `e2e/golden-path/tests/golden_path.rs::replay_crate_has_no_model_or_network_dependency`, `crates/safety/tests/audit_export.rs::airgap_safety_closure_pulls_in_no_networking_crate` |
| 5 | "Works fully offline after teaching: Yes" | README.md:286; site/index.html:161 | `crates/replay/tests/replay_notepad.rs::replay_performs_zero_network_operations`, `crates/orchestrator/tests/backend_contract.rs::fixture_grounder_is_deterministic_and_needs_no_network_or_gpu`, `just check-airgap` |
| 6 | "One key stops everything, instantly. The kill switch runs at the action layer, below the planner... CI holds the freeze under 100 ms" | README.md:79; site/index.html:95 | `crates/action/tests/killswitch.rs::engage_freezes_the_next_dispatch_within_100ms_of_a_mid_run_trigger`, `crates/action/tests/killswitch.rs::frozen_run_leaves_no_partial_synthesizer_calls` |
| 7 | "Kill switch under 100ms: Yes, latency-tested" | README.md:291; site/index.html:166 | `crates/action/tests/killswitch.rs::engage_freezes_the_next_dispatch_within_100ms_of_a_mid_run_trigger`, `ui/src/safety/panic.test.ts::the backstop kill still fires when the cooperative stop throws (never-cut: a wedged stop cannot skip the kill)` |
| 8 | "the panic hotkey fires and the run freezes" (kill switch releases held modifiers, no chord rides along) | README.md:40; LAUNCH.md:87 | `crates/action/tests/killswitch.rs::engaging_before_a_dispatch_performs_the_release_all_sweep`, `ui/src/safety/panic.test.ts::the panic path releases held modifiers before either stop, so a chord modifier cannot ride along` |
| 9 | "Every run can be undone: write actions record an inverse before they run, so Undo last run is a real replay of real inverses" | README.md:85; site/index.html:95 | `crates/recorder/tests/undo_journal.rs::undo_last_run_restores_temp_dir_byte_identical`, `crates/recorder/tests/undo_journal.rs::undo_previewed_bus_event_carries_real_items_and_undo_still_restores_byte_identical` |
| 10 | "Undo last run is a real replay of real inverses, narrated in plain English" | README.md:86; site/index.html:95 | `ui/src/undo/view.test.ts::preview: heading, a checkmark per restorable item, no mark and a grayed class on the irreversible one, Confirm and Cancel both present`, `ui/src/__tests__/undo-entry-points.test.ts::both entry points open the identical preview for the same completed run` |
| 11 | "Anything without a safe inverse, like a sent email, is labeled irreversible before you run it, not after" | README.md:87; site/index.html:95 | `crates/recorder/tests/undo_journal.rs::email_send_step_renders_cannot_be_undone_label`, `crates/recorder/tests/undo_journal.rs::irreversible_step_coexists_with_real_inverses` |
| 12 | "One-click undo: Yes" | README.md:290; site/index.html:165 | `ui/src/__tests__/undo-entry-points.test.ts::run detail: the entry point is unreachable while a run is live, and reachable once it is done, opening the same run's undo preview` |
| 13 | "Operant re-grounds that one step, proposes a patch diff, and waits for a human approval before merging a new version" | README.md:113; site/index.html:108 | `crates/compiler/tests/drift.rs::reground_finds_the_renamed_button`, `crates/compiler/tests/drift.rs::patch_diff_maps_old_selectors_to_new`, `crates/compiler/tests/drift.rs::approval_bumps_version_records_changelog_and_repairs_the_step` |
| 14 | "The workflow heals. It never silently mutates" | README.md:115; site/index.html:108 | `crates/replay/tests/replay_browser_fixture.rs::a_renamed_save_button_breaks_replay_instead_of_silently_passing`, `crates/compiler/tests/drift.rs::rejection_archives_the_patch_without_touching_the_version`, `crates/compiler/tests/drift.rs::destructive_step_is_refused_for_auto_repair` |
| 15 | "Self-heals on UI drift, human-approved: Yes" | README.md:287; site/index.html:162 | `crates/compiler/tests/drift.rs::renamed_button_is_drift_eligible_not_wrong_state`, `crates/compiler/tests/drift.rs::reground_finds_the_renamed_button` |
| 16 | "compiled replay succeeds 5 out of 5 runs with zero model calls" (benchmark, regenerated each release) | README.md:119; site/index.html:111; LAUNCH.md:57 | `crates/bench/tests/suite_run.rs::suite_run_produces_benchmarks_md_and_passes_the_regression_threshold`, `crates/bench/tests/suite_run.rs::a_deliberately_broken_replay_fails_the_regression_threshold`, `BENCHMARKS.md` |
| 17 | "Perception: Windows accessibility tree (UIA) first" (re-resolves against a live perceiver) | README.md:229; site/index.html:132 | `crates/replay/tests/live_reresolve.rs::replay_reresolves_the_selector_chain_against_a_live_perceiver_not_stale_coords`, `docs/evidence/P0-live-engine.md` |
| 18 | "CDP for browsers" | README.md:230; site/index.html:132 | `crates/action/src/adapters/browser/cdp.rs::attach_to_an_unreachable_target_is_a_typed_cdp_error`, `crates/replay/tests/replay_browser_fixture.rs::replays_the_playground_fixture_through_the_browser_adapter` |
| 19 | "OCR for PDFs and images" | README.md:230; site/index.html:132 | `crates/action/src/adapters/ocr/mod.rs::pdf_fixture_round_trips_through_action_ir_and_finds_the_invoice_tokens`, `crates/action/src/adapters/ocr/mod.rs::png_fixture_round_trips_through_action_ir_and_finds_the_invoice_tokens` |
| 20 | "a vision-grounding fallback... Every vision step stores an anchor image, so replay resolves by template match, never by calling a model" | README.md:230; site/index.html:132 | `crates/compiler/tests/drift.rs::a_matching_anchor_blocks_drift`, `docs/models.md` |
| 21 | "a typed, serializable Action IR for every step, with a risk class of read, write, or destructive" | README.md:233; site/index.html:133 | `crates/action/tests/fixture_trajectory.rs::trajectory_replays_the_expected_synthesizer_calls`, `contracts/fixtures/workflow_notepad/workflow.ts` |
| 22 | "normalizes a recorded run, turns varying literals into typed inputs, scores selectors for stability, and emits a TypeScript file plus a signed manifest" | README.md:237; site/index.html:134 | `e2e/golden-path/tests/golden_path.rs::golden_path_explores_compiles_and_replays_with_zero_model_calls`, `contracts/fixtures/workflow_notepad/workflow.ts`, `crates/registry/src/sign.rs::a_freshly_signed_draft_manifest_verifies` |
| 23 | "precondition, postcondition, and safety checks run in both explore and replay" | README.md:240; site/index.html:135 | `crates/gates/tests/gates_fixture.rs::post_gates_pass_against_a_successful_run`, `crates/replay/tests/live_gates.rs::live_gate_snapshots_override_a_stale_post_context`, `crates/replay/tests/replay_notepad.rs::postcondition_fails_when_the_note_was_not_written` |
| 24 | "Hard safety invariants... live in the runtime, not in workflow files, and no workflow can turn them off" | README.md:241; site/index.html:135 | `crates/safety/src/manifest_guard.rs::manifest_that_tries_to_disable_safety_fails_to_load`, `crates/safety/src/invariants.rs::password_field_requires_approval` |
| 25 | "never type into a credential field" (LAUNCH safety suite) | LAUNCH.md:184 | `crates/safety/src/invariants.rs::password_field_requires_approval`, `crates/orchestrator/tests/explore_loop.rs::the_safety_gate_blocks_a_credential_field_and_halts_the_run`, `crates/recorder/tests/redaction.rs::credential_form_password_field_is_blacked_out_pixel_for_pixel` |
| 26 | "never confirm a payment or a delete without a person approving it" (LAUNCH safety suite) | LAUNCH.md:184 | `crates/safety/src/invariants.rs::payment_dialog_requires_approval`, `crates/safety/src/invariants.rs::deletion_dialog_requires_approval` |
| 27 | "capability grants per workflow" | README.md:244; site/index.html:136 | `crates/replay/tests/composition.rs::refuses_at_load_when_the_child_needs_more_than_the_parent_grants`, `crates/safety/tests/safety_contract.rs::property_no_action_that_exceeds_grants_is_ever_allowed` |
| 28 | "a dry-run mode with zero side effects" | README.md:244; site/index.html:136 | `crates/safety/tests/safety_contract.rs::dry_run_leaves_temp_dir_byte_identical`, `crates/safety/src/dryrun.rs` |
| 29 | "a hash-chained append-only audit log with JSON and PDF export" | README.md:245; site/index.html:136 | `crates/safety/src/audit/mod.rs::verify_fails_on_a_tampered_payload`, `crates/safety/src/audit/mod.rs::jsonl_roundtrip_preserves_verification`, `crates/safety/tests/audit_export.rs::export_pdf_is_valid_and_carries_head_and_event_text` |
| 30 | "Full audit trail, hash-chained, exportable: Yes" | README.md:288; site/index.html:163 | `crates/safety/src/audit/mod.rs::verify_fails_on_a_tampered_hash_that_breaks_the_link`, `crates/safety/tests/audit_export.rs::export_pdf_xref_offsets_point_at_real_objects` |
| 31 | "operant install <name>... verifies its Ed25519 signature against a publisher key... installs only after approval. Unsigned or unverified workflows... run in dry-run only" | README.md:258; site/index.html:147 | `crates/registry/src/verify.rs::fixture_signature_is_valid`, `crates/registry/src/verify.rs::wrong_key_is_rejected_before_touching_the_signature`, `crates/registry/src/install.rs::unsigned_manifest_installs_dry_run_only`, `ui/src/gallery/state.test.ts::install() opens a preview with plain-language steps and permissions before approval` |
| 32 | "MCP runs both directions. Operant serves every compiled workflow as an MCP tool... and it consumes external MCP servers as adapters" | README.md:268; site/index.html:147 | `crates/orchestrator/tests/mcp_handshake.rs::server_direction_initialize_lists_and_invokes_the_compiled_workflow_tool`, `crates/orchestrator/tests/mcp_handshake.rs::client_direction_handshakes_discovers_and_registers_a_mock_peers_tool_as_an_mcp_adapter` |
| 33 | "operant run, compile, dry-run, list, install, bench, doctor, explain" (CLI surface, plain-English explain) | README.md:273; site/index.html:147 | `cli/tests/explain_exhaustive.rs::test_explain_exhaustive_fixtures`, `cli/tests/release_gate.rs::real_capability_blob_passes_the_gate` |
| 34 | "bring your own backend... across local runners, API keys, and sign-in-with-subscription, or run fully offline" (the capability, stated without a numeric count) | README.md:246; site/index.html:137; LAUNCH.md:114 | `crates/orchestrator/src/backends/quirks.rs::table_covers_every_provider_docs_specs_backends_promises`, `crates/orchestrator/tests/backend_contract.rs::anthropic_dialect_round_trips_through_complete`, `crates/orchestrator/src/oauth/provider.rs`, `docs/models.md` |
| 35 | "signing in with a subscription you already pay for" (Sign in with ChatGPT / Sign in with Claude) | LAUNCH.md:115; README.md:246 | `ui/src/wizard/state.test.ts::choosing ChatGPT writes real engine config through configure_backend (provider, model, and a real planner)`, `crates/orchestrator/src/oauth/provider.rs` |
| 36 | "run viewer with a model on/off indicator" (Thinking live / Running from memory) | README.md:250; site/index.html:139 | `ui/src/runViewer/state.test.ts::run.started moves to running and sets the model indicator from mode`, `ui/src/runViewer/view.test.ts::a saved-workflow run shows the quiet gray no-AI chip with the exact design copy` |
| 37 | "a plain-English workflow view... the same text the plain-English workflow view shows a non-coder, from the same file" | README.md:220; site/index.html:139 | `ui/src/runViewer/sdkRender.test.ts::renders a real step through the plain-English renderer`, `cli/tests/explain_exhaustive.rs::test_explain_exhaustive_fixtures` |
| 38 | "No-code path for non-developers: Yes, wizard plus plain-English steps" (Setup never touches a terminal) | README.md:289; site/index.html:164; LAUNCH.md:116 | `ui/src/wizard/state.test.ts::BAR: a scripted wizard run reaches a working demo in default mode via the quiet demo link, with zero grants`, `ui/src/wizard/state.test.ts::welcome is the first screen, with real visible content, in default mode` |
| 39 | "Operant also keeps score... the tray showing estimated time saved this week" | README.md:92; LAUNCH.md:297 | `crates/recorder/tests/metrics.rs::weekly_rollup_aggregates_multiple_workflows`, `ui/src/tray/state.test.ts::metrics.week.rolled updates the saved-time tooltip and raises a weekly digest notification`, `ui/src/dashboard/state.test.ts::hero line and sparkline render from the fixture metrics: design.md's own '3.2 hours this week' example` |
| 40 | "run it again with one click" | README.md:44; site/index.html:50 | `ui/src/gallery/state.test.ts::a publisher already pinned installs ready to run, not preview-only`, `ui/src/tray/state.test.ts::quick runs rank saved workflows by frecency, highest first, capped at the top three` |
| 41 | "the scheduler is built and tested, but starting a schedule from the app is not wired up yet" (engine proven; app wiring is an honest gap, see Caveats and KNOWN_ISSUES) | README.md:44; site/index.html:50 | `crates/scheduler/src/lib.rs::file_watch_event_produces_replay_run_with_path_input`, `crates/scheduler/src/lib.rs::unattended_rejects_non_replay_mode`, `crates/scheduler/src/trigger.rs::cron_valid_expression`, `ui/src/scheduler/commands.test.ts::upsert_trigger answers not_implemented: no trigger store is wired, so no schedule is created` |
| 42 | "the engine can automate a live desktop... reliably and model-free (5/5)" (live-desktop proof) | docs/evidence/P0-live-engine.md | `docs/evidence/P0-live-engine.md`, `crates/replay/tests/live_reresolve.rs::wired_run_path_construction_reresolves_not_stale_coords` |
| 43 | "pick which open app it should run in... a target-app picker so a taught task binds to the app you mean and not to Operant" (ADR 0003) | README.md:21,250; site/index.html:42,139 | `ui/src/palette/targetApp.test.ts::the default selection is the front-app row, which resolves to the topmost window (windows[0])`, `ui/src/palette/targetApp.test.ts::confirm() with the default selection hands back the goal and the topmost window's process, and closes`, `cli/src/commands/serve.rs::list_windows_returns_a_windows_array`, `docs/adr/0003-target-app-selection.md` |
| 44 | "a live readout of the real model-call count, read from a measured counter, so replay's zero is a fact it can show you, not a label painted on" (MODEL CALLS readout; nonzero on explore proves it is measured) | README.md:250; site/index.html:139 | `ui/src/runViewer/instrumentReadout.test.ts::an explore run that made 3 model calls displays MODEL CALLS 3 (read from the event, not a constant)`, `ui/src/runViewer/instrumentReadout.test.ts::a replay run that made 0 model calls displays MODEL CALLS 0 (the honest zero, proven measured by the 3-vs-0 pair)`, `crates/orchestrator/tests/explore_loop.rs::explore_run_reports_a_real_nonzero_model_call_count_equal_to_rounds`, `crates/core/src/bus/events.rs::run_completed_model_calls_is_visible_and_additive` |
| 45 | "The app you download builds a real backend straight from your config; the scripted mock is a test fixture, never the execution path that ships" | README.md:246; site/index.html:137 | `cli/src/commands/serve.rs::build_planner_with_a_configured_provider_is_not_the_mock`, `crates/orchestrator/src/backends/quirks.rs::table_covers_every_provider_docs_specs_backends_promises`, `crates/orchestrator/tests/backend_contract.rs::anthropic_dialect_round_trips_through_complete` |
| 46 | "a full-screen overlay drops over the desktop inside that same budget: pre-built and hidden, so revealing it is a single toggle that never waits on anything being constructed" (kill-switch overlay) | README.md:79; site/index.html:95 | `ui/src/safety/killOverlay.test.ts::mounts pre-built but hidden: the panel exists in the DOM while the backdrop is still hidden`, `ui/src/safety/killOverlay.test.ts::the panic path reveals it by toggling the pre-mounted element's hidden attribute only (no construction)`, `ui/src/safety/killOverlay.accessibility.test.ts::the revealed kill-switch overlay has no axe violations` |
| 47 | "The flight recorder shifts material with state, warm and alive while a model explores, still and sharp on model-free replay: honest look-and-feel, with fallbacks for reduced transparency and reduced motion" (GLASS.md; look-and-feel, not a capability claim) | README.md:250; site/index.html:139 | `ui/src/runViewer/glassMaterial.test.ts::explore and replay are PROVABLY different materials by getComputedStyle, not by eye (GLASS.md GL2 bar)`, `ui/src/runViewer/glassMaterial.test.ts::explore renders the LIVE material: op-glass--live, and the amber glow is present in the computed box-shadow`, `ui/src/styles/base.css`, `GLASS.md` |

## Unbacked - must delete or back before shipping

None. Every published capability claim is backed in the section above. Do not
fabricate a citation to keep this section empty.

This section is the gate's landing point. If a future claim cannot be backed,
record it here (as a table row or a bullet) and `just claims` turns red until
the copy is cut or the test that backs it lands. One count once lived here: the
copy read "17 named options", which matched no counted source (the quirk table
asserts exactly 16 providers and the OAuth layer adds 2 subscription sign-ins,
so 18 in total, never 17). The M2 copy pass dropped the number and kept the
backed categorical claim (row 34).

## Caveats on otherwise-backed claims

These claims are backed and stay in `## Backed claims`, but carry a nuance the
copy team should know before the M2 copy pass. The gate does not fail on these;
they are recorded here for honesty, not as red flags.

- **Teach path is describe-it, not record-by-doing (row 1).** The shipped teach
  path is model-driven: you describe a task in plain language, pick which open app
  it targets (row 43), and a model drives a live explore run that figures it out on
  your desktop; you then save it. The recorder that watches you perform a task by
  hand does NOT exist; it is a labeled roadmap item
  (`docs/roadmap/demonstration-capture.md`, marked "NOT BUILT"). The F2 copy pass
  rewrote every "watch you do it once" / "show it by demonstration" line to the
  describe-it truth. See the Deleted section below.
- **Scheduling wiring (row 41).** The scheduler engine is genuinely tested
  (cron, file-watch, unattended replay-only, scope parallelism in
  `crates/scheduler`). But the shell's trigger commands currently answer
  `not_implemented`: see `ui/src/scheduler/commands.test.ts` ("upsert_trigger
  answers not_implemented") and the dashboard's honest "scheduling unavailable"
  path (`ui/src/dashboard/state.test.ts`). So "set a schedule from the app"
  end to end is not wired yet, and row 41's published copy now says exactly that.
  "Run it again with one click" (row 40) is fully wired; the schedule half is
  engine-complete but not surfaced.
- **Undo demo asset (rows 9 to 12).** The undo capability is backed by tests,
  but the README alt text for `assets/10-undo.gif` still says "Placeholder, not
  a real capture" and "this screen does not exist in ui/src yet". That alt text
  is stale: the undo view is tested in `ui/src/undo/view.test.ts`. The claim is
  fine; the capture asset and its caption are the TODO.
- **Voice is a capability, not a teach affordance (row 2).** Local speech in and
  out is backed by `sidecars/voice/test/roundtrip.test.js` (STT -> intent -> TTS)
  and stays as the Voice architecture bullet. A full end-to-end "workflow taught
  entirely by voice" is not separately tested, so the F2 pass removed "by voice"
  from every teach headline; only the narrow "local speech in and out" (row 2)
  remains, and it is fully backed.

## Deleted or softened in the F2 copy-truth pass

Recorded per the header's "an unbacked claim is DELETED, never softened" rule (and
its honest converse: an overstated-but-real claim is softened to the truth).

- **Deleted: "by demonstration" / "watch you do it once" / "record me" as a
  present-tense teach claim.** No recorder captures a hand-performed demonstration
  today (`docs/roadmap/demonstration-capture.md` is marked NOT BUILT). Every such
  line in README, site, and LAUNCH was rewritten to the model-driven describe-it
  path.
- **Deleted: "by voice" as a teach affordance.** A voice-taught workflow is not
  separately proven. Kept only "local speech in and out" (row 2, backed).
- **Softened: "Put it on a schedule and it just happens without you."** The
  scheduler engine is tested but not wired to the app (row 41 caveat). Copy now
  says the scheduler is built and tested but starting a schedule from the app is
  not wired yet.
- **Softened: `operant install` "fetches a workflow manifest from a git-backed
  index."** Install resolves a manifest from a LOCAL registry index checkout
  (`cli/src/commands/install.rs`); over-the-wire fetch is not wired. Copy now says
  "reads... from a git-backed index" and flags the local-checkout gap. The signing,
  verification, and dry-run-only refusal claims (row 31) are untouched and backed.
