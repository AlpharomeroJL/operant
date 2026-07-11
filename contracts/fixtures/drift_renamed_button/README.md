# Drift fixture: renamed button

The fixture web app's "Save invoice" button (automation id `save-btn`) is renamed to
"Store invoice" (automation id `store-btn`) in the drift variant (`webapp/drift.html`).
All stored selectors for the old button miss; the element still exists with the same
role and position, so single-step re-grounding must find it.

- `before.json`: snapshot of `webapp/index.html` (the state the workflow was compiled against).
- `after.json`: snapshot of `webapp/drift.html` (the state replay encounters; drift-eligible).

Expected repair: a patch replacing the button's selectors
(`save-btn` / name "Save invoice") with (`store-btn` / name "Store invoice"),
approval required, version bump on merge. The precondition gate (right page title)
still holds in `after.json`, which is what makes the failure drift-eligible rather
than a wrong-state halt.
