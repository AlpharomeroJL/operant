// The wizard's guided first task (docs/specs/zero-code.md screen 4): "a
// suggested goal against the fixture web app, narrated explore, ending on one
// button labeled 'Save as workflow'". contracts/fixtures/webapp/index.html is
// the fixture: a one-page invoice form (Customer, Amount, Date, "Save
// invoice").
//
// This file holds the guided task's data only -- its goal, the window it runs
// against, and the Action IR steps it teaches. The run itself is streamed
// through the teach client's start_explore (ui/src/teach/client.ts), the same
// seam every teach entry point uses, so the wizard's guided teach is a real
// invocation of that command and not a second, wizard-private way to stream a
// run. ui/src/wizard/state.ts assembles these into that call.

import type { ActionIR } from "../bus/types.ts";

export const GUIDED_TASK_GOAL = "Fill out a sample invoice on the practice page";

/**
 * The window the guided task is taught against (contracts/ipc.md's
 * start_explore window_process). The guided task runs against the bundled
 * fixture web app (contracts/fixtures/webapp/index.html), so this names that
 * practice page rather than a real foreground process: the guided teach is
 * deliberately sandboxed, nothing of the user's is touched.
 */
export const GUIDED_TASK_WINDOW = "operant-practice-page";

function field(name: string): ActionIR["target"] {
  return { selectors: [{ kind: "name_role_path", path: [{ role: "textbox", name }] }] };
}

function button(name: string): ActionIR["target"] {
  return { selectors: [{ kind: "name_role_path", path: [{ role: "button", name }] }] };
}

/**
 * Action IR fragments (contracts/action_ir.schema.json shape), not
 * hand-written sentences: sdk/ts/src/render turns these into the plain
 * English the guided-task screen and the run viewer both show, from the same
 * renderer every other run in this shell uses.
 */
export const GUIDED_TASK_STEPS: ReadonlyArray<Pick<ActionIR, "kind" | "target" | "params">> = [
  { kind: "type", target: field("Customer"), params: { text: "Acme Co" } },
  { kind: "type", target: field("Amount"), params: { text: "420.00" } },
  { kind: "type", target: field("Date"), params: { text: "2026-01-15" } },
  { kind: "click", target: button("Save invoice") },
];
