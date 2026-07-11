# Soak Test Runner

Reliability testing harness for Operant. Drives a fixture workflow on configurable intervals and samples memory (RSS) and handle counts to detect leaks and stability issues.

## Quick Start

### CI Mode (5-10 seconds, for CI pipelines)

```bash
npm run soak:ci
# or
node soak.mjs --ci
```

Runs 5 ticks at 2-second intervals (10 seconds total). Samples memory and handles; produces a report with PASS/FAIL verdict based on flat memory trend and zero errors.

### Release Gate (30 minutes, for release validation)

```bash
npm run soak:release
# or
node soak.mjs --minutes 30
```

Runs a full 30-minute soak as part of the release gate. Validates long-running stability under repeated workflow execution.

### Other Modes

**Dry run** (validation, quick):
```bash
npm run soak:dry
# or
node soak.mjs --dry
```

**Full soak** (30 minutes, default):
```bash
npm run soak:full
# or
node soak.mjs
```

**Custom duration**:
```bash
node soak.mjs --minutes 5
```

## CLI Flags

- `--ci`: Fast mode for CI (10 seconds, 5 ticks at 2s interval)
- `--dry`: Dry run mode (10 seconds, validation)
- `--minutes N`: Run for N minutes (e.g., `--minutes 30` for release gate)

## Report Output

Each run produces two files in `e2e/soak/`:
- `soak-report-{timestamp}.md` - Human-readable markdown report
- `soak-report-{timestamp}.json` - Machine-readable JSON with detailed metrics

### Report Structure

**Verdict**: PASS or FAIL
- PASS: Memory is flat (slope < 100 MB/tick) and zero errors
- FAIL: Memory is growing or errors detected

**Key Metrics**:
- Run count: Number of workflow invocations
- Memory trend: Slope (bytes/tick) and fit quality (R-squared)
- Average/Max/Min RSS in MB
- Handle count samples
- Error details if any

## Integration with Release Process

The 30-minute soak is a release blocker:

```bash
npm run soak:release
```

The run must complete with a PASS verdict (flat memory + zero silent failures) before shipping. The JSON report is machine-parseable for automation.

## Workflow Customization

The fixture workflow is defined in `workflow.mjs`. Modify `simulateWorkflow()` to test your target scenario.

## Memory Analysis

The runner uses linear regression to determine if memory is stable:

```
slope = (memory change) / (number of ticks)
slopePerHour = extrapolated MB/hour if trend continues

PASS if: slope < 100 MB/tick
FAIL if: slope >= 100 MB/tick or errors detected
```

The R-squared value (0-1) indicates trend strength; higher values mean a stronger linear trend (positive or negative).
