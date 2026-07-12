---
name: verify
description: Build/launch/drive recipe for verifying changes to the Operant UI (ui/) at runtime, in a real browser, against the mocked bus. Use before claiming a ui/ change works.
---

# Verifying ui/ changes

The Tauri shell's frontend (`ui/`) renders standalone against a mocked event
bus (`ui/src/bus/mockClient.ts`), so you do not need the Rust backend or a
Tauri build to see it run — a plain Vite dev server is enough.

## Launch

```
cd ui
npm run dev
```

Vite serves on `http://localhost:1420` (fixed port, see `ui/vite.config.ts`).
Open it in the Browser pane (`preview_start` with that URL, or `navigate` if
a tab is already open).

## Reaching app state

- **First load is the onboarding wizard**, a full-screen modal
  (`role="dialog"`) over everything else, focus-trapped
  (`ui/src/styles/focusPreserve.ts`'s `trapFocus`). To get past it without
  driving the whole wizard flow: `localStorage.setItem("operant.wizard.completed", "1")`
  then reload. This mirrors the app's own "never show again on this device"
  logic (`ui/src/main.ts`'s `WIZARD_DONE_KEY`), so it is a faithful shortcut,
  not a hack.
- **Theme**: `localStorage.setItem("operant.ui.theme", "dark"|"light"|"system")`
  then reload, or click `#op-theme-toggle` at runtime (cycles
  dark -> light -> system). `document.documentElement.getAttribute("data-theme")`
  and `getComputedStyle(document.body)` are the fastest way to confirm a
  token value actually reached the page (see Gotchas below for exact dark
  palette RGB values to check against).
- **Nav**: `#op-nav-dashboard` / `#op-nav-library` / `#op-nav-runs` /
  `#op-nav-settings` show/hide `#op-screen-dashboard` / `-library` / `-runs` /
  `-settings` respectively (mutually exclusive `hidden`).

## Gotchas

- **`computer` (coordinate-based click) and `screenshot` were both
  unreliable in this sandbox** during D1 (tokens-and-shell): `screenshot`
  timed out every time, and a coordinate click landed within a button's own
  `getBoundingClientRect()` yet did not fire its handler. `javascript_tool`
  (`element.click()`, which dispatches a real event through real
  `addEventListener` handlers) and `read_page` both worked reliably and are
  a legitimate substitute — `.click()` still exercises the actual running
  app's real code path, it just is not literally simulating mouse hardware.
  If `computer`/`screenshot` are flaky again, do not burn time retrying;
  switch to `javascript_tool` + `read_page`/computed-style assertions.
- `javascript_exec` calls share one page-global scope across calls in the
  same tab: a bare `const results = ...` in one call collides with the same
  name in a later call (`SyntaxError: Identifier already declared`). Wrap
  each snippet in an IIFE, or use distinct variable names.
- Dark is the default theme (`ui/src/theme/tokens.ts`,
  `docs/specs/design.md` section 1). Fresh `localStorage`, OS reporting dark
  (`matchMedia("(prefers-color-scheme: dark)").matches === true` in this
  sandbox's browser) → expect `data-theme="dark"`, body background
  `rgb(16, 17, 20)` (`#101114`), body color `rgb(236, 237, 239)` (`#ecedef`).
  Light theme's equivalents: `rgb(247, 247, 245)` (`#f7f7f5`) /
  `rgb(27, 28, 30)` (`#1b1c1e`).
