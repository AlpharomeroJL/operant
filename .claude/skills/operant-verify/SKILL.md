---
name: operant-verify
description: Orchestrator gate procedure for verifying and merging a packet.
---
Run just gate <id> yourself. Trust nothing in RESULT.md a command can check.
Green: rebase on main, merge, push, tick the checkpoint, queue the successor packet.
Red once: redispatch same tier with the failure log appended. Red twice: one tier up.
Strong tier red twice: park with PARKED.md, ledger, move on. Fix-at-gate only for
imports, paths, version pins; log every fix-at-gate.
Before merging, verify the lane's log carries only `Josef Long <Josefdean@protonmail.com>`
and zero attribution trailers (git log --format='%an %ae%n%(trailers)'); rewrite messages
if a lane violated it.
