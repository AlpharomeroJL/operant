# Operant Launch Copy

Draft launch-day copy for v1.0.0, written in Wave 1 (packet M1A) before the product
exists, so every measured number and every media file below is a named placeholder.
Two placeholder shapes only: an all-caps underscored token in curly braces for a
number, date, handle, or link, for example `{BENCH_REPLAY_P50_MS}`, and an asset
token in curly braces for a capture asset, for example `{ASSET:04-replay.gif}`.
Nothing else in curly braces is a placeholder: the TypeScript sample in the Show HN
prepared answers uses curly-brace input references such as `{invoice_date}` for the
product's own workflow-input syntax, not a fill-in-later token, and it is called out
there again so it cannot be mistaken for one. Section 6 lists every placeholder in
this file and which Wave 4 packet fills it. Do not post anything out of this file
with an unfilled placeholder token still in it.

Target launch date: `{LAUNCH_DATE}`. Post from: HN as `{HN_USERNAME}`, X as
`{X_HANDLE}`, Mastodon as `{MASTODON_HANDLE}@{MASTODON_INSTANCE}`.

Positioning source: `docs/PRD.md` (problem, thesis, launch cut) and `docs/ROADMAP.md`
(v1.0.0 scope, hook line). Voice source: `.claude/skills/operant-marketing/SKILL.md`.
Nothing below restates those files; it draws on them.

---

## 1. Show HN post

### Title options

All five are under HN's ~80-character guidance (checked at draft time; recount if the
name or thesis line changes).

1. `Show HN: Operant - teach your computer once, it runs forever without the model` (78 chars)
2. `Show HN: Operant, a desktop agent that compiles itself out of the loop` (70 chars)
3. `Show HN: Operant - the model is a compiler, not a runtime` (57 chars)
4. `Show HN: Operant, free local Windows agent, deterministic replay after one demo` (79 chars)
5. `Show HN: Operant - zero-code agent that turns a demo into an auditable script` (77 chars)

Recommendation: lead with 1 or 3. Option 1 sells the non-coder outcome, option 3 is
the purest developer hook. HN's own crowd tends to rank the plain-outcome title higher
before 9am and the thesis title higher in a technical cluster; either is defensible.

### Body

The literal text to submit. Plain text only, no markdown table, since HN does not
render one: the developer section below carries the thesis and a short honest
win/concede list, and points to the full comparison table and the prepared answers
for anyone who asks in the comments.

