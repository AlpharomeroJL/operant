// Type surface for @operant/sdk. A compiled workflow.ts type-checks against
// these declarations: the shapes here accept exactly what the compiler emits
// (see contracts/fixtures/workflow_notepad/workflow.ts).

/** Risk class of a step. Mirrors operant_ir::RiskClass. */
export type Risk = "read" | "write" | "destructive";

/** A window match: process image name and/or a title regex. */
export interface WindowMatch {
  process?: string;
  titlePattern?: string;
}

/** One segment of a name+role selector path. */
export interface NameRoleSeg {
  role: string;
  name: string;
}

/** One segment of an ordinal selector path. */
export interface OrdinalSeg {
  role: string;
  ordinal: number;
}

/** A selector alternative. Replay tries selectors in list order. */
export type Selector =
  | { kind: "automation_id"; value: string }
  | { kind: "name_role_path"; path: NameRoleSeg[] }
  | { kind: "ordinal_path"; path: OrdinalSeg[] }
  | { kind: "css"; value: string };

/** A gate predicate AST node (data, never strings-of-code). */
export interface GateExpr {
  op: string;
  [key: string]: unknown;
}

// ---- inputs -----------------------------------------------------------------

export type InputType = "date" | "currency" | "text" | "file_path" | "email" | "url";

export interface InputOptions {
  default?: string;
  label?: string;
}

export interface InputDescriptor extends InputOptions {
  type: InputType;
}

export interface InputBuilders {
  date(opts?: InputOptions): InputDescriptor;
  currency(opts?: InputOptions): InputDescriptor;
  text(opts?: InputOptions): InputDescriptor;
  filePath(opts?: InputOptions): InputDescriptor;
  email(opts?: InputOptions): InputDescriptor;
  url(opts?: InputOptions): InputDescriptor;
}

export declare const input: InputBuilders;

// ---- steps ------------------------------------------------------------------

export interface ClickOptions {
  intent: string;
  window?: WindowMatch;
  selectors?: Selector[];
  risk?: Risk;
}

export interface TypeOptions extends ClickOptions {
  text: string;
}

export interface KeyOptions {
  intent: string;
  window?: WindowMatch;
  combo: string;
  risk?: Risk;
}

export interface ScrollOptions {
  intent: string;
  window?: WindowMatch;
  direction: "up" | "down" | "left" | "right";
  amount?: number;
  risk?: Risk;
}

export interface WaitOptions {
  intent: string;
  scope?: { window?: WindowMatch };
  timeoutMs?: number;
}

export interface AssertOptions {
  intent: string;
  window?: WindowMatch;
  expr: GateExpr;
}

export type Step =
  | ({ kind: "click" } & ClickOptions)
  | ({ kind: "type" } & TypeOptions)
  | ({ kind: "key" } & KeyOptions)
  | ({ kind: "scroll" } & ScrollOptions)
  | ({ kind: "wait" } & WaitOptions)
  | ({ kind: "assert" } & AssertOptions);

export interface StepBuilders {
  click(opts: ClickOptions): { kind: "click" } & ClickOptions;
  type(opts: TypeOptions): { kind: "type" } & TypeOptions;
  key(opts: KeyOptions): { kind: "key" } & KeyOptions;
  scroll(opts: ScrollOptions): { kind: "scroll" } & ScrollOptions;
  wait(opts: WaitOptions): { kind: "wait" } & WaitOptions;
  assert(opts: AssertOptions): { kind: "assert" } & AssertOptions;
}

export declare const step: StepBuilders;

// ---- workflow ---------------------------------------------------------------

export interface WorkflowConfig {
  name: string;
  version: string;
  description?: string;
  inputs?: Record<string, InputDescriptor>;
  steps: Step[];
}

export declare function defineWorkflow(config: WorkflowConfig): WorkflowConfig;
