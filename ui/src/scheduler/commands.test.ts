import { test } from "node:test";
import assert from "node:assert/strict";
import {
  createUnavailableSchedulerCommands,
  isNotImplemented,
  NOT_IMPLEMENTED,
  TRIGGER_KIND_CRON,
  type CommandResult,
} from "./commands.ts";

// The honest not-implemented path (contracts/ipc.md section 5g): a build with
// no core trigger store MUST answer both scheduler commands with error code
// `not_implemented`. These tests pin that the default surface does exactly
// that, and never invents a trigger id or a trigger list.

test("upsert_trigger answers not_implemented: no trigger store is wired, so no schedule is created", async () => {
  const scheduler = createUnavailableSchedulerCommands();

  const res = await scheduler.upsertTrigger({
    kind: TRIGGER_KIND_CRON,
    workflow_name: "weekly-report-email",
    spec: "0 9 * * 1-5",
    enabled: true,
  });

  assert.equal(res.ok, false, "there is no trigger store, so this cannot succeed");
  assert.equal(res.ok === false && res.error.code, NOT_IMPLEMENTED);
  assert.equal(res.ok === false && res.error.retryable, false, "not_implemented never becomes true by retrying");
  assert.ok(res.ok === false && res.error.message.length > 0, "the refusal carries a plain-language message");
});

test("list_triggers answers not_implemented: it never fabricates a list of upcoming triggers", async () => {
  const scheduler = createUnavailableSchedulerCommands();

  const res = await scheduler.listTriggers();

  assert.equal(res.ok, false);
  assert.equal(res.ok === false && res.error.code, NOT_IMPLEMENTED);
  // Crucially there is no `result` array to read: the caller cannot mistake an
  // absent store for an empty-but-working one.
  assert.ok(!("result" in res));
});

test("isNotImplemented distinguishes the reserved-but-unwired refusal from other outcomes", () => {
  const notImpl: CommandResult<never> = { ok: false, error: { code: NOT_IMPLEMENTED, message: "x", retryable: false } };
  const otherError: CommandResult<never> = { ok: false, error: { code: "internal", message: "boom", retryable: true } };
  const success: CommandResult<{ trigger_id: string }> = { ok: true, result: { trigger_id: "t1" } };

  assert.equal(isNotImplemented(notImpl), true);
  assert.equal(isNotImplemented(otherError), false, "a real failure is not the same as 'not available yet'");
  assert.equal(isNotImplemented(success), false);
});