```text
Teach your computer once. It does it forever. No code. Free.

Operant is a free, open source desktop agent for Windows. Show it a task once, by
demonstration or by voice (copy rows from a supplier portal into a spreadsheet, file
a report in three places, whatever your Monday chore is), and it saves what it
learned as a workflow you can run again with one click or on a schedule. Setup never
touches a terminal: a guided wizard helps you download a free local model, sign in
with a ChatGPT or Claude account you already pay for, paste an access key, or just
try a demo first.

After the first time, Operant does not need the model again. Every later run plays
back from a small file on your own machine: nothing about your screen leaves the
computer, and there is no per-run bill. {ASSET:04-replay.gif} shows the same task
twice, once with the model thinking and once without it. Same result. The second
time is instant.

Every run shows what it is about to do in plain English before it does it, any step
that changes something on your computer can be undone afterward, and one key stops
everything, instantly, no matter what Operant is doing when you press it.
{ASSET:12-killswitch.gif} and {ASSET:10-undo.gif} show both.

That is the whole pitch if you are not a developer. Everything from here down is for
people who want to know how it actually works.

For developers: the thesis is that the model is a compiler, not a runtime.
Exploration should be probabilistic. Execution should not be. A 40-step workflow at
98 percent per-step model accuracy fails about 55 percent of the time in production;
probabilistic execution does not survive multiplication. So Operant explores a task
with a model once, or a few times with correction, then freezes the successful run
into a typed TypeScript file plus a signed manifest, guarded by invariant gates.
Replay after that is deterministic: zero model calls, zero network calls, both
asserted in CI, not just promised.

Compiling does five things in order: normalize (drop retried steps, fold human
corrections into the corrected path), parameterize (a literal becomes a typed input
when it varies across runs or matches a date, amount, path, email, or URL shape),
selectorize (score every stored selector: automation ID 100, name-plus-role path 60
minus 5 per path level, ordinal 20, a vision anchor at 0.85 template-match tolerance
as last resort), insert waits and postcondition asserts from what was actually
observed, then emit the TypeScript file and manifest. Every step carries a
plain-English intent string, and that string is not a developer comment: it is the
same text the plain-English view shows a non-coder, from the same file, with no
translation layer between what a developer reads and what a layman sees.

Gates, not vibes: every workflow runs against precondition, postcondition, and
safety checks in both teaching and replay. A hard safety invariant (credential
fields, payment or delete confirmations) is enforced by the runtime itself, not by
the workflow file, and no workflow can turn it off. When a compiled step fails
because the screen changed, Operant re-grounds that one step, proposes a patch diff,
waits for a human approval, then merges a new version with a changelog entry. The
workflow heals. It never silently mutates. {ASSET:06-drift.gif} is that loop end to
end, and {ASSET:05-gate.png} is a safety halt in plain language.

Trust features that sit below the planner, so no model state can block them: a kill
switch (default hotkey, freeze-to-halt measured at {KILLSWITCH_LATENCY_MS}ms mid-run,
CI gate is under 100ms), a journal-ahead undo log (the inverse is written before the
action runs, so undo is a real replay of real inverses, and anything irreversible is
labeled before you run it, not after), and anchor redaction (password fields and
credential dialogs are blacked out before any pixel touches disk, and a redaction
error blocks the write instead of falling through).

Numbers, not adjectives: compiled replay measured {BENCH_REPLAY_P50_MS}ms p50 per
step ({BENCH_REPLAY_P95_MS}ms p95) against {BENCH_REINFER_P50_MS}ms p50 for the same
task re-inferred every step, across {BENCH_TASK_COUNT} tasks: {BENCH_SPEEDUP_X}x,
{BENCH_REPLAY_SUCCESS_RATE} on unchanged UI. Full methodology, including where the
mock re-inference numbers use recorded latencies instead of a live model, is in
BENCHMARKS.md, regenerated every release and gated in CI, not a one-time screenshot.
{ASSET:07-bench.png}

Bring your own model: 17 named backends (local runners Ollama, llama.cpp server, LM
Studio, vLLM, or any OpenAI-compatible endpoint by base URL; 11 cloud providers by
API key, Anthropic through OpenRouter; or sign in with a ChatGPT or Claude
subscription you already have). Default build makes zero network calls except
signed update checks, and that is air-gap-tested in CI, not asserted in a README.

Also in v1.0.0: a TypeScript SDK over the same file format, a CLI
(run/compile/dry-run/list/install/bench/doctor/explain), MCP in both directions
(Operant serves compiled workflows as MCP tools and consumes external MCP servers as
adapters), a Playwright importer, workflow composition (one workflow calling another
at the intersection of both grant sets), and a signed registry (Ed25519-verified,
unsigned workflows run dry-run only).

Honest, short version of how this compares: Operant wins on determinism, audit
trail, offline replay, drift repair, a genuine zero-code path, undo, and a
sub-100ms kill switch, and it is free. It concedes raw model quality on screens
it has never seen before to dedicated grounding teams, and it does not do mobile.
Full table, checked against each project's own docs: {README_COMPARISON_URL}

What is not here yet, plainly: macOS and Linux perception compile behind the trait
but are not tier 1 (Windows only at v1.0.0), no mobile device control, and one
maintainer.

Repo: https://github.com/AlpharomeroJL/operant (Apache 2.0)
Docs: {DOCS_URL}
Registry: https://github.com/AlpharomeroJL/operant-registry
Download: {DOWNLOAD_URL}

I will read every comment, including "why should I trust a kill switch you wrote
yourself," because that is a fair question and the answer is in the test suite, not
this post.
```

### Prepared answers for likely comments

Reference material, not for posting verbatim. Keep these current as the build
progresses; a wrong claim about a competitor in an HN comment is worse than no claim.

