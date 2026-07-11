#!/usr/bin/env node
// Operant updater signing keys: generate, sign, verify.
//
// Produces and consumes a minisign-compatible key/signature format (the format
// tauri-plugin-updater parses via the `minisign-verify` crate: 2-byte algorithm
// id "Ed" + 8-byte key id + 32-byte Ed25519 public key for keys, and
// "Ed" + key id + 64-byte signature, followed by a trusted-comment line and a
// 64-byte global signature over (signature bytes + trusted comment), for
// signature files). Only the legacy, non-prehashed "Ed" mode is implemented,
// which is what a single Ed25519 sign/verify call in node:crypto produces
// directly with no extra hashing step.
//
// This is a from-scratch reimplementation against the public minisign format
// (see release/KEYS.md for how it was validated) written because cargo-tauri
// is not installed in this environment, so the official `tauri signer`
// command is not available here. The public-key and signature wire format
// were cross-checked against a real tauri-plugin-updater test fixture and the
// public minisign-verify parser source. They have not been round-trip tested
// against a compiled tauri-plugin-updater binary, because no such binary can
// be built here. Validate with a real `cargo tauri build` before the first
// production release ships; see release/KEYS.md and release/REPRODUCIBLE.md.
//
// The private key at rest uses a separate, simpler format that is entirely
// our own (JSON, optionally AES-256-GCM encrypted): it is never parsed by
// Tauri, only by this script, so it does not need to match minisign's own
// (scrypt-encrypted) secret key file layout.
//
// Usage:
//   node release/scripts/updater-keys.mjs generate [--force]
//   node release/scripts/updater-keys.mjs sign <file> [--out <file.sig>] [--comment <text>]
//   node release/scripts/updater-keys.mjs verify <file> [--sig <file.sig>] [--pubkey <file.pub>]
//   node release/scripts/updater-keys.mjs roundtrip [--keep]
//   node release/scripts/updater-keys.mjs print-pubkey
//
// Environment:
//   OPERANT_UPDATER_KEY_PATH        override the vault path for the private key
//   OPERANT_UPDATER_KEY_PASSPHRASE  if set at generate time, encrypts the private
//                                   key at rest with AES-256-GCM (scrypt-derived
//                                   key); must also be set for sign/roundtrip.

import crypto from "node:crypto";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const RELEASE_DIR = path.resolve(SCRIPT_DIR, "..");
const DEFAULT_PUB_PATH = path.join(RELEASE_DIR, "keys", "updater_pubkey.pub");
const DEFAULT_TAURI_VALUE_PATH = path.join(RELEASE_DIR, "keys", "updater_pubkey.tauri-config-value.txt");

const SIG_ALG = Buffer.from([0x45, 0x64]); // "Ed": legacy (non-prehashed) mode
const SIG_ALG_PREHASH = Buffer.from([0x45, 0x44]); // "ED": prehashed mode (not produced or accepted here)
const KEY_ID_LEN = 8;
const PUBKEY_LEN = 32;
const SEED_LEN = 32;
const SIGNATURE_LEN = 64;
const KEY_FILE_FORMAT = "operant-updater-key-v1";
const SCRYPT_N = 16384;
const SCRYPT_R = 8;
const SCRYPT_P = 1;

function fail(message) {
  console.error(`updater-keys: ${message}`);
  process.exit(1);
}

function b64(buf) {
  return buf.toString("base64");
}

function b64url(buf) {
  return buf.toString("base64url");
}

function fromB64url(s) {
  return Buffer.from(s, "base64url");
}

// ---------------------------------------------------------------------------
// Raw key material <-> Node KeyObject, via JWK export/import. This avoids any
// hand-parsing of DER (SPKI/PKCS8) framing: JWK for an OKP/Ed25519 key is just
// { kty: "OKP", crv: "Ed25519", x: <base64url pubkey>, d: <base64url seed> }.
// ---------------------------------------------------------------------------

function generateKeyPair() {
  const { publicKey, privateKey } = crypto.generateKeyPairSync("ed25519");
  const pubJwk = publicKey.export({ format: "jwk" });
  const privJwk = privateKey.export({ format: "jwk" });
  return {
    rawPublicKey: fromB64url(pubJwk.x),
    rawSeed: fromB64url(privJwk.d),
  };
}

