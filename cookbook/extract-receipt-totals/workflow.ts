// Cookbook workflow: Extract totals from a scanned receipt
// Source prose: ../extract-receipt-totals.md
// Benchmark: no
//
// One representative receipt (prose step 8, "repeat for the next receipt",
// is a re-run of this same workflow with a new receipt_file/receipt_date/
// total_amount, not a loop inside the compiled trace).
import { defineWorkflow, step, input } from "../../sdk/ts/index.js";

const WINDOW_VIEWER = { process: "photos.exe", titlePattern: ".*" };
const WINDOW_EXCEL = { process: "excel.exe", titlePattern: ".* - Excel" };

const SEL_DATE_CELL = [
  { kind: "automation_id", value: "Cell_A2" },
  { kind: "name_role_path", path: [{ role: "window", name: "Expenses-2024.xlsx - Excel" }, { role: "cell", name: "A2" }] },
  { kind: "ordinal_path", path: [{ role: "window", ordinal: 0 }, { role: "worksheet", ordinal: 0 }, { role: "cell", ordinal: 0 }] },
];
const SEL_AMOUNT_CELL = [
  { kind: "automation_id", value: "Cell_B2" },
  { kind: "name_role_path", path: [{ role: "window", name: "Expenses-2024.xlsx - Excel" }, { role: "cell", name: "B2" }] },
];
const SEL_DESCRIPTION_CELL = [
  { kind: "automation_id", value: "Cell_C2" },
  { kind: "name_role_path", path: [{ role: "window", name: "Expenses-2024.xlsx - Excel" }, { role: "cell", name: "C2" }] },
];

export default defineWorkflow({
  name: "extract-receipt-totals",
  version: "1.0.0",
  description: "Reads the date, total, and description off a scanned receipt into an expense spreadsheet row.",
  inputs: {
    receipt_file: input.filePath({ default: "receipt-2024-01-15.jpg", label: "Receipt image or PDF" }),
    spreadsheet_file: input.filePath({ default: "Expenses-2024.xlsx", label: "Expense spreadsheet file" }),
    receipt_date: input.date({ default: "2024-01-15", label: "Date on the receipt" }),
    total_amount: input.currency({ default: "42.17", label: "Total dollar amount on the receipt" }),
    description: input.text({ default: "Office supplies", label: "Short description of the expense" }),
  },
  steps: [
    // 1. Open the receipt image or PDF.
    step.wait({ intent: "Wait for the receipt image to open", scope: { window: WINDOW_VIEWER }, timeoutMs: 5000 }),
    // 2. Open the spreadsheet file where expenses are tracked.
    step.key({ intent: "Switch to the expense spreadsheet", window: WINDOW_EXCEL, combo: "alt+tab", risk: "read" }),
    // 3-4. Place the cursor in the date cell and type the date from the receipt.
    step.click({
      intent: "Click the date cell",
      window: WINDOW_EXCEL,
      selectors: SEL_DATE_CELL,
      risk: "read",
    }),
    step.type({
      intent: "Type the receipt date",
      window: WINDOW_EXCEL,
      selectors: SEL_DATE_CELL,
      text: "{receipt_date}",
      risk: "write",
    }),
    // 5. Tab to the next cell and type the total dollar amount.
    step.key({ intent: "Tab to the amount cell", window: WINDOW_EXCEL, combo: "tab", risk: "read" }),
    step.type({
      intent: "Type the total amount from the receipt",
      window: WINDOW_EXCEL,
      selectors: SEL_AMOUNT_CELL,
      text: "{total_amount}",
      risk: "write",
    }),
    // 6. In the next cell, type a short description of the expense.
    step.key({ intent: "Tab to the description cell", window: WINDOW_EXCEL, combo: "tab", risk: "read" }),
    step.type({
      intent: "Type a short description of the receipt",
      window: WINDOW_EXCEL,
      selectors: SEL_DESCRIPTION_CELL,
      text: "{description}",
      risk: "write",
    }),
    step.wait({ intent: "Wait for the row to settle", scope: { window: WINDOW_EXCEL }, timeoutMs: 2000 }),
    step.assert({
      intent: "Check that the amount cell holds a dollar value",
      window: WINDOW_EXCEL,
      expr: {
        op: "matches",
        query: { kind: "snapshot_element_value", role: "cell", name: "B2" },
        regex: "^\\d+\\.\\d{2}$",
      },
    }),
    // 9. Save the spreadsheet.
    step.key({ intent: "Save the spreadsheet", window: WINDOW_EXCEL, combo: "ctrl+s", risk: "write" }),
    step.wait({ intent: "Wait for the save to complete", scope: { window: WINDOW_EXCEL }, timeoutMs: 5000 }),
  ],
});

export const benchmark = false;