**"How is this different from Simular, UI-TARS Desktop, Open Interpreter, or UFO?"**

| | Operant | Simular | UI-TARS Desktop | Open Interpreter | UFO |
|---|---|---|---|---|---|
| Deterministic, model-free replay | Yes, CI-asserted | No, cloud execution | No, re-infers every step | No, re-infers every step | No, re-infers every step |
| Works fully offline after teaching | Yes | No | Partial | Partial | Partial |
| Self-heals on UI drift, human-approved | Yes | Not documented | Not documented | Not documented | Not documented |
| Full audit trail, hash-chained, exportable | Yes | Not documented | Not documented | Not documented | Not documented |
| No-code path for non-developers | Yes, wizard plus plain-English steps | Yes | No, developer tool | No, developer tool | No, research framework |
| One-click undo | Yes | Not documented | No | No | No |
| Kill switch under 100ms | Yes, latency-tested | Not documented | No | No | No |
| Price | Free, Apache 2.0 | Paid, per-seat | Free, open source | Free, open source | Free, research license |
| Raw exploration-time model quality | Conceding: dedicated grounding teams are likely ahead here | n/a | n/a | n/a | n/a |
| Mobile device control | Conceding: not at v1.0.0 | Not documented | No | No | No |

Checked {COMPARISON_LAST_CHECKED_DATE}. "Not documented" means no public evidence
either way was found, not a claim that the feature is absent. Corrections welcome,
especially anywhere "not documented" turns out to be wrong. Live version, kept
current: {README_COMPARISON_URL}

**"Isn't this just RPA with extra steps?"**

Traditional RPA is hand-scripted from day one: a person builds the automation
selector by selector, and it breaks silently the moment the UI changes, with no
repair path beyond someone going back into the tool by hand. Operant is taught, not
scripted: a model watches the task once, or a few times with correction, and the
compiler writes the script, with alternate selectors (automation ID, name-plus-role,
ordinal, vision anchor) scored by stability, not just the first one that worked. When
the UI drifts, Operant does not fail silently: it re-finds the one broken step,
proposes a patch, and asks a human to approve it before merging. The output looks
like RPA, a deterministic script. How it gets written, and how it survives change, is
the actual product.

**"What does the compiled output look like? Is 'deterministic' just a marketing word?"**

A trimmed real example, from the compiler's own fixture suite, not invented for this
post:

```typescript
export default defineWorkflow({
  name: "notepad-invoice-note",
  version: "1.0.0",
  inputs: {
    invoice_date: input.date({ default: "2026-07-11", label: "Invoice date" }),
    amount: input.currency({ default: "142.50", label: "Amount" }),
  },
  steps: [
    step.click({
      intent: "Click the text editor",
      selectors: [
        { kind: "automation_id", value: "RichEditD2DPT" },
        { kind: "name_role_path", path: [{ role: "window", name: "Untitled - Notepad" }] },
        { kind: "ordinal_path", path: [{ role: "window", ordinal: 0 }] },
      ],
      risk: "read",
    }),
    step.type({ intent: "Type the invoice note", text: "Invoice {invoice_date} total ${amount}", risk: "write" }),
    step.assert({ intent: "Check that the note was written", expr: { op: "matches", regex: "^Invoice \\d{4}-\\d{2}-\\d{2} total \\$\\d+\\.\\d{2}$" } }),
  ],
});
```

(The `{invoice_date}` and `${amount}` above are the workflow's own input
placeholders, part of the product's template syntax, not a LAUNCH.md fill-in-later
token.)

Determinism is enforced two ways, not just claimed: the replay executor links
against a backend-free crate, so a model call during replay is not a runtime setting
that could be flipped on by accident, it is a compile-time impossibility, and CI
asserts zero network calls in the default configuration. {ASSET:03-steps.png} is the
same file rendered as the plain-English steps a non-coder sees: there is no separate,
dumbed-down copy that could drift from the real logic.

---

## 2. The funded-startup reframe post

