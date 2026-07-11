// Provider detection for the "I have an access key" setup path
// (docs/specs/zero-code.md: "paste field, provider auto-detected from key
// shape where possible, dropdown otherwise"). Pure, no I/O: runs under plain
// `node --test`.

export type Provider = "chatgpt" | "claude";

/**
 * Best-effort guess at which provider an access key belongs to, from its
 * shape alone (no network call: this shell has no live endpoint to ask).
 * Returns null when the shape does not match a known pattern, which the
 * caller shows as "pick it from the list below"
 * (setupPathStrings.cards.accessKey.providerPickManually).
 */
export function detectProviderFromKey(rawKey: string): Provider | null {
  const key = rawKey.trim();
  if (!key) return null;
  // Anthropic keys: sk-ant-...
  if (/^sk-ant-/i.test(key)) return "claude";
  // OpenAI keys: sk-... (not sk-ant-, already handled above) or the newer
  // project-scoped sk-proj-... shape.
  if (/^sk-(proj-)?[a-z0-9]/i.test(key)) return "chatgpt";
  return null;
}
