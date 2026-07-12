# ADR-0001: Operant v1.0.0 build-campaign outcome

Status: Accepted
Date: 2026-07-11

## Context

Operant v1.0.0 was built as a single autonomous, checkpointed build campaign
(`campaign/MEGA_PROMPT.md`): 64 packets across four waves plus a backlog, dispatched
to tiered subagents in isolated git worktrees, gated against frozen contracts
(`contracts/`), and merged to a `main` that only ever advances on a green `just ci`.
This ADR records what shipped, what was verified against real systems versus fixtures,
and what is parked, per the campaign's own honesty rules.

## Outcome: shipped

All 64 packets merged. `main` is green on `just ci` (workspace build, all tests,
schema validation, em-dash grep, microcopy lint, air-gap check), on `just golden`
(the determinism proof), and on `just ui` (typecheck plus 205 UI tests). The launch
cut in PRD section 9 shipped in full:

- Perception: Windows UIA (windows-rs, behind the `real-uia` feature) plus an
  always-built fixture perceiver; shared resolve/diff/BLAKE3-digest core.
- Action: input synthesis plus filesystem, email (IMAP/SMTP), OCR/PDF, Office COM,
  browser (CDP) adapters; risk classes and an approval-gated destructive path.
- Recorder: SQLite WAL, content-addressed blobs, refcounted GC, undo journal, backup,
  anchor redaction.
- Safety: capability grants, dry-run, hash-chained audit with JSONL and PDF export,
  runtime-enforced FR-S4 credential and payment invariants no workflow can disable.
- The moat: trajectory compiler (five passes), deterministic model-free replay
  (backend-free by crate graph, CI-asserted zero network), and the full drift-repair
  loop (detect, re-ground, patch, approve, versioned merge).
- Model backends: one OpenAI-compatible client with a 16-provider quirk table plus
  native Anthropic and Gemini dialects, a hermetic mock transport, capability probe,
  secrets redaction, and the OAuth broker (PKCE loopback for ChatGPT and Claude plans,
  tokens in the OS vault only).
- Orchestration: the explore loop with HITL, scheduler (compiled-only unattended),
  CLI (`compile`/`run`/`dry-run`/`list`/`doctor`/`explain`), and MCP both directions.
- Zero-code layer: onboarding wizard (media-presence checked), plain-English renderer
  (total over the Action IR), demo mode, template gallery, doctor plus diagnostics
  bundle, microcopy glossary lint, first-run tour, i18n scaffold with a Spanish locale.
- Guardian set: sub-100ms kill switch, undo journal, anchor redaction.
- Insight set: time-saved ledger, opt-in watch-and-suggest (off by default).
- Proof and distribution: benchmark harness with a published `BENCHMARKS.md` and a CI
  regression threshold; registry client (Ed25519 verify, unsigned dry-run-only) plus a
  signed index repo; signed NSIS installer with an Ed25519 updater keypair, SBOM, and a
  reproducible-build doc; docs site deployed to GitHub Pages from the `gh-pages` branch.

## Verification: real versus fixture

- The determinism thesis is proven end to end by `e2e/golden-path`: a mock-model
  explore run compiles to a workflow that replays reproducing the exact action sequence
  with zero model calls, plus a structural test that the replay crate links no backend.
- The first-timer path (NFR-7, a release blocker) was verified GREEN against the REAL
  installed NSIS binary, driven end to end via tauri-driver (V5), twice, well under the
  15-minute budget, followed by a verified clean uninstall. This exceeds the fixture-only
  floor the campaign plan allowed for.
- Perception, vision, and voice run in fixture or mock mode in CI (deterministic, no GPU),
  as the specs require; the real Windows UIA backend compiles and is unit-tested but was
  not exercised against a live window headlessly.
- 12 of the 13 launch assets are real captures of the running UI (driven through the live
  Vite frontend). Asset 10 (undo) is a labeled placeholder because no dedicated undo
  screen exists in the UI yet; the undo journal backend and irreversible-action labeling
  are real and tested.

## Drift from ARCHITECTURE.md and the honest ceiling

- Signing is Ed25519 updater-signature only. There is no Authenticode certificate on the
  build machine, so the installer is not OS-code-signed and Windows SmartScreen shows an
  "unknown publisher" warning on first run. Stated plainly in `release/KEYS.md` and the
  `release/RELEASE_NOTES_TEMPLATE.md` shown to downloaders.
- The narrated demo video is assembled from the real captured assets but is silent or
  captioned: the voice sidecar ships a mock TTS provider, so a real spoken track requires
  wiring the real Kokoro voice model (documented, not faked).

## Parked (known issues, with fix direction)

- KI-1: the explore loop records the proposed action without backfilling the
  perception-resolved point, and replay leans on `coords_last_known` rather than
  re-resolving the selector chain against a live perceiver at run time. Fixture and mock
  paths are green; a real planner using selectors alone needs replay to re-resolve via a
  `Perceiver` (which is not a model backend, so replay stays model-free) for live Windows.
- KI-2: the explore loop records `human_correction.at_seq` while the compiler collapses on
  `human_correction.supersedes_seq`, so a live HITL redirect will not collapse the way the
  hand-authored fixture does. Reconcile the field name across the loop and the compiler.
- `tauri-plugin-updater` is configured but not yet registered in `ui/src-tauri/Cargo.toml`
  and `main.rs`; the updater config is inert until wired.
- The installer's non-elevated `/CURRENTUSER` reinstall still triggers one UAC prompt,
  which cannot be answered non-interactively; the uninstaller does not have this issue.
- The UI accessibility tests require `npm install` in `ui/` (jsdom and axe-core) before
  `just ui` passes; `node_modules` is correctly gitignored.

## Fix-at-gate log

- FG-001 (ir): removed `Eq` from `Anchor`/`Target` (contain f64).
- FG-002 (ci): defined the em-dash sentinels by code point so the checker does not match itself.
- FG-003 (S1A): relocated a stray root `package.json` into `e2e/soak/`.
- FG-004 (contract): documented the `config.changed` and `voice.intent` topics in `bus_events.md`.
- FG-005 (build): shared `CARGO_TARGET_DIR` across worktrees can reuse a cached test binary
  whose baked-in fixture path points at a removed worktree; workaround is `cargo clean -p`
  of the fixture-path crates before each gate.
- FG-006 (ui): made `tsc --noEmit` clean (ActionIR to RenderableStep cast; unused tour
  symbols; a vestigial locales assertion) and added the `just ui` gate.
- FG-007 (release): the uninstaller now clears the real identifier-scoped data dirs
  (`dev.operant.shell`) rather than a nonexistent `Operant` folder.
- FG-008 (ui gate): `just ui` runs `npm test` (jsdom DOM setup via testHooks), not bare
  `node --test`.

## Campaign mechanics and the checkpoint protocol

Durable state lived in git: `campaign/state.json`, per-packet `campaign/merged/<id>.ok`
markers, and `campaign/RESUME.md`. The protocol was validated in practice: the scheduled
fallback resumed from pushed state and autonomously merged several lanes, and a benign
concurrent double-dispatch of L14A self-healed via the idempotent markers (main stayed
coherent; the correctly-merged copy won). CI is on-device only, with no GitHub Actions,
per the owner's directive.

## Repositories

- Product: https://github.com/AlpharomeroJL/operant
- Registry: https://github.com/AlpharomeroJL/operant-registry
- Docs: https://alpharomerojl.github.io/operant/