Longer-form, for a personal blog or as a linked companion to the Show HN post.
Plain text body so it survives being pasted into any platform (blog, dev.to, a HN
self-post, LinkedIn); a markdown-capable destination can add real links to the bare
URLs without changing the words.

### Title options

1. `One founder, no funding, a funded team's scope: how Operant actually got built`
2. `I orchestrated my own build campaign. Here is the honest accounting.`

### Body

```text
Operant is a local-first desktop agent with an accessibility-tree perception engine,
a vision-grounding fallback, a five-pass trajectory compiler, an invariant gate
system, a kill switch tested under 100ms, a journal-ahead undo log, hash-chained
audit export, a local voice stack, a signed workflow registry, a docs site, a
published determinism benchmark, and a zero-code onboarding wizard, all shipped at
v1.0.0, Apache 2.0, by one person. That is normally a funded team's first year: a
founding engineer or two on perception, one on the compiler, one on safety and
audit, one on the UI, one on docs and the registry, someone part-time on the launch
posts. Here is the honest version of how one person actually did it, because a
"solo dev" claim that skips the how is not worth much.

The how: I did not hand-type most of this code. I wrote the PRD, the architecture
doc, and the roadmap myself, then ran a single autonomous build campaign in Claude
Code: up to fifteen concurrent agent sessions, four waves, a five-hour wall-clock
budget. I was the orchestrator, not a contributor. I never wrote feature code
inline. My job was to scope each unit of work small enough to verify (a "packet":
three to six sentences, one owned path, a success bar with exact commands to run),
dispatch it to a tier-matched agent, and refuse to merge anything until I had run
its bar myself and watched it pass. The plan budgeted five hours. It actually took
{CAMPAIGN_ACTUAL_HOURS}. {PACKETS_SHIPPED_COUNT} packets shipped clean,
{PACKETS_PARKED_COUNT} got parked, and the fix-at-gate log (imports, paths, and
version pins only, nothing else, on purpose) has {FIX_AT_GATE_COUNT} entries,
because pretending a five-hour, fifteen-lane build produced zero friction would be
the least honest part of this post.

That process is not a gimmick sitting on top of the product. It is the product's
own argument, applied to building the product. Operant's whole thesis is that a
model should explore probabilistically and then have its output frozen into
something gated, inspectable, and verified, never left to free-run in production. I
built it the same way: every packet had a scope, an owned path, and a checkable bar
before it was allowed to merge. Nothing landed on "looks right." If you do not
trust a kill switch an agent wrote, that is a completely fair instinct, and the
answer is the same one Operant gives its own workflows: check the gate, not the
vibes. The safety suite that proves the kill switch, the undo journal, and the hard
invariants (never type into a credential field, never confirm a payment or delete,
without an explicit human approval, and no workflow file can turn that off) is in
the repo, and it blocks the release if it is red.

What actually shipped, so this reframe has something to check against:
- Perception: Windows accessibility-tree reading plus a vision fallback with
  anchor-based, model-free replay
- Action: input synthesis, plus shell, filesystem, Excel, Word, email, OCR/PDF,
  browser, and MCP adapters
- The compiler: a five-pass pipeline from a taught run to a typed, readable
  TypeScript file, with deterministic replay, zero model calls and zero network
  calls, CI-asserted
- The full drift repair loop: re-ground one step, propose a patch, a human
  approves, versioned merge
- Safety: capability grants, preview-before-run, a hash-chained audit log with
  export, hard safety invariants the runtime enforces that no workflow file can
  disable
- Guardians: a kill switch tested under 100ms, a journal-ahead undo log, credential
  redaction before anything touches disk
- Local voice, speech in and speech out, lazy-loaded so it does not tax the
  graphics-memory budget at idle
- The zero-code layer end to end: onboarding wizard, plain-English workflow view,
  demo mode, template gallery, human-readable error messages with one-click fixes,
  a "check my setup" doctor, a first-run tour, all proven by a release-blocking
  end-to-end test that installs, sets up a model, teaches, saves, runs, and
  schedules a workflow with zero code and zero terminal, budgeted at fifteen
  minutes of simulated interaction and enforced in CI (it actually finished in
  {FIRST_TIMER_MINUTES})
- A signed registry with in-app install, a {COOKBOOK_WORKFLOW_COUNT}-workflow
  cookbook with every sample doc-tested, a deployed docs site, a Playwright
  importer, MCP support in both directions, and a benchmark harness with numbers
  regenerated every release, not screenshotted once and left to rot

What did not ship, just as plainly: macOS and Linux perception compile behind the
trait but are not tier 1 yet, that is a later release on the public roadmap.
Workflow files are TypeScript only; Python emission is a later release too. There
is no mobile device control, and that one is not a roadmap gap, it is a stated
non-goal: this automates the computer in front of you, not a phone. Team sharing
and a paid tier exist only as a flagged-off skeleton, because the core product is
free by design, not by omission (Apache 2.0, not a copyleft license, on purpose:
the distribution is the moat, not the license terms). And the biggest real risk
is not a feature gap at all. It is a solo maintainer, which is written into the
project's own risk log, not something a reader had to go digging for.

Why this is worth reading past the launch-day novelty of "an AI built it": the
interesting part is not that agents wrote code fast. The interesting part is what
happens when you refuse to let them free-run: every packet gated, every merge
verified against a command I ran myself, contracts frozen before implementation
started, fixtures shared instead of lanes reading each other's work. Slow that
process down on paper and it reads like a well-run engineering org, minus the org
chart. That is the actual argument for the product, made by the way the product
got built.

Try it: {DOWNLOAD_URL}. Read the PRD and every decision record:
https://github.com/AlpharomeroJL/operant. It is free, it runs offline, and I would
like to know what breaks.

- Josef
```

