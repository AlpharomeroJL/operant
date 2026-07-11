// Compiled by Operant from run 01JZFIXTURERUN0000000000
// Goal: Write an invoice note in Notepad and save it
// This file is the canonical compiler OUTPUT shape: declarative, one step per
// statement, plain-English intent on every step, zero model calls at replay.
import { defineWorkflow, step, input } from "@operant/sdk";

export default defineWorkflow({
  name: "notepad-invoice-note",
  version: "1.0.0",
  description: "Writes a dated invoice note into Notepad and saves it.",
  inputs: {
    invoice_date: input.date({ default: "2026-07-11", label: "Invoice date" }),
    amount: input.currency({ default: "142.50", label: "Amount" }),
  },
  steps: [
    // 1. Click the text editor
    step.click({
      intent: "Click the text editor",
      window: { process: "notepad.exe", titlePattern: ".* - Notepad" },
      selectors: [
        { kind: "automation_id", value: "RichEditD2DPT" },
        { kind: "name_role_path", path: [{ role: "window", name: "Untitled - Notepad" }, { role: "document", name: "Text editor" }] },
        { kind: "ordinal_path", path: [{ role: "window", ordinal: 0 }, { role: "document", ordinal: 0 }] },
      ],
      risk: "read",
    }),
    // 2. Type the invoice note
    step.type({
      intent: "Type the invoice note",
      window: { process: "notepad.exe", titlePattern: ".* - Notepad" },
      selectors: [
        { kind: "automation_id", value: "RichEditD2DPT" },
        { kind: "name_role_path", path: [{ role: "window", name: "Untitled - Notepad" }, { role: "document", name: "Text editor" }] },
        { kind: "ordinal_path", path: [{ role: "window", ordinal: 0 }, { role: "document", ordinal: 0 }] },
      ],
      text: "Invoice {invoice_date} total ${amount}",
      risk: "write",
    }),
    // 3. Wait for the screen to update
    step.wait({
      intent: "Wait for the screen to update",
      scope: { window: { process: "notepad.exe", titlePattern: ".* - Notepad" } },
      timeoutMs: 5000,
    }),
    // 4. Save the file
    step.key({
      intent: "Save the file",
      window: { process: "notepad.exe", titlePattern: ".* - Notepad" },
      combo: "ctrl+s",
      risk: "write",
    }),
    // 5. Wait for the screen to update
    step.wait({
      intent: "Wait for the screen to update",
      scope: { window: { process: "notepad.exe", titlePattern: ".* - Notepad" } },
      timeoutMs: 5000,
    }),
    // 6. Check that the note was written
    step.assert({
      intent: "Check that the note was written",
      window: { process: "notepad.exe", titlePattern: ".* - Notepad" },
      expr: {
        op: "matches",
        query: { kind: "snapshot_element_value", role: "document", name: "Text editor" },
        regex: "^Invoice \\d{4}-\\d{2}-\\d{2} total \\$\\d+\\.\\d{2}$",
      },
    }),
  ],
});
