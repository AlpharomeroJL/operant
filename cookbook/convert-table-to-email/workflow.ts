// Cookbook workflow: Convert a Word document table into an email message
// Source prose: ../convert-table-to-email.md
// Benchmark: no
//
// Prose step 8 ("if the table looks wrong, simplify or retype it") is a
// conditional fallback a human takes only when the paste looks bad. A
// compiled replay is one concrete trace, so that branch is not represented
// here; the assert step at the end is what would trip a gate and hand
// control back to a human if the paste came out wrong.
import { defineWorkflow, step, input } from "../../sdk/ts/index.js";

const WINDOW_WORD = { process: "winword.exe", titlePattern: ".* - Word" };
const WINDOW_EMAIL = { process: "outlook.exe", titlePattern: ".*" };

const SEL_TABLE = [
  { kind: "automation_id", value: "TableGrid1" },
  { kind: "name_role_path", path: [{ role: "document", name: "monthly-report.docx" }, { role: "table", name: "Sales by Region" }] },
  { kind: "ordinal_path", path: [{ role: "window", ordinal: 0 }, { role: "document", ordinal: 0 }, { role: "table", ordinal: 0 }] },
];
const SEL_COMPOSE_BTN = [
  { kind: "automation_id", value: "NewMailButton" },
  { kind: "name_role_path", path: [{ role: "window", name: "Outlook" }, { role: "button", name: "New Email" }] },
];
const SEL_EMAIL_BODY = [
  { kind: "automation_id", value: "MessageBodyEditor" },
  { kind: "name_role_path", path: [{ role: "window", name: "New Message" }, { role: "document", name: "Message body" }] },
  { kind: "ordinal_path", path: [{ role: "window", ordinal: 0 }, { role: "document", ordinal: 0 }] },
];
const SEL_SUBJECT_FIELD = [
  { kind: "automation_id", value: "SubjectEditBox" },
  { kind: "name_role_path", path: [{ role: "window", name: "New Message" }, { role: "edit", name: "Subject" }] },
];
const SEL_TO_FIELD = [
  { kind: "automation_id", value: "ToEditBox" },
  { kind: "name_role_path", path: [{ role: "window", name: "New Message" }, { role: "edit", name: "To" }] },
];
const SEL_SEND_BTN = [
  { kind: "automation_id", value: "SendButton" },
  { kind: "name_role_path", path: [{ role: "window", name: "New Message" }, { role: "button", name: "Send" }] },
];

export default defineWorkflow({
  name: "convert-table-to-email",
  version: "1.0.0",
  description: "Copies a table from a Word document into a new email message and sends it.",
  inputs: {
    word_document: input.filePath({ default: "monthly-report.docx", label: "Word document containing the table" }),
    recipients: input.email({ default: "manager@company.com", label: "Email recipient" }),
    subject: input.text({ default: "Q1 2024 Sales Summary", label: "Email subject line" }),
    message: input.text({ default: "Attached below is this quarter's sales by region.", label: "Message above the table" }),
  },
  steps: [
    // 1-2. Open the Word document and click the table to select it.
    step.click({
      intent: "Click the table to select it",
      window: WINDOW_WORD,
      selectors: SEL_TABLE,
      risk: "read",
    }),
    // 3. Copy the entire table.
    step.key({ intent: "Copy the table", window: WINDOW_WORD, combo: "ctrl+c", risk: "read" }),
    // 4-5. Open the email application and click Compose.
    step.click({
      intent: "Click Compose to start a new email",
      window: WINDOW_EMAIL,
      selectors: SEL_COMPOSE_BTN,
      risk: "write",
    }),
    step.wait({ intent: "Wait for the new message window to open", scope: { window: WINDOW_EMAIL }, timeoutMs: 5000 }),
    // 6-7. Click in the email body and paste the table.
    step.click({
      intent: "Click the email body to place the cursor",
      window: WINDOW_EMAIL,
      selectors: SEL_EMAIL_BODY,
      risk: "read",
    }),
    step.key({ intent: "Paste the table", window: WINDOW_EMAIL, combo: "ctrl+v", risk: "write" }),
    step.wait({ intent: "Wait for the table to render in the message body", scope: { window: WINDOW_EMAIL }, timeoutMs: 3000 }),
    // 10. Type a message explaining what the table contains.
    step.type({
      intent: "Type a message describing the table",
      window: WINDOW_EMAIL,
      selectors: SEL_EMAIL_BODY,
      text: "{message}",
      risk: "write",
    }),
    // 9. Add a subject line for the email.
    step.click({
      intent: "Click the subject field",
      window: WINDOW_EMAIL,
      selectors: SEL_SUBJECT_FIELD,
      risk: "read",
    }),
    step.type({
      intent: "Type the email subject",
      window: WINDOW_EMAIL,
      selectors: SEL_SUBJECT_FIELD,
      text: "{subject}",
      risk: "write",
    }),
    // 11. Enter the recipient email address in the To field.
    step.click({
      intent: "Click the To field",
      window: WINDOW_EMAIL,
      selectors: SEL_TO_FIELD,
      risk: "read",
    }),
    step.type({
      intent: "Type the recipient email address",
      window: WINDOW_EMAIL,
      selectors: SEL_TO_FIELD,
      text: "{recipients}",
      risk: "write",
    }),
    // 12. Click Send.
    step.click({
      intent: "Click Send",
      window: WINDOW_EMAIL,
      selectors: SEL_SEND_BTN,
      risk: "write",
    }),
    step.wait({ intent: "Wait for the message to leave the outbox", scope: { window: WINDOW_EMAIL }, timeoutMs: 5000 }),
    step.assert({
      intent: "Check that the email with the table was sent",
      window: WINDOW_EMAIL,
      expr: {
        op: "not_exists",
        query: { kind: "snapshot_window_title_contains", value: "New Message" },
      },
    }),
  ],
});

export const benchmark = false;
