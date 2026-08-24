import { svelte } from "@sveltejs/vite-plugin-svelte";
import { defineConfig } from "vite";

// Tauri owns this dev server: `tauri.conf.json`'s `devUrl` points at the port
// below, so it is fixed and `strictPort` makes a clash a hard error rather
// than a silent move to 1421 that the window would then fail to load.
export default defineConfig(({ mode }) => ({
  plugins: [svelte()],
  // Tauri's CLI prints the Rust build output into the same terminal; letting
  // Vite clear the screen would eat compiler errors.
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
  },
  build: {
    // The only consumer is the WebView bundled with the app, so there is no
    // legacy browser to down-level for.
    target: "es2022",
    // Development only. `generate_context!` embeds everything under `dist/`
    // into the binary, and a source map carries the full TypeScript and Svelte
    // source with it -- a 460 kB shipped copy of the frontend nobody asked for.
    sourcemap: mode !== "production",
  },
}));
