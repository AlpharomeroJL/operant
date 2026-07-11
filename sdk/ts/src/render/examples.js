// Render targets used by the render tests: the canonical notepad fixture (in
// the @operant/sdk author shape, so the renderer's shape-normalization is
// exercised) plus one Action IR workflow per cookbook entry (so every prose
// workflow in cookbook/ has a machine workflow that renders to clean, numbered
// plain English). These are examples/fixtures, not part of the shipped surface.

// ---- the canonical notepad fixture ------------------------------------------
// A faithful copy of contracts/fixtures/workflow_notepad/{manifest.json,
// workflow.ts}. The steps are in SDK author shape on purpose.

export const notepadManifest = {
  v: 1,
  name: "notepad-invoice-note",
  version: "1.0.0",
  description: "Writes a dated invoice note into Notepad and saves it.",
  step_summary: [
    "Click the text editor",
    "Type the invoice note",
    "Wait for the screen to update",
    "Save the file",
    "Wait for the screen to update",
    "Check that the note was written",
  ],
  inputs_schema: {
    type: "object",
    properties: {
      invoice_date: { type: "string", format: "date", default: "2026-07-11", title: "Invoice date" },
      amount: { type: "string", pattern: "^\\d+\\.\\d{2}$", default: "142.50", title: "Amount" },
    },
    additionalProperties: false,
  },
  capabilities: { apps: ["notepad.exe"], paths: [], network: false, risk_ceiling: "write" },
  gates: [],
  min_operant_version: "1.0.0",
  source_run_id: "01JZFIXTURERUN0000000000",
  dsl: { path: "workflow.ts", hash: "89b0cd44ca415e03142236dca738112362430705a30a682fa92ecc3cda286c8f" },
  signature: null,
};

const NP_WINDOW = { process: "notepad.exe", titlePattern: ".* - Notepad" };
const NP_SELECTORS = [
  { kind: "automation_id", value: "RichEditD2DPT" },
  { kind: "name_role_path", path: [{ role: "window", name: "Untitled - Notepad" }, { role: "document", name: "Text editor" }] },
  { kind: "ordinal_path", path: [{ role: "window", ordinal: 0 }, { role: "document", ordinal: 0 }] },
];

export const notepadSteps = [
  { kind: "click", intent: "Click the text editor", window: NP_WINDOW, selectors: NP_SELECTORS, risk: "read" },
  { kind: "type", intent: "Type the invoice note", window: NP_WINDOW, selectors: NP_SELECTORS, text: "Invoice {invoice_date} total ${amount}", risk: "write" },
  { kind: "wait", intent: "Wait for the screen to update", scope: { window: NP_WINDOW }, timeoutMs: 5000 },
  { kind: "key", intent: "Save the file", window: NP_WINDOW, combo: "ctrl+s", risk: "write" },
  { kind: "wait", intent: "Wait for the screen to update", scope: { window: NP_WINDOW }, timeoutMs: 5000 },
  {
    kind: "assert",
    intent: "Check that the note was written",
    window: NP_WINDOW,
    expr: { op: "matches", query: { kind: "snapshot_element_value", role: "document", name: "Text editor" }, regex: "^Invoice \\d{4}-\\d{2}-\\d{2} total \\$\\d+\\.\\d{2}$" },
  },
];

export const notepadWorkflow = { manifest: notepadManifest, steps: notepadSteps };

// ---- Action IR builders for the cookbook targets ----------------------------

let seq = 0;
const sid = () => `01STEP${String(++seq).padStart(20, "0")}`;
const win = (process) => ({ process, title_pattern: ".*" });
const nameRole = (name, role = "control") => ({ kind: "name_role_path", path: [{ role: "window", name: "App" }, { role, name }] });

