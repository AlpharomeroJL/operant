import { defineConfig } from "vite";

// Standard Tauri-friendly Vite config: fixed dev port matching tauri.conf.json's
// devUrl, and no dashboard clear so Rust build errors stay visible when the
// shell is launched through `tauri dev`. src-tauri is excluded from the watcher
// since Rust rebuilds are the Cargo side's job, not Vite's.
export default defineConfig({
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },
  build: {
    outDir: "dist",
    emptyOutDir: true,
  },
});
