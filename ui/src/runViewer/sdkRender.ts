// Thin relative import of @operant/sdk's plain-English step renderer (U4A,
// C19, FR-U2/FR-U11: sdk/ts/src/render). ui/package.json declares no runtime
// dependency on @operant/sdk and this workspace has no npm-workspace link
// for it, so a bare "@operant/sdk/render" specifier would not resolve under
// plain `node --test`. Imported by relative path instead, for the same
// reason the sdk's own test suite gives for doing the same thing: no
// install, no network (see sdk/ts/test/render.test.js). Nothing here edits
// the renderer itself; sdk/ts/src/render is out of scope for this lane, same
// as ui/src/render.
import { renderStep, type RenderableStep } from "../../../sdk/ts/src/render/index.js";

export type { RenderableStep };

/**
 * Render one step to its plain-English sentence, or fall back to plain
 * text if the step cannot be rendered. The renderer is documented as total
 * over every real Action IR kind (sdk/ts/test/render-totality.test.js), so
 * the fallback exists only so a step shape this shell never produces itself
 * shows a plain sentence in the run viewer instead of breaking it.
 */
export function renderStepSentence(step: RenderableStep, fallback: string): string {
  try {
    const sentence = renderStep(step);
    return sentence && sentence.trim().length > 0 ? sentence : fallback;
  } catch {
    return fallback;
  }
}
