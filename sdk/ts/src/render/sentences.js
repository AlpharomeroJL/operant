// The sentence engine: one template per Action IR kind, plus the gate-condition
// and adapter renderers the templates lean on. Everything here is TOTAL over the
// Action IR: `renderStepParts` handles every kind in ACTION_IR_KINDS and throws
// on anything else, so an unhandled kind can never silently fall through to raw
// JSON. Default mode forbids that fallback; the totality test proves the switch
// is exhaustive against contracts/action_ir.schema.json.

import { asText, elementName, isObject, normalizeStep, valueParts, sentenceOf, argOf } from "./util.js";

/** The closed set of Action IR kinds the renderer speaks. Mirrors the schema. */
export const ACTION_IR_KINDS = Object.freeze([
  "click",
  "type",
  "key",
  "scroll",
  "drag",
  "wait",
  "assert",
  "adapter_call",
]);

const KIND_SET = new Set(ACTION_IR_KINDS);

// ---- small part builders -----------------------------------------------------

function phrase(text) {
  return [{ t: "text", text }];
}

/** `prefix` + a quoted value (with inline chips), or just `prefix` if empty. */
function withValue(prefix, value, ctx, suffix = "") {
  if (value === undefined || value === null || String(value).length === 0) {
    return [{ t: "text", text: prefix + suffix }];
  }
  return [{ t: "text", text: `${prefix} "` }, ...valueParts(value, ctx), { t: "text", text: `"${suffix}` }];
}

/** `Verb "src" to "dest"` with inline chips on both operands. */
function pair(verbWord, src, dest, ctx) {
  return [
    { t: "text", text: `${verbWord} "` },
    ...valueParts(src, ctx),
    { t: "text", text: '" to "' },
    ...valueParts(dest, ctx),
    { t: "text", text: '"' },
  ];
}

// ---- key combos -> plain phrases --------------------------------------------

const KEY_PHRASES = {
  "ctrl+s": "Save the file",
  "ctrl+c": "Copy the selection",
  "ctrl+v": "Paste",
  "ctrl+x": "Cut the selection",
  "ctrl+a": "Select everything",
  "ctrl+z": "Undo the last change",
  "ctrl+y": "Redo the last change",
  "ctrl+p": "Print",
  "ctrl+f": "Find",
  enter: "Confirm",
  return: "Confirm",
  tab: "Move to the next field",
  esc: "Close this",
  escape: "Close this",
  delete: "Delete the selection",
  backspace: "Delete the last character",
};

function normalizeCombo(combo) {
  return asText(combo).toLowerCase().replace(/\s+/g, "").trim();
}

function prettyCombo(combo) {
  return asText(combo)
    .split("+")
    .map((k) => k.trim())
    .filter(Boolean)
    .map((k) => k.charAt(0).toUpperCase() + k.slice(1))
    .join("+");
}

function keyPhrase(combo) {
  const c = normalizeCombo(combo);
  if (!c) return "Press a key";
  if (KEY_PHRASES[c]) return KEY_PHRASES[c];
  return `Press ${prettyCombo(combo)}`;
}

// ---- gate expression -> plain-English condition -----------------------------

function operand(o) {
  if (!isObject(o)) return asText(o) || "the result";
  switch (o.kind) {
    case "literal":
      return asText(o.value) || "the value";
    case "snapshot_window_process":
      return "the open app";
    case "snapshot_element_value":
    case "snapshot_element_exists":
      return o.name ? `"${asText(o.name)}"` : "the item";
    default:
      return "the result";
  }
}

/**
 * Render a gate predicate AST as a readable condition (never raw JSON). Total:
 * any unknown operator collapses to a safe, jargon-free phrase.
 */
export function renderCondition(expr) {
  if (!isObject(expr)) return "the result is what we expect";
  const q = expr.query;
  switch (expr.op) {
    case "equals":
      return `${operand(expr.left)} is ${operand(expr.right)}`;
    case "not_equals":
      return `${operand(expr.left)} is not ${operand(expr.right)}`;
    case "matches":
      return `${operand(q)} shows the expected text`;
    case "contains":
      return `${operand(expr.left ?? q)} contains what we expect`;
    case "exists":
      return `${operand(q)} is there`;
    case "gt":
      return `${operand(expr.left)} is more than ${operand(expr.right)}`;
    case "lt":
      return `${operand(expr.left)} is less than ${operand(expr.right)}`;
    case "and": {
      const terms = Array.isArray(expr.terms) ? expr.terms : Array.isArray(expr.args) ? expr.args : [];
      const parts = terms.map(renderCondition);
      return parts.length ? parts.join(" and ") : "the result is what we expect";
    }
    case "or": {
      const terms = Array.isArray(expr.terms) ? expr.terms : Array.isArray(expr.args) ? expr.args : [];
      const parts = terms.map(renderCondition);
      return parts.length ? parts.join(" or ") : "the result is what we expect";
    }
    case "not":
      return `${renderCondition(expr.expr ?? expr.term)} is not true`;
    default:
      return "the result is what we expect";
  }
}

// ---- adapter_call -> plain-English parts -------------------------------------

function ns(namespace) {
  const n = asText(namespace).toLowerCase();
  if (n === "file" || n === "files" || n === "filesystem") return "fs";
  return n;
}