const S = {
  click: (name, window) => ({ v: 1, id: sid(), kind: "click", intent: `click ${name}`, target: { window, selectors: [nameRole(name, "button")] }, risk_class: "read", grounding: "uia" }),
  type: (field, text, window) => ({ v: 1, id: sid(), kind: "type", intent: `type into ${field}`, target: { window, selectors: [nameRole(field, "edit")] }, params: { text }, risk_class: "write", grounding: "uia" }),
  key: (combo, window) => ({ v: 1, id: sid(), kind: "key", intent: `press ${combo}`, target: { window }, params: { combo }, risk_class: "write", grounding: "uia" }),
  scroll: (direction, window) => ({ v: 1, id: sid(), kind: "scroll", intent: `scroll ${direction}`, target: { window }, params: { direction, amount: 3 }, risk_class: "read", grounding: "uia" }),
  drag: (from, to, window) => ({ v: 1, id: sid(), kind: "drag", intent: `drag ${from}`, target: { window, selectors: [nameRole(from, "listitem")] }, params: { to: { selectors: [nameRole(to, "treeitem")] } }, risk_class: "write", grounding: "uia" }),
  wait: (window) => ({ v: 1, id: sid(), kind: "wait", intent: "wait", params: { scope: { window }, timeout_ms: 5000 }, risk_class: "read", grounding: "uia" }),
  assert: (expr) => ({ v: 1, id: sid(), kind: "assert", intent: "check", params: { expr }, risk_class: "read", grounding: "uia" }),
  adapter: (namespace, verb, args = {}, { risk = "write", irreversible = false } = {}) => ({ v: 1, id: sid(), kind: "adapter_call", intent: `${namespace} ${verb}`, params: { namespace, verb, args }, risk_class: risk, irreversible, grounding: "adapter" }),
};

const inText = (title, def) => ({ type: "string", default: def, title });
const inDate = (title, def) => ({ type: "string", format: "date", default: def, title });
const inCurrency = (title, def) => ({ type: "string", pattern: "^\\d+\\.\\d{2}$", default: def, title });

function mkManifest({ name, description, apps = [], paths = [], network = false, risk = "write", inputs = {} }) {
  return {
    v: 1,
    name,
    version: "1.0.0",
    description,
    step_summary: [],
    inputs_schema: { type: "object", properties: { ...inputs }, additionalProperties: false },
    capabilities: { apps, paths, network, risk_ceiling: risk },
    gates: [],
    min_operant_version: "1.0.0",
    source_run_id: "01COOKBOOKEXAMPLE00000000",
    dsl: { path: "workflow.ts", hash: "0" },
    signature: null,
  };
}

const existsAssert = (name) => S.assert({ op: "exists", query: { kind: "snapshot_element_exists", name } });
const matchesAssert = (name) => S.assert({ op: "matches", query: { kind: "snapshot_element_value", name } });

// ---- the ten cookbook targets -----------------------------------------------

const chrome = win("chrome.exe");
const excel = win("excel.exe");
const outlook = win("outlook.exe");
const word = win("winword.exe");
const explorer = win("explorer.exe");

