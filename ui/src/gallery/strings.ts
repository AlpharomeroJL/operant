// Default-mode strings for the template gallery (docs/specs/registry.md:
// "render the embedded step summary and grants in plain language, require
// approval"). Extra strings beyond ui/src/strings/default.ts, same split as
// ui/src/library/strings.ts. Every word here comes from the user-facing
// column of contracts/microcopy_glossary.json; scripts/microcopy_lint.mjs
// checks that in CI.

export const galleryStrings = {
  title: "Add a workflow",
  empty: "No workflows are available to add right now.",
  install: "Install",
  installed: "Installed",
  previewHeading: "Before you install",
  stepsHeading: "What it will do",
  permissionsHeading: "What it needs",
  trustedNote: (publisher: string) =>
    `${publisher} is already trusted, so this workflow is ready to run right away.`,
  firstTimeNote: (publisher: string) =>
    `This is the first workflow from ${publisher}. It will only preview its steps until you turn it on yourself.`,
  unverifiedNote: "Nobody has checked this workflow yet. It will only preview its steps until you turn it on yourself.",
  installedNotice: (title: string) => `"${title}" was added to your workflows.`,
  cancelled: "Not installed.",
  errorTitle: "This workflow could not be installed.",
  errorWhy: "Its details could not be checked.",
  errorAction: "Try installing it again, or check with whoever shared it with you.",
};
