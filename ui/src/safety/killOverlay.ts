// GLASS.md GL3 (kill-switch overlay): SAFETY, never-cut. A pre-mounted, hidden,
// full-viewport panic surface that the panic chord / tray panic reveals by a
// single attribute toggle, so it lands inside the same sub-100ms freeze budget
// the stop itself meets (GLASS.md section 4, G3). It is built ONCE at startup
// and never constructed on trigger: reveal() is only ever `backdrop.hidden =
// false` on an element that already exists.
//
// Pure DOM, no bus: ui/src/main.ts owns wiring reveal()/hide() to the two panic
// triggers (the kill chord and the tray panic row) and to the core's echoed
// killswitch.engaged / killswitch.released, the same "logic here, glue in
// main.ts" split every other view module in ui/src uses. This file only builds
// the overlay and toggles its visibility, so it runs under plain jsdom in
// ./killOverlay.test.ts and ./killOverlay.accessibility.test.ts.
//
// Material (GLASS.md section 4, G3): the panel wears op-glass + op-glass--overlay
// (the heavier blurOverlay that physically severs the operator from the
// automation surface underneath) plus op-kill-overlay (the danger edge). Both
// the reduced-transparency and reduced-motion fallbacks the .op-glass* classes
// already carry apply, so the severed state survives without translucency (a
// solid, still-danger-edged surface) for a person who asked for either
// (GLASS.md section 7).

/**
 * The kill-switch overlay's default-mode copy. design.md section 4's error rule:
 * say what happened, then one calm line, no apology. "Emergency stop engaged"
 * matches the tray's own panic notification wording so the two read as one
 * event. Clean microcopy: none of these words is a glossary-internal term.
 */
export const killOverlayStrings = {
  title: "Emergency stop engaged",
  body: "Everything Operant was doing has stopped. You are back in control.",
} as const;

export interface KillSwitchOverlay {
  /** The visibility-gated backdrop; its hidden attribute is the only reveal toggle. */
  readonly backdrop: HTMLElement;
  /** The glass panic panel, built once at mount and never rebuilt on trigger. */
  readonly panel: HTMLElement;
  /** Reveal the overlay: a single `hidden = false` toggle, no construction. */
  reveal(): void;
  /** Hide the overlay again (e.g. on the core's killswitch.released echo). */
  hide(): void;
  /** Whether the overlay is currently on screen. */
  revealed(): boolean;
}

/**
 * Build the pre-mounted, hidden kill-switch overlay into `mount`, gated by
 * `backdrop`'s hidden attribute. Call ONCE at startup (ui/src/main.ts), so
 * reveal() on the panic path is only ever an attribute toggle on the
 * already-built panel, well inside the freeze budget (GLASS.md section 4, G3).
 */
export function mountKillSwitchOverlay(backdrop: HTMLElement, mount: HTMLElement): KillSwitchOverlay {
  mount.textContent = "";

  const panel = document.createElement("section");
  panel.className = "op-kill-overlay op-glass op-glass--overlay";
  panel.setAttribute("role", "alertdialog");
  panel.setAttribute("aria-modal", "true");
  panel.setAttribute("aria-labelledby", "op-kill-overlay-title");
  panel.setAttribute("aria-describedby", "op-kill-overlay-body");

  const title = document.createElement("p");
  title.className = "op-kill-overlay__title";
  title.id = "op-kill-overlay-title";
  title.textContent = killOverlayStrings.title;

  const body = document.createElement("p");
  body.className = "op-kill-overlay__body";
  body.id = "op-kill-overlay-body";
  body.textContent = killOverlayStrings.body;

  panel.append(title, body);
  mount.append(panel);

  // Pre-mounted but hidden from the first frame: everything above is built now,
  // so the panic path never constructs anything (GLASS.md section 4, G3).
  backdrop.hidden = true;

  return {
    backdrop,
    panel,
    reveal(): void {
      backdrop.hidden = false;
    },
    hide(): void {
      backdrop.hidden = true;
    },
    revealed(): boolean {
      return backdrop.hidden === false;
    },
  };
}