---

## 3. Launch thread (X / Mastodon)

Ten posts, numbered for sequencing. Each is written to fit inside X's ~280-character
limit assuming short placeholder values; the count noted after each post is the draft
length with placeholder tokens counted literally. Re-check length once Wave 4 fills
the placeholders, especially any post where a long URL lands, and split a post rather
than truncate the claim it is making. Mastodon has more headroom, so these also work
unedited there. Attach the named asset as native media on the post it is listed
under; do not rely on the GIF rendering from a link preview.

### Post 1/10

```text
Teach your computer once. It does it forever. No code. Free.

Show HN today: Operant, an open source Windows agent that compiles what it learns
into a script that runs without the model. {ASSET:04-replay.gif}

https://github.com/AlpharomeroJL/operant
```

Attach: `{ASSET:04-replay.gif}` (250 chars as drafted)

### Post 2/10

```text
The problem with agents that redo the thinking every run: at 98 percent accuracy
per step, a 40-step task fails about 55 percent of the time. Reliability decays
exactly when a task is long enough to be worth automating.
```

No asset. (219 chars as drafted)

### Post 3/10

```text
Operant explores with a model once, or a few times with correction, then freezes
the successful run into a typed, readable script guarded by safety checks. After
that: zero model calls, zero network calls, both checked in CI.
{ASSET:02-explore.gif} then {ASSET:04-replay.gif}
```

Attach: `{ASSET:02-explore.gif}` and `{ASSET:04-replay.gif}`, ideally as a
before/after pair. (275 chars as drafted)

### Post 4/10

```text
None of this needs code. Setup is a wizard: a free local model, sign in with
ChatGPT or Claude, paste an access key, or just try a demo first. Saved workflows
read as plain-English steps unless you ask for code. {ASSET:00-onboarding.gif}
```

Attach: `{ASSET:00-onboarding.gif}` (237 chars as drafted)

### Post 5/10

```text
Trust features that do not depend on the model behaving, because they sit below
it: one key stops every run instantly, and any step that changed something can be
undone afterward, narrated in plain English. {ASSET:12-killswitch.gif}
{ASSET:10-undo.gif}
```

Attach: `{ASSET:12-killswitch.gif}` and `{ASSET:10-undo.gif}`. (252 chars as drafted)

### Post 6/10

```text
When the app you automated changes a button, Operant does not fail silently. It
re-finds the one step that broke, shows the fix as a diff, and waits for your
approval before merging a new version. {ASSET:06-drift.gif}
```