/** Returns { parts } for an adapter_call. Total over unknown namespaces/verbs. */
function adapterParts(n, ctx) {
  const namespace = ns(n.namespace);
  const verb = asText(n.verb).toLowerCase();
  const args = n.args;
  const src = argOf(args, ["src", "from", "source", "path", "file"]);
  const dest = argOf(args, ["dest", "to", "target", "destination", "folder"]);

  switch (namespace) {
    case "fs":
      switch (verb) {
        case "move":
          return { parts: pair("Move", src, dest, ctx) };
        case "rename":
          return { parts: pair("Rename", src, dest, ctx) };
        case "copy":
          return { parts: pair("Copy", src, dest, ctx) };
        case "create_folder":
        case "make_folder":
        case "mkdir":
          return { parts: withValue("Make a folder called", argOf(args, ["name", "folder", "path", "dest", "to"], "New folder"), ctx) };
        case "delete":
        case "remove":
          return { parts: withValue("Delete", src, ctx) };
        default:
          return { parts: phrase("Work with your files") };
      }
    case "browser":
      switch (verb) {
        case "open":
        case "navigate":
        case "goto":
          return { parts: withValue("Open", argOf(args, ["url", "address", "path"]), ctx, " in the browser") };
        case "click":
          return { parts: phrase("Click on the page") };
        case "download":
          return { parts: phrase("Download the file") };
        case "type":
          return { parts: withValue("Type", argOf(args, ["text", "value"]), ctx, " on the page") };
        default:
          return { parts: phrase("Use the browser") };
      }
    case "email":
      switch (verb) {
        case "send": {
          const to = argOf(args, ["to", "recipient", "recipients"]);
          return { parts: to ? [{ t: "text", text: "Send an email to " }, ...valueParts(to, ctx)] : phrase("Send an email") };
        }
        case "reply":
          return { parts: phrase("Send the reply") };
        case "read":
        case "fetch":
          return { parts: phrase("Check the inbox") };
        default:
          return { parts: phrase("Work with your email") };
      }
    case "ocr":
      return { parts: src ? withValue("Read the text from", src, ctx) : phrase("Read the text from the picture") };
    case "pdf":
      return { parts: src ? withValue("Read the PDF", src, ctx) : phrase("Read the PDF") };
    case "excel":
      switch (verb) {
        case "open":
          return { parts: withValue("Open the spreadsheet", src, ctx) };
        case "read":
          return { parts: phrase("Copy the rows from the spreadsheet") };
        case "write":
        case "append":
          return { parts: phrase("Put the values into the spreadsheet") };
        case "save":
          return { parts: phrase("Save the spreadsheet") };
        default:
          return { parts: phrase("Work with the spreadsheet") };
      }
    case "word":
      switch (verb) {
        case "open":
          return { parts: withValue("Open the document", src, ctx) };
        case "read":
          return { parts: phrase("Copy the text from the document") };
        case "save":
          return { parts: phrase("Save the document") };
        default:
          return { parts: phrase("Work with the document") };
      }
    case "shell":
      return { parts: phrase("Run the prepared command") };
    default:
      return { parts: phrase("Do the next automated step") };
  }
}

// ---- the per-kind templates --------------------------------------------------

function scrollDir(direction) {
  const d = asText(direction).toLowerCase();
  return d === "up" || d === "down" || d === "left" || d === "right" ? d : "down";
}

/**
 * Render one step to structured parts + a flat sentence.
 * @param {object} step  an Action IR step, or an @operant/sdk step object.
 * @param {{values?: Record<string,string>, inputs?: object}} [ctx]  input values
 *   so parameter chips resolve to their current value.
 * @returns {{ kind: string, parts: Array, sentence: string, irreversible: boolean }}
 */
export function renderStepParts(step, ctx) {
  const n = normalizeStep(step);
  if (!KIND_SET.has(n.kind)) {
    // Forbidden in default mode: an unknown kind must fail loudly, never render
    // as raw JSON. Totality over the real kinds is proven by the property test.
    throw new Error(`cannot render unknown step kind: ${asText(n.kind) || "(none)"}`);
  }

  let parts;
  switch (n.kind) {
    case "click":
      parts = phrase(`Click "${elementName(n, "the item")}"`);
      break;
    case "type": {
      const field = elementName(n, "the text box");
      const value = n.text !== undefined ? n.text : n.inputRef !== undefined ? `{${n.inputRef}}` : "";
      parts = [{ t: "text", text: 'Type "' }, ...valueParts(value, ctx), { t: "text", text: `" into "${field}"` }];
      break;
    }
    case "key":
      parts = phrase(keyPhrase(n.combo));
      break;
    case "scroll":
      parts = phrase(`Scroll ${scrollDir(n.direction)}`);
      break;
    case "drag":
      parts = phrase(`Drag "${elementName(n, "the item")}" onto "${elementName(n.to, "the new spot")}"`);
      break;
    case "wait":
      parts = phrase("Wait for the screen to update");
      break;
    case "assert":
      parts = phrase(`Check that ${renderCondition(n.expr)}`);
      break;
    case "adapter_call":
      parts = adapterParts(n, ctx).parts;
      break;
    default:
      // Unreachable: KIND_SET guards the entry above. Present so a future kind
      // that reaches here still fails loudly instead of emitting raw JSON.
      throw new Error(`cannot render unknown step kind: ${asText(n.kind)}`);
  }

  return { kind: n.kind, parts, sentence: sentenceOf(parts), irreversible: n.irreversible };
}

/** Render one step to just its plain-English sentence. */
export function renderStep(step, ctx) {
  return renderStepParts(step, ctx).sentence;
}
