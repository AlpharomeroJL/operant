# Roadmap: demonstration capture (the "watch you do it once" recorder)

Status: NOT BUILT. This is a v1.x fast-follow, specified here so product copy has
a roadmap anchor to point at and never implies the recorder exists today. Nothing
in the shipping build records a demonstration. If you are writing UI copy, a
README, or a marketing line, treat this whole document as future tense.

## The one distinction this document exists to protect

There are two ways to teach Operant a task. Only the first ships now.

1. **Describe it and it does it (present tense, this release).** You give a goal
   in plain language and name the window; a model drives a live explore run that
   figures the task out on your real desktop, you watch it happen in the flight
   recorder, and when it is done you save it as a workflow. This is the model-
   driven teach path proven live in `docs/evidence/P0-live-engine.md` and wired
   through `start_explore` + `compile_run` (`contracts/ipc.md` sections 5b, 5c)
   by the teach client (`ui/src/teach/client.ts`). The entry points are the
   command palette's submit and the wizard's guided first task.

2. **Watch you do it once (future, this document).** You perform the task
   yourself, by hand, and Operant records what you did, then compiles that single
   demonstration into a workflow. No model proposes the steps; you did. This
   recorder does not exist yet.

Copy must never blur these. The present-tense promise is "describe it and it does
it," NOT "watch you do it." "Show Operant by doing it once," "record me," and
"just do the task and Operant learns it" all describe capability 2 and are false
today. Until this recorder ships, every teach affordance is the describe-it path.

## What the recorder is

A demonstration-capture recorder turns a person performing a task once into the
same recorded trajectory an explore run produces, so it can flow into the exact
same compile step and produce a normal workflow. The output is model-free by
construction: capturing a demonstration needs no planner, and the compiled
workflow it yields replays with no model, the same as any other compiled
workflow (replay stays model-free by crate graph, and this path never introduces
a model, so it strengthens that guarantee rather than weakening it).

It is the model-free sibling of the explore loop: same recorder, same compiler,
same registry, same replay engine, different front half. Explore proposes actions
with a model and executes them; demonstration capture observes actions a human
executes and never proposes anything.

## What it needs (the front half that does not exist)

1. **Input capture.** A low-level listener for the real keyboard and pointer
   input the person generates (the read side of the same Win32 input surface the
   synthesizer drives for replay). It records timing, targets, and key/pointer
   detail, and it must be startable and stoppable by an explicit user gesture,
   never ambient. This is distinct from opt-in watch-and-suggest (C21), which
   detects repetition passively; demonstration capture is a deliberate, bounded
   recording the person starts.

2. **Perception pairing.** A UIA (and, where needed, vision) perception snapshot
   taken at each captured action, so a raw click at a pixel becomes an action
   against an identified element (name/role path or automation id), not a stale
   coordinate. Without this, a demonstration is unreplayable the moment a window
   moves. This is the same grounding the explore loop already does per step; the
   recorder reuses it around human-generated actions instead of model-proposed
   ones.

3. **Raw input to Action IR.** A pass that folds the low-level input stream and
   its paired perception into `contracts/action_ir.schema.json` steps: coalescing
   a burst of keystrokes into one type action, resolving a click to the element
   under it, dropping incidental motion. The compiler and everything downstream
   then treat the result identically to an explored trajectory.

4. **Redaction, fail-closed.** The same decode -> redact -> encode discipline the
   flight recorder already enforces (`crates/recorder/src/redact.rs`, contract
   section 7): a demonstration of a real task will cross sensitive fields, and no
   raw or half-redacted capture may ever reach disk. This is a hard gate on the
   recorder, not a setting.

5. **A UI entry point.** A distinct teach affordance ("Show Operant by doing it
   once" or similar) that starts and stops the recording and then lands on the
   same `compile_run` (Save as workflow) step the explore path already ends on.
   It shares the compile handoff and the library outcome with the present-tense
   flow; only the capture front half is new. Until it ships, this affordance is
   absent, not merely disabled.

## How it reuses what exists

- **`compile_run` is already shared.** The compile step (`contracts/ipc.md` 5c,
  `ui/src/teach/client.ts`'s `compileRun`) takes a run id and produces a saved
  workflow regardless of how that run was produced. A demonstration recorder
  feeds the same command; the library picks the workflow up the same way, on the
  same `workflow.compiled` event.
- **Replay is untouched.** Demonstration-captured workflows replay through
  `operant-replay` with no model and no network, exactly like explored ones. This
  path adds no model anywhere, so the determinism story only gets stronger.
- **The flight recorder is untouched.** A demonstration streams the same run.*
  events; the person watches their own actions land as plain-English rows, then
  saves.

## Sequencing

Fast-follow after the model-driven teach path is solid in the shipping build. It
is deliberately second: describe-it proves the compiler-and-replay differentiator
end to end without a new capture surface, and demonstration capture is a second
front half onto that same proven back half, not a prerequisite for it. It slots
into the v1.x "listening releases" track once real usage says how often people
would rather show a task than describe it.
