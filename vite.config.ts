import { defineConfig } from "vite";

// Vite config tuned for Tauri: fixed dev port, no clobbering the Tauri console,
// and don't watch the Rust side.
export default defineConfig({
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: false,
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },
  // Produce assets Tauri can embed as a relative bundle.
  build: {
    target: "es2021",
    minify: "esbuild",
    sourcemap: false,
  },
});
