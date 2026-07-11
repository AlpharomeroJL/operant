// Fails CI if an em dash (U+2014) or horizontal bar (U+2015) appears in any
// tracked text file. Style is enforced by machine, not memory.
import { execSync } from "node:child_process";
import { readFileSync } from "node:fs";

const EM = "—";
const BAR = "―";
const BINARY = /\.(png|pdf|bin|ico|gif|mp4|woff2|jpg|jpeg|zip|exe)$/i;

let files;
try {
  files = execSync("git ls-files", { cwd: process.cwd(), encoding: "utf8" })
    .split("\n")
    .map((s) => s.trim())
    .filter(Boolean)
    .filter((f) => !BINARY.test(f));
} catch (e) {
  console.error("check-emdash: could not list git files:", e.message);
  process.exit(2);
}

const hits = [];
for (const f of files) {
  let text;
  try {
    text = readFileSync(f, "utf8");
  } catch {
    continue;
  }
  const lines = text.split(/\r?\n/);
  lines.forEach((line, i) => {
    if (line.includes(EM) || line.includes(BAR)) {
      hits.push(`${f}:${i + 1}: ${line.trim().slice(0, 80)}`);
    }
  });
}

if (hits.length) {
  console.error("check-emdash: FAILED. Em dashes are forbidden. Use a hyphen, colon, or rewrite.");
  for (const h of hits.slice(0, 50)) console.error("  " + h);
  console.error(`  ...${hits.length} total`);
  process.exit(1);
}
console.log(`check-emdash: OK (${files.length} files clean)`);
