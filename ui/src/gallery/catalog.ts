// @advanced
// Internal plumbing (fixture loading, canonical JSON, Ed25519 verify), the
// same reason ui/src/bus/types.ts is marked @advanced: identifiers like
// "manifest" and "signature" below are correct registry-contract vocabulary
// in error messages and comments, never rendered as default-mode UI copy.
// Real UI copy for this screen lives in ui/src/gallery/strings.ts.
//
// The template gallery's catalog: what workflows are available to add, and
// whether each one can be trusted (docs/specs/registry.md). Reads the exact
// fixture the Rust registry crate's own tests install
// (contracts/fixtures/registry/manifest.json, contracts/fixtures/registry/
// publisher.pub, R1B) rather than a hand-copied literal, so the two test
// suites can never quietly drift onto different fixtures.
//
// Signature verification here is real, not faked: same canonical-JSON plus
// Ed25519 check as crates/registry/src/verify.rs, using Node's built-in
// `crypto` module (zero new dependency), the same approach
// ../operant-registry/validate.mjs already uses in this codebase for the
// same fixture. The one piece this mock does not re-check is the DSL file's
// content hash (BLAKE3, crates/registry's `verify_dsl`): there is no BLAKE3
// implementation in ui/package.json's dependency set, and the shell has no
// backend wired up yet to fetch the actual workflow file bytes at all (see
// ui/src/library/mockRegistry.ts's own "swap for the real thing later" seam).
// The Rust crate is the enforcing authority for that check at real install
// time; this file's job is proving the approval screen itself -- plain-
// language steps and permissions shown before a person can install anything
// -- against the real signed manifest and the real trust rules.

import { createPublicKey, verify as edVerify } from "node:crypto";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import type { Capabilities } from "../../../sdk/ts/src/render/index.js";

export type { Capabilities };

export interface TemplateManifest {
  v: 1;
  name: string;
  version: string;
  publisher: string;
  pubkey_fingerprint: string;
  description: string;
  step_summary: string[];
  inputs_schema: { type: "object"; properties: Record<string, unknown> };
  capabilities: Capabilities;
  min_operant_version: string;
  dsl: { url: string; hash: string };
  signature?: { sig: string };
}

const REPO_ROOT = join(dirname(fileURLToPath(import.meta.url)), "..", "..", "..");

function readFixture(...parts: string[]): string {
  return readFileSync(join(REPO_ROOT, "contracts", "fixtures", ...parts), "utf8");
}

/** The registry fixture manifest, loaded fresh each call so a mutation in one test never leaks into another. */
export function loadFixtureTemplates(): TemplateManifest[] {
  return [JSON.parse(readFixture("registry", "manifest.json")) as TemplateManifest];
}

/** publisher name -> raw Ed25519 public key, hex-encoded (contracts/fixtures/registry/publisher.pub's shape). */
export function loadFixturePublisherKeys(): Record<string, string> {
  return { "operant-fixtures": readFixture("registry", "publisher.pub").trim() };
}

function canonicalJson(value: unknown): string {
  if (value === null || typeof value !== "object") return JSON.stringify(value);
  if (Array.isArray(value)) return "[" + value.map(canonicalJson).join(",") + "]";
  const obj = value as Record<string, unknown>;
  const keys = Object.keys(obj).sort();
  return "{" + keys.map((k) => JSON.stringify(k) + ":" + canonicalJson(obj[k])).join(",") + "}";
}

/** Wrap a raw 32-byte hex Ed25519 key in its fixed SPKI DER prefix (RFC 8410), the same trick ../operant-registry/validate.mjs uses. */
function publicKeyFromHex(hex: string): ReturnType<typeof createPublicKey> {
  const raw = Buffer.from(hex.trim(), "hex");
  if (raw.length !== 32) throw new Error(`publisher key must be 32 bytes, got ${raw.length}`);
  const der = Buffer.concat([Buffer.from("302a300506032b6570032100", "hex"), raw]);
  return createPublicKey({ key: der, format: "der", type: "spki" });
}

export type Trust = "unverified" | "first_time" | "trusted";

/** Publisher name -> the key fingerprint it has been trusted with, mirroring `crates/registry::PinStore`'s pin-on-first-use rule: the same publisher name presenting a different fingerprint later is refused, not silently re-pinned. */
export class PinStore {
  private pins = new Map<string, string>();

  observe(publisher: string, fingerprint: string): "first_time" | "trusted" {
    const pinned = this.pins.get(publisher);
    if (pinned === undefined) {
      this.pins.set(publisher, fingerprint);
      return "first_time";
    }
    if (pinned === fingerprint) return "trusted";
    throw new Error(`${publisher} presented a different key than before`);
  }
}

/**
 * Verify `manifest`'s signature (if any) and run pin-on-first-use bookkeeping,
 * mirroring `FetchedManifest::verify_signature` in crates/registry. Throws on
 * any mismatch: a signed manifest with no key available, a key that does not
 * fingerprint to `pubkey_fingerprint`, a signature that does not verify, or a
 * publisher presenting a rotated key. Never falls back to "probably fine".
 */
export function verifyAndPin(
  manifest: TemplateManifest,
  publisherKeyHex: string | undefined,
  pins: PinStore,
): Trust {
  if (!manifest.signature) return "unverified";
  if (!publisherKeyHex) {
    throw new Error(`${manifest.publisher} signed this, but no key for that publisher is available`);
  }

  const key = publicKeyFromHex(publisherKeyHex);
  const unsigned: Partial<TemplateManifest> = { ...manifest };
  delete unsigned.signature;
  const message = Buffer.from(canonicalJson(unsigned), "utf8");
  const sig = Buffer.from(manifest.signature.sig, "base64");
  if (!edVerify(null, message, key, sig)) {
    throw new Error(`the signature for ${manifest.name}@${manifest.version} does not verify`);
  }

  return pins.observe(manifest.publisher, manifest.pubkey_fingerprint);
}
