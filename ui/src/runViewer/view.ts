// DOM mount for the run viewer (C13, FR-O1; docs/specs/ui.md: "run viewer
// (step list streaming, each row is the plain-English sentence plus a
// status dot, model ON/OFF indicator top right, Stop and Pause buttons,
// intervene text field when paused)"). Pure DOM, no bus: same split as
// ui/src/wizard/view.ts and ui/src/library/view.ts (callbacks in, elements
// out); ./state.ts stays DOM-free so it runs under plain `node --test`.
//
// This view.ts is new (X8 app-accessibility): the run viewer previously had
// no view.ts of its own, its DOM was inlined directly in ui/src/main.ts
// (outside this lane's owned paths, see scratch/lanes/X8/RESULT.md's
// followups for the small follow-up main.ts needs to adopt this). It is
// built from scratch with the same accessibility properties this lane added
// to ui/src/wizard/view.ts: the step list is a live region (steps stream in
// one at a time while a run is in progress, so a screen-reader user needs to
// hear each one arrive, not just see it), and the intervene text field
// carries keyboard focus across a rebuild (ui/src/styles/focusPreserve.ts)
// so typing a redirect instruction does not lose focus on every keystroke,
// the same bug this lane fixed on the wizard's access-key field.

import type { RunViewerSnapshot, StepStatus } from "./state.ts";
import { runViewerStrings } from "../strings/default.ts";
import { captureFocus, restoreFocus, FOCUS_KEY_ATTR } from "../styles/focusPreserve.ts";

export interface RunViewerMountOptions {
  onStop?: () => void;
  onTogglePause?: () => void;
  onIntervene?: (instruction: string) => void;
}

const STEP_STATUS_LABEL: Record<StepStatus, string> = runViewerStrings.stepStatus;

function el<K extends keyof HTMLElementTagNameMap>(tag: K, className?: string, text?: string): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text !== undefined) node.textContent = text;
  return node;
}

function stepRow(step: RunViewerSnapshot["steps"][number]): HTMLLIElement {
  const li = el("li", "op-step");

  const dot = el("span", "op-status__dot");
  dot.dataset.state = step.status;
  dot.setAttribute("aria-hidden", "true");

  // The dot alone conveys status by color only; this is the screen-reader
  // equivalent, visually hidden the same way ui/src/main.ts's previous
  // inline version was (.op-visually-hidden, ui/src/styles/base.css).
  const statusText = el("span", "op-visually-hidden", STEP_STATUS_LABEL[step.status]);

  const label = el("span", "op-step__sentence", step.sentence);

  li.append(dot, statusText, label);
  return li;
}

/**
 * Mount the run viewer into `container`. Clears the container first so it
 * can be re-mounted on every snapshot change (a new step streaming in, a
 * status change, Pause toggling), the same rebuild-from-scratch pattern
 * every screen in ui/src uses. Carries keyboard focus across that rebuild
 * (see this file's header comment) so a person typing an intervene
 * instruction is not knocked back to the top of the page on every
 * keystroke.
 */
export function mountRunViewer(container: HTMLElement, snapshot: RunViewerSnapshot, opts: RunViewerMountOptions = {}): HTMLElement {
  const captured = captureFocus(container);
  container.textContent = "";

  const root = el("section", "op-panel");
  root.setAttribute("aria-labelledby", "op-run-viewer-heading");

  const heading = el("h2", "op-panel__title", runViewerStrings.title);
  heading.id = "op-run-viewer-heading";
  root.append(heading);

  const modelIndicator = el("p", undefined, snapshot.modelIndicatorLabel);
  root.append(modelIndicator);

  // Run state (idle/running/paused/halted/done) is the one thing on this
  // screen that changes without any nearby button press causing it (a
  // scheduled step failing, a run finishing on its own): a live region so
  // it is announced whichever way it changes.
  const status = el("p", "op-status");
  status.setAttribute("aria-live", "polite");
  const dot = el("span", "op-status__dot");
  dot.dataset.state = snapshot.runState;
  dot.setAttribute("aria-hidden", "true");
  const statusLabel = el("span", undefined, snapshot.runStateLabel);
  status.append(dot, statusLabel);
  root.append(status);

  const list = el("ol", "op-step-list");
  // "additions text": announce a newly streamed-in step, and announce an
  // existing step's text changing (pending -> done keeps the same sentence
  // but ui/src/runViewer/state.ts's upsertStep can also update it), without
  // re-reading the entire list on every single update.
  list.setAttribute("aria-live", "polite");
  list.setAttribute("aria-relevant", "additions text");
  for (const step of snapshot.steps) list.append(stepRow(step));
  root.append(list);

  const actions = el("div");
  const stopButton = el("button", "op-button", runViewerStrings.stop);
  stopButton.type = "button";
  stopButton.disabled = !snapshot.canStop;
  stopButton.setAttribute(FOCUS_KEY_ATTR, "run-stop");
  if (opts.onStop) stopButton.addEventListener("click", opts.onStop);

  const pauseButton = el("button", "op-button", snapshot.pauseButtonLabel);
  pauseButton.type = "button";
  pauseButton.disabled = !snapshot.canPause;
  pauseButton.setAttribute(FOCUS_KEY_ATTR, "run-pause");
  if (opts.onTogglePause) pauseButton.addEventListener("click", opts.onTogglePause);

  actions.append(stopButton, pauseButton);
  root.append(actions);

  if (snapshot.showIntervene) {
    const form = el("form", "op-palette op-intervene");
    const label = el("label", "op-visually-hidden", runViewerStrings.intervenePlaceholder);
    const input = el("input", "op-palette__input");
    input.id = "op-intervene-input";
    input.type = "text";
    input.autocomplete = "off";
    input.placeholder = runViewerStrings.intervenePlaceholder;
    input.setAttribute(FOCUS_KEY_ATTR, "run-intervene-input");
    label.htmlFor = input.id;
    const submit = el("button", "op-button", runViewerStrings.interveneSubmit);
    submit.type = "submit";
    submit.setAttribute(FOCUS_KEY_ATTR, "run-intervene-submit");
    form.append(label, input, submit);
    form.addEventListener("submit", (event) => {
      event.preventDefault();
      if (!opts.onIntervene) return;
      opts.onIntervene(input.value);
      input.value = "";
    });
    root.append(form);
  }

  container.append(root);
  restoreFocus(container, captured);
  return root;
}