Attach: `{ASSET:06-drift.gif}` (217 chars as drafted)

### Post 7/10

```text
The proof, not just the pitch: replay measured {BENCH_REPLAY_P50_MS}ms p50 vs
{BENCH_REINFER_P50_MS}ms re-inferring every step ({BENCH_SPEEDUP_X}x),
{BENCH_REPLAY_SUCCESS_RATE} on unchanged UI. Methodology in BENCHMARKS.md,
regenerated every release. {ASSET:07-bench.png}
```

Attach: `{ASSET:07-bench.png}` (271 chars as drafted)

### Post 8/10

```text
Where Operant wins vs Simular, UI-TARS Desktop, Open Interpreter, and UFO:
determinism, audit trail, offline replay, drift repair, zero-code, undo, kill
switch, price. Where it does not: raw model quality on unseen screens, and
mobile. Table: {README_COMPARISON_URL}
```

No asset. (266 chars as drafted)

### Post 9/10

```text
How one person shipped this: I orchestrated a single autonomous build campaign in
Claude Code instead of hand-typing most of it, and gated every merge the way
Operant gates every workflow. Honest writeup, including what ran long:
{REFRAME_POST_URL}
```

No asset. Post this one only after the reframe post (section 2) is live. (248 chars
as drafted)

### Post 10/10

```text
Apache 2.0, local-first, zero telemetry without opt-in. Repo:
https://github.com/AlpharomeroJL/operant. Docs: {DOCS_URL}. Registry:
https://github.com/AlpharomeroJL/operant-registry. Star it, watch the demo, or
check the benchmark methodology.
```

No asset, or optionally `{ASSET:11-timesaved.png}` if it made the cut. (243 chars
as drafted)

---

## 4. Demo shot list

Maps the thirteen capture assets (`operant-capture` skill, packet V1) to what each
one needs to show and where in this file it is used. Capture spec for all thirteen:
max width 800px, under 8MB each, 12fps, two-pass ffmpeg palettegen. If any single
asset is impossible to capture in CI, the fallback is to leave its placeholder token
unresolved and flagged right here in LAUNCH.md, never a broken link in a post: do
not point a post at a file that does not exist.

| # | Asset | Shot | Ledger status | Used in |
|---|---|---|---|---|
| 00 | `{ASSET:00-onboarding.gif}` | Wizard: pick a free local model, watch the download progress bar, land on done. | Never cut | Thread 4/10 |
| 01 | `{ASSET:01-palette.gif}` | Hit the hotkey, type a goal in plain English, watch the agent start. | Never cut | Shot list only |
| 02 | `{ASSET:02-explore.gif}` | Run viewer stepping through a live teach run, model indicator lit ON. | Never cut | Show HN body, Thread 3/10 |
| 03 | `{ASSET:03-steps.png}` | The same run compiled into numbered plain-English steps, Advanced toggle visible but closed. | Never cut | Show HN prepared answers |
| 04 | `{ASSET:04-replay.gif}` | The same task again, instant, model indicator OFF. The money shot. | Never cut | Show HN body, Thread 1/10 and 3/10 |
| 05 | `{ASSET:05-gate.png}` | A safety halt on a payment confirmation dialog, message in plain human language. | Never cut | Show HN body |
| 06 | `{ASSET:06-drift.gif}` | A button gets renamed, Operant asks to update the workflow, human approves, rerun goes green. | Never cut | Show HN body, Thread 6/10 |
| 07 | `{ASSET:07-bench.png}` | The BENCHMARKS.md headline table. | Never cut | Show HN body, Thread 7/10 |
| 08 | `{ASSET:08-gallery.png}` | Template gallery, grants written as plain sentences, one-click install. | At risk, low: cut only after roughly fifteen other ledger items already fall (MEGA_PROMPT section 6) | Shot list only |
| 09 | `{ASSET:09-tray.png}` | The tray icon and menu at rest. | At risk, low, same as 08 | Shot list only |
| 10 | `{ASSET:10-undo.gif}` | A run finishes, "Undo last run," files restored, narrated. | Never cut | Show HN body, Thread 5/10 |
| 11 | `{ASSET:11-timesaved.png}` | Tray showing the estimated time saved this week. | At risk, low, same as 08, second in line after 08 and 09 | Thread 10/10 (optional) |
| 12 | `{ASSET:12-killswitch.gif}` | Mid-run panic hotkey, everything freezes, tray goes red. | Never cut | Show HN body, Thread 1/10 and 5/10 |

