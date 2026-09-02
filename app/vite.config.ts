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
    // Pinned, not inherited. `test-setup.ts`'s rejection guard is the *first*
    // `afterEach` registered, and it only works because `"stack"` unwinds
    // `afterEach` in reverse, so it runs after each file's own teardown --
    // including the `vi.useRealTimers()` in `launcher/session` and
    // `shell/toasts`. Under `"list"` it would run first, and its
    // `setTimeout(0)` would wait on a clock those files still have frozen:
    // measured, that is a 10 s hook timeout per test rather than a leak
    // anyone can read. Relying on the default here means the guard's
    // correctness is a Vitest release note away from changing.
    sequence: { hooks: "stack" },
    // Four forks, not the eleven Vitest picks on its own. Its non-watch
    // default is `os.availableParallelism() - 1` (`getDefaultThreadsCount`),
    // each fork boots its own jsdom, and that is the whole box. The gate never
    // has the whole box: the working model's concurrency section budgets a
    // neighbour's `cargo test --workspace` at ~4.7 cores and allows two of
    // them, and eleven jsdoms on top of that is how a fork times out before it
    // starts -- `Failed to start forks worker`, `Timeout waiting for worker to
    // respond`, and a red gate with zero assertion failures (#249). Measured
    // idle on the 12-core box (load 2-4): 11 forks 7.3-9.0 s wall, 6 forks
    // 9.5 s, 4 forks 12.0-12.3 s, 3 forks 15.5 s. Four costs 3-5 s and leaves
    // seven cores for whoever else is building. A number rather than a
    // percentage because the thing that has to fit beside this pool is an
    // absolute cost (one cargo build is ~4.7 cores on any machine), so the
    // cap is cores minus neighbours, not a share of cores; `"33%"` lands on 4
    // here only because `Math.round(0.33 * 12)` happens to.
    maxWorkers: 4,
  },
  // Vitest must resolve Svelte's *browser* build, or `mount` runs the SSR
  // entry point and produces no DOM.
  resolve: { conditions: ["browser"] },
}));
