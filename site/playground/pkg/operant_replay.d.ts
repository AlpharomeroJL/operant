/* tslint:disable */
/* eslint-disable */

/**
 * Replay `workflow_json` (a serialized [`CompiledWorkflow`]) against
 * `page_html` (the fixture webapp's markup) through the real `browser`
 * namespace adapter: a fresh [`FixtureBrowser`] attached to `page_html`
 * stands in for a live CDP-attached tab, so every `adapter_call` step
 * (`namespace: "browser"`) dispatches through
 * [`operant_action::AdapterRegistry`] exactly as it would natively.
 *
 * Returns a JSON object `{ "steps_executed": number, "pre_pass": bool,
 * "post_pass": bool }` on success. On any replay error (a failing gate, an
 * unregistered adapter, an assert step that fails for real against the
 * fixture page) returns `Err` with the error's `Display` text, so the
 * caller can show it rather than silently treating a broken replay as a
 * pass.
 */
export function replay_fixture(workflow_json: string, page_html: string): string;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly replay_fixture: (a: number, b: number, c: number, d: number) => [number, number, number, number];
    readonly __wbindgen_exn_store: (a: number) => void;
    readonly __externref_table_alloc: () => number;
    readonly __wbindgen_externrefs: WebAssembly.Table;
    readonly __wbindgen_malloc: (a: number, b: number) => number;
    readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
    readonly __externref_table_dealloc: (a: number) => void;
    readonly __wbindgen_free: (a: number, b: number, c: number) => void;
    readonly __wbindgen_start: () => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
 * Instantiates the given `module`, which can either be bytes or
 * a precompiled `WebAssembly.Module`.
 *
 * @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
 *
 * @returns {InitOutput}
 */
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
 * If `module_or_path` is {RequestInfo} or {URL}, makes a request and
 * for everything else, calls `WebAssembly.instantiate` directly.
 *
 * @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
 *
 * @returns {Promise<InitOutput>}
 */
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
