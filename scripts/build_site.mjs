// Stages the deployable docs site into dist/site/, ready to publish as the
// root of the gh-pages branch (see the `site` recipe in the justfile for the
// one-time human step that points GitHub Pages at it; this script never
// touches git or the gh-pages branch itself).
//
// Always regenerates site/tokens.css first (via build_site_tokens.mjs) so
// the staged output can never carry a stale token snapshot.
//
// site/playground/ is intentionally excluded: it is a separate sub-project
// (its own package.json, a wasm build, and Playwright tests) with its own
// build process, not part of this zero-dependency static docs site, and it
// is not currently served from gh-pages either (`git ls-tree gh-pages` has no
// playground/ entry). If it ever needs to ship, that is its own packet.
//
// Dependency-free Node, matching the rest of scripts/ (check_*.mjs, doctest.mjs).

import { cpSync, rmSync, mkdirSync, existsSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { join } from "node:path";
import { buildSiteTokens } from "./build_site_tokens.mjs";

const repoRoot = fileURLToPath(new URL("..", import.meta.url));
const siteDir = join(repoRoot, "site");
const outDir = join(repoRoot, "dist", "site");

// 1. Regenerate site/tokens.css from ui/src/theme/tokens.ts so the staged
//    output never carries a stale snapshot.
buildSiteTokens();

// 2. Clean rebuild of dist/site/, so a file renamed or removed from site/
//    never lingers in a stale previous build.
rmSync(outDir, { recursive: true, force: true });
mkdirSync(outDir, { recursive: true });

// 3. Stage everything under site/ except playground/ (see file header).
const STAGED_ENTRIES = ["README.md", "index.html", "style.css", "tokens.css", "guides"];
for (const entry of STAGED_ENTRIES) {
  const src = join(siteDir, entry);
  if (!existsSync(src)) {
    console.error(`build_site: expected ${src} to exist`);
    process.exit(1);
  }
  cpSync(src, join(outDir, entry), { recursive: true });
}

console.log(`build_site: staged deployable docs site at ${outDir}`);
