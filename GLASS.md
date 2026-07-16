# OPERANT MATERIAL SYSTEM: semantic glass

Status: SPEC. A lane pack that runs AFTER the bridge lands and the four defects are
closed. It does not touch the critical path. Do not implement it during engine repair,
bridge freeze, or core wiring; the D-lanes it touches (`main.ts`, `base.css`,
`tokens.ts`) are the same files the bridge work rewrites, and merge churn there costs
more than the polish is worth. (Campaign note: the FINISH packet runs this as Phase D
after Phase B merges, and the truth gate E1 then exercises the kill-switch overlay and
the readout, so it lands before E1 rather than after.)

## 0. Rejected approaches and why (read this before writing any code)

The obvious way to build glass in a Tauri app is wrong for this repo in six specific
ways. Each is a trap a generic implementation walks into:

- Do not use the Tauri v1 config schema. `"tauri": { "windows": [...] }` is v1. This
  repo is Tauri 2.11.x, where the schema is `"app": { "windows": [...] }`.
- Do not use `tauri-plugin-vibrancy` or `app.get_window()`. Neither applies in v2. Use
  the `window-vibrancy` crate (`apply_mica`, `apply_acrylic`) or the built-in window
  `effects` config, and `get_webview_window()`. Verify the exact API against the
  installed 2.11.x docs before writing code.
- Do not write React, JSX, or Tailwind. This UI is vanilla TypeScript with a token
  pipeline and imperative DOM, verified by `getComputedStyle` assertions. There is no
  React and no Tailwind here.
- Do not introduce new identity colors (indigo, emerald, or anything else). Amber
  (`signal`) is the only identity color, and H2 brought the status palette to 3:1
  across 27 axe scans. Raw hex anywhere outside `tokens.ts` fails the raw-hex lint.
- Do not set `transparent: true` or `decorations: false`. That removes the native
  titlebar (you then own drag regions, window controls, and snap layouts) and conflicts
  with Mica, which needs an opaque window with a DWM backdrop. Mica is already wired and
  working. Keep it.
- Do not animate `backdrop-filter`. Animating blur or saturate forces continuous
  re-rasterization of the blur region and will blow the Q2 budgets. Animate opacity or
  transform on a pre-composited overlay instead.

Two ideas from the generic approach ARE worth building, and this spec builds them
properly: material as a state indicator, and the zero-cost instrument readout.

## 1. Thesis

The material carries the same argument the product does. When the model is in the loop,
the surface is alive: refractive, warm, amber-lit, in motion. When the workflow replays
from memory, the surface is still: clear, colorless, high-contrast, sharp-edged. The
model makes things shimmer. Determinism makes them still.

Glass is used in exactly four places and means something in each. Everywhere else stays
solid and calm. A precision instrument that shimmers everywhere is a toy.

## 2. Token additions (all in `ui/src/theme/tokens.ts`; no raw hex anywhere else)

Add a material scale, not new identity colors. Amber (`signal`) remains the only
identity color.

```
material: {
  // in-app glass surfaces, layered over the app's own backdrop
  scrimWeak:    rgba(bg1, 0.55)   // text may sit here only with a solid scrim behind it
  scrimStrong:  rgba(bg1, 0.82)   // default text surface on glass
  blurPanel:    16px              // palette, cards
  blurOverlay:  40px              // kill switch
  satPanel:     140%
  edgeStill:    hairline @ 0.14   // replay / idle: crisp, colorless
  edgeLive:     signal @ 0.32     // explore: warm refractive edge
  glowLive:     signal @ 0.10     // explore only, one level, no spread animation
}
motionGlass: {
  edgeShimmer:  2400ms ease-in-out infinite   // animates an ::after overlay's opacity ONLY
}
```

Contrast rule, non-negotiable: text never sits on raw translucency. Text sits on
`scrimStrong`, whose effective luminance is a known value, so H2's 3:1 status-dot
battery and the axe scans keep passing with a computed background rather than a variable
one.

## 3. Window layer (Tauri v2, keep what works)

- Keep Mica. Keep decorations. Do not go transparent, do not go frameless. The OS
  backdrop (Mica) and in-app glass (backdrop-filter panels) are two different layers and
  both can exist; the sketch conflated them.
- Frameless is a separate, later decision with a real cost (custom titlebar, drag
  regions, window controls, snap layouts, new tests). If it is ever wanted, it gets its
  own lane and its own bar, not a side effect of a CSS change.
- Note for expectations: inside WebView2, `backdrop-filter` blurs the app's own content
  behind an element, not the desktop behind the window. Desktop-behind blur comes from
  the OS backdrop (Mica/Acrylic). Design for in-app glass; that is what Raycast's panels
  actually read as anyway.

## 4. The four glass moments

