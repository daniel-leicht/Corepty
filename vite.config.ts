import { readFileSync } from "node:fs";
import { defineConfig } from "vite";

const version = JSON.parse(readFileSync("./package.json", "utf-8")).version;

// Vite config tuned for Tauri: fixed dev port, no clobbering the Tauri console,
// and don't watch the Rust side.
export default defineConfig({
  clearScreen: false,
  // Expose the package version to the app via the `__APP_VERSION__` global.
  define: {
    __APP_VERSION__: JSON.stringify(version),
  },
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
