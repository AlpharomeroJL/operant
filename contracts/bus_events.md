# Contract: Bus Events

The typed, versioned pub/sub vocabulary of the Operant runtime (C1). Every component speaks only these events. Append-only in released versions: new topics and new OPTIONAL payload fields may be added; nothing is renamed or removed. Serialization is JSON via serde.

## Envelope

Every event on the bus is wrapped:

```json
{
  "v": 1,
  "seq": 12345,
  "ts": "2026-07-11T12:00:00.000Z",
  "topic": "run.step.executed",
  "payload": { }
}
```

- `v`: envelope version, integer, currently 1.
- `seq`: monotonically increasing per-process sequence number, assigned by the bus.
- `ts`: ISO 8601 UTC, assigned by the bus at publish.
- `topic`: dot-separated topic string from the catalog below.
- `payload`: topic-specific object, versioned by the envelope `v`.

Subscribers match on exact topic or prefix (`run.*`). Delivery is in-process ordered per publisher; cross-process (sidecars) rides the supervisor pipe with the same envelope.

## Topic catalog

### Runs
| Topic | Payload (required fields) | Notes |
|---|---|---|
| run.started | run_id, goal, mode (explore/replay/dry), workflow_name? | |
| run.step.proposed | run_id, step (Action IR object) | explore only, pre-gate |
| run.step.gated | run_id, step_id, gate_kind (pre/post/safety), result (pass/fail), expr? | |
| run.step.executed | run_id, step_id, outcome (ok/failed/retried), ms, grounding | |
| run.step.failed | run_id, step_id, error_id, message | error_id keys the error catalog |
| run.paused | run_id, by (human/system) | |
| run.redirected | run_id, instruction | HITL natural-language redirect |
| run.resumed | run_id | |
| run.halted | run_id, reason (gate/killswitch/human/error), error_id? | |
| run.completed | run_id, outcome (ok/failed), steps, wall_ms | |

### Gates, approvals, escalations
| Topic | Payload | Notes |
|---|---|---|
| gate.escalation | run_id, step_id?, sentence, requires_approval (bool) | sentence is plain language |
| approval.requested | approval_id, run_id, step_id?, proposed_action (Action IR), sentence | |
| approval.granted | approval_id, approver | recorded in the audit chain |
| approval.denied | approval_id, approver | |

### Perception
| Topic | Payload | Notes |
|---|---|---|
| perception.snapshot | snapshot_digest, window, source, element_count, truncated | full snapshot goes to the recorder, not the bus |
| perception.changed | scope, digest_before, digest_after | emitted by wait_until_changed |

### Sidecars and VRAM
| Topic | Payload | Notes |
|---|---|---|
| sidecar.started | name, pid | |
| sidecar.health | name, ok (bool), rss_mb?, vram_mb? | |
| sidecar.crashed | name, exit_code | |
| sidecar.restarted | name, attempt | watchdog |
| vram.request | requester, mb | broker arbitration |
| vram.grant | requester, mb | |
| vram.yield | yielder, mb | e.g. voice yields to vision grounder |

### Workflows
| Topic | Payload | Notes |
|---|---|---|
| workflow.compiled | name, version, manifest_path, dsl_path, source_run_id | |
| workflow.installed | name, version, publisher?, signed (bool), dry_run_only (bool) | |
| workflow.drift.detected | name, step_id, reason (selectors_missed/anchor_below_tolerance) | |
| workflow.patch.proposed | name, patch_id, step_id, diff_path | |
| workflow.patch.approved | name, patch_id, new_version | |
| workflow.patch.rejected | name, patch_id | |

### Scheduler
| Topic | Payload | Notes |
|---|---|---|
| trigger.fired | trigger_id, kind (cron/file/window/email), workflow_name, input? | |
| schedule.enqueued | run_id, workflow_name, trigger_id? | |
| schedule.rejected | workflow_name, reason (mode_not_replay/scope_conflict) | typed refusal, tested |

### Guardian
| Topic | Payload | Notes |
|---|---|---|
| killswitch.engaged | at_ms | tray red; all input synthesis frozen |
| killswitch.released | run_id? | explicit human resume, per run |
| undo.previewed | run_id, entries (count), irreversible (count) | |
| undo.applied | run_id, restored (count), narration (array of sentences) | |

### Doctor, metrics, suggestions
| Topic | Payload | Notes |
|---|---|---|
| doctor.finding | finding_id, severity (info/warn/error), what, why, action, fix_command? | plain-language triple |
| doctor.fixed | finding_id | |
| metrics.week.rolled | week, minutes_saved_total | |
| suggestion.offered | suggestion_id, pattern_digest, occurrences | watch-and-suggest, opt-in only |
| suggestion.accepted | suggestion_id | seeds a supervised explore run |
| suggestion.dismissed | suggestion_id | |

### Config

| Topic | Payload | Notes |
|---|---|---|
| config.changed | key, value, old_value? | published by the config store on every set when a bus is attached |

### Voice

| Topic | Payload | Notes |
|---|---|---|
| voice.intent | source, text | recognized speech intent from the voice sidecar, routed to the palette |

## Versioning rules

1. The envelope `v` bumps only on envelope shape change (never in a released version).
2. Payload evolution: add optional fields only. A consumer must ignore unknown fields.
3. New topics may be added freely; consumers subscribe by explicit topic or prefix and must not crash on unknown topics.
4. Breaking need: ADR, envelope version bump, fixtures in both versions (per operant-contracts skill).
