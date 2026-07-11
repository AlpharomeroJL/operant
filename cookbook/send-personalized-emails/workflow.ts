// Cookbook workflow: Send personalized emails using a contact list
// Source prose: ../send-personalized-emails.md
// Benchmark: no
//
// The prose offers two paths: a Mail Merge add-in, or sending manually one
// row at a time. Mail Merge is a third-party extension outside the step
// vocabulary, so this compiles the manual path (prose step 8): one
// representative recipient, with placeholders replaced from the contact
// list. Re-running with the next row's inputs covers "repeat for each row."
import { defineWorkflow, step, input } from "../../sdk/ts/index.js";

const WINDOW_EXCEL = { process: "excel.exe", titlePattern: ".* - Excel" };
const WINDOW_EMAIL = { process: "outlook.exe", titlePattern: ".*" };

const SEL_COMPOSE_BTN = [
  { kind: "automation_id", value: "NewMailButton" },
  { kind: "name_role_path", path: [{ role: "window", name: "Outlook" }, { role: "button", name: "New Email" }] },
];
const SEL_TO_FIELD = [
  { kind: "automation_id", value: "ToEditBox" },
  { kind: "name_role_path", path: [{ role: "window", name: "New Message" }, { role: "edit", name: "To" }] },
];
const SEL_SUBJECT_FIELD = [
  { kind: "automation_id", value: "SubjectEditBox" },
  { kind: "name_role_path", path: [{ role: "window", name: "New Message" }, { role: "edit", name: "Subject" }] },
];
const SEL_BODY_FIELD = [
  { kind: "automation_id", value: "MessageBodyEditor" },
  { kind: "name_role_path", path: [{ role: "window", name: "New Message" }, { role: "document", name: "Message body" }] },
  { kind: "ordinal_path", path: [{ role: "window", ordinal: 0 }, { role: "document", ordinal: 0 }] },
];
const SEL_SEND_BTN = [
  { kind: "automation_id", value: "SendButton" },
  { kind: "name_role_path", path: [{ role: "window", name: "New Message" }, { role: "button", name: "Send" }] },
];

export default defineWorkflow({
  name: "send-personalized-emails",
  version: "1.0.0",
  description: "Sends one personalized email built from a contact list row and a placeholder template.",
  inputs: {
    contact_list: input.filePath({ default: "contacts.xlsx", label: "Spreadsheet with names and email addresses" }),
    recipient_name: input.text({ default: "Jordan", label: "Recipient's name from the contact list" }),
    recipient_email: input.email({ default: "jordan@example.com", label: "Recipient's email address" }),
    project_name: input.text({ default: "Atlas Migration", label: "Personalization field from the contact list" }),
    subject: input.text({ default: "Project Approval: Atlas Migration", label: "Email subject line" }),
    email_template: input.text({
      default: "Hi {recipient_name}, your project {project_name} is approved.",
      label: "Message text with placeholders",
    }),
  },
  steps: [
    // 1-2. The contact list and drafted template are read from inputs above.
    step.wait({ intent: "Wait for the contact list spreadsheet to be ready", scope: { window: WINDOW_EXCEL }, timeoutMs: 3000 }),
    // 3-4. Open the email application; Mail Merge is unavailable, send manually.
    step.click({
      intent: "Click Compose to start a new email",
      window: WINDOW_EMAIL,
      selectors: SEL_COMPOSE_BTN,
      risk: "write",
    }),
    step.wait({ intent: "Wait for the new message window to open", scope: { window: WINDOW_EMAIL }, timeoutMs: 5000 }),
    // 8. Replace the placeholders with this recipient's information.
    step.click({ intent: "Click the To field", window: WINDOW_EMAIL, selectors: SEL_TO_FIELD, risk: "read" }),
    step.type({
      intent: "Type the recipient email address",
      window: WINDOW_EMAIL,
      selectors: SEL_TO_FIELD,
      text: "{recipient_email}",
      risk: "write",
    }),
    step.click({ intent: "Click the Subject field", window: WINDOW_EMAIL, selectors: SEL_SUBJECT_FIELD, risk: "read" }),
    step.type({
      intent: "Type the email subject",
      window: WINDOW_EMAIL,
      selectors: SEL_SUBJECT_FIELD,
      text: "{subject}",
      risk: "write",
    }),
    step.click({ intent: "Click the message body", window: WINDOW_EMAIL, selectors: SEL_BODY_FIELD, risk: "read" }),
    step.type({
      intent: "Type the personalized message",
      window: WINDOW_EMAIL,
      selectors: SEL_BODY_FIELD,
      text: "{email_template}",
      risk: "write",
    }),
    // 6. Review the preview before sending.
    step.wait({ intent: "Wait for the personalized message to render", scope: { window: WINDOW_EMAIL }, timeoutMs: 2000 }),
    step.assert({
      intent: "Check that the placeholders were replaced with the recipient's details",
      window: WINDOW_EMAIL,
      expr: {
        op: "contains",
        query: { kind: "snapshot_element_value", role: "document", name: "Message body" },
        value: "{recipient_name}",
      },
    }),
    // 8. Click Send.
    step.click({ intent: "Click Send", window: WINDOW_EMAIL, selectors: SEL_SEND_BTN, risk: "write" }),
    step.wait({ intent: "Wait for the message to leave the outbox", scope: { window: WINDOW_EMAIL }, timeoutMs: 5000 }),
    // 9. Confirm the email was sent.
    step.assert({
      intent: "Check that the personalized email was sent",
      window: WINDOW_EMAIL,
      expr: {
        op: "not_exists",
        query: { kind: "snapshot_window_title_contains", value: "New Message" },
      },
    }),
  ],
});

export const benchmark = false;
