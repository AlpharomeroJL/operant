// Advanced-mode surface visibility (docs/specs/ui.md: "Advanced toggle
// revealing: DSL editor pane, raw manifest, audit browser, MCP config").
// Pure predicate over ui/src/state/mode.ts's UiMode, so main.ts's wiring and
// this module's own tests do not need a DOM to prove default mode hides
// every Advanced surface and Advanced mode shows all four.
//
// Not itself marked @advanced: this file's own strings (none) and
// identifiers ("dslEditor" etc.) are English words that describe an
// Advanced-only *feature*, not internal jargon in default-mode copy; the
// microcopy lint only scans quoted string literals, and this module has
// none. It still lives under ui/src/advanced because the feature it gates
// is Advanced-only end to end.
import type { UiMode } from "../state/mode.ts";

export interface AdvancedSurfaceVisibility {
  dslEditor: boolean;
  rawWorkflowDetails: boolean;
  auditBrowser: boolean;
  connectedTools: boolean;
}

export function advancedSurfaceVisibility(mode: UiMode): AdvancedSurfaceVisibility {
  const on = mode === "advanced";
  return { dslEditor: on, rawWorkflowDetails: on, auditBrowser: on, connectedTools: on };
}
