// Deterministic fixture generator. Rerunnable: same outputs byte-for-byte,
// except the Ed25519 keypair which is generated ONCE (guarded on existence)
// and committed as an intentional TEST key, never used for anything real.
//
// Produces:
//   model_download/model.bin + SHA256SUMS      (wizard downloader fixture)
//   docs/sample.pdf                            (OCR/PDF adapter fixture)
//   registry/publisher.key|.pub|.pub.pem       (fixture publisher keypair)
//   registry/manifest.json                     (signed registry manifest)
//   workflow_notepad/manifest.json             (dsl.hash patched to real BLAKE3)
import { createHash, generateKeyPairSync, sign, createPrivateKey, createPublicKey } from "node:crypto";
import { readFileSync, writeFileSync, existsSync, mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { blake3 } from "@noble/hashes/blake3";
import { bytesToHex } from "@noble/hashes/utils";

const ROOT = dirname(fileURLToPath(import.meta.url));
const p = (...xs) => join(ROOT, ...xs);
const ensure = (d) => mkdirSync(d, { recursive: true });

// Canonical JSON: recursively sorted keys, no whitespace, UTF-8.
export function canonicalJson(value) {
  if (value === null || typeof value !== "object") return JSON.stringify(value);
  if (Array.isArray(value)) return "[" + value.map(canonicalJson).join(",") + "]";
  const keys = Object.keys(value).sort();
  return "{" + keys.map((k) => JSON.stringify(k) + ":" + canonicalJson(value[k])).join(",") + "}";
}

// ---------- 1. model download fixture ----------
ensure(p("model_download"));
const SIZE = 256 * 1024;
const model = Buffer.alloc(SIZE);
for (let i = 0; i < SIZE; i++) model[i] = i % 251; // deterministic, non-trivial period
writeFileSync(p("model_download", "model.bin"), model);
const sha256 = createHash("sha256").update(model).digest("hex");
writeFileSync(p("model_download", "SHA256SUMS"), `${sha256}  model.bin\n`);
console.log("model.bin", SIZE, "bytes sha256", sha256.slice(0, 16) + "...");

// ---------- 2. minimal valid PDF with extractable text ----------
function makePdf(lines) {
  const esc = (s) => s.replace(/\\/g, "\\\\").replace(/\(/g, "\\(").replace(/\)/g, "\\)");
  let content = "BT\n/F1 24 Tf\n";
  let y = 720;
  for (const line of lines) {
    content += `1 0 0 1 72 ${y} Tm\n(${esc(line)}) Tj\n`;
    y -= 36;
  }
  content += "ET";
  const objects = [
    "<< /Type /Catalog /Pages 2 0 R >>",
    "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>",
    "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
    `<< /Length ${Buffer.byteLength(content)} >>\nstream\n${content}\nendstream`,
  ];
  let pdf = "%PDF-1.4\n";
  const offsets = [0];
  for (let i = 0; i < objects.length; i++) {
    offsets.push(Buffer.byteLength(pdf));
    pdf += `${i + 1} 0 obj\n${objects[i]}\nendobj\n`;
  }
  const xrefStart = Buffer.byteLength(pdf);
  pdf += `xref\n0 ${objects.length + 1}\n`;
  pdf += "0000000000 65535 f \n";
  for (let i = 1; i <= objects.length; i++) {
    pdf += String(offsets[i]).padStart(10, "0") + " 00000 n \n";
  }
  pdf += `trailer\n<< /Size ${objects.length + 1} /Root 1 0 R >>\nstartxref\n${xrefStart}\n%%EOF\n`;
  return Buffer.from(pdf, "latin1");
}
ensure(p("docs"));
writeFileSync(
  p("docs", "sample.pdf"),
  makePdf(["Operant fixture invoice", "Invoice INV-2026-0711", "Total $142.50", "Due 2026-07-25"])
);
console.log("sample.pdf written");

// ---------- 3. fixture publisher keypair (guarded: generated once) ----------
ensure(p("registry"));
let privPem, pubPem;
if (existsSync(p("registry", "publisher.key"))) {
  privPem = readFileSync(p("registry", "publisher.key"), "utf8");
  pubPem = readFileSync(p("registry", "publisher.pub.pem"), "utf8");
  console.log("keypair exists, reusing");
} else {
  const { privateKey, publicKey } = generateKeyPairSync("ed25519");
  privPem = privateKey.export({ type: "pkcs8", format: "pem" });
  pubPem = publicKey.export({ type: "spki", format: "pem" });
  writeFileSync(p("registry", "publisher.key"), privPem);
  writeFileSync(p("registry", "publisher.pub.pem"), pubPem);
  console.log("keypair generated (TEST KEY, intentionally committed)");
}
const privKey = createPrivateKey(privPem);
const pubKey = createPublicKey(pubPem);
// Raw 32-byte public key from JWK x (base64url).
const jwk = pubKey.export({ format: "jwk" });
const rawPub = Buffer.from(jwk.x, "base64url");
writeFileSync(p("registry", "publisher.pub"), rawPub.toString("hex") + "\n");
const fingerprint = bytesToHex(blake3(rawPub, { dkLen: 16 }));
console.log("pubkey fingerprint", fingerprint);

// ---------- 4. real BLAKE3 of the DSL file; patch workflow manifest ----------
const dslBytes = readFileSync(p("workflow_notepad", "workflow.ts"));
const dslHash = bytesToHex(blake3(dslBytes));
const wfManifest = JSON.parse(readFileSync(p("workflow_notepad", "manifest.json"), "utf8"));
wfManifest.dsl.hash = dslHash;
writeFileSync(p("workflow_notepad", "manifest.json"), JSON.stringify(wfManifest, null, 2) + "\n");
console.log("workflow_notepad dsl.hash", dslHash.slice(0, 16) + "...");

// ---------- 5. signed registry manifest ----------
const regManifest = {
  v: 1,
  name: wfManifest.name,
  version: wfManifest.version,
  publisher: "operant-fixtures",
  pubkey_fingerprint: fingerprint,
  description: wfManifest.description,
  step_summary: wfManifest.step_summary,
  inputs_schema: wfManifest.inputs_schema,
  capabilities: wfManifest.capabilities,
  min_operant_version: "1.0.0",
  dsl: { url: "workflow_notepad/workflow.ts", hash: dslHash },
};
const sig = sign(null, Buffer.from(canonicalJson(regManifest), "utf8"), privKey);
regManifest.signature = { sig: sig.toString("base64") };
writeFileSync(p("registry", "manifest.json"), JSON.stringify(regManifest, null, 2) + "\n");
console.log("registry manifest signed");

// ---------- 6. self-verify ----------
import { verify } from "node:crypto";
const check = JSON.parse(readFileSync(p("registry", "manifest.json"), "utf8"));
const sigB = Buffer.from(check.signature.sig, "base64");
delete check.signature;
const ok = verify(null, Buffer.from(canonicalJson(check), "utf8"), pubKey, sigB);
if (!ok) { console.error("SELF-VERIFY FAILED"); process.exit(1); }
console.log("self-verify OK");
