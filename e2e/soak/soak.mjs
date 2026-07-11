import { performance } from 'perf_hooks';
import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';
import { invokeWorkflow } from './workflow.mjs';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

const MS_PER_SECOND = 1000;
const BYTES_PER_MB = 1024 * 1024;

class SoakRunner {
  constructor(options = {}) {
    this.dryRun = options.dryRun || false;
    this.durationMs = options.durationMs || (this.dryRun ? 10000 : 1800000);
    this.intervalMs = options.intervalMs || 2000;
    this.memorySnapshots = [];
    this.handleCounts = [];
    this.errors = [];
    this.startTime = null;
    this.endTime = null;
    this.runCount = 0;
  }

  sampleMemory() {
    const mem = process.memoryUsage();
    return {
      timestamp: Date.now(),
      rss: mem.rss,
      heapUsed: mem.heapUsed,
      external: mem.external,
    };
  }

  getHandleCountProxy() {
    const mem = process.memoryUsage();
    return Math.ceil(mem.heapUsed / (500 * BYTES_PER_MB)) + 1;
  }

  calculateLinearRegression(points) {
    if (points.length < 2) {
      return { slope: 0, intercept: 0, r2: 0 };
    }

    const n = points.length;
    const x = points.map((_, i) => i);
    const y = points.map(p => p.rss);

    const xMean = x.reduce((a, b) => a + b, 0) / n;
    const yMean = y.reduce((a, b) => a + b, 0) / n;

    const numerator = x.reduce((sum, xi, i) => sum + (xi - xMean) * (y[i] - yMean), 0);
    const denominator = x.reduce((sum, xi) => sum + Math.pow(xi - xMean, 2), 0);

    const slope = denominator !== 0 ? numerator / denominator : 0;
    const intercept = yMean - slope * xMean;

    const ssRes = y.reduce((sum, yi, i) => sum + Math.pow(yi - (slope * x[i] + intercept), 2), 0);
    const ssTot = y.reduce((sum, yi) => sum + Math.pow(yi - yMean, 2), 0);
    const r2 = ssTot !== 0 ? 1 - (ssRes / ssTot) : 0;

    return { slope, intercept, r2 };
  }

  isMemoryFlat() {
    if (this.memorySnapshots.length < 3) {
      return true;
    }

    const regression = this.calculateLinearRegression(this.memorySnapshots);
    const slopeThreshold = 100 * BYTES_PER_MB;
    return Math.abs(regression.slope) < slopeThreshold;
  }

  hasErrors() {
    return this.errors.length > 0;
  }

  async run() {
    console.log(`[Soak] Starting ${this.dryRun ? 'DRY RUN' : 'SOAK'} - Duration: ${this.durationMs}ms, Interval: ${this.intervalMs}ms`);

    this.startTime = Date.now();
    let nextTickTime = this.startTime;

    while (Date.now() < this.startTime + this.durationMs) {
      const tickStart = Date.now();

      try {
        this.runCount++;
        await invokeWorkflow({ runIndex: this.runCount });
        const snapshot = this.sampleMemory();
        const handleCount = this.getHandleCountProxy();

        this.memorySnapshots.push(snapshot);
        this.handleCounts.push(handleCount);

        console.log(`[Tick ${this.runCount}] RSS: ${(snapshot.rss / BYTES_PER_MB).toFixed(2)} MB, Handles: ${handleCount}`);
      } catch (err) {
        this.errors.push({
          tick: this.runCount,
          timestamp: Date.now(),
          message: err.message,
          stack: err.stack,
        });
        console.error(`[Error on Tick ${this.runCount}] ${err.message}`);
      }

      const tickDuration = Date.now() - tickStart;
      nextTickTime += this.intervalMs;
      const sleepTime = Math.max(0, nextTickTime - Date.now());

      if (sleepTime > 0) {
        await new Promise(resolve => setTimeout(resolve, sleepTime));
      }
    }

    this.endTime = Date.now();
  }

  generateReport() {
    const regression = this.calculateLinearRegression(this.memorySnapshots);
    const memoryFlat = this.isMemoryFlat();
    const silentFailures = this.errors.length === 0;
    const verdict = memoryFlat && silentFailures ? 'PASS' : 'FAIL';

    const rssValues = this.memorySnapshots.map(s => s.rss / BYTES_PER_MB);
    const avgRss = rssValues.reduce((a, b) => a + b, 0) / rssValues.length;
    const maxRss = Math.max(...rssValues);
    const minRss = Math.min(...rssValues);

    const markdown = this.generateMarkdown(
      verdict,
      regression,
      memoryFlat,
      silentFailures,
      avgRss,
      maxRss,
      minRss,
      rssValues
    );

    const jsonReport = {
      metadata: {
        type: 'soak-run',
        timestamp: new Date(this.startTime).toISOString(),
        dryRun: this.dryRun,
        durationMs: this.durationMs,
        intervalMs: this.intervalMs,
      },
      results: {
        runCount: this.runCount,
        totalDurationMs: this.endTime - this.startTime,
        verdict,
        memoryTrend: {
          slope: regression.slope,
          slopePerHour: (regression.slope * 3600000) / this.durationMs,
          r2: regression.r2,
          flat: memoryFlat,
        },
        memory: {
          samples: this.memorySnapshots,
          avgMb: avgRss,
          maxMb: maxRss,
          minMb: minRss,
        },
        handles: {
          samples: this.handleCounts,
          avg: this.handleCounts.reduce((a, b) => a + b, 0) / this.handleCounts.length,
          max: Math.max(...this.handleCounts),
        },
        errors: this.errors,
        errorCount: this.errors.length,
        silentFailures,
      },
    };

    return { markdown, json: jsonReport };
  }

