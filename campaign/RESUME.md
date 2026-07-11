# RESUME: how to continue this campaign after any stop

This campaign is crash-safe and usage-limit-safe. If the PC turned off, the window ran
out of usage, or the session ended for any reason, the durable state is entirely in this
git repo. A single `continue` message restarts exactly where it left off.

## The one move

When the user says `continue` (or `continue the operant build`), the orchestrator does:

1. `cd D:/dev/operant`
2. `git fetch` and `git status` to confirm the working tree is clean and `main` is intact.
3. Set `CARGO_TARGET_DIR=D:/dev/operant-target`, then run `just ci` on `main` to confirm
   the foundation is still green. (If red, fix-at-gate before dispatching anything new.)
4. `node campaign/frontier.mjs` to reconcile `campaign/merged/*.ok` against
   `campaign/state.json` and print the dispatchable frontier.
5. Dispatch the frontier packets (worktree-before-launch, tiered agents), gate each with
   `just gate <id>`, merge green ones to `main`, write `campaign/merged/<id>.ok`, commit
   the ledger, and `git push`.
6. Repeat step 4-5 until the frontier is empty, then run the final gate (campaign/MEGA_PROMPT.md section 5).

Nothing else is required from the user. No re-planning, no re-approval.

## Invariants that make this safe

- `main` only ever advances on a green gate, so it is always buildable.
- Lane work lives in `git worktree` `lanes/<id>` on `lane/<id>`; an interrupted lane just
  leaves an incomplete side branch that gets re-dispatched (lane briefs are idempotent).
- `campaign/merged/<id>.ok` is the single source of truth for "done". `frontier.mjs`
  trusts these markers, not memory.
- Every commit is authored `Josef Long <Josefdean@protonmail.com>` with zero AI attribution.
- Bootstrap steps are guarded: repo exists -> skip create; contracts committed -> skip.

## Regenerating the ledger

`node campaign/gen_state.mjs` rewrites `state.json` from the packet table, preserving any
existing per-packet status. Safe to run any time; it never loses merged status (that lives
in the `.ok` markers).