function publicKeyObjectFromRaw(rawPublicKey) {
  return crypto.createPublicKey({
    key: { kty: "OKP", crv: "Ed25519", x: b64url(rawPublicKey) },
    format: "jwk",
  });
}

function privateKeyObjectFromRaw(rawSeed, rawPublicKey) {
  return crypto.createPrivateKey({
    key: { kty: "OKP", crv: "Ed25519", x: b64url(rawPublicKey), d: b64url(rawSeed) },
    format: "jwk",
  });
}

// ---------------------------------------------------------------------------
// minisign-compatible public key file and signature file framing.
// ---------------------------------------------------------------------------

function keyIdHex(keyId) {
  return keyId.toString("hex").toUpperCase();
}

function buildPubFileText(keyId, rawPublicKey, comment) {
  const blob = Buffer.concat([SIG_ALG, keyId, rawPublicKey]);
  return `untrusted comment: ${comment}\n${b64(blob)}\n`;
}

function tauriPubkeyValue(pubFileText) {
  return b64(Buffer.from(pubFileText, "utf8"));
}

function parsePubFile(text) {
  const lines = text.split(/\r?\n/).filter((l) => l.length > 0);
  if (lines.length < 2) fail("public key file is malformed (expected at least 2 lines)");
  if (!lines[0].startsWith("untrusted comment: ")) {
    fail('public key file is malformed (line 1 must start with "untrusted comment: ")');
  }
  const blob = Buffer.from(lines[1], "base64");
  if (blob.length !== 2 + KEY_ID_LEN + PUBKEY_LEN) {
    fail(`public key blob has wrong length: ${blob.length}, expected ${2 + KEY_ID_LEN + PUBKEY_LEN}`);
  }
  const sigAlg = blob.subarray(0, 2);
  if (!sigAlg.equals(SIG_ALG)) {
    fail(`unsupported public key algorithm: ${sigAlg.toString("hex")} (only legacy "Ed" is supported)`);
  }
  const keyId = blob.subarray(2, 2 + KEY_ID_LEN);
  const rawPublicKey = blob.subarray(2 + KEY_ID_LEN, 2 + KEY_ID_LEN + PUBKEY_LEN);
  return { keyId, rawPublicKey };
}

function signArtifact({ rawSeed, rawPublicKey, keyId }, messageBuffer, trustedComment) {
  const privKey = privateKeyObjectFromRaw(rawSeed, rawPublicKey);
  const signature = crypto.sign(null, messageBuffer, privKey);
  if (signature.length !== SIGNATURE_LEN) fail("internal error: unexpected signature length");
  const sigBlob = Buffer.concat([SIG_ALG, keyId, signature]);
  const globalMessage = Buffer.concat([signature, Buffer.from(trustedComment, "utf8")]);
  const globalSignature = crypto.sign(null, globalMessage, privKey);
  return (
    `untrusted comment: signature from operant updater secret key\n` +
    `${b64(sigBlob)}\n` +
    `trusted comment: ${trustedComment}\n` +
    `${b64(globalSignature)}\n`
  );
}

function parseSigFile(text) {
  const lines = text.split(/\r?\n/).filter((l) => l.length > 0);
  if (lines.length < 4) fail("signature file is malformed (expected 4 lines)");
  if (!lines[0].startsWith("untrusted comment: ")) {
    fail('signature file is malformed (line 1 must start with "untrusted comment: ")');
  }
  const sigBlob = Buffer.from(lines[1], "base64");
  if (sigBlob.length !== 2 + KEY_ID_LEN + SIGNATURE_LEN) {
    fail(`signature blob has wrong length: ${sigBlob.length}, expected ${2 + KEY_ID_LEN + SIGNATURE_LEN}`);
  }
  const sigAlg = sigBlob.subarray(0, 2);
  if (sigAlg.equals(SIG_ALG_PREHASH)) {
    fail('signature uses prehashed "ED" mode, which this script does not implement');
  }
  if (!sigAlg.equals(SIG_ALG)) {
    fail(`unsupported signature algorithm: ${sigAlg.toString("hex")}`);
  }
  const keyId = sigBlob.subarray(2, 2 + KEY_ID_LEN);
  const signature = sigBlob.subarray(2 + KEY_ID_LEN, 2 + KEY_ID_LEN + SIGNATURE_LEN);
  if (!lines[2].startsWith("trusted comment: ")) {
    fail('signature file is malformed (line 3 must start with "trusted comment: ")');
  }
  const trustedComment = lines[2].slice("trusted comment: ".length);
  const globalSignature = Buffer.from(lines[3], "base64");
  if (globalSignature.length !== SIGNATURE_LEN) {
    fail(`global signature has wrong length: ${globalSignature.length}, expected ${SIGNATURE_LEN}`);
  }
  return { keyId, signature, trustedComment, globalSignature };
}

