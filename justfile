# Operant command surface.
# `just verify` is the full local gate. There is no hosted CI: this machine is
# the only gate, so `just verify` must be green before every push. Run
# `just setup` once to install the pre-push hook that enforces it. `just ci` is
# the core build, test, and lint subset that `verify` builds on.

set windows-shell := ["powershell.exe", "-NoProfile", "-Command"]

# Shared cargo target so 15 lanes do not each rebuild every dependency.
export CARGO_TARGET_DIR := env_var_or_default("CARGO_TARGET_DIR", "D:/dev/operant-target")

default:
    @just --list

# Core gate: workspace build, tests, and the content linters. `just verify` is
# the full gate that adds the determinism proof and the UI suite on top.
ci: build test check-json check-emdash check-microcopy check-airgap check-rawhex
    @echo "CI GREEN"

# The full local gate. There is no hosted CI, so this must be green before every
# push; the pre-push hook installed by `just setup` enforces it. As new checks
# land (raw-hex token lint, visual-regression diff, benchmark-regression
# threshold) they are added to this recipe's dependency list.
verify: ci golden ui
    @echo "VERIFY GREEN"

# One-time developer setup: point git at the committed hooks so the pre-push hook
# runs `just verify` and blocks a push that is not green, then check the toolchain.
setup:
    git config core.hooksPath hooks
    @echo "pre-push hook installed via core.hooksPath = hooks"
    just --version
    cargo --version
    node --version
    @echo "setup OK: run 'just verify' before pushing"

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

# No raw hex color literal outside ui/src/theme/tokens.ts (the single source
# of truth, docs/specs/design.md section 2) and its generated ui/src/styles/tokens.css.
check-rawhex:
    node scripts/check_rawhex.mjs

# Markdown lint (best-effort; requires markdownlint-cli via npx, skipped offline).
check-markdown:
    -npx --no-install markdownlint-cli "**/*.md" --ignore node_modules --ignore target

# Golden path: the standalone e2e crate proving explore -> compile -> replay with zero model calls.
golden:
    cd e2e/golden-path; cargo test

# UI gate: regenerate ui/src/styles/tokens.css from ui/src/theme/tokens.ts (the
# single source of truth, docs/specs/design.md section 2) so CSS and TS can
# never drift, then TypeScript typecheck (tsc, since node --test only strips
# types) plus the test suite via `npm test` (which sets up the jsdom DOM env
# through testHooks). Run `cd ui; npm install` once first (pulls jsdom +
# axe-core for the a11y tests). `npm test`/`npm run dev`/`npm run build` also
# regenerate tokens.css on their own (package.json's pretest/predev/prebuild),
# so this is belt-and-suspenders for a bare `npm run typecheck`.
ui:
    cd ui; npm run build:tokens; npm run typecheck; npm test

# Regenerate signed/binary fixtures (deterministic; keypair guarded).
fixtures:
    cd contracts/fixtures; node generate.mjs

fmt:
    cargo fmt --all

clippy:
    cargo clippy --workspace --all-targets

# Authenticode-sign the built installer if a signing certificate is configured
# (OPERANT_SIGN_THUMBPRINT, or OPERANT_SIGN_PFX + OPERANT_SIGN_PFX_PASSWORD).
# No certificate configured, or signtool.exe not found: skips cleanly, exit 0.
# See docs/signing.md for how to obtain and configure a certificate.
sign *args:
    powershell -NoProfile -ExecutionPolicy Bypass -File "{{justfile_directory()}}/scripts/sign.ps1" {{args}}

# Build the NSIS installer end to end (frontend, Tauri shell, installer, and
# updater artifacts; the scripted form of release/REPRODUCIBLE.md's
# "one-command rebuild"), then sign it. Signing skips cleanly when no
# certificate is configured, so this recipe is safe to run either way.
package:
    cd ui; npm ci; cd src-tauri; cargo tauri build -b nsis --ci
    just sign
