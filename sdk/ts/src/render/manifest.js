// Renders the parts of a workflow that come from its details (the manifest):
//   - grant prose from capabilities ("This workflow can ...")
//   - drift offers ("The Save button moved. Update the workflow?")
//   - the changeable inputs as form fields
//   - the bidirectional round-trip: a form edit of an input value flows back
//     into the workflow details and the details still validate in shape.
//
// PARAMETERS ONLY: applyInputEdits touches input defaults and nothing else.
// Step logic is never hand-edited here.

import { asText, friendlyApp, friendlyFolder, isObject, joinList } from "./util.js";

const RISK_CLASSES = new Set(["read", "write", "destructive"]);

/**
 * Grant prose from a workflow's capabilities.
 * @param {{apps?: string[], paths?: string[], network?: boolean}} capabilities
 * @returns {string} e.g. "This workflow can read files in Downloads and control Chrome."
 */
export function renderGrant(capabilities) {
  if (!isObject(capabilities)) return "This workflow does not need any permission.";

  const clauses = [];
  const paths = Array.isArray(capabilities.paths) ? capabilities.paths.filter(Boolean) : [];
  if (paths.length) {
    clauses.push(`read files in ${joinList(paths.map(friendlyFolder))}`);
  }
  const apps = Array.isArray(capabilities.apps) ? capabilities.apps.filter(Boolean) : [];
  if (apps.length) {
    clauses.push(`control ${joinList(apps.map(friendlyApp))}`);
  }
  if (capabilities.network === true) {
    clauses.push("connect to the internet");
  }

  if (!clauses.length) return "This workflow does not need any permission.";
  return `This workflow can ${joinList(clauses)}.`;
}

/**
 * A drift offer: a plain-English heads-up plus the yes/no choice. Not a diff.
 * @param {{element: string, change?: string, preview?: string}} opts
 * @returns {{element:string, change:string, headline:string, question:string,
 *   text:string, accept:string, dismiss:string, preview?:string}}
 */
export function renderDriftOffer(opts) {
  const element = asText(isObject(opts) ? opts.element : opts) || "Something on screen";
  const change = (isObject(opts) && asText(opts.change)) || "moved";
  const headline = `The ${element} ${change}.`;
  const question = "Update the workflow?";
  const out = {
    element,
    change,
    headline,
    question,
    text: `${headline} ${question}`,
    accept: "Update the workflow",
    dismiss: "Not now",
  };
  if (isObject(opts) && opts.preview) out.preview = asText(opts.preview);
  return out;
}

function inputsSchemaOf(manifest) {
  return isObject(manifest) && isObject(manifest.inputs_schema) ? manifest.inputs_schema : null;
}

function propsOf(manifest) {
  const schema = inputsSchemaOf(manifest);
  return schema && isObject(schema.properties) ? schema.properties : {};
}

/**
 * The changeable inputs of a workflow, as form fields. Order is preserved.
 * @returns {Array<{name:string, label:string, kind:string, value:string,
 *   pattern?:string, format?:string}>}
 */
export function renderInputs(manifest) {
  const props = propsOf(manifest);
  return Object.keys(props).map((name) => {
    const spec = isObject(props[name]) ? props[name] : {};
    const field = {
      name,
      label: asText(spec.title) || name,
      kind: asText(spec.format) || asText(spec.type) || "text",
      value: spec.default === undefined || spec.default === null ? "" : asText(spec.default),
    };
    if (spec.pattern) field.pattern = asText(spec.pattern);
    if (spec.format) field.format = asText(spec.format);
    return field;
  });
}

/** A single input value's spec-level validity (pattern + date format). */
function valueFits(spec, value) {
  if (!isObject(spec)) return true;
  const v = asText(value);
  if (spec.pattern) {
    try {
      if (!new RegExp(spec.pattern).test(v)) return false;
    } catch {
      // A malformed pattern in the details cannot make an edit unsafe; ignore.
    }
  }
  if (spec.format === "date" && !/^\d{4}-\d{2}-\d{2}$/.test(v)) return false;
  return true;
}

/**
 * Bidirectional edit: take edited input values and return updated workflow
 * details. Only input defaults change. Unknown keys and values that break an
 * input's declared shape are refused, so the returned details still validate.
 * @param {object} manifest
 * @param {Record<string,string>} edits
 * @returns {object} a new manifest with the edits applied
 */
export function applyInputEdits(manifest, edits) {
  const props = propsOf(manifest);
  if (!Object.keys(props).length) {
    throw new Error("this workflow has no changeable details");
  }
  const next = structuredClone(manifest);
  const nextProps = next.inputs_schema.properties;
  for (const [key, raw] of Object.entries(isObject(edits) ? edits : {})) {
    if (!Object.prototype.hasOwnProperty.call(props, key)) {
      throw new Error(`this workflow has no detail called "${key}"`);
    }
    const value = asText(raw);
    if (!valueFits(props[key], value)) {
      const label = asText(props[key].title) || key;
      throw new Error(`the value for "${label}" does not look right`);
    }
    nextProps[key].default = value;
  }
  return next;
}

/**
 * Structural check that a workflow's details are well-formed. Used to prove a
 * form edit round-trips without corrupting the details.
 * @returns {{ok: boolean, errors: string[]}}
 */
export function validateManifestShape(manifest) {
  const errors = [];
  if (!isObject(manifest)) return { ok: false, errors: ["details are missing"] };

  if (manifest.v !== 1) errors.push("version marker must be 1");
  if (typeof manifest.name !== "string" || !manifest.name) errors.push("name is missing");
  if (typeof manifest.version !== "string" || !manifest.version) errors.push("version is missing");

  const schema = inputsSchemaOf(manifest);
  if (!schema) {
    errors.push("inputs are missing");
  } else {
    if (schema.type !== "object") errors.push("inputs must be an object");
    if (!isObject(schema.properties)) {
      errors.push("inputs have no properties");
    } else {
      for (const [name, spec] of Object.entries(schema.properties)) {
        if (!isObject(spec)) {
          errors.push(`input "${name}" is malformed`);
          continue;
        }
        if (spec.default !== undefined && spec.default !== null && !valueFits(spec, spec.default)) {
          errors.push(`input "${name}" default does not fit its shape`);
        }
      }
    }
  }

  const caps = manifest.capabilities;
  if (!isObject(caps)) {
    errors.push("permissions are missing");
  } else {
    if (!Array.isArray(caps.apps)) errors.push("apps must be a list");
    if (!Array.isArray(caps.paths)) errors.push("paths must be a list");
    if (typeof caps.network !== "boolean") errors.push("network must be true or false");
    if (!RISK_CLASSES.has(caps.risk_ceiling)) errors.push("action ceiling is invalid");
  }

  return { ok: errors.length === 0, errors };
}