function verifyArtifact(rawPublicKey, messageBuffer, sigFileText) {
  const parsed = parseSigFile(sigFileText);
  const pubKey = publicKeyObjectFromRaw(rawPublicKey);
  const primaryOk = crypto.verify(null, messageBuffer, pubKey, parsed.signature);
  const globalMessage = Buffer.concat([parsed.signature, Buffer.from(parsed.trustedComment, "utf8")]);
  const globalOk = crypto.verify(null, globalMessage, pubKey, parsed.globalSignature);
  return { ok: primaryOk && globalOk, primaryOk, globalOk, keyId: parsed.keyId };
}

// ---------------------------------------------------------------------------
// Private key vault file: our own format, JSON, optionally AES-256-GCM
// encrypted at rest. Never parsed by Tauri; only by this script.
// ---------------------------------------------------------------------------

function defaultVaultPath() {
  if (process.env.OPERANT_UPDATER_KEY_PATH) return process.env.OPERANT_UPDATER_KEY_PATH;
  if (process.platform === "win32") {
    const base = process.env.LOCALAPPDATA || path.join(os.homedir(), "AppData", "Local");
    return path.join(base, "Operant", "updater-keys", "updater_ed25519.key");
  }
  return path.join(os.homedir(), ".local", "share", "operant", "updater-keys", "updater_ed25519.key");
}

function encryptSeed(rawSeed, passphrase) {
  const salt = crypto.randomBytes(32);
  const key = crypto.scryptSync(passphrase, salt, 32, { N: SCRYPT_N, r: SCRYPT_R, p: SCRYPT_P });
  const iv = crypto.randomBytes(12);
  const cipher = crypto.createCipheriv("aes-256-gcm", key, iv);
  const ciphertext = Buffer.concat([cipher.update(rawSeed), cipher.final()]);
  const authTag = cipher.getAuthTag();
  return { ciphertext, salt, iv, authTag };
}

function decryptSeed(ciphertext, passphrase, salt, iv, authTag) {
  const key = crypto.scryptSync(passphrase, salt, 32, { N: SCRYPT_N, r: SCRYPT_R, p: SCRYPT_P });
  const decipher = crypto.createDecipheriv("aes-256-gcm", key, iv);
  decipher.setAuthTag(authTag);
  return Buffer.concat([decipher.update(ciphertext), decipher.final()]);
}

function writeVaultFile(vaultPath, { rawSeed, rawPublicKey, keyId }, passphrase) {
  fs.mkdirSync(path.dirname(vaultPath), { recursive: true });
  const record = {
    format: KEY_FILE_FORMAT,
    algorithm: "Ed25519",
    keyId: keyIdHex(keyId),
    createdAt: new Date().toISOString(),
    encrypted: Boolean(passphrase),
    publicKey: b64(rawPublicKey),
  };
  if (passphrase) {
    const { ciphertext, salt, iv, authTag } = encryptSeed(rawSeed, passphrase);
    record.kdf = { name: "scrypt", N: SCRYPT_N, r: SCRYPT_R, p: SCRYPT_P, salt: b64(salt) };
    record.cipher = "aes-256-gcm";
    record.iv = b64(iv);
    record.authTag = b64(authTag);
    record.seed = b64(ciphertext);
  } else {
    record.seed = b64(rawSeed);
  }
  fs.writeFileSync(vaultPath, JSON.stringify(record, null, 2) + "\n", { mode: 0o600 });
  try {
    fs.chmodSync(vaultPath, 0o600);
  } catch {
    // best effort; on Windows the real protection boundary is the per-user
    // LOCALAPPDATA ACL inherited from the profile directory, not chmod bits.
  }
}