Never-cut here means on the same pre-authorized list as the compiler, the gates,
and the v1.0.0 release itself, not a promise I am making unilaterally.

---

## 5. Social preview note

The social card image (`og:image` / `twitter:image`) is not one of the thirteen
numbered assets above. It is a separate composite, `{ASSET:og-preview.png}`, built
by the same capture toolchain but assembled after copy is final so it can carry real
words instead of a generic screenshot in a frame. Owned by packet V4, since it needs
the finished tagline and a guaranteed (never-cut) screenshot, not just the raw
capture output.

Recommended composition, 1200x630 (standard Open Graph size; keep essential content
inside the center ~1200x600 to survive platform cropping):

- A band with the Operant wordmark and the hook line, "Teach your computer once. It
  does it forever." Keep "No code. Free." implied by the rest of the card if space
  is tight; do not shrink the type to fit it in.
- A real product screenshot next to the wordmark, not a mockup. In priority order:
  the final frame of `{ASSET:04-replay.gif}` (the model-OFF replay, guaranteed to
  exist and it is the actual money shot), or `{ASSET:03-steps.png}` (plain-English
  steps, reads clearly even shrunk to a link-preview thumbnail). Do not use
  `{ASSET:11-timesaved.png}` as the base image since it is a low-priority cut
  candidate and may not exist.
- One small corner tag with three checkable claims, not three adjectives: "Free.
  Open source. Runs offline."
- Legible at phone-timeline thumbnail size, roughly 300px wide: one headline, one
  screenshot, no dense paragraph.

Wire the same `{ASSET:og-preview.png}` into both the docs site and the README meta
tags, so a link to either carries the same card.

---

## 6. Placeholders

Every placeholder token used above, grouped by what fills it. "Filled by" names the
Wave 4 packet that is the source of truth for the value, even where packet V4
(`launch-final`) is the one that literally edits this file: V4 assembles LAUNCH.md's
final numbers and links, but the values themselves come from the packet named here.

### Capture assets: filled by V1 (capture)

All thirteen resolve to `assets/<filename>` once captured; see section 4 for the
shot each one needs.

| Placeholder | What goes there |
|---|---|
| `{ASSET:00-onboarding.gif}` | Wizard: pick a model, download progress, done. |
| `{ASSET:01-palette.gif}` | Hotkey, plain-English goal typed, agent starts. |
| `{ASSET:02-explore.gif}` | Run viewer stepping through a live teach run, model indicator ON. |
| `{ASSET:03-steps.png}` | Compiled workflow as numbered plain-English steps, Advanced toggle closed. |
| `{ASSET:04-replay.gif}` | Same task, instant, model indicator OFF. The money shot. |
| `{ASSET:05-gate.png}` | Safety halt on a payment dialog, plain-language message. |
| `{ASSET:06-drift.gif}` | Button moved, update-the-workflow prompt, approve, green rerun. |
| `{ASSET:07-bench.png}` | The BENCHMARKS.md headline table. |
| `{ASSET:08-gallery.png}` | Template gallery, plain-language grants. |
| `{ASSET:09-tray.png}` | Tray icon and menu at rest. |
| `{ASSET:10-undo.gif}` | Run finishes, undo last run, files restored, narrated. |
| `{ASSET:11-timesaved.png}` | Tray showing estimated time saved this week. |
| `{ASSET:12-killswitch.gif}` | Mid-run panic hotkey, everything freezes, tray red. |

### Social asset: filled by V4 (launch-final)

| Placeholder | What goes there |
|---|---|
| `{ASSET:og-preview.png}` | The composited social preview image described in section 5. Produced by the capture toolchain but assembled during V4, not part of V1's thirteen-asset pass. |

