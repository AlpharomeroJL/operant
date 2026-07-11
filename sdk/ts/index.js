// @operant/sdk: the declarative surface a compiled workflow.ts is written
// against. Every builder is pure and returns plain data (a tagged object);
// there is no runtime behavior and no I/O here. The Operant engine reads these
// objects; at replay time it drives them with zero model calls.
//
// Types live in index.d.ts. This file is the runtime the `node --test` suite
// exercises (and what a real workflow module imports at author time).

/**
 * Define a workflow from its metadata, typed inputs, and ordered steps.
 * @param {import("./index.d.ts").WorkflowConfig} config
 * @returns {import("./index.d.ts").WorkflowConfig}
 */
export function defineWorkflow(config) {
  if (!config || typeof config.name !== "string") {
    throw new TypeError("defineWorkflow requires a `name`");
  }
  if (!Array.isArray(config.steps)) {
    throw new TypeError("defineWorkflow requires a `steps` array");
  }
  return {
    name: config.name,
    version: config.version,
    description: config.description,
    inputs: config.inputs ?? {},
    steps: config.steps,
  };
}

/** Typed-input builders. Each returns an input descriptor `{ type, ... }`. */
export const input = {
  date: (opts = {}) => ({ type: "date", ...opts }),
  currency: (opts = {}) => ({ type: "currency", ...opts }),
  text: (opts = {}) => ({ type: "text", ...opts }),
  filePath: (opts = {}) => ({ type: "file_path", ...opts }),
  email: (opts = {}) => ({ type: "email", ...opts }),
  url: (opts = {}) => ({ type: "url", ...opts }),
};

/** Step builders. Each tags its options object with a `kind`. */
export const step = {
  click: (opts) => ({ kind: "click", ...opts }),
  type: (opts) => ({ kind: "type", ...opts }),
  key: (opts) => ({ kind: "key", ...opts }),
  scroll: (opts) => ({ kind: "scroll", ...opts }),
  wait: (opts) => ({ kind: "wait", ...opts }),
  assert: (opts) => ({ kind: "assert", ...opts }),
};

// Plain-English renderer (C19, FR-U2/FR-U11): turn a compiled workflow's details
// and steps into numbered plain-English steps with form-field parameters. Pure,
// no I/O. See src/render/.
export {
  ACTION_IR_KINDS,
  renderStep,
  renderStepParts,
  renderCondition,
  renderGrant,
  renderDriftOffer,
  renderInputs,
  applyInputEdits,
  validateManifestShape,
  renderWorkflow,
  sentenceOf,
} from "./src/render/index.js";
