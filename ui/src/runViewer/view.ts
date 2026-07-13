// DOM mount for the run viewer, aka the flight recorder (C13, FR-O1;
// docs/specs/design.md section 3, the signature screen; docs/specs/ui.md:
// "run viewer (step list streaming, each row is the plain-English sentence
// plus a status dot, model ON/OFF indicator top right, Stop and Pause
// buttons, intervene text field when paused)"). Pure DOM, no bus: same split
// as ui/src/wizard/view.ts and ui/src/library/view.ts (callbacks in, elements
// out); ./state.ts stays DOM-free so it runs under plain `node --test`.
//
// design.md section 3 adds, on top of the streaming step list this file
// already had: a horizontal filmstrip of redacted step thumbnails that
// auto-follows a live run; two-way scrub sync between a strip frame and its
// step row; an amber REC chip while teaching and a quiet gray "no AI, exact
// replay" chip while running a saved workflow; and a failed safety check
// rendered as an inline card in the list, never a modal.
//
// The accessibility properties this lane's view.ts introduced are preserved:
// the step list is a live region (steps stream in one at a time while a run is
// in progress, so a screen-reader user hears each one arrive), and the
// intervene text field carries keyboard focus and its own text across a
// rebuild (ui/src/styles/focusPreserve.ts) so typing a redirect instruction
// does not lose focus on every keystroke. The filmstrip frames and step rows
// carry the same focus keys, so scrubbing by keyboard keeps focus on the
// frame/row you picked across the rebuild the selection triggers.

import type { RunViewerSnapshot, StepStatus, StepRow, RunChip } from "./state.ts";
import { commonStrings, runViewerStrings } from "../strings/default.ts";
import { captureFocus, restoreFocus, FOCUS_KEY_ATTR } from "../styles/focusPreserve.ts";
import { redactionBars } from "./thumbnails.ts";

export interface RunViewerMountOptions {
  onStop?: () => void;
  onTogglePause?: () => void;
  onIntervene?: (instruction: string) => void;
  /** Scrub to a step by selecting its filmstrip frame or its step row. */
  onSelectStep?: (stepId: string) => void;
  /**
   * H1: the empty state's one specific action, shown only before any run has
   * ever started this session (docs/specs/design.md section 4's copy rule,
   * "Empty states invite one specific action"). Wired in ui/src/main.ts to
   * the same command palette the dashboard and library empty states open.
   */
  onTeach?: () => void;
}

const STEP_STATUS_LABEL: Record<StepStatus, string> = runViewerStrings.stepStatus;

function el<K extends keyof HTMLElementTagNameMap>(tag: K, className?: string, text?: string): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text !== undefined) node.textContent = text;
  return node;
}

/** The amber REC chip while teaching, or the quiet gray no-AI chip for a saved-workflow run (design.md section 3); nothing before a run has started. */
function modeChip(kind: RunChip | null): HTMLElement | null {
  if (!kind) return null;
  if (kind === "rec") {
    const chip = el("span", "op-chip op-chip--rec");
    chip.setAttribute("aria-label", runViewerStrings.recChipAria);
    // design.md section 4: the REC chip tooltip is one of exactly two places
    // the word "AI" is allowed to appear in-app.
    chip.title = runViewerStrings.recChipTooltip;
    const dot = el("span", "op-chip__dot");
    dot.setAttribute("aria-hidden", "true");
    chip.append(dot, el("span", undefined, runViewerStrings.recChip));
    return chip;
  }
  const chip = el("span", "op-chip op-chip--exact", runViewerStrings.replayChip);
  chip.title = runViewerStrings.replayChipTooltip;
  return chip;
}

/**
 * One filmstrip frame: a redacted-thumbnail button whose accessible name is
 * the step's own plain-English sentence. The thumbnail is the redacted
 * screenshot that rode the step's `evt` frame (contracts/ipc.md section 7) when
 * one is present, or the generated placeholder from ./thumbnails.ts when none
 * is (a headless/mock core has no pixels, so nothing can leak). Either way the
 * thumbnail is aria-hidden, with a visually-hidden "redacted" note beside it.
 */