function readVaultFile(vaultPath) {
  if (!fs.existsSync(vaultPath)) {
    fail(
      `no private key found at ${vaultPath}\n` +
        `  Run "node release/scripts/updater-keys.mjs generate" first, or set\n` +
        `  OPERANT_UPDATER_KEY_PATH to point at an existing vault file.`,
    );
  }
  const record = JSON.parse(fs.readFileSync(vaultPath, "utf8"));
  if (record.format !== KEY_FILE_FORMAT) {
    fail(`${vaultPath} is not an ${KEY_FILE_FORMAT} file (found format: ${record.format})`);
  }
  const keyId = Buffer.from(record.keyId, "hex");
  const rawPublicKey = Buffer.from(record.publicKey, "base64");
  let rawSeed;
  if (record.encrypted) {
    const passphrase = process.env.OPERANT_UPDATER_KEY_PASSPHRASE;
    if (!passphrase) {
      fail(`${vaultPath} is passphrase-encrypted; set OPERANT_UPDATER_KEY_PASSPHRASE`);
    }
    rawSeed = decryptSeed(
      Buffer.from(record.seed, "base64"),
      passphrase,
      Buffer.from(record.kdf.salt, "base64"),
      Buffer.from(record.iv, "base64"),
      Buffer.from(record.authTag, "base64"),
    );
  } else {
    rawSeed = Buffer.from(record.seed, "base64");
  }
  if (rawSeed.length !== SEED_LEN || rawPublicKey.length !== PUBKEY_LEN || keyId.length !== KEY_ID_LEN) {
    fail(`${vaultPath} has malformed key material`);
  }
  return { rawSeed, rawPublicKey, keyId };
}

// ---------------------------------------------------------------------------
// CLI
// ---------------------------------------------------------------------------

function parseArgs(argv) {
  const [command, ...rest] = argv;
  const positional = [];
  const flags = {};
  for (let i = 0; i < rest.length; i++) {
    const a = rest[i];
    if (a.startsWith("--")) {
      const key = a.slice(2);
      const next = rest[i + 1];
      if (next !== undefined && !next.startsWith("--")) {
        flags[key] = next;
        i++;
      } else {
        flags[key] = true;
      }
    } else {
      positional.push(a);
    }
  }
  return { command, positional, flags };
}

function cmdGenerate(flags) {
  const vaultPath = defaultVaultPath();
  if (fs.existsSync(vaultPath) && !flags.force) {
    fail(`a private key already exists at ${vaultPath}. Pass --force to overwrite (this orphans old signatures).`);
  }
  const { rawPublicKey, rawSeed } = generateKeyPair();
  const keyId = crypto.randomBytes(KEY_ID_LEN);
  const passphrase = process.env.OPERANT_UPDATER_KEY_PASSPHRASE || null;

  writeVaultFile(vaultPath, { rawSeed, rawPublicKey, keyId }, passphrase);

  const comment = `operant updater public key ${keyIdHex(keyId)}`;
  const pubFileText = buildPubFileText(keyId, rawPublicKey, comment);
  fs.mkdirSync(path.dirname(DEFAULT_PUB_PATH), { recursive: true });
  fs.writeFileSync(DEFAULT_PUB_PATH, pubFileText);

  const tauriValue = tauriPubkeyValue(pubFileText);
  fs.writeFileSync(DEFAULT_TAURI_VALUE_PATH, tauriValue + "\n");

  console.log("updater-keys: generated a new Ed25519 updater keypair");
  console.log(`  key id:            ${keyIdHex(keyId)}`);
  console.log(`  private key vault: ${vaultPath}${passphrase ? " (passphrase-encrypted)" : " (not passphrase-encrypted)"}`);
  console.log(`  public key file:   ${DEFAULT_PUB_PATH}`);
  console.log(`  tauri config value written to: ${DEFAULT_TAURI_VALUE_PATH}`);
  console.log(`  paste that value into ui/src-tauri/tauri.conf.json -> plugins.updater.pubkey`);
}

function cmdPrintPubkey() {
  if (!fs.existsSync(DEFAULT_PUB_PATH)) fail(`no public key at ${DEFAULT_PUB_PATH}; run "generate" first`);
  const text = fs.readFileSync(DEFAULT_PUB_PATH, "utf8");
  console.log(tauriPubkeyValue(text));
}

function cmdSign(positional, flags) {
  const file = positional[0];
  if (!file) fail("usage: sign <file> [--out <file.sig>] [--comment <text>]");
  if (!fs.existsSync(file)) fail(`no such file: ${file}`);
  const vaultPath = defaultVaultPath();
  const key = readVaultFile(vaultPath);
  const message = fs.readFileSync(file);
  const trustedComment =
    flags.comment || `timestamp:${Math.floor(Date.now() / 1000)}\tfile:${path.basename(file)}`;
  const sigText = signArtifact(key, message, trustedComment);
  const outPath = flags.out || `${file}.sig`;
  fs.writeFileSync(outPath, sigText);
  console.log(`updater-keys: signed ${file} -> ${outPath} (key id ${keyIdHex(key.keyId)})`);
}

