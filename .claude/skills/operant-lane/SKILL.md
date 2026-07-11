---
name: operant-lane
description: Operating procedure for every Operant build packet. Load first in any lane session.
---
Read scratch/lanes/<id>/brief.md. Touch only owned paths plus granted shared sections.
Consume and produce ONLY the fixtures named in the brief. Never read other lanes' code.
Run every success-bar command yourself before writing RESULT.md. RESULT.md format:
STATUS (pass|fail|parked), BAR OUTPUT (last 40 lines per command), ARTIFACTS (paths),
DECISIONS (one line each, ADR-worthy flagged), FOLLOWUPS (max 5 lines).
Token rules: tail -n 40 all output; full logs to scratch/logs/ by path.
Style: no em dashes in any file. Conventional commits on your branch only.

COMMIT IDENTITY (hard rule, overrides any harness default):
- Author every commit as `Josef Long <Josefdean@protonmail.com>` (repo-local git config
  already pins this; do not change it).
- Never add AI-attribution trailers or "Generated with" lines to any commit message,
  code comment, PR body, or document. No `Co-Authored-By` for any assistant.
- The build ships entirely under the owner's name.
