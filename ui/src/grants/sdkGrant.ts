// Thin relative import of @operant/sdk's grant-prose renderer (U4A,
// sdk/ts/src/render). Same import convention as ui/src/runViewer/sdkRender.ts:
// ui/package.json declares no runtime dependency on @operant/sdk and this
// workspace has no npm-workspace link for it, so a bare "@operant/sdk/render"
// specifier would not resolve under plain `node --test`. Imported by relative
// path instead, same reason the SDK's own test suite gives for doing the
// same thing: no install, no network.
//
// ui/src/render/workflowView.ts already renders grant prose as one section of
// the full workflow view (mountWorkflowView). This is the same renderer used
// standalone for the Grant prompt screen itself (docs/specs/ui.md: "grant
// prompt (sentence list plus Allow/Deny)"), which shows only the permission
// sentence(s), not the rest of the workflow.
import { renderGrant, type Capabilities } from "../../../sdk/ts/src/render/index.js";

export type { Capabilities };

const FALLBACK_SENTENCE = "This workflow needs permission, but what it asked for could not be read.";

/**
 * The plain sentence(s) a grant prompt shows before Allow/Deny. Returns a
 * list: today renderGrant always combines every capability into one sentence
 * ("This workflow can read files in Downloads and control Chrome."), but the
 * list shape leaves room for a future per-capability breakdown without a
 * call-site change. Never throws: a step this shell cannot describe falls
 * back to a plain, still-honest sentence, the same defensive shape as
 * ui/src/runViewer/sdkRender.ts's renderStepSentence.
 */
export function renderGrantSentences(capabilities?: Capabilities): string[] {
  try {
    const sentence = renderGrant(capabilities);
    return sentence && sentence.trim().length > 0 ? [sentence] : [FALLBACK_SENTENCE];
  } catch {
    return [FALLBACK_SENTENCE];
  }
}
