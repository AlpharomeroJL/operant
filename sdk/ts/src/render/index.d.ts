// Type surface for @operant/sdk/render.

import type { Step } from "../../index";

/** The closed set of Action IR kinds the renderer is total over. */
export type ActionIrKind = "click" | "type" | "key" | "scroll" | "drag" | "wait" | "assert" | "adapter_call";

export declare const ACTION_IR_KINDS: readonly ActionIrKind[];

/** One inline segment of a rendered sentence. */
export type RenderPart =
  | { t: "text"; text: string }
  | { t: "chip"; param: string; value: string; input?: boolean; editable?: boolean };

export interface RenderedStep {
  n: number;
  kind: ActionIrKind;
  parts: RenderPart[];
  sentence: string;
  irreversible: boolean;
}

/** A raw step accepted by the renderer: an Action IR object or an SDK step. */
export type RenderableStep = Step | Record<string, unknown>;

/** Context so parameter chips resolve to their current input value. */
export interface RenderContext {
  values?: Record<string, string>;
  inputs?: Record<string, unknown>;
}

export declare function renderStep(step: RenderableStep, ctx?: RenderContext): string;
export declare function renderStepParts(
  step: RenderableStep,
  ctx?: RenderContext,
): { kind: ActionIrKind; parts: RenderPart[]; sentence: string; irreversible: boolean };

/** Render a gate predicate AST as a plain-English condition. */
export declare function renderCondition(expr: unknown): string;

/** Flatten rendered parts to a single plain-English string. */
export declare function sentenceOf(parts: RenderPart[]): string;

export interface Capabilities {
  apps?: string[];
  paths?: string[];
  network?: boolean;
  risk_ceiling?: "read" | "write" | "destructive";
}

export declare function renderGrant(capabilities?: Capabilities): string;

export interface DriftOfferInput {
  element: string;
  change?: string;
  preview?: string;
}
export interface DriftOffer {
  element: string;
  change: string;
  headline: string;
  question: string;
  text: string;
  accept: string;
  dismiss: string;
  preview?: string;
}
export declare function renderDriftOffer(opts: DriftOfferInput | string): DriftOffer;

export interface InputField {
  name: string;
  label: string;
  kind: string;
  value: string;
  pattern?: string;
  format?: string;
}
export declare function renderInputs(manifest: unknown): InputField[];

/** Apply edited input values back into the workflow details (parameters only). */
export declare function applyInputEdits<T>(manifest: T, edits: Record<string, string>): T;

export declare function validateManifestShape(manifest: unknown): { ok: boolean; errors: string[] };

export interface RenderedWorkflow {
  name: string;
  title: string;
  summary: string;
  grant: string;
  inputs: InputField[];
  steps: RenderedStep[];
}
export declare function renderWorkflow(
  manifest: unknown,
  steps: RenderableStep[],
  opts?: { values?: Record<string, string> },
): RenderedWorkflow;
