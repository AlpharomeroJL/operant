// Cookbook workflow: Reply to a canned customer email
// Source prose: ../reply-canned-customer-email.md
// Benchmark: no
//
// One representative email (prose steps 8-9, "repeat for each similar
// email", is a re-run of this workflow against the next inbox item, not a
// loop inside the compiled trace).
import { defineWorkflow, step, input } from "../../sdk/ts/index.js";

const WINDOW_EMAIL = { process: "outlook.exe", titlePattern: ".*" };

const SEL_FIRST_MESSAGE = [
  { kind: "automation_id", value: "MessageListItem_0" },
  { kind: "name_role_path", path: [{ role: "list", name: "Inbox" }, { role: "list_item", ordinal: 0 }] },
];
const SEL_REPLY_BTN = [
  { kind: "automation_id", value: "ReplyButton" },
  { kind: "name_role_path", path: [{ role: "window", name: "Outlook" }, { role: "button", name: "Reply" }] },
];
const SEL_REPLY_BODY = [
  { kind: "automation_id", value: "MessageBodyEditor" },
  { kind: "name_role_path", path: [{ role: "window", name: "RE:" }, { role: "document", name: "Message body" }] },
  { kind: "ordinal_path", path: [{ role: "window", ordinal: 0 }, { role: "document", ordinal: 0 }] },
];
const SEL_SEND_BTN = [
  { kind: "automation_id", value: "SendButton" },
  { kind: "name_role_path", path: [{ role: "window", name: "RE:" }, { role: "button", name: "Send" }] },
];

export default defineWorkflow({
  name: "reply-canned-customer-email",
  version: "1.0.0",
  description: "Replies to a customer email with a canned message, personalized with the customer's name.",
  inputs: {
    customer_name: input.text({ default: "Alex", label: "Customer name from their email signature" }),
    message_template: input.text({
      default: "Thank you for contacting us. Your request has been received and will be handled within 24 hours.",
      label: "Canned reply text",
    }),
  },
  steps: [
    // 1-2. Open the inbox and find the first email to reply to.
    step.click({
      intent: "Click the first email in the inbox",
      window: WINDOW_EMAIL,
      selectors: SEL_FIRST_MESSAGE,
      risk: "read",
    }),
    // 3. Click Reply.
    step.click({
      intent: "Click Reply",
      window: WINDOW_EMAIL,
      selectors: SEL_REPLY_BTN,
      risk: "write",
    }),
    step.wait({ intent: "Wait for the reply window to open", scope: { window: WINDOW_EMAIL }, timeoutMs: 4000 }),
    // 4-5. Type the canned response, addressed to the customer by name.
    step.click({
      intent: "Click the reply body to place the cursor",
      window: WINDOW_EMAIL,
      selectors: SEL_REPLY_BODY,
      risk: "read",
    }),
    step.type({
      intent: "Type the personalized canned reply",
      window: WINDOW_EMAIL,
      selectors: SEL_REPLY_BODY,
      text: "Hi {customer_name}, {message_template}",
      risk: "write",
    }),
    step.wait({ intent: "Wait for the reply text to render", scope: { window: WINDOW_EMAIL }, timeoutMs: 2000 }),
    // 6-7. Read the reply once, then click Send.
    step.click({
      intent: "Click Send",
      window: WINDOW_EMAIL,
      selectors: SEL_SEND_BTN,
      risk: "write",
    }),
    step.wait({ intent: "Wait for the reply to leave the outbox", scope: { window: WINDOW_EMAIL }, timeoutMs: 5000 }),
    // 10. Delete the original email now that it has been handled.
    step.click({
      intent: "Click the original email in the inbox",
      window: WINDOW_EMAIL,
      selectors: SEL_FIRST_MESSAGE,
      risk: "read",
    }),
    step.key({ intent: "Delete the handled email", window: WINDOW_EMAIL, combo: "delete", risk: "destructive" }),
    step.assert({
      intent: "Check that the reply was sent and the original email was handled",
      window: WINDOW_EMAIL,
      expr: {
        op: "not_exists",
        query: { kind: "snapshot_window_title_contains", value: "RE:" },
      },
    }),
  ],
});

export const benchmark = false;