function cmdVerify(positional, flags) {
  const file = positional[0];
  if (!file) fail("usage: verify <file> [--sig <file.sig>] [--pubkey <file.pub>]");
  const sigPath = flags.sig || `${file}.sig`;
  const pubPath = flags.pubkey || DEFAULT_PUB_PATH;
  if (!fs.existsSync(file)) fail(`no such file: ${file}`);
  if (!fs.existsSync(sigPath)) fail(`no such signature file: ${sigPath}`);
  if (!fs.existsSync(pubPath)) fail(`no such public key file: ${pubPath}`);
  const message = fs.readFileSync(file);
  const sigText = fs.readFileSync(sigPath, "utf8");
  const { rawPublicKey, keyId: pubKeyId } = parsePubFile(fs.readFileSync(pubPath, "utf8"));
  const result = verifyArtifact(rawPublicKey, message, sigText);
  if (!result.keyId.equals(pubKeyId)) {
    console.error(
      `updater-keys: WARNING: signature key id ${keyIdHex(result.keyId)} does not match ` +
        `public key id ${keyIdHex(pubKeyId)}`,
    );
  }
  console.log(result.ok ? "true" : "false");
  if (!result.ok) {
    console.error(`  primary signature valid:  ${result.primaryOk}`);
    console.error(`  trusted-comment sig valid: ${result.globalOk}`);
  }
  process.exit(result.ok ? 0 : 1);
}

function cmdRoundtrip(flags) {
  const vaultPath = defaultVaultPath();
  if (!fs.existsSync(vaultPath)) {
    console.log("updater-keys: no key found, generating one for the roundtrip test");
    cmdGenerate({});
  }
  const key = readVaultFile(vaultPath);
  const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), "operant-updater-roundtrip-"));
  const artifactPath = path.join(tmpDir, "sample-update-artifact.bin");
  const payload = Buffer.concat([
    Buffer.from("operant updater roundtrip self-test\n", "utf8"),
    crypto.randomBytes(4096),
  ]);
  fs.writeFileSync(artifactPath, payload);

  const trustedComment = `timestamp:${Math.floor(Date.now() / 1000)}\tfile:sample-update-artifact.bin\troundtrip:true`;
  const sigText = signArtifact(key, payload, trustedComment);
  const sigPath = `${artifactPath}.sig`;
  fs.writeFileSync(sigPath, sigText);

  const result = verifyArtifact(key.rawPublicKey, fs.readFileSync(artifactPath), sigText);

  console.log(`updater-keys: roundtrip artifact:  ${artifactPath}`);
  console.log(`updater-keys: roundtrip signature: ${sigPath}`);
  console.log(`updater-keys: sign -> verify result: ${result.ok}`);

  if (!flags.keep) {
    fs.rmSync(tmpDir, { recursive: true, force: true });
  } else {
    console.log(`updater-keys: kept temp dir (--keep): ${tmpDir}`);
  }

  if (!result.ok) fail("roundtrip FAILED: sign then verify did not return true");
  console.log("updater-keys: roundtrip PASSED");
}

function main() {
  const { command, positional, flags } = parseArgs(process.argv.slice(2));
  switch (command) {
    case "generate":
      cmdGenerate(flags);
      break;
    case "print-pubkey":
      cmdPrintPubkey();
      break;
    case "sign":
      cmdSign(positional, flags);
      break;
    case "verify":
      cmdVerify(positional, flags);
      break;
    case "roundtrip":
      cmdRoundtrip(flags);
      break;
    default:
      console.error(
        "usage: node updater-keys.mjs <generate|sign|verify|roundtrip|print-pubkey> [args]\n" +
          "  generate    [--force]\n" +
          "  sign        <file> [--out <file.sig>] [--comment <text>]\n" +
          "  verify      <file> [--sig <file.sig>] [--pubkey <file.pub>]\n" +
          "  roundtrip   [--keep]\n" +
          "  print-pubkey",
      );
      process.exit(command ? 1 : 0);
  }
}

main();
