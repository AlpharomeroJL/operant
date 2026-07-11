// @advanced
// DOM mounts for the four Advanced surfaces (docs/specs/ui.md: "Advanced
// toggle revealing: DSL editor pane, raw manifest, audit browser, MCP
// config"). Pure DOM, no bus, no store: same split as
// ui/src/render/workflowView.ts. main.ts owns wiring these to
// ./state.ts, ./connectedTools.ts, ui/src/library/mockRegistry.ts, and the
// bus event log, and to toggling each one's `hidden` attribute from
// advancedSurfaceVisibility.

import type { MockWorkflowRecord } from "../library/mockRegistry.ts";
import type { BusEvent } from "../bus/types.ts";
import type { ConnectedTool } from "./mockTools.ts";
import { advancedStrings } from "./strings.ts";
import { dslPreview } from "./dslPreview.ts";

function el<K extends keyof HTMLElementTagNameMap>(tag: K, className?: string, text?: string): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text !== undefined) node.textContent = text;
  return node;
}

export function mountDslEditor(container: HTMLElement, record: MockWorkflowRecord | undefined): void {
  container.textContent = "";
  container.append(el("h3", "op-panel__title", advancedStrings.navDslEditor));
  const text = dslPreview(record);
  if (!text) {
    container.append(el("p", "op-empty", advancedStrings.dslEmpty));
    return;
  }
  const textarea = el("textarea", "op-advanced-panel__dsl");
  textarea.value = text;
  textarea.spellcheck = false;
  textarea.rows = Math.min(20, text.split("\n").length + 1);
  textarea.setAttribute("aria-label", advancedStrings.navDslEditor);
  container.append(textarea);
}

export function mountRawWorkflowDetails(container: HTMLElement, record: MockWorkflowRecord | undefined): void {
  container.textContent = "";
  container.append(el("h3", "op-panel__title", advancedStrings.navRawManifest));
  if (!record) {
    container.append(el("p", "op-empty", advancedStrings.manifestEmpty));
    return;
  }
  container.append(el("pre", "op-advanced-panel__raw", JSON.stringify(record.manifest, null, 2)));
}

export function mountAuditBrowser(container: HTMLElement, events: BusEvent[]): void {
  container.textContent = "";
  container.append(el("h3", "op-panel__title", advancedStrings.navAuditBrowser));
  const pre = el("pre", "op-advanced-panel__raw");
  pre.textContent = events.length ? JSON.stringify(events.slice(-20), null, 2) : advancedStrings.auditEmpty;
  container.append(pre);
}

export interface ConnectedToolsMountOptions {
  onToggle?: (name: string, enabled: boolean) => void;
}

export function mountConnectedTools(container: HTMLElement, tools: ConnectedTool[], opts: ConnectedToolsMountOptions = {}): void {
  container.textContent = "";
  container.append(el("h3", "op-panel__title", advancedStrings.navMcpConfig));
  if (!tools.length) {
    container.append(el("p", "op-empty", advancedStrings.mcpEmpty));
    return;
  }
  const list = el("ul", "op-advanced-panel__tools");
  for (const tool of tools) {
    const item = el("li", "op-advanced-panel__tool");
    const label = el("label");
    const checkbox = el("input");
    checkbox.type = "checkbox";
    checkbox.checked = tool.enabled;
    checkbox.addEventListener("change", () => opts.onToggle?.(tool.name, checkbox.checked));
    label.append(checkbox, document.createTextNode(` ${tool.namespace} (${tool.riskClass})`));
    item.append(label);
    list.append(item);
  }
  container.append(list);
}