function filmstripFrame(step: StepRow, index: number, active: boolean, onSelectStep?: (id: string) => void): HTMLButtonElement {
  const frame = el("button", "op-filmstrip__frame");
  frame.type = "button";
  frame.dataset.state = step.status;
  frame.dataset.stepId = step.id;
  if (active) frame.dataset.selected = "true";
  frame.setAttribute("aria-pressed", String(active));
  frame.setAttribute("aria-label", runViewerStrings.frameAria(index + 1, step.sentence));
  frame.setAttribute(FOCUS_KEY_ATTR, `run-frame-${step.id}`);

  const thumb = el("span", "op-filmstrip__thumb");
  thumb.setAttribute("aria-hidden", "true");
  if (step.thumb) {
    // A real redacted screenshot rode this step's evt frame (contracts/ipc.md
    // section 7). The producer already redacted and downscaled it (fail-closed:
    // a frame it could not cleanly redact ships no thumbnail at all), so the
    // shell only sizes the image; it never holds raw pixels to scrub here.
    thumb.classList.add("op-filmstrip__thumb--image");
    const img = el("img", "op-filmstrip__image");
    img.src = `data:image/${step.thumb.format};base64,${step.thumb.data_b64}`;
    img.width = step.thumb.w;
    img.height = step.thumb.h;
    img.alt = "";
    img.setAttribute("aria-hidden", "true");
    thumb.append(img);
  } else {
    // No captured pixels exist for this frame: draw the deterministic
    // placeholder, whose bar widths hash only the step id (./thumbnails.ts),
    // never any step content.
    for (const width of redactionBars(step.id)) {
      const bar = el("span", "op-filmstrip__bar");
      bar.style.width = `${width}%`;
      thumb.append(bar);
    }
  }

  const redacted = el("span", "op-visually-hidden", runViewerStrings.thumbnailRedacted);
  const number = el("span", "op-filmstrip__index", String(index + 1));
  number.setAttribute("aria-hidden", "true");

  frame.append(thumb, redacted, number);
  if (onSelectStep) frame.addEventListener("click", () => onSelectStep(step.id));
  return frame;
}

/** The horizontal filmstrip above the step list. Auto-following is a scroll concern handled after mount; the highlight itself is driven by `activeStepId`. */
function filmstrip(snapshot: RunViewerSnapshot, onSelectStep?: (id: string) => void): HTMLElement {
  const strip = el("div", "op-filmstrip");
  strip.setAttribute("role", "group");
  strip.setAttribute("aria-label", runViewerStrings.filmstripLabel);
  snapshot.steps.forEach((step, index) => {
    strip.append(filmstripFrame(step, index, step.id === snapshot.activeStepId, onSelectStep));
  });
  return strip;
}

/** A failed safety check, rendered inline in the list (design.md section 3: "not a modal"). */
function gateCard(): HTMLElement {
  const card = el("div", "op-safety-card");
  card.setAttribute("role", "note");
  card.append(
    el("p", "op-safety-card__title", runViewerStrings.gateFailedTitle),
    el("p", "op-safety-card__body", runViewerStrings.gateFailedBody),
  );
  return card;
}

/** A step row: status dot, plain-English sentence, mono duration; selectable, and carrying an inline gate card when a safety check failed. */
function stepRow(step: StepRow, index: number, active: boolean, onSelectStep?: (id: string) => void): HTMLLIElement {
  const li = el("li", "op-step-item");

  const row = el("button", "op-step-row");
  row.type = "button";
  row.dataset.stepId = step.id;
  if (active) row.dataset.selected = "true";
  row.setAttribute("aria-pressed", String(active));
  row.setAttribute("aria-label", runViewerStrings.frameAria(index + 1, step.sentence));
  row.setAttribute(FOCUS_KEY_ATTR, `run-step-${step.id}`);

  const dot = el("span", "op-status__dot");
  dot.dataset.state = step.status;
  dot.setAttribute("aria-hidden", "true");

  // The dot conveys status by color only; this is its screen-reader equivalent.
  const statusText = el("span", "op-visually-hidden", STEP_STATUS_LABEL[step.status]);
  const label = el("span", "op-step__sentence", step.sentence);
  row.append(dot, statusText, label);

  if (step.durationMs !== undefined) {
    row.append(el("span", "op-step-row__time", runViewerStrings.stepDuration(step.durationMs)));
  }

  if (onSelectStep) row.addEventListener("click", () => onSelectStep(step.id));

  li.append(row);
  if (step.gate) li.append(gateCard());
  return li;
}

