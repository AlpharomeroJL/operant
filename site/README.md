# Operant Documentation Site

This directory contains the plain static HTML documentation site for Operant, deployed to GitHub Pages from the `gh-pages` branch.

## Structure

- `index.html` - Landing page: hero, what it does, get started, how it works, and how Operant compares
- `style.css` - Shared, hand-written stylesheet; imports `tokens.css` and paints only from its custom properties
- `tokens.css` - Generated file, do not hand-edit; produced from `ui/src/theme/tokens.ts` by `scripts/build_site_tokens.mjs` (repo root)
- `guides/` - Guide pages (install, first-workflow, scheduling, privacy)
- `playground/` - A separate in-browser workflow demo with its own build (package.json, a wasm build, Playwright tests); not part of this static site's build and not currently published

## Building and deploying

The hand-written parts of the site (`index.html`, `style.css`, `guides/`) need zero dependencies and zero build steps beyond regenerating `tokens.css`.

To view locally: run `just site` from the repo root. It regenerates `tokens.css` from `ui/src/theme/tokens.ts` and stages the deployable output at `dist/site/`; open `dist/site/index.html` in a browser, or open `site/index.html` directly since it resolves the same relative paths.

There is no hosted CI for this repo (see the justfile's own header comment): the site is not built or deployed by a GitHub Actions workflow. `just site` produces the deployable output locally; a human publishes it by replacing the `gh-pages` branch's contents with `dist/site/`'s, after pointing GitHub Pages at that branch once (repo Settings -> Pages -> Deploy from a branch -> gh-pages -> / (root)).

## Design

Colors, type, spacing, radius, and motion all come from `ui/src/theme/tokens.ts` (docs/specs/design.md section 2, BINDING) through the generated `tokens.css`. The site works in both light and dark modes, respecting the user's system preference via `prefers-color-scheme`; dark is the default. All links use relative paths and work correctly whether served from the docs root or a subdirectory.

## Contributing

Guide content in `guides/` was written by the D1 lane: each guide is a complete HTML page with navigation and styling already in place.
