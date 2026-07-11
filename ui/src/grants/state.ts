// The grant prompt (docs/specs/ui.md: "grant prompt (sentence list plus
// Allow/Deny)"; docs/specs/registry.md: "render the embedded step summary
// and grants in plain language, require approval"). A local yes/no decision
// the user makes before a workflow with permissions runs or installs. Pure
// and DOM-free, same split as ui/src/runViewer/state.ts, so it runs under
// plain `node --test`.
//
// Deliberately not bus-coupled: contracts/bus_events.md's approval.requested/
// granted/denied are tied to one in-flight step mid-run (proposed_action is
// required); this instead gates a whole workflow before anything starts, so
// the caller decides what Allow actually does next (start a run, install a
// workflow, ...) via onAllow/onDeny, the same callback-based seam
// ui/src/render/workflowView.ts uses for its drift card.

import { renderGrantSentences, type Capabilities } from "./sdkGrant.ts";
import { grantPromptStrings } from "../strings/default.ts";

export type GrantStatus = "pending" | "allowed" | "denied";

export interface GrantPromptSnapshot {
  title: string;
  sentences: string[];
  status: GrantStatus;
  allowLabel: string;
  denyLabel: string;
}

export interface GrantPromptOptions {
  onAllow?: () => void;
  onDeny?: () => void;
}

export interface GrantPrompt {
  getSnapshot(): GrantPromptSnapshot;
  subscribe(fn: (snap: GrantPromptSnapshot) => void): () => void;
  allow(): void;
  deny(): void;
}

export function createGrantPrompt(capabilities?: Capabilities, opts: GrantPromptOptions = {}): GrantPrompt {
  const sentences = renderGrantSentences(capabilities);
  let status: GrantStatus = "pending";
  const listeners = new Set<(snap: GrantPromptSnapshot) => void>();

  function snapshot(): GrantPromptSnapshot {
    return {
      title: grantPromptStrings.title,
      sentences,
      status,
      allowLabel: grantPromptStrings.allow,
      denyLabel: grantPromptStrings.deny,
    };
  }

  function emit(): void {
    const snap = snapshot();
    for (const fn of listeners) fn(snap);
  }

  return {
    getSnapshot: snapshot,
    subscribe(fn) {
      listeners.add(fn);
      return () => listeners.delete(fn);
    },
    allow() {
      if (status !== "pending") return;
      status = "allowed";
      emit();
      opts.onAllow?.();
    },
    deny() {
      if (status !== "pending") return;
      status = "denied";
      emit();
      opts.onDeny?.();
    },
  };
}
