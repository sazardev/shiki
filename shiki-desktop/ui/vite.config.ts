import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

// Tauri expects a fixed dev port (tauri.conf.json's build.devUrl) and dies
// if Vite silently picks the next one; clearScreen:false keeps Rust compiler
// errors from wiping the terminal.
export default defineConfig({
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
  },
  build: {
    target: "es2021",
    outDir: "dist",
    emptyOutDir: true,
  },
  plugins: [svelte()],
});
