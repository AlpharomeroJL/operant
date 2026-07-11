// @operant/sdk/render: turn a compiled workflow's details and steps into the
// numbered plain-English view the zero-code UI shows. Pure functions, no I/O,
// no model calls. Default-mode output carries no internal vocabulary and never
// falls back to raw JSON (see contracts/microcopy_glossary.json and the
// totality property in the render tests).

export { ACTION_IR_KINDS, renderStep, renderStepParts, renderCondition } from "./sentences.js";
export { renderGrant, renderDriftOffer, renderInputs, applyInputEdits, validateManifestShape } from "./manifest.js";
export { renderWorkflow } from "./workflow.js";
export { sentenceOf } from "./util.js";
