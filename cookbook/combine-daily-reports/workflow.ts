// Cookbook workflow: Download daily reports and combine them into one file
// Source prose: ../combine-daily-reports.md
// Benchmark: no
//
// One representative pass through the daily-report loop (download one day,
// paste it into the combined sheet). The DSL has no loop construct, so
// running this workflow once per date and re-running with a new
// `report_date` input is how the cookbook step "repeat for each date" is
// realized in a compiled, replayable form.
import { defineWorkflow, step, input } from "../../sdk/ts/index.js";

const WINDOW_BROWSER = { process: "chrome.exe", titlePattern: ".*Reports.*Chrome$" };
const WINDOW_EXCEL = { process: "excel.exe", titlePattern: ".* - Excel" };

const SEL_ADDRESS_BAR = [
  { kind: "css", value: "input#omnibox-input" },
  { kind: "ordinal_path", path: [{ role: "toolbar", ordinal: 0 }, { role: "edit", ordinal: 0 }] },
];
const SEL_DATE_PICKER = [
  { kind: "css", value: "[data-testid='report-date-picker']" },
  { kind: "name_role_path", path: [{ role: "document", name: "Reports" }, { role: "button", name: "Choose date" }] },
];
const SEL_DOWNLOAD_BTN = [
  { kind: "css", value: "button#download-report" },
  { kind: "name_role_path", path: [{ role: "document", name: "Reports" }, { role: "button", name: "Download" }] },
];
const SEL_FIRST_CELL = [
  { kind: "automation_id", value: "Cell_A1" },
  { kind: "name_role_path", path: [{ role: "window", name: "Weekly-Report - Excel" }, { role: "cell", name: "A1" }] },
  { kind: "ordinal_path", path: [{ role: "window", ordinal: 0 }, { role: "worksheet", ordinal: 0 }, { role: "cell", ordinal: 0 }] },
];
const SEL_HEADER_ROW = [
  { kind: "automation_id", value: "Cell_A1" },
  { kind: "name_role_path", path: [{ role: "window", name: "Weekly-Report - Excel" }, { role: "cell", name: "A1" }] },
];

export default defineWorkflow({
  name: "combine-daily-reports",
  version: "1.0.0",
  description: "Downloads a daily report from a reports portal and merges it into a combined weekly spreadsheet.",
  inputs: {
    portal_url: input.url({ default: "https://analytics.company.com/reports", label: "Reports portal URL" }),
    report_date: input.date({ default: "2024-01-08", label: "Date of the report to download" }),
    output_file: input.filePath({
      default: "C:\\Users\\Name\\Documents\\Weekly-Reports\\Weekly-Report-2024-01-08-to-2024-01-14.xlsx",
      label: "Combined output spreadsheet path",
    }),
  },
  steps: [
    // 1. Open your web browser and go to the reports portal.
    step.key({ intent: "Focus the browser address bar", window: WINDOW_BROWSER, combo: "ctrl+l", risk: "read" }),
    step.type({
      intent: "Type the reports portal address",
      window: WINDOW_BROWSER,
      selectors: SEL_ADDRESS_BAR,
      text: "{portal_url}",
      risk: "write",
    }),
    step.key({ intent: "Go to the reports portal", window: WINDOW_BROWSER, combo: "enter", risk: "read" }),
    step.wait({ intent: "Wait for the reports page to load", scope: { window: WINDOW_BROWSER }, timeoutMs: 8000 }),
    // 2-3. Log in if needed, then select the first date on the date picker.
    step.click({
      intent: "Click the date picker",
      window: WINDOW_BROWSER,
      selectors: SEL_DATE_PICKER,
      risk: "read",
    }),
    step.type({
      intent: "Type the report date",
      window: WINDOW_BROWSER,
      selectors: SEL_DATE_PICKER,
      text: "{report_date}",
      risk: "write",
    }),
    // 4. Click Download to save the daily report.
    step.click({
      intent: "Click the Download button",
      window: WINDOW_BROWSER,
      selectors: SEL_DOWNLOAD_BTN,
      risk: "write",
    }),
    step.wait({ intent: "Wait for the download to finish", scope: { window: WINDOW_BROWSER }, timeoutMs: 10000 }),
    // 6-8. Open the spreadsheet application and the first downloaded report, then select and copy it.
    step.key({ intent: "Switch to the spreadsheet application", window: WINDOW_EXCEL, combo: "alt+tab", risk: "read" }),
    step.key({ intent: "Select all data in the downloaded report", window: WINDOW_EXCEL, combo: "ctrl+a", risk: "read" }),
    step.key({ intent: "Copy the report data", window: WINDOW_EXCEL, combo: "ctrl+c", risk: "read" }),
    // 9. Click the first cell in the combined spreadsheet and paste.
    step.click({
      intent: "Click the first cell in the combined spreadsheet",
      window: WINDOW_EXCEL,
      selectors: SEL_FIRST_CELL,
      risk: "read",
    }),
    step.key({ intent: "Paste the report data", window: WINDOW_EXCEL, combo: "ctrl+v", risk: "write" }),
    // 10-11. Scroll to the bottom of the combined data and add a header row if missing.
    step.scroll({ intent: "Scroll to the bottom of the combined data", window: WINDOW_EXCEL, direction: "down", amount: 10, risk: "read" }),
    step.click({
      intent: "Click the header row cell",
      window: WINDOW_EXCEL,
      selectors: SEL_HEADER_ROW,
      risk: "read",
    }),
    step.type({
      intent: "Type the header row if it is missing",
      window: WINDOW_EXCEL,
      selectors: SEL_HEADER_ROW,
      text: "Date\tRegion\tMetric\tValue",
      risk: "write",
    }),
    // 12. Save the combined file with a name that indicates it contains multiple days.
    step.key({ intent: "Save the combined file", window: WINDOW_EXCEL, combo: "ctrl+s", risk: "write" }),
    step.wait({ intent: "Wait for the save to complete", scope: { window: WINDOW_EXCEL }, timeoutMs: 5000 }),
    step.assert({
      intent: "Check that the combined report was saved",
      window: WINDOW_EXCEL,
      expr: {
        op: "matches",
        query: { kind: "snapshot_window_title" },
        regex: "^Weekly-Report.*\\.xlsx.* - Excel$",
      },
    }),
  ],
});

export const benchmark = false;