export const cookbookWorkflows = [
  {
    slug: "copy-invoice-rows-web-to-spreadsheet",
    manifest: mkManifest({
      name: "copy-invoice-rows",
      description: "Copies invoice rows from a web portal into a spreadsheet.",
      apps: ["chrome.exe", "excel.exe"],
      network: true,
      inputs: { portal_url: inText("Portal address", "vendor.example.com/invoices"), start_cell: inText("Start cell", "A5") },
    }),
    steps: [
      S.adapter("browser", "open", { url: "{portal_url}" }, { risk: "read" }),
      S.click("Invoice rows", chrome),
      S.key("ctrl+c", chrome),
      S.click("First cell", excel),
      S.key("ctrl+v", excel),
      S.key("ctrl+s", excel),
    ],
  },
  {
    slug: "rename-file-pdfs-by-date",
    manifest: mkManifest({
      name: "rename-pdfs-by-date",
      description: "Renames downloaded PDFs by date and files them into dated folders.",
      apps: ["explorer.exe"],
      paths: ["C:\\Users\\Name\\Downloads"],
      inputs: { naming: inText("Naming pattern", "[Date]-[Description].pdf") },
    }),
    steps: [
      S.adapter("fs", "rename", { src: "invoice.pdf", dest: "2024-01-15-Invoice-Acme.pdf" }),
      S.adapter("fs", "create_folder", { name: "2024-01-January" }),
      S.adapter("fs", "move", { src: "2024-01-15-Invoice-Acme.pdf", dest: "2024-01-January" }),
      existsAssert("2024-01-January"),
    ],
  },
  {
    slug: "reply-canned-customer-email",
    manifest: mkManifest({
      name: "reply-canned-email",
      description: "Sends the same reply to similar support emails.",
      apps: ["outlook.exe"],
      network: true,
      inputs: { message: inText("Reply text", "Thank you for contacting us. Your request will be handled within 24 hours.") },
    }),
    steps: [
      S.click("Reply", outlook),
      S.type("Message", "{message}", outlook),
      S.adapter("email", "send", {}, { irreversible: true }),
    ],
  },
  {
    slug: "extract-receipt-totals",
    manifest: mkManifest({
      name: "extract-receipt-totals",
      description: "Pulls the date and total from a receipt image into a spreadsheet.",
      apps: ["excel.exe"],
      paths: ["C:\\Users\\Name\\Receipts"],
      inputs: { receipt_date: inDate("Receipt date", "2024-01-15"), total: inCurrency("Total", "142.50") },
    }),
    steps: [
      S.adapter("ocr", "extract", { src: "receipt.jpg" }, { risk: "read" }),
      S.type("Date cell", "{receipt_date}", excel),
      S.type("Amount cell", "{total}", excel),
      S.key("ctrl+s", excel),
    ],
  },
  {
    slug: "fill-form-from-spreadsheet",
    manifest: mkManifest({
      name: "fill-form-from-spreadsheet",
      description: "Fills a web form from one spreadsheet row.",
      apps: ["excel.exe", "chrome.exe"],
      network: true,
      inputs: { form_url: inText("Form address", "company.example.com/registration") },
    }),
    steps: [
      S.adapter("excel", "read", {}, { risk: "read" }),
      S.adapter("browser", "open", { url: "{form_url}" }, { risk: "read" }),
      S.type("Name field", "Ada Lovelace", chrome),
      S.type("Email field", "ada@example.com", chrome),
      S.click("Submit", chrome),
    ],
  },
  {
    slug: "combine-daily-reports",
    manifest: mkManifest({
      name: "combine-daily-reports",
      description: "Downloads daily reports and combines them into one spreadsheet.",
      apps: ["chrome.exe", "excel.exe"],
      paths: ["C:\\Users\\Name\\Downloads"],
      network: true,
    }),
    steps: [
      S.adapter("browser", "download", {}, { risk: "read" }),
      S.adapter("fs", "move", { src: "daily-report.csv", dest: "Reports" }),
      S.adapter("excel", "open", { src: "Weekly-Report.xlsx" }),
      S.key("ctrl+v", excel),
      S.key("ctrl+s", excel),
    ],
  },
  {
    slug: "convert-table-to-email",
    manifest: mkManifest({
      name: "convert-table-to-email",
      description: "Copies a table from a document into an email.",
      apps: ["winword.exe", "outlook.exe"],
      network: true,
      inputs: { subject: inText("Subject", "Q1 2024 Sales Summary") },
    }),
    steps: [
      S.adapter("word", "open", { src: "monthly-report.docx" }, { risk: "read" }),
      S.key("ctrl+c", word),
      S.click("Compose", outlook),
      S.key("ctrl+v", outlook),
      S.type("Subject", "{subject}", outlook),
      S.adapter("email", "send", { to: "manager@example.com" }, { irreversible: true }),
    ],
  },
  {
    slug: "organize-files-by-type",
    manifest: mkManifest({
      name: "organize-files-by-type",
      description: "Sorts a folder of mixed files into folders by type.",
      apps: ["explorer.exe"],
      paths: ["C:\\Users\\Name\\Documents\\Archive"],
    }),
    steps: [
      S.adapter("fs", "create_folder", { name: "PDFs" }),
      S.drag("Invoice.pdf", "PDFs", explorer),
      S.adapter("fs", "move", { src: "Report.pdf", dest: "PDFs" }),
      S.scroll("down", explorer),
    ],
  },
  {
    slug: "extract-text-from-images",
    manifest: mkManifest({
      name: "extract-text-from-images",
      description: "Reads text out of images into a document.",
      apps: ["winword.exe"],
      paths: ["C:\\Users\\Name\\Images"],
    }),
    steps: [
      S.adapter("ocr", "extract", { src: "whiteboard.jpg" }, { risk: "read" }),
      S.adapter("word", "open", { src: "Whiteboard-notes.docx" }),
      S.type("Document", "Meeting notes", word),
      S.key("ctrl+s", word),
    ],
  },
  {
    slug: "send-personalized-emails",
    manifest: mkManifest({
      name: "send-personalized-emails",
      description: "Sends similar emails with each person's details filled in.",
      apps: ["excel.exe", "outlook.exe"],
      network: true,
      inputs: { subject: inText("Subject", "Project Approval"), template: inText("Message", "Hi there, your project is approved.") },
    }),
    steps: [
      S.adapter("excel", "read", {}, { risk: "read" }),
      S.type("Subject", "{subject}", outlook),
      S.type("Body", "{template}", outlook),
      S.adapter("email", "send", { to: "team@example.com" }, { irreversible: true }),
      matchesAssert("Sent"),
    ],
  },
];

/** Every render target: the notepad fixture plus the ten cookbook workflows. */
export const allWorkflows = [
  { slug: "workflow_notepad", manifest: notepadManifest, steps: notepadSteps },
  ...cookbookWorkflows,
];