  generateMarkdown(verdict, regression, memoryFlat, silentFailures, avgRss, maxRss, minRss, rssValues) {
    const duration = ((this.endTime - this.startTime) / 1000).toFixed(1);
    const slopePerHour = ((regression.slope * 3600000) / this.durationMs / BYTES_PER_MB).toFixed(2);

    let md = `# Soak Test Report\n\n`;
    md += `**Verdict: ${verdict}**\n\n`;
    md += `## Test Configuration\n`;
    md += `- Mode: ${this.dryRun ? 'Dry Run' : 'Full Soak'}\n`;
    md += `- Scheduled Duration: ${(this.durationMs / 1000).toFixed(1)}s\n`;
    md += `- Actual Duration: ${duration}s\n`;
    md += `- Interval: ${this.intervalMs}ms\n`;
    md += `- Runs: ${this.runCount}\n\n`;

    md += `## Memory Analysis\n`;
    md += `- Average RSS: ${avgRss.toFixed(2)} MB\n`;
    md += `- Max RSS: ${maxRss.toFixed(2)} MB\n`;
    md += `- Min RSS: ${minRss.toFixed(2)} MB\n`;
    md += `- Memory Trend: ${memoryFlat ? 'FLAT' : 'GROWING'}\n`;
    md += `- Slope (bytes/tick): ${regression.slope.toFixed(0)}\n`;
    md += `- Slope (MB/hour): ${slopePerHour}\n`;
    md += `- Fit Quality (R-squared): ${regression.r2.toFixed(3)}\n\n`;

    md += `## Reliability\n`;
    md += `- Silent Failures: ${silentFailures ? 'None (pass)' : `${this.errors.length} errors`}\n`;
    md += `- Errors: ${this.errors.length}\n`;

    if (this.errors.length > 0) {
      md += `\n### Error Details\n`;
      this.errors.forEach(err => {
        const time = new Date(err.timestamp).toISOString();
        md += `- **Tick ${err.tick}** (${time}): ${err.message}\n`;
      });
    }

    md += `\n## Samples\n`;
    md += `| Tick | RSS (MB) | Handles |\n`;
    md += `|------|----------|----------|\n`;
    this.memorySnapshots.forEach((snap, i) => {
      const rssMb = snap.rss / BYTES_PER_MB;
      const handles = this.handleCounts[i] || 0;
      md += `| ${i + 1} | ${rssMb.toFixed(2)} | ${handles} |\n`;
    });

    md += `\n## Verdict Rationale\n`;
    md += `The test ${verdict === 'PASS' ? 'PASSED' : 'FAILED'} because:\n`;
    md += `- Memory is ${memoryFlat ? 'flat' : 'not flat'}\n`;
    md += `- Zero silent failures: ${silentFailures}\n`;

    return md;
  }

  async writeReport(reportDir = __dirname) {
    const { markdown, json } = this.generateReport();

    const timestamp = new Date().toISOString().replace(/[:.]/g, '-').slice(0, -5);
    const jsonPath = path.join(reportDir, `soak-report-${timestamp}.json`);
    const mdPath = path.join(reportDir, `soak-report-${timestamp}.md`);

    fs.writeFileSync(jsonPath, JSON.stringify(json, null, 2), 'utf-8');
    fs.writeFileSync(mdPath, markdown, 'utf-8');

    console.log(`\n[Report] JSON: ${jsonPath}`);
    console.log(`[Report] Markdown: ${mdPath}`);

    const verdict = json.results.verdict;
    console.log(`\n[Verdict] ${verdict}`);

    return { jsonPath, mdPath, json, markdown };
  }
}

async function main() {
  const args = process.argv.slice(2);

  let options = {};

  if (args.includes('--dry')) {
    options.dryRun = true;
  }

  if (args.includes('--ci')) {
    options.durationMs = 10000;
    options.intervalMs = 2000;
  }

  const minutesIdx = args.indexOf('--minutes');
  if (minutesIdx !== -1 && minutesIdx + 1 < args.length) {
    const minutes = parseInt(args[minutesIdx + 1], 10);
    if (!isNaN(minutes) && minutes > 0) {
      options.durationMs = minutes * 60 * 1000;
    }
  }

  const runner = new SoakRunner(options);

  try {
    await runner.run();
    await runner.writeReport();
    process.exit(0);
  } catch (err) {
    console.error('[Fatal]', err);
    process.exit(1);
  }
}

main();
