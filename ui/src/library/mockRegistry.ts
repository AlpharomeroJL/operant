// A mocked local workflow registry standing in for the real one
// (docs/specs/registry.md: manifests stored locally after install/compile).
// Same seam pattern as ui/src/bus/mockClient.ts (mocks the transport this
// lane does not own) and ui/src/settings/mockStore.ts (mocks persistence):
// contracts/bus_events.md's workflow.compiled/workflow.installed only ever
// carry a manifest_path, never the manifest bytes, so the library screen
// needs *some* stand-in source for the plain summary and grant prose each
// card shows. Swap for a real file-backed registry client later; the shape
// returned here matches contracts/workflow_manifest.schema.json exactly so
// that swap is a same-shape data-source change, not a rendering change.

export interface WorkflowManifest {
  v: 1;
  name: string;
  version: string;
  description: string;
  step_summary: string[];
  inputs_schema: { type: "object"; properties: Record<string, unknown> };
  capabilities: {
    apps?: string[];
    paths?: string[];
    network?: boolean;
    risk_ceiling: "read" | "write" | "destructive";
  };
  dsl: { path: string; hash: string };
}

export interface MockWorkflowRecord {
  manifest: WorkflowManifest;
  /** Action IR-shaped steps (contracts/action_ir.schema.json), for the Explain view and the Advanced DSL/raw-details panes. */
  steps: ReadonlyArray<Record<string, unknown>>;
  publisher?: string;
  signed: boolean;
  dryRunOnly: boolean;
  /**
   * The path start_replay and explain_workflow take (contracts/ipc.md sections
   * 5b/5c). Populated only for records loaded from the real bridge's
   * list_workflows (the shell's DTO carries a runnable/compiled path per
   * workflow); undefined for the seeded demo records below, which run
   * synthetically with no backend. ui/src/library/state.ts falls back to
   * `manifest.dsl.path` when this is absent.
   */
  path?: string;
}

// Seeded demo data so the library renders end to end with no backend process
// running, the same "renders end to end" goal ui/src/bus/mockClient.ts's own
// header comment states. The first entry's capabilities are deliberately the
// literal example from docs/specs/ui.md's grant prompt spec.
const SEED_WORKFLOWS: readonly MockWorkflowRecord[] = [
  {
    manifest: {
      v: 1,
      name: "copy-invoice-total",
      version: "1.0.0",
      description: "Copy the invoice total into the spreadsheet",
      step_summary: ['Click "Downloads"', 'Click "Invoice.pdf"', "Copy the selection", "Paste"],
      inputs_schema: { type: "object", properties: {} },
      capabilities: { paths: ["C:\\Users\\demo\\Downloads"], apps: ["chrome.exe"], network: false, risk_ceiling: "write" },
      dsl: { path: "workflows/copy-invoice-total.ts", hash: "0".repeat(64) },
    },
    steps: [
      { kind: "click", target: { selectors: [{ kind: "name_role_path", path: [{ role: "treeitem", name: "Downloads" }] }] } },
      { kind: "click", target: { selectors: [{ kind: "name_role_path", path: [{ role: "listitem", name: "Invoice.pdf" }] }] } },
      { kind: "key", params: { combo: "ctrl+c" } },
      { kind: "key", params: { combo: "ctrl+v" } },
    ],
    publisher: "demo",
    signed: true,
    dryRunOnly: false,
  },
  {
    manifest: {
      v: 1,
      name: "weekly-report-email",
      version: "1.0.0",
      description: "Email the weekly report",
      step_summary: ['Click "New email"', "Type the report body", 'Click "Send"'],
      inputs_schema: {
        type: "object",
        properties: { recipient: { title: "Send to", type: "string", format: "email", default: "" } },
      },
      capabilities: { apps: ["outlook.exe"], paths: [], network: true, risk_ceiling: "write" },
      dsl: { path: "workflows/weekly-report-email.ts", hash: "1".repeat(64) },
    },
    steps: [
      { kind: "click", target: { selectors: [{ kind: "name_role_path", path: [{ role: "button", name: "New email" }] }] } },
      { kind: "type", params: { text: "This week's numbers are attached.", input_ref: "body" } },
      { kind: "click", target: { selectors: [{ kind: "name_role_path", path: [{ role: "button", name: "Send" }] }] } },
    ],
    publisher: "demo",
    signed: true,
    dryRunOnly: false,
  },
  {
    manifest: {
      v: 1,
      name: "backup-photos",
      version: "1.0.0",
      description: "Back up this month's photos",
      step_summary: ["Wait for the screen to update"],
      inputs_schema: { type: "object", properties: {} },
      capabilities: { apps: [], paths: [], network: false, risk_ceiling: "read" },
      dsl: { path: "workflows/backup-photos.ts", hash: "2".repeat(64) },
    },
    steps: [{ kind: "wait" }],
    signed: false,
    dryRunOnly: true,
  },
];

function placeholderRecord(name: string): MockWorkflowRecord {
  return {
    manifest: {
      v: 1,
      name,
      version: "0.0.0",
      description: name,
      step_summary: [],
      inputs_schema: { type: "object", properties: {} },
      capabilities: { apps: [], paths: [], network: false, risk_ceiling: "read" },
      dsl: { path: "", hash: "0".repeat(64) },
    },
    steps: [],
    signed: false,
    dryRunOnly: true,
  };
}

export interface MockRegistry {
  list(): MockWorkflowRecord[];
  get(name: string): MockWorkflowRecord | undefined;
  /**
   * Registers or refreshes an entry from a workflow.installed/compiled bus
   * event. Keeps whatever manifest is already known for that name (the
   * event never carries manifest bytes, only a path); seeds a bare
   * placeholder for a name never seen before so the card still renders
   * something honest instead of nothing.
   */
  upsert(name: string, patch: Partial<Pick<MockWorkflowRecord, "publisher" | "signed" | "dryRunOnly">>): MockWorkflowRecord;
  /**
   * Replaces the entire set with a fresh list, as loaded from the real bridge's
   * list_workflows (contracts/ipc.md section 5c). Notifies subscribers once.
   * ui/src/library/state.ts calls this when a CommandClient is present, so the
   * cards show the real saved workflows instead of the seeded demo data; the
   * seed stays only for dev/Demo, where no client is wired.
   */
  replaceAll(records: readonly MockWorkflowRecord[]): void;
  subscribe(fn: (records: MockWorkflowRecord[]) => void): () => void;
}

export function createMockRegistry(seed: readonly MockWorkflowRecord[] = SEED_WORKFLOWS): MockRegistry {
  const records = new Map<string, MockWorkflowRecord>(seed.map((r) => [r.manifest.name, r]));
  const listeners = new Set<(records: MockWorkflowRecord[]) => void>();

  function notify(): void {
    const all = Array.from(records.values());
    for (const fn of listeners) fn(all);
  }

  return {
    list: () => Array.from(records.values()),
    get: (name) => records.get(name),
    upsert(name, patch) {
      const existing = records.get(name) ?? placeholderRecord(name);
      const next: MockWorkflowRecord = { ...existing, ...patch };
      records.set(name, next);
      notify();
      return next;
    },
    replaceAll(next) {
      records.clear();
      for (const record of next) records.set(record.manifest.name, record);
      notify();
    },
    subscribe(fn) {
      listeners.add(fn);
      return () => listeners.delete(fn);
    },
  };
}
