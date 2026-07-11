// Cookbook workflow: Fill a form from a spreadsheet row
// Source prose: ../fill-form-from-spreadsheet.md
// Benchmark: no
//
// One representative row with two fields (Name, Email) standing in for
// "repeat steps 4-7 for each field" and "repeat steps 4-10 for each row".
// The DSL has no loop construct, so a full field/row sweep is this workflow
// compiled once per field and re-run per row of input data.
import { defineWorkflow, step, input } from "../../sdk/ts/index.js";

const WINDOW_EXCEL = { process: "excel.exe", titlePattern: ".* - Excel" };
const WINDOW_BROWSER = { process: "chrome.exe", titlePattern: ".*registration-form.*Chrome$" };

const SEL_NAME_CELL = [
  { kind: "automation_id", value: "Cell_A2" },
  { kind: "name_role_path", path: [{ role: "window", name: "customer-data.xlsx - Excel" }, { role: "cell", name: "A2" }] },
];
const SEL_EMAIL_CELL = [
  { kind: "automation_id", value: "Cell_B2" },
  { kind: "name_role_path", path: [{ role: "window", name: "customer-data.xlsx - Excel" }, { role: "cell", name: "B2" }] },
];
const SEL_FORM_NAME_FIELD = [
  { kind: "css", value: "input[name='name']" },
  { kind: "name_role_path", path: [{ role: "form", name: "Registration" }, { role: "edit", name: "Name" }] },
];
const SEL_FORM_EMAIL_FIELD = [
  { kind: "css", value: "input[name='email']" },
  { kind: "name_role_path", path: [{ role: "form", name: "Registration" }, { role: "edit", name: "Email" }] },
];
const SEL_SUBMIT_BTN = [
  { kind: "css", value: "button[type='submit']" },
  { kind: "name_role_path", path: [{ role: "form", name: "Registration" }, { role: "button", name: "Submit" }] },
];

export default defineWorkflow({
  name: "fill-form-from-spreadsheet",
  version: "1.0.0",
  description: "Copies one row of spreadsheet data into a web form, field by field, and submits it.",
  inputs: {
    spreadsheet_file: input.filePath({ default: "customer-data.xlsx", label: "Spreadsheet with data to enter" }),
    form_url: input.url({ default: "https://company.com/registration-form", label: "Web form address" }),
    data_range: input.text({ default: "Rows 2 through 50", label: "Which rows of data to process" }),
  },
  steps: [
    // 1. Open the spreadsheet that contains the data to be entered.
    step.wait({ intent: "Wait for the spreadsheet to be ready", scope: { window: WINDOW_EXCEL }, timeoutMs: 3000 }),
    // 2. Open the browser and navigate to the form.
    step.key({ intent: "Focus the browser address bar", window: WINDOW_BROWSER, combo: "ctrl+l", risk: "read" }),
    step.type({
      intent: "Type the web form address",
      window: WINDOW_BROWSER,
      selectors: [{ kind: "css", value: "input#omnibox-input" }],
      text: "{form_url}",
      risk: "write",
    }),
    step.key({ intent: "Go to the web form", window: WINDOW_BROWSER, combo: "enter", risk: "read" }),
    step.wait({ intent: "Wait for the form to load", scope: { window: WINDOW_BROWSER }, timeoutMs: 6000 }),
    // 3-7. Copy the Name cell and paste it into the Name field.
    step.click({ intent: "Click the Name cell in the spreadsheet", window: WINDOW_EXCEL, selectors: SEL_NAME_CELL, risk: "read" }),
    step.key({ intent: "Copy the Name cell", window: WINDOW_EXCEL, combo: "ctrl+c", risk: "read" }),
    step.click({ intent: "Click the Name field on the form", window: WINDOW_BROWSER, selectors: SEL_FORM_NAME_FIELD, risk: "read" }),
    step.key({ intent: "Paste into the Name field", window: WINDOW_BROWSER, combo: "ctrl+v", risk: "write" }),
    step.key({ intent: "Move to the next form field", window: WINDOW_BROWSER, combo: "tab", risk: "read" }),
    // 4-7 (second field). Copy the Email cell and paste it into the Email field.
    step.click({ intent: "Click the Email cell in the spreadsheet", window: WINDOW_EXCEL, selectors: SEL_EMAIL_CELL, risk: "read" }),
    step.key({ intent: "Copy the Email cell", window: WINDOW_EXCEL, combo: "ctrl+c", risk: "read" }),
    step.click({ intent: "Click the Email field on the form", window: WINDOW_BROWSER, selectors: SEL_FORM_EMAIL_FIELD, risk: "read" }),
    step.key({ intent: "Paste into the Email field", window: WINDOW_BROWSER, combo: "ctrl+v", risk: "write" }),
    // 9. Click Submit.
    step.click({ intent: "Click the Submit button", window: WINDOW_BROWSER, selectors: SEL_SUBMIT_BTN, risk: "write" }),
    step.wait({ intent: "Wait for the confirmation message", scope: { window: WINDOW_BROWSER }, timeoutMs: 6000 }),
    // 10. Note the confirmation message to know the form was sent successfully.
    step.assert({
      intent: "Check that the form submission was confirmed",
      window: WINDOW_BROWSER,
      expr: {
        op: "matches",
        query: { kind: "snapshot_element_value", role: "status", name: "form-confirmation" },
        regex: "(?i)thank you|submitted|received",
      },
    }),
  ],
});

export const benchmark = false;