G1. Command palette. Floating panel, `blurPanel` over the app content, `edgeStill`,
`scrimStrong` behind rows. This is the Raycast read: it is glass because it is floating
above something, not because glass is pretty.

G2. Run viewer material state (the thesis, made physical).

- Explore: panel carries `edgeLive` and `glowLive`; an `::after` overlay shimmers its
  opacity on `edgeShimmer`. The REC chip stays as specified. The surface is warm and
  alive.
- Replay: panel carries `edgeStill`, zero glow, zero motion, higher contrast. The chip
  stays "no AI, exact replay". The surface is cold, sharp, and completely still.
- The transition between the two is the single orchestrated motion moment already
  allowed by design.md. Nothing else animates.

G3. Kill-switch overlay. On the panic chord: `blurOverlay` locks the viewport in one
frame, borders go `danger`, everything under the glass becomes unreadable. This is the
best semantic use of blur in the whole product, because it physically severs the
operator from the automation surface. It must appear within the same 100 ms budget the
freeze itself has, which means the overlay is pre-mounted and hidden, never constructed
on trigger.

G4. Drift patch panel. The broken version sits in a faded, receded glass container; the
proposed patch sits in a live-edged one, inviting approval. Same material grammar as G2:
the thing the model produced is alive, the thing being replaced is inert.

Dashboard, library, settings, wizard, tray: solid. No glass. Calm is the default and the
contrast is the point.

## 5. The instrument readout (the best idea in the sketch, done honestly)

During replay, the run viewer shows a live readout in tabular mono: `MODEL CALLS 0` /
`NETWORK 0 KB`.

This MUST be real telemetry from the core, streamed over the bridge from actual
counters. A hardcoded zero is the same class of falsehood as `simulateDemoRun` and
cannot ship. Wire it to the counters the replay executor already needs for the benchmark.

Add to CLAIMS.md: the claim "replay makes zero model calls" is now displayed in-app, so
it must be backed by a test that asserts the displayed value comes from a measured
counter and that the counter is nonzero on an explore run (proving it is not a constant).

## 6. Performance rules (Q2 budgets stand)

- Never animate `backdrop-filter`, `filter`, or `box-shadow`. Animate opacity and
  transform on a pre-composited overlay element.
- One blur radius per layer. No stacked blurred parents (blur of a blur re-rasterizes
  both).
- Blurred surfaces get `will-change: opacity` only while animating, removed after.
- Budgets unchanged and re-measured after this pack: palette open under 120 ms,
  dashboard render under 200 ms with 100 workflows, cold start under 1.5 s. If glass
  costs more than the budget, the glass loses.

## 7. Accessibility rules (H2's gate stands)

- `prefers-reduced-motion`: all shimmer off, material states differ by edge and contrast
  only.
- `prefers-reduced-transparency`: all glass falls back to solid `bg1`/`bg2` surfaces. The
  state distinction survives entirely without translucency, which is a good test of
  whether the design is actually communicating or just decorating.
- Text contrast is computed against `scrimStrong`, never against a variable backdrop. The
  axe scan and the 3:1 dot battery must stay at zero violations.

## 8. Capture rules (this will save the marketing)

Glass gradients dither badly in GIF's 256-color palette; the beautiful surfaces will band
into mud. Therefore:

- Every glassy screen is captured as MP4 or WebM, not GIF. GitHub renders video in
  markdown; use it.
- GIF is retained only where a short loop on a flat surface is genuinely better, and
  those shots avoid the blurred panels.
- The existing asset verification (non-blank, dimensions, template match) extends to the
  video assets.

## 9. Lanes (dispatch only after the four defects are closed)

- GL1 material-tokens: the token additions, the scrim rule, the reduced-motion and
  reduced-transparency fallbacks. Bar: raw-hex lint green; a11y scan green; every existing
  screen unchanged (glass is opt-in per component).
- GL2 palette-and-runviewer [strong model]: G1 and G2. Bar: explore and replay render
  visibly different materials, verified by computed-style assertions, not by eye; Q2
  budgets re-measured green.
- GL3 killswitch-overlay [safety-adjacent]: G3, pre-mounted, within the 100 ms freeze
  budget. Bar: the overlay is on screen inside the same budget the freeze meets, measured,
  with real input synthesis in flight.
- GL4 drift-panel: G4. Bar: a real drift patch renders in the two-material grammar.
- GL5 instrument-readout: real counters over the bridge, CLAIMS-backed. Bar: displayed
  value provably comes from a measured counter; nonzero on explore, zero on replay.
- GL6 recapture-video: re-shoot the glassy screens as video per section 8. Bar: no
  banding; assets verified; README and site updated.

Ledger: GL4, then GL6 (GIF stays one release), then GL1 shimmer (states differ by edge
only). Never cut: the reduced-transparency fallback, the contrast rule, and the honesty
of the readout.
