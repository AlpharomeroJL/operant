// @advanced
// Exempt from scripts/microcopy_lint.mjs (same reason ui/src/boot/realSeams.test.ts
// and ui/src/bus/commands.test.ts are): asserts against wire vocabulary (the
// contracts/ipc.md section 5a command names and the model-backend quirk-table
// provider ids), never user-facing UI copy.
//
// Proves the real engine-config seam (ui/src/wizard/engine.ts
// createRealBackendConfigurator): the wizard's engine choice issues a REAL
// configure_backend over coreCall with the right quirk-table provider id, so
// completing a setup path on the real core configures a real backend instead of
// the demo mock. The load-bearing assertions are the provider translation
// (ChatGPT -> openai, on-device -> ollama, access key -> the chosen provider)
// and key safety (the raw key rides api_key to the core, never kept here).
// Pure logic, no DOM, same split as ui/src/boot/realSeams.test.ts.

import { test } from "node:test";
import assert from "node:assert/strict";
import { createRealBackendConfigurator } from "./engine.ts";
import type { CoreCall } from "../boot/realSeams.ts";

interface Call {
  cmd: string;
  args: Record<string, unknown>;
}

/** A spy coreCall that records every command and answers with `answer`. */
function spyCoreCall(answer: (cmd: string) => Promise<unknown> = async () => ({ ok: true })) {
  const calls: Call[] = [];
  const coreCall = (async (cmd: string, args?: Record<string, unknown>): Promise<unknown> => {
    calls.push({ cmd, args: args ?? {} });
    return answer(cmd);
  }) as CoreCall;
  return { coreCall, calls };
}

test("the ChatGPT choice issues configure_backend as provider openai (quirk-table id, not the wizard's product id)", async () => {
  const { coreCall, calls } = spyCoreCall();
  const configurator = createRealBackendConfigurator(coreCall);

  await configurator.configureBackend({ provider: "chatgpt", model: "gpt-4o" });

  assert.equal(calls.length, 1);
  assert.equal(calls[0].cmd, "configure_backend");
  assert.deepEqual(calls[0].args, { provider: "openai", model: "gpt-4o" });
});

test("the on-device choice issues configure_backend as provider ollama, carrying the local endpoint", async () => {
  const { coreCall, calls } = spyCoreCall();
  const configurator = createRealBackendConfigurator(coreCall);

  await configurator.configureBackend({ provider: "local", model: "llama3.1:8b", endpoint: "http://localhost:11434/v1" });

  assert.equal(calls.length, 1);
  assert.equal(calls[0].cmd, "configure_backend");
  assert.deepEqual(calls[0].args, { provider: "ollama", model: "llama3.1:8b", endpoint: "http://localhost:11434/v1" });
});

test("the access-key choice maps to the chosen provider and hands the key to the core as api_key", async () => {
  const { coreCall, calls } = spyCoreCall();
  const configurator = createRealBackendConfigurator(coreCall);

  const SECRET = "sk-ant-super-secret-do-not-leak";
  await configurator.configureBackend({ provider: "claude", model: "claude-sonnet-5", apiKey: SECRET });

  assert.equal(calls.length, 1);
  assert.equal(calls[0].cmd, "configure_backend");
  // Claude -> anthropic, and the raw key rides api_key straight to the core.
  assert.deepEqual(calls[0].args, { provider: "anthropic", model: "claude-sonnet-5", api_key: SECRET });
});

test("no endpoint and no key ride the wire unless present (the core sees exactly the contract keys)", async () => {
  const { coreCall, calls } = spyCoreCall();
  const configurator = createRealBackendConfigurator(coreCall);

  await configurator.configureBackend({ provider: "chatgpt", model: "gpt-4o", apiKey: "   " });

  // A blank key is not a handoff: no api_key key at all, and no endpoint.
  assert.deepEqual(calls[0].args, { provider: "openai", model: "gpt-4o" });
  assert.ok(!("api_key" in calls[0].args), "a blank key must not reach the wire");
  assert.ok(!("endpoint" in calls[0].args), "no endpoint for a hosted provider");
});

test("configureBackend rejects when the core rejects the write (a failed configure is not swallowed)", async () => {
  const coreCall = (async () => {
    throw { code: "refused", message: "bad config", retryable: false };
  }) as CoreCall;
  const configurator = createRealBackendConfigurator(coreCall);

  await assert.rejects(configurator.configureBackend({ provider: "chatgpt", model: "gpt-4o" }));
});

test("probe_backend is genuinely asked and a not_implemented core is surfaced honestly, never a faked green", async () => {
  const calls: Call[] = [];
  const coreCall = (async (cmd: string, args?: Record<string, unknown>): Promise<unknown> => {
    calls.push({ cmd, args: args ?? {} });
    throw { code: "not_implemented", message: "no probe entrypoint yet", retryable: false };
  }) as CoreCall;
  const configurator = createRealBackendConfigurator(coreCall);

  const res = await configurator.probeBackend({ provider: "chatgpt", model: "gpt-4o" });

  assert.equal(calls.length, 1);
  assert.equal(calls[0].cmd, "probe_backend");
  assert.deepEqual(calls[0].args, { provider: "openai", model: "gpt-4o" });
  assert.equal(res.state, "not_implemented");
  assert.notEqual(res.state, "reachable", "a not-yet-implemented probe must never render as reachable");
});

test("a reachable probe result maps through from the core (the probe is wired, not hardcoded)", async () => {
  const coreCall = (async () => ({ reachable: true, detail: "ok" })) as CoreCall;
  const configurator = createRealBackendConfigurator(coreCall);

  const res = await configurator.probeBackend({ provider: "claude", model: "claude-sonnet-5" });
  assert.equal(res.state, "reachable");
});

test("an unreachable probe maps to unreachable, and any other fault degrades to unavailable, never a green", async () => {
  const unreachableCall = (async () => ({ reachable: false, detail: "no route" })) as CoreCall;
  const unreachable = await createRealBackendConfigurator(unreachableCall).probeBackend({ provider: "chatgpt", model: "gpt-4o" });
  assert.equal(unreachable.state, "unreachable");

  const faultCall = (async () => {
    throw { code: "core_unavailable", message: "down", retryable: true };
  }) as CoreCall;
  const fault = await createRealBackendConfigurator(faultCall).probeBackend({ provider: "chatgpt", model: "gpt-4o" });
  assert.equal(fault.state, "unavailable");
  assert.notEqual(fault.state, "reachable");
});
