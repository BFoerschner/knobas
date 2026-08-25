import { svelte } from "@sveltejs/vite-plugin-svelte";
import { defineConfig } from "vitest/config";

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
    // Vite inlines assets under 4 kB as `data:` URIs. `default-src 'self'` has
    // no `data:` for fonts, so an inlined woff2 is a font that silently fails
    // to load in a bundled build and works perfectly in dev -- the worst shape
    // a bug can have here, because `just dev` gets no CSP at all.
    assetsInlineLimit: 0,
  },
  test: {
    environment: "jsdom",
    // Two suffixes, and the second one is load-bearing. Runes (`$state`) are
    // compiler syntax: vite-plugin-svelte only compiles modules whose name
    // ends in `.svelte.ts`, so a test that drives a component through a
    // reactive props object has to be called `*.test.svelte.ts`. A plain
    // `*.test.ts` is passed through untouched and `$state` is an undefined
    // function at run time.
    include: ["src/**/*.test.ts", "src/**/*.test.svelte.ts"],
    setupFiles: ["src/lib/shell/test-setup.ts"],
  },
  // Vitest must resolve Svelte's *browser* build, or `mount` runs the SSR
  // entry point and produces no DOM.
  resolve: { conditions: ["browser"] },
}));
