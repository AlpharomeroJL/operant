// Fails if a raw hex color literal (#rgb, #rgba, #rrggbb, #rrggbbaa) appears
// anywhere under ui/src, except the two files that ARE the design tokens:
// ui/src/theme/tokens.ts (the single source of truth, docs/specs/design.md
// section 2) and ui/src/styles/tokens.css (generated from it,
// ui/scripts/build-tokens.mjs). Every other color in the app must reference
// a token (a CSS custom property, or an import from tokens.ts), never a
// literal, so the palette can never quietly drift screen by screen.
//
// Scope, stated honestly: this lint only walks ui/src (docs/specs/design.md
// section 2 itself only binds the app's own UI: "the single visual and
// interaction reference for the Operant desktop app and the docs site" is
// the doc's own claim about docs/site coverage, but ui/src is this packet's
// owned surface and the only tree this lint's exemption list, and this
// packet's actual token migration, was scoped to). ui/src-tauri (native
// window chrome config, not stylesheet colors) and site/ (the separate
// docs-site stylesheet, its own build, not part of the `just ui` gate) are
// out of scope for this check; if they ever need the same guarantee, they
// need their own token file and their own run of this script against them.

import { execSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { join, sep } from "node:path";

const repoRoot = process.cwd();
const scanRoot = "ui/src";

// #rgb / #rgba / #rrggbb / #rrggbbaa, the CSS/hex-literal forms this lint
// forbids. Longest forms first, and a negative lookahead so a 6-digit match
// never gets reported as a truncated prefix of an 8-digit one.
const HEX_COLOR = /#(?:[0-9a-fA-F]{8}|[0-9a-fA-F]{6}|[0-9a-fA-F]{4}|[0-9a-fA-F]{3})(?![0-9a-fA-F])/g;

const SOURCE_EXTENSIONS = /\.(ts|tsx|js|mjs|cjs|css|html)$/i;

function toPosix(p) {
  return p.split(sep).join("/");
}

const EXEMPT = new Set(["ui/src/theme/tokens.ts", "ui/src/styles/tokens.css"]);

let files;
try {
  files = execSync(`git ls-files -- ${scanRoot}`, { cwd: repoRoot, encoding: "utf8" })
    .split("\n")
    .map((s) => s.trim())
    .filter(Boolean)
    .filter((f) => SOURCE_EXTENSIONS.test(f));
} catch (e) {
  console.error("check-rawhex: could not list git files:", e.message);
  process.exit(2);
}

const hits = [];
let exemptCount = 0;
for (const f of files) {
  const posixPath = toPosix(f);
  if (EXEMPT.has(posixPath)) {
    exemptCount++;
    continue;
  }

  let text;
  try {
    text = readFileSync(join(repoRoot, f), "utf8");
  } catch {
    continue;
  }

  const lines = text.split(/\r?\n/);
  lines.forEach((line, i) => {
    const matches = line.match(HEX_COLOR);
    if (matches) {
      hits.push(`${posixPath}:${i + 1}: ${matches.join(", ")} -- ${line.trim().slice(0, 100)}`);
    }
  });
}

if (hits.length) {
  console.error(
    `check-rawhex: FAILED. Raw hex color literals are forbidden under ${scanRoot} outside ${[...EXEMPT].join(" and ")}. Reference a token instead (ui/src/theme/tokens.ts, or a var(--op-color-...) from ui/src/styles/tokens.css).`,
  );
  for (const h of hits.slice(0, 50)) console.error("  " + h);
  if (hits.length > 50) console.error(`  ...${hits.length} total`);
  process.exit(1);
}

console.log(`check-rawhex: OK (${files.length - exemptCount} files clean under ${scanRoot}, ${exemptCount} exempt)`);
