// Test-only module hook: `node --test` runs ui/src/**/*.test.ts straight off
// disk with Node's native TypeScript stripping, and Node has no built-in
// notion of a `.css` module. Every view.ts under ui/src imports its
// stylesheet for its side effect (bundled for real by Vite at build time),
// so any test that imports a view.ts transitively imports a .css file and
// needs a stand-in for it. This hook turns a `.css` specifier into an empty
// module instead of leaving that unknown-extension failure on view-level
// tests, which is what this lane needs to test DOM mounting, keyboard
// navigation, and axe-core scans against the real view.ts files rather than
// re-implementing them under test.
//
// Registered by ./testHooks.mjs, loaded via `node --import` (see ui/package.json).

export async function load(url, context, nextLoad) {
  if (url.endsWith(".css")) {
    return { format: "module", source: "export default {};", shortCircuit: true };
  }
  return nextLoad(url, context);
}
