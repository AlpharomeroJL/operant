# Fixtures

The only shared surface between lanes. Code vs fixture disagreement: the fixture wins until an ADR says otherwise. Additions are append-only; a lane may add new fixture files under a subdirectory it owns, never edit another lane's fixtures.

| Fixture | Path | Consumed by |
|---|---|---|
| Notepad snapshot | `snapshot_notepad.json` | perception, action, recorder |
| Sample trajectory | `trajectory_notepad.json` | recorder, compiler, bench |
| Compiled workflow | `workflow_notepad/` (manifest.json + workflow.ts) | compiler (expected output shape), replay, renderer, registry, scheduler |
| Gate set | `gates_basic.json` | gates engine, compiler pass 4 |
| Drift fixture | `drift_renamed_button/` (before.json, after.json) | drift repair, bench |
| Fixture web app | `webapp/` (index.html, drift.html) | browser adapter, e2e, demo mode, capture |
| Credential form | `credential_form/index.html` | safety (FR-S4), anchor redaction (X4) |
| IMAP dump | `imap/*.eml` | email adapter, email trigger |
| Model download endpoint | `model_download/` (model.bin + SHA256SUMS) | wizard downloader |
| Fixture PDF and image | `docs/sample.pdf`, `docs/sample.png` | OCR/PDF adapter |
| Registry manifest + keypair | `registry/` | registry verify/install, registry-index repo |
| OAuth server config | `oauth/` (config.json; mock server implemented by X16) | oauth broker |

## Regeneration

Binary and signed fixtures are produced by `generate.mjs` (deterministic: same inputs, same bytes, except the Ed25519 keypair which is generated once and committed as an intentional TEST key, never used for anything real):

```
cd contracts/fixtures
npm install
node generate.mjs
```

`docs/sample.png` is rendered by `render_png.ps1` (System.Drawing text render, OCR-able).

## Synthetic digests

Trajectory and snapshot fixtures use recognizable synthetic BLAKE3-shaped digests (`d0d0...`, `d1d1...`) where only internal consistency matters. Real digests appear wherever a consumer verifies bytes (dsl.hash, model_download checksums).
