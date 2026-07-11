# Operant campaign command surface.
# `just ci` is the merge gate. Lanes run `just gate <id>`; the orchestrator runs
# `just gate <id>` itself before merging (trust nothing a command can check).

set windows-shell := ["powershell.exe", "-NoProfile", "-Command"]

# Shared cargo target so 15 lanes do not each rebuild every dependency.
export CARGO_TARGET_DIR := env_var_or_default("CARGO_TARGET_DIR", "D:/dev/operant-target")

default:
    @just --list

# Full CI: the machine-enforced gate. Green here means mergeable.
ci: build test check-json check-emdash check-microcopy check-airgap
    @echo "CI GREEN"

build:
    cargo build --workspace

test:
    cargo test --workspace

# Every contract schema and fixture must be valid JSON of the right shape.
check-json:
    node scripts/check_json.mjs

# Style, machine-enforced: no em dashes anywhere in tracked text.
check-emdash:
    node scripts/check_emdash.mjs

# Default-mode UI strings must not contain glossary internal terms.
check-microcopy:
    node scripts/microcopy_lint.mjs

# Default configuration must make zero unexpected network calls (L6B hardens this).
check-airgap:
    node scripts/check_airgap.mjs

# Markdown lint (best-effort; requires markdownlint-cli via npx, skipped offline).
check-markdown:
    -npx --no-install markdownlint-cli "**/*.md" --ignore node_modules --ignore target

# Create an isolated worktree for a lane. Orchestrator calls this before dispatch.
lane id:
    git worktree add lanes/{{id}} -b lane/{{id}} main

# Run a lane's gate. Delegates to the lane's own bar script if present, else full CI.
gate id:
    @if (Test-Path "scratch/lanes/{{id}}/gate.ps1") { powershell -NoProfile -File "scratch/lanes/{{id}}/gate.ps1" } else { just ci }

# Golden path: the standalone e2e crate proving explore -> compile -> replay with zero model calls.
golden:
    cd e2e/golden-path; cargo test

# Regenerate signed/binary fixtures (deterministic; keypair guarded).
fixtures:
    cd contracts/fixtures; node generate.mjs

fmt:
    cargo fmt --all

clippy:
    cargo clippy --workspace --all-targets