### Benchmark numbers: filled by L9B (bench-suite)

| Placeholder | What goes there |
|---|---|
| `{BENCH_REPLAY_P50_MS}` | Compiled replay, median per-step latency, from BENCHMARKS.md (`p50_step_ms`, mode `replay`). |
| `{BENCH_REPLAY_P95_MS}` | Compiled replay, p95 per-step latency (`p95_step_ms`, mode `replay`). |
| `{BENCH_REINFER_P50_MS}` | Median per-step latency for the same tasks re-inferred every step (`p50_step_ms`, mode `reinfer_mock` or `reinfer_real`, whichever BENCHMARKS.md headlines). |
| `{BENCH_SPEEDUP_X}` | `{BENCH_REINFER_P50_MS}` divided by `{BENCH_REPLAY_P50_MS}`, rounded to one decimal. |
| `{BENCH_REPLAY_SUCCESS_RATE}` | Replay success across the suite, in whatever format BENCHMARKS.md publishes (for example `25/25` or `100 percent`). |
| `{BENCH_TASK_COUNT}` | Number of distinct tasks in the published suite (fixture tasks plus cookbook workflows). |
| `{KILLSWITCH_LATENCY_MS}` | Measured kill-switch trigger-to-frozen latency from the guardian latency test (the CI gate is under 100ms; this is the actual measured number). |

### Numbers that must match the README: filled by V2 (readme-final)

| Placeholder | What goes there |
|---|---|
| `{COOKBOOK_WORKFLOW_COUNT}` | Final cookbook workflow count. Target is ten; the ledger allows a cut to six if the clock is short. Must equal whatever the README states. |
| `{COMPARISON_LAST_CHECKED_DATE}` | Date the honest comparison table (Show HN prepared answers, section 1) was last checked against each competitor's own docs. Should match the date the README's comparison table was last checked, since both should be verified together. |
| `{README_COMPARISON_URL}` | Anchor link to the full comparison table in the finished README, referenced from the Show HN body, the prepared answers, and thread post 8/10 instead of repeating the table where it cannot render. |

### Links, identity, and launch numbers: filled by V4 (launch-final)

| Placeholder | What goes there |
|---|---|
| `{DOWNLOAD_URL}` | Primary install / getting-started link (installer download or docs quick-start page; V4's call which). |
| `{DOCS_URL}` | Deployed docs site URL (site deploys in the same wave; V4 confirms it resolves before posts go out). |
| `{HN_USERNAME}` | Account to submit the Show HN post from. |
| `{X_HANDLE}` | Account to post the thread from on X. |
| `{MASTODON_HANDLE}` / `{MASTODON_INSTANCE}` | Account and instance to post the thread from on Mastodon. |
| `{FIRST_TIMER_MINUTES}` | Actual measured duration of the first-timer path E2E on the release artifact (V5 rerun), against the fifteen-minute budget stated in the reframe post. |
| `{CAMPAIGN_ACTUAL_HOURS}` | Actual wall-clock time the build campaign took, against the five-hour budget, from `campaign/checkpoint.md`. |
| `{PACKETS_SHIPPED_COUNT}` | Final count of packets that merged clean, from `campaign/state.json`. |
| `{PACKETS_PARKED_COUNT}` | Final count of packets parked rather than shipped, from `campaign/state.json`. |
| `{FIX_AT_GATE_COUNT}` | Final count of fix-at-gate log entries, from `campaign/checkpoint.md`. |
| `{REFRAME_POST_URL}` | Live URL of the published funded-startup reframe post (section 2), once it has a home. |
| `{LAUNCH_DATE}` | The actual date these posts go out. |

Not a placeholder, stated directly because it is already fixed and Wave 4 has
nothing to fill in: the repo (`https://github.com/AlpharomeroJL/operant`) and the
registry (`https://github.com/AlpharomeroJL/operant-registry`) both exist from Phase
0 and their URLs do not change. The seventeen named model backends, the license
(Apache 2.0), and the product thesis are quoted from the frozen PRD, not measured,
so they are not placeholders either.
