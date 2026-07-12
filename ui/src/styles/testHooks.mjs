// Registers ./cssLoaderHook.mjs so `node --test` can import view.ts modules
// (and anything else that pulls in a .css side-effect import) without
// tripping over Node's lack of a native CSS module type. Loaded via
// `node --import ./src/styles/testHooks.mjs --test` (see ui/package.json's
// test script); this file's only job is the registration call, so the hook
// itself stays a plain, independently readable module.

import { register } from "node:module";

register("./cssLoaderHook.mjs", import.meta.url);