/**
 * Mount the run viewer into `container`. Clears the container first so it can
 * be re-mounted on every snapshot change (a new step streaming in, a status
 * change, a scrub selection, Pause toggling), the same rebuild-from-scratch
 * pattern every screen in ui/src uses. Carries keyboard focus across that
 * rebuild (see this file's header comment) so a person typing an intervene
 * instruction, or scrubbing the filmstrip by keyboard, is not knocked back to
 * the top of the page on every keystroke or selection.
 */
export function mountRunViewer(container: HTMLElement, snapshot: RunViewerSnapshot, opts: RunViewerMountOptions = {}): HTMLElement {
  const captured = captureFocus(container);
  container.textContent = "";

  const root = el("section", "op-panel op-run-viewer");
  root.setAttribute("aria-labelledby", "op-run-viewer-heading");

  // Heading plus the mode chip, top right (ui.md's "model ON/OFF indicator top
  // right", in design.md section 3's chip form).
  const head = el("div", "op-run-viewer__head");
  const heading = el("h2", "op-panel__title", runViewerStrings.title);
  heading.id = "op-run-viewer-heading";
  head.append(heading);
  const chip = modeChip(snapshot.runChip);
  if (chip) head.append(chip);
  root.append(head);

  // The plain-language model reading kept alongside the chip; empty before any
  // run, when it collapses to nothing.
  root.append(el("p", "op-run-viewer__mode", snapshot.modelIndicatorLabel));

  // Run state (idle/running/paused/halted/done) is the one thing on this screen
  // that changes without any nearby button press causing it (a scheduled step
  // failing, a run finishing on its own): a live region so it is announced
  // whichever way it changes.
  const status = el("p", "op-status");
  status.setAttribute("aria-live", "polite");
  const statusDot = el("span", "op-status__dot");
  statusDot.dataset.state = snapshot.runState;
  statusDot.setAttribute("aria-hidden", "true");
  status.append(statusDot, el("span", undefined, snapshot.runStateLabel));
  root.append(status);

  // H1: before any run has ever started this session -- not merely a lull
  // between steps, which "idle" alone (with zero steps) always exactly
  // identifies (RunViewerSnapshot's runState never returns to "idle" once a
  // run has begun, ./state.ts's toSnapshot) -- invite the one specific
  // action that produces this screen's very first content (docs/specs/
  // design.md section 4: "Empty states invite one specific action"). Teaching
  // a workflow starts an explore run and switches straight to this screen
  // (ui/src/main.ts's handlePaletteCommit), so this is the same action that
  // fills the screen in, not a dead end.
  if (snapshot.runState === "idle" && snapshot.steps.length === 0) {
    const empty = el("div", "op-empty-state");
    const teach = el("button", "op-button op-button--primary", commonStrings.teachFirstWorkflow);
    teach.type = "button";
    teach.setAttribute(FOCUS_KEY_ATTR, "run-viewer-teach");
    if (opts.onTeach) teach.addEventListener("click", opts.onTeach);
    empty.append(el("p", "op-empty", runViewerStrings.emptyInvite), teach);
    root.append(empty);
  }

  // Filmstrip only once there is something to show; an empty strip would be a
  // bare labeled box before the first step arrives.
  if (snapshot.steps.length > 0) {
    root.append(filmstrip(snapshot, opts.onSelectStep));
  }

  const list = el("ol", "op-step-list");
  // "additions text": announce a newly streamed-in step, and announce an
  // existing step's text changing (pending -> done keeps the same sentence but
  // ./state.ts's upsertStep can also update it), without re-reading the entire
  // list on every single update.
  list.setAttribute("aria-live", "polite");
  list.setAttribute("aria-relevant", "additions text");
  snapshot.steps.forEach((step, index) => {
    list.append(stepRow(step, index, step.id === snapshot.activeStepId, opts.onSelectStep));
  });
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

  // Auto-follow: bring the highlighted frame into view so a live run's newest
  // frame, or the scrubbed-to one, is visible without a manual scroll. A no-op
  // under jsdom (no layout), which is fine: the highlight above is what the
  // tests assert on.
  if (snapshot.activeStepId) {
    const activeFrame = root.querySelector<HTMLElement>(`.op-filmstrip__frame[data-step-id="${snapshot.activeStepId}"]`);
    if (activeFrame && typeof activeFrame.scrollIntoView === "function") {
      try {
        activeFrame.scrollIntoView({ block: "nearest", inline: "nearest" });
      } catch {
        // Older/headless engines may reject the options object; a missed
        // auto-scroll is cosmetic, never worth throwing over.
      }
    }
  }

  restoreFocus(container, captured);
  return root;
}
