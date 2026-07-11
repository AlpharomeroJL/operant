// @advanced
// A small store for the Advanced "connected-tools config" surface
// (docs/specs/ui.md's MCP config; docs/specs/mcp.md). Pure and DOM-free,
// same split as every other state module in ui/src. Real registration of an
// MCP server is a Rust/crates concern, out of this lane's owned path; this
// only tracks the per-tool enabled toggle the Settings-adjacent Advanced
// screen shows.

import { MOCK_CONNECTED_TOOLS, type ConnectedTool } from "./mockTools.ts";

export interface ConnectedToolsStore {
  list(): ConnectedTool[];
  setEnabled(name: string, enabled: boolean): void;
  subscribe(fn: (tools: ConnectedTool[]) => void): () => void;
}

export function createConnectedToolsStore(seed: readonly ConnectedTool[] = MOCK_CONNECTED_TOOLS): ConnectedToolsStore {
  let tools = seed.map((t) => ({ ...t }));
  const listeners = new Set<(tools: ConnectedTool[]) => void>();

  function notify(): void {
    for (const fn of listeners) fn(tools);
  }

  return {
    list: () => tools,
    setEnabled(name, enabled) {
      const idx = tools.findIndex((t) => t.name === name);
      if (idx === -1) return;
      const next = tools.slice();
      next[idx] = { ...next[idx], enabled };
      tools = next;
      notify();
    },
    subscribe(fn) {
      listeners.add(fn);
      return () => listeners.delete(fn);
    },
  };
}
