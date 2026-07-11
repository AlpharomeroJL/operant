# BENCHMARKS

| Task | replay | reinfer_mock |
|---|---|---|
|drift_repaired | 5/5 | p50: 0ms, p95: 0ms | calls: 0, tokens: 0 | 5/5 | p50: 6ms, p95: 6ms | calls: 15, tokens: 2700 |
|notepad | 5/5 | p50: 1ms, p95: 1ms | calls: 0, tokens: 0 | 5/5 | p50: 7ms, p95: 7ms | calls: 25, tokens: 4500 |
|web | 5/5 | p50: 0ms, p95: 0ms | calls: 0, tokens: 0 | 5/5 | p50: 6ms, p95: 6ms | calls: 25, tokens: 4500 |

## Methods

Measurements capture per-step latency, total wall time, model calls, and token usage.

**Honesty note:** reinfer_mock uses recorded latencies from the actual replay,
simulating agent-at-every-step cost without hitting a real backend.

## Cookbook workflows referenced

Cookbook workflows that feed crates/bench (see docs/specs/bench.md, 'plus three cookbook workflows'). Generated for L9B (bench-suite) to consume without needing to run Node or parse comments. Source of truth cross-checked by cookbook/doctest.mjs against each workflow module's `benchmark` export and the prose file's Benchmark tag.

| Slug | Workflow | Prose |
|---|---|---|
| copy-invoice-rows-web-to-spreadsheet | cookbook/copy-invoice-rows-web-to-spreadsheet/workflow.ts | cookbook/copy-invoice-rows-web-to-spreadsheet.md |
| rename-file-pdfs-by-date | cookbook/rename-file-pdfs-by-date/workflow.ts | cookbook/rename-file-pdfs-by-date.md |
| extract-text-from-images | cookbook/extract-text-from-images/workflow.ts | cookbook/extract-text-from-images.md |
