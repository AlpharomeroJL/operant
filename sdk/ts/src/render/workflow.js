// Composes the whole plain-English view of a workflow: its grant prose, its
// changeable inputs, and its numbered steps with inline parameter chips. This
// is what the UI mounts (ui/src/render) and what the render tests scan.

import { renderStepParts } from "./sentences.js";
import { renderGrant, renderInputs } from "./manifest.js";
import { isObject } from "./util.js";

/**
 * Render a workflow (its details + its steps) to a numbered plain-English view.
 * @param {object} manifest  the workflow details (contracts manifest shape).
 * @param {Array<object>} steps  Action IR steps, or @operant/sdk step objects.
 * @param {{values?: Record<string,string>}} [opts]  override input values used
 *   when resolving parameter chips (defaults come from the manifest inputs).
 * @returns {{
 *   name: string, title: string, summary: string, grant: string,
 *   inputs: Array<object>,
 *   steps: Array<{n:number, kind:string, parts:Array, sentence:string, irreversible:boolean}>
 * }}
 */
export function renderWorkflow(manifest, steps, opts = {}) {
  const m = isObject(manifest) ? manifest : {};
  const inputs = renderInputs(m);

  const values = { ...Object.fromEntries(inputs.map((f) => [f.name, f.value])), ...(opts.values || {}) };
  const props = isObject(m.inputs_schema) && isObject(m.inputs_schema.properties) ? m.inputs_schema.properties : {};
  const ctx = { values, inputs: props };

  const list = Array.isArray(steps) ? steps : [];
  const renderedSteps = list.map((step, i) => {
    const r = renderStepParts(step, ctx);
    return { n: i + 1, kind: r.kind, parts: r.parts, sentence: r.sentence, irreversible: r.irreversible };
  });

  return {
    name: typeof m.name === "string" ? m.name : "",
    title: (typeof m.description === "string" && m.description) || (typeof m.name === "string" ? m.name : "Workflow"),
    summary: typeof m.description === "string" ? m.description : "",
    grant: renderGrant(m.capabilities),
    inputs,
    steps: renderedSteps,
  };
}
