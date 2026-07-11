// @advanced
// A synthesized preview of the file docs/specs/compiler.md's Pass 5 would
// emit ("a TypeScript file over the SDK: readable, one statement per step,
// comments carrying the plain-English step text"). This lane has no
// compiler and no real DSL bytes to show (contracts/bus_events.md's
// workflow.compiled only ever carries a dsl_path, never file contents); this
// stub gives the Advanced DSL editor pane something honest and readable
// instead of the raw file it has no way to fetch.

import type { MockWorkflowRecord } from "../library/mockRegistry.ts";

export function dslPreview(record: MockWorkflowRecord | undefined): string {
  if (!record || record.manifest.step_summary.length === 0) return "";
  const lines = [`// ${record.manifest.name}@${record.manifest.version}`, ""];
  record.manifest.step_summary.forEach((sentence, i) => {
    lines.push(`// ${sentence}`);
    lines.push(`await step${i + 1}();`, "");
  });
  return lines.join("\n").trimEnd();
}
