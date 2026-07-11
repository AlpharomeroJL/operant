# Contributing to Operant

Thank you for your interest in contributing to Operant, an open-source, local-first agentic desktop assistant. This guide explains how to build, test, and submit changes.

## Getting started

Operant is a Rust workspace with the following structure:
- `crates/` - core Rust components
- `cli/` - CLI binaries
- `contracts/` - versioned schemas, fixtures, and glossaries
- `cookbook/` - example workflows
- `docs/` - product documentation
- `campaign/` - build orchestration (see `RESUME.md`)

For the product vision and architecture, start with:
- `docs/PRD.md` - full product specification and requirements
- `docs/ARCHITECTURE.md` - component breakdown, data model, trade-offs
- `campaign/RESUME.md` - how to continue multi-session work safely

## Building and testing

The merge gate is `just ci`, which runs:

```powershell
just build
just test
just check-json          # Contract schemas and fixtures
just check-emdash        # No em-dashes (U+2014) in tracked text
just check-microcopy     # Default-mode strings don't use internal jargon
just check-airgap        # Default config makes zero unexpected network calls
```

To build locally:
```powershell
cargo build --workspace
cargo test --workspace
just ci                   # Full gate before you push
```

All commits must pass `just ci`. The CI check runs against every PR.

## The contract-first rule

Operant speaks a versioned, typed vocabulary. Everything that crosses a component boundary lives in `contracts/`:
- `bus_events.md` - typed pub/sub topics and payloads (append-only)
- `*.schema.json` - contract schemas for perception snapshots, trajectories, workflows
- `microcopy_glossary.json` - internal terms mapped to user-facing language
- `fixtures/` - sample data and test cases that win over generated output

**Append-only invariant**: once a contract exists, only new optional fields may be added. Nothing is renamed or removed. This lets lanes work in parallel without merge conflicts. When you touch a contract, coordinate with the campaign lead.

**Fixtures win**: test output must match versioned fixtures exactly. If you change behavior that affects a fixture, regenerate with `just fixtures` and commit both the code change and the updated fixture.

## Style: no em-dashes

The linter forbids em-dashes (U+2014). Use hyphens or colons instead:
- ~~"The model runs--which is fast"~~ -> "The model runs: it is fast" or "The model runs - it is fast"
- ~~"Teach once--compile forever"~~ -> "Teach once: compile forever"

The check is in `just check-emdash` and runs in CI.

## Microcopy and the zero-code design bar

Default-mode UI strings must never contain internal terms from `contracts/microcopy_glossary.json`. The linter checks this in `just check-microcopy` and fails CI if any internal term appears in a default-mode string catalog.

Internal terms like "trajectory", "compile", "grounding", "DSL", "manifest", "MCP", "invariant", "gate", "replay", "explore", "drift", "sidecar", "backend", "inference", "token", "VRAM", etc. must be mapped to user-facing equivalents (see the glossary).

**Why**: Operant's non-coder persona is the design bar. Default mode is zero-jargon. Advanced mode is exempt. Copy is reviewed under this lens.

## Commits and pull requests

Use conventional commits:
```
feat: add kill switch hotkey
fix: handle vision sidecar crash
test: add redaction fixture for credential forms
docs: update drift repair walkthrough
```

Every commit is authored `Josef Long <Josefdean@protonmail.com>` with no AI attribution or "Co-Authored-By" lines.

When you submit a PR:
1. Title is short (under 70 characters)
2. All commits pass `just ci` green
3. No em-dashes anywhere in your text
4. Contracts remain append-only (no renames or removals)
5. Fixtures are regenerated if behavior changed
6. Tests are added (or marked as TODO with a brief reason)
7. Default-mode microcopy is clean (no internal terms)

The PR description can reference relevant sections of PRD.md or ARCHITECTURE.md.

## Contributing as a lane

Operant uses `git worktree` lanes for parallel contribution. The campaign orchestration (`campaign/MEGA_PROMPT.md` and `campaign/frontier.mjs`) dispatches work into isolated lanes on dedicated branches. Each lane:
1. Branches from `main` on `lane/<id>`
2. Works in a worktree at `lanes/<id>/`
3. Gates with `just gate <id>` before merge
4. Merges back to `main` only when green
5. Marks completion with `campaign/merged/<id>.ok`

To understand this flow, see `campaign/RESUME.md`. Lanes make parallel work safe because `main` is always buildable.

## Reporting issues and questions

Use GitHub Discussions for:
- Questions about how to use Operant
- Feature brainstorms
- Design feedback
- Build or platform issues

Use Issues for:
- Confirmed bugs with reproduction steps
- Request for `operant doctor` output
- Security concerns (see SECURITY.md for responsible disclosure)

## Code review expectations

- Changes are reviewed for correctness, safety, and alignment with the PRD and ARCHITECTURE
- Guardian set features (kill switch, undo, redaction) and invariant gates are regression-tested forever
- The zero-code experience is the design bar; deviations are justified in the PR
- Determinism in replay is validated (no model calls in compiled workflows, tested in CI)

## Help wanted

Good entry points for first contributors:
- Expanding the cookbook with example workflows
- Documentation improvements and examples
- Accessibility audits (the product reads and controls screens; screen-reader support is first-class)
- Test coverage for perception snapshots and fixture workflows
- Platform-specific debugging (Windows UIA edge cases)

Look for issues marked `good-first-issue` or reach out in Discussions.

## Questions?

- Product: `docs/PRD.md`, `docs/ARCHITECTURE.md`
- Build: `justfile`, `scripts/`
- Campaign orchestration: `campaign/RESUME.md`, `campaign/MEGA_PROMPT.md`
- Open a Discussion or comment on an issue

Thank you for building with us.
