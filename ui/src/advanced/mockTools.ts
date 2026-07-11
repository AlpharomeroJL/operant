// @advanced
// A mocked "connected tools" (MCP) list standing in for the real client
// config (docs/specs/mcp.md: "external MCP servers configured in settings
// register their tools as adapters under the mcp: namespace with risk class
// write by default"). Advanced-only data: this file lives under
// ui/src/advanced (exempt from scripts/microcopy_lint.mjs by directory) and
// its jargon is never rendered outside the Advanced surface.

export interface ConnectedTool {
  name: string;
  namespace: string;
  riskClass: "read" | "write" | "destructive";
  enabled: boolean;
}

export const MOCK_CONNECTED_TOOLS: readonly ConnectedTool[] = [
  { name: "filesystem", namespace: "mcp:filesystem", riskClass: "write", enabled: true },
  { name: "browser", namespace: "mcp:browser", riskClass: "write", enabled: false },
];
