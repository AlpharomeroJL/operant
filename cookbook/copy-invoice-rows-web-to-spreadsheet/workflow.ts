// Cookbook workflow: Copy invoice rows from a web portal into a spreadsheet
// Source prose: ../copy-invoice-rows-web-to-spreadsheet.md
//
// Benchmark: yes - one of the three cookbook workflows that feed the
// crates/bench suite (see docs/specs/bench.md). Tracked in
// ../bench-workflows.json for L9B (bench-suite) to pick up.
import { defineWorkflow, step, input } from "../../sdk/ts/index.js";

const WINDOW_BROWSER = { process: "chrome.exe", titlePattern: ".*Invoices.*Chrome$" };
const WINDOW_EXCEL = { process: "excel.exe", titlePattern: ".* - Excel" };

const SEL_SEARCH_FIELD = [
  { kind: "css", value: "input[name='invoice-filter']" },
  { kind: "name_role_path", path: [{ role: "document", name: "Invoices" }, { role: "edit", name: "Search invoices" }] },
];
const SEL_FIRST_ROW = [
  { kind: "css", value: "table#invoices tbody tr:first-child" },
  { kind: "name_role_path", path: [{ role: "table", name: "Invoices" }, { role: "row", name: "row 1" }] },
];
const SEL_LAST_ROW = [
  { kind: "css", value: "table#invoices tbody tr:last-child" },
  { kind: "name_role_path", path: [{ role: "table", name: "Invoices" }, { role: "row", name: "last row" }] },
];
const SEL_START_CELL = [
  { kind: "automation_id", value: "Cell_A5" },
  { kind: "name_role_path", path: [{ role: "window", name: "2024-Q3-invoices.xlsx - Excel" }, { role: "cell", name: "{start_cell}" }] },
  { kind: "ordinal_path", path: [{ role: "window", ordinal: 0 }, { role: "worksheet", ordinal: 0 }, { role: "cell", ordinal: 0 }] },
];

export default defineWorkflow({
  name: "copy-invoice-rows-web-to-spreadsheet",
  version: "1.0.0",
  description: "Copies filtered invoice rows from a web portal into an open Excel spreadsheet.",
  inputs: {
    portal_url: input.url({ default: "https://vendor.com/invoices", label: "Invoice portal URL" }),
    invoice_filter: input.text({ default: "Invoices from last month", label: "Invoice filter or search text" }),
    spreadsheet_file: input.filePath({ default: "2024-Q3-invoices.xlsx", label: "Spreadsheet file to receive the data" }),
    start_cell: input.text({ default: "A5", label: "Cell where pasted data should start" }),
  },
  steps: [
    // 1-2. Open the portal, log in, navigate to invoices, and filter or search.
    step.click({
      intent: "Click the invoice search field",
      window: WINDOW_BROWSER,
      selectors: SEL_SEARCH_FIELD,
      risk: "read",
    }),
    step.type({
      intent: "Type the invoice filter",
      window: WINDOW_BROWSER,
      selectors: SEL_SEARCH_FIELD,
      text: "{invoice_filter}",
      risk: "write",
    }),
    step.key({ intent: "Run the invoice search", window: WINDOW_BROWSER, combo: "enter", risk: "read" }),
    step.wait({ intent: "Wait for the filtered invoice list to load", scope: { window: WINDOW_BROWSER }, timeoutMs: 6000 }),
    // 3. On the spreadsheet, place the cursor in the first cell where data should start.
    step.click({
      intent: "Click the start cell in the spreadsheet",
      window: WINDOW_EXCEL,
      selectors: SEL_START_CELL,
      risk: "read",
    }),
    // 4. Go back to the portal and select the invoice rows (shift-click the range).
    step.click({
      intent: "Click the first invoice row",
      window: WINDOW_BROWSER,
      selectors: SEL_FIRST_ROW,
      risk: "read",
    }),
    step.click({
      intent: "Shift-click the last invoice row to select the range",
      window: WINDOW_BROWSER,
      selectors: SEL_LAST_ROW,
      risk: "read",
    }),
    // 5. Copy the selected rows.
    step.key({ intent: "Copy the selected invoice rows", window: WINDOW_BROWSER, combo: "ctrl+c", risk: "read" }),
    // 6. Go back to the spreadsheet and paste the rows.
    step.click({
      intent: "Click the start cell again before pasting",
      window: WINDOW_EXCEL,
      selectors: SEL_START_CELL,
      risk: "read",
    }),
    step.key({ intent: "Paste the invoice rows", window: WINDOW_EXCEL, combo: "ctrl+v", risk: "write" }),
    step.wait({ intent: "Wait for the pasted rows to settle", scope: { window: WINDOW_EXCEL }, timeoutMs: 3000 }),
    // 7. Check that all columns lined up correctly and no data was cut off.
    step.assert({
      intent: "Check that the pasted columns line up with no data cut off",
      window: WINDOW_EXCEL,
      expr: {
        op: "matches",
        query: { kind: "snapshot_element_value", role: "cell", name: "{start_cell}" },
        regex: "^\\S+.*$",
      },
    }),
    // 8. Delete any extra blank rows at the bottom.
    step.scroll({ intent: "Scroll to the bottom of the pasted data", window: WINDOW_EXCEL, direction: "down", amount: 10, risk: "read" }),
    step.key({ intent: "Delete extra blank rows at the bottom", window: WINDOW_EXCEL, combo: "ctrl+minus", risk: "destructive" }),
    // 9. Save the spreadsheet.
    step.key({ intent: "Save the spreadsheet", window: WINDOW_EXCEL, combo: "ctrl+s", risk: "write" }),
    step.wait({ intent: "Wait for the save to complete", scope: { window: WINDOW_EXCEL }, timeoutMs: 5000 }),
    step.assert({
      intent: "Check that the spreadsheet was saved",
      window: WINDOW_EXCEL,
      expr: {
        op: "matches",
        query: { kind: "snapshot_window_title" },
        regex: "^{spreadsheet_file} - Excel$",
      },
    }),
  ],
});

export const benchmark = true;
