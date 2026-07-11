// Shared helpers for the plain-English renderer. Pure, no I/O.
//
// The renderer accepts a step in either shape and normalizes it:
//   - the Action IR wire shape (contracts/action_ir.schema.json): fields live
//     under `target` and `params`.
//   - the @operant/sdk author shape (a compiled workflow.ts): fields live at
//     the top level (window, selectors, text, combo, timeoutMs, expr, ...).
// Both collapse to one flat internal step so every downstream template reads
// the same field names regardless of where the step came from.

/** True for a non-null, non-array object. */
export function isObject(x) {
  return typeof x === "object" && x !== null && !Array.isArray(x);
}

/** Coerce any value to a brace-free display string (never leaks JSON). */
export function asText(v) {
  if (v === null || v === undefined) return "";
  if (typeof v === "string") return v;
  if (typeof v === "number" || typeof v === "boolean" || typeof v === "bigint") {
    return String(v);
  }
  // Objects/arrays must never render as raw JSON in default mode.
  return "the value";
}

/** Flatten a normalized step from either the IR wire shape or the SDK shape. */
export function normalizeStep(step) {
  const s = isObject(step) ? step : {};
  const target = isObject(s.target) ? s.target : {};
  const params = isObject(s.params) ? s.params : {};
  const scope = params.scope ?? s.scope ?? {};

  return {
    kind: s.kind,
    intent: typeof s.intent === "string" ? s.intent : undefined,
    irreversible: Boolean(s.irreversible),
    window: target.window ?? s.window ?? (isObject(scope) ? scope.window : undefined),
    selectors: target.selectors ?? s.selectors ?? [],
    // type
    text: params.text ?? s.text,
    inputRef: params.input_ref ?? s.inputRef ?? s.input_ref,
    // key
    combo: params.combo ?? s.combo,
    // scroll
    direction: params.direction ?? s.direction,
    amount: params.amount ?? s.amount,
    // drag
    to: params.to ?? s.to,
    // assert
    expr: params.expr ?? s.expr,
    // adapter_call
    namespace: params.namespace ?? s.namespace,
    verb: params.verb ?? s.verb,
    args: isObject(params.args) ? params.args : isObject(s.args) ? s.args : {},
  };
}

/** Pull selectors from a step-like or target-like object. */
function selectorsOf(x) {
  if (!isObject(x)) return [];
  if (Array.isArray(x.selectors)) return x.selectors;
  if (isObject(x.target) && Array.isArray(x.target.selectors)) return x.target.selectors;
  return [];
}

/**
 * Best human label for an element: the trailing name of a name+role path.
 * Automation ids and css/ordinal paths are machine strings, so they are never
 * shown as a name; we fall back to a plain noun instead.
 */
export function elementName(x, fallback = "the item") {
  for (const sel of selectorsOf(x)) {
    if (isObject(sel) && sel.kind === "name_role_path" && Array.isArray(sel.path) && sel.path.length) {
      const last = sel.path[sel.path.length - 1];
      if (isObject(last) && typeof last.name === "string" && last.name.trim()) {
        return last.name.trim();
      }
    }
  }
  return fallback;
}

const APP_NAMES = {
  notepad: "Notepad",
  chrome: "Chrome",
  msedge: "Edge",
  firefox: "Firefox",
  excel: "Excel",
  winword: "Word",
  outlook: "Outlook",
  explorer: "File Explorer",
  powerpnt: "PowerPoint",
  code: "VS Code",
};

/** notepad.exe -> Notepad; unknown-app.exe -> Unknown-App. */
export function friendlyApp(exe) {
  const base = asText(exe).replace(/\.exe$/i, "").trim();
  if (!base) return "an app";
  const key = base.toLowerCase();
  if (APP_NAMES[key]) return APP_NAMES[key];
  return base
    .split(/[\s._-]+/)
    .filter(Boolean)
    .map((w) => w.charAt(0).toUpperCase() + w.slice(1))
    .join(" ");
}

/** C:\Users\Name\Downloads -> Downloads. */
export function friendlyFolder(path) {
  const parts = asText(path).split(/[\\/]+/).filter(Boolean);
  return parts.length ? parts[parts.length - 1] : asText(path) || "a folder";
}

/** Oxford-comma list: [a] -> "a"; [a,b] -> "a and b"; [a,b,c] -> "a, b, and c". */
export function joinList(items) {
  const xs = items.filter((s) => typeof s === "string" && s.length);
  if (xs.length === 0) return "";
  if (xs.length === 1) return xs[0];
  if (xs.length === 2) return `${xs[0]} and ${xs[1]}`;
  return `${xs.slice(0, -1).join(", ")}, and ${xs[xs.length - 1]}`;
}

/** Resolve an input reference to its current display value (chip text). */
export function chipValue(ref, ctx) {
  if (ctx && ctx.values && Object.prototype.hasOwnProperty.call(ctx.values, ref)) {
    const v = ctx.values[ref];
    if (v !== undefined && v !== null && String(v).length) return asText(v);
  }
  if (ctx && ctx.inputs && isObject(ctx.inputs[ref]) && ctx.inputs[ref].title) {
    return asText(ctx.inputs[ref].title);
  }
  return asText(ref);
}

/**
 * Split a value template into inline parts. `{input_name}` tokens become chips
 * (editable form fields); everything else is literal text. Guarantees no `{`/`}`
 * survives into a rendered sentence.
 */
export function valueParts(template, ctx) {
  const str = asText(template);
  const parts = [];
  const re = /\{(\w+)\}/g;
  let last = 0;
  let m;
  while ((m = re.exec(str)) !== null) {
    if (m.index > last) parts.push({ t: "text", text: str.slice(last, m.index) });
    parts.push({ t: "chip", param: m[1], value: chipValue(m[1], ctx), input: true, editable: true });
    last = re.lastIndex;
  }
  if (last < str.length) parts.push({ t: "text", text: str.slice(last) });
  if (parts.length === 0) parts.push({ t: "text", text: str });
  return parts;
}

/** Flatten parts to a single plain-English string (chips shown as their value). */
export function sentenceOf(parts) {
  return parts.map((p) => (p.t === "text" ? p.text : asText(p.value))).join("");
}

/** First present key from `args`, else fallback. */
export function argOf(args, keys, fallback = "") {
  if (!isObject(args)) return fallback;
  for (const k of keys) {
    if (args[k] !== undefined && args[k] !== null && String(args[k]).length) return args[k];
  }
  return fallback;
}
