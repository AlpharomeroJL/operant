#!/usr/bin/env node
// Generates the fixture material ui/src-tauri/tests/updater_signature.rs reads
// with include_str!/include_bytes!: a throwaway Ed25519 test keypair (never
// the real release signing key from release/KEYS.md, and never itself
// written to disk) signs a fake "artifact" using the exact minisign wire
// format tauri-plugin-updater / minisign-verify expect. That format is
// documented in full in release/KEYS.md; this script duplicates only the
// couple dozen lines of framing it needs rather than importing
// release/scripts/updater-keys.mjs, so this fixture stays entirely inside
// ui/src-tauri and never touches the real vault key or its path.
//
// Only PUBLIC material is written to disk: the fixture artifact, its
// manifest (Tauri's "dynamic" updater JSON shape: top-level url + signature,
// see tauri-plugin-updater's src/updater.rs Deserialize impl for
// RemoteRelease), the tauri-config pubkey value, and a deliberately
// corrupted copy of the signature for the tampered-manifest test. The
// generated private key exists only in this process's memory.
//
// Re-run with: node ui/src-tauri/tests/fixtures/updater/generate.mjs
// Every run mints a fresh throwaway keypair, so the committed output changes
// byte-for-byte each time this is re-run (same as contracts/fixtures's own
// generate.mjs); that is expected and fine, since the test only cares that
// signature and pubkey are internally consistent with each other, not that
// they are stable across runs.

import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const DIR = path.dirname(fileURLToPath(import.meta.url));
const SIG_ALG = Buffer.from([0x45, 0x64]); // "Ed": minisign legacy (non-prehashed) mode
const KEY_ID_LEN = 8;

function b64(buf) {
  return buf.toString("base64");
}

function generateKeyPair() {
  const { publicKey, privateKey } = crypto.generateKeyPairSync("ed25519");
  const pubJwk = publicKey.export({ format: "jwk" });
  return {
    rawPublicKey: Buffer.from(pubJwk.x, "base64url"),
    privateKeyObject: privateKey,
    keyId: crypto.randomBytes(KEY_ID_LEN),
  };
}

function pubFileText(keyId, rawPublicKey) {
  const blob = Buffer.concat([SIG_ALG, keyId, rawPublicKey]);
  return `untrusted comment: operant updater TEST-ONLY fixture key (not a release key)\n${b64(blob)}\n`;
}

function signSigFileText(privateKeyObject, keyId, message, trustedComment) {
  const signature = crypto.sign(null, message, privateKeyObject);
  const sigBlob = Buffer.concat([SIG_ALG, keyId, signature]);
  const globalMessage = Buffer.concat([signature, Buffer.from(trustedComment, "utf8")]);
  const globalSignature = crypto.sign(null, globalMessage, privateKeyObject);
  return (
    `untrusted comment: signature from an operant updater TEST-ONLY secret key\n` +
    `${b64(sigBlob)}\n` +
    `trusted comment: ${trustedComment}\n` +
    `${b64(globalSignature)}\n`
  );
}

// Flips one base64 character in the middle of the signature's outer base64
// encoding. The result stays syntactically valid base64 (same length, same
// alphabet), so it still decodes structurally, but to different bytes: this
// simulates a manifest whose signature field was altered in transit,
// independent of the artifact bytes themselves (which stay exactly as
// signed and are served unchanged).
function tamperBase64(value) {
  const alphabet = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
  const mid = Math.floor(value.length / 2);
  const original = value[mid];
  const replacement = alphabet[(alphabet.indexOf(original) + 1) % alphabet.length];
  if (replacement === original) {
    throw new Error("tamperBase64: failed to pick a different character");
  }
  return value.slice(0, mid) + replacement + value.slice(mid + 1);
}

const { rawPublicKey, privateKeyObject, keyId } = generateKeyPair();

const artifact = Buffer.concat([
  Buffer.from("operant updater fixture artifact - not a real installer\n", "utf8"),
  crypto.randomBytes(256),
]);
fs.writeFileSync(path.join(DIR, "artifact.bin"), artifact);

const pubText = pubFileText(keyId, rawPublicKey);
const pubkeyConfigValue = b64(Buffer.from(pubText, "utf8"));
fs.writeFileSync(path.join(DIR, "pubkey.tauri-config-value.txt"), pubkeyConfigValue);

const trustedComment = "operant updater fixture, test-only, not a release artifact";
const sigText = signSigFileText(privateKeyObject, keyId, artifact, trustedComment);
const signatureValue = b64(Buffer.from(sigText, "utf8"));

const manifest = {
  version: "9.9.9",
  notes: "operant-updater-fixture: synthetic release for automated tests only",
  pub_date: "2026-01-01T00:00:00.000Z",
  url: "{{BASE_URL}}/artifact.bin",
  signature: signatureValue,
};
fs.writeFileSync(path.join(DIR, "latest.json"), `${JSON.stringify(manifest, null, 2)}\n`);

fs.writeFileSync(path.join(DIR, "tampered-signature.txt"), tamperBase64(signatureValue));

console.log("updater fixture generated (test-only key, discarded after signing):");
console.log(`  key id: ${keyId.toString("hex").toUpperCase()}`);
console.log(`  ${path.join(DIR, "artifact.bin")}`);
console.log(`  ${path.join(DIR, "pubkey.tauri-config-value.txt")}`);
console.log(`  ${path.join(DIR, "latest.json")}`);
console.log(`  ${path.join(DIR, "tampered-signature.txt")}`);
