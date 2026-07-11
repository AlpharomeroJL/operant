// Default-mode labels for the plain-English workflow view (ui/src/render).
//
// Every string here must use only user-facing vocabulary from
// contracts/microcopy_glossary.json; scripts/microcopy_lint.mjs enforces it in
// CI by scanning quoted string literals under ui/src. The step sentences, grant
// prose, and drift text themselves come already-rendered from @operant/sdk and
// carry no internal vocabulary either.

export const workflowViewStrings = {
  stepsHeading: "Steps",
  grantHeading: "What this workflow can do",
  detailsHeading: "Details you can change",
  irreversibleBadge: "Can't be undone",
  runButton: "Run",
  scheduleButton: "Schedule",
  emptySteps: "This workflow has no steps yet.",
};
