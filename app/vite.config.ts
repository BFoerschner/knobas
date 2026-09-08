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
    // **Two documents, not one** (#503). `index.html` is the shell; the second
    // is the capture window, which `knobas_app::capture` opens as its own
    // Tauri window and which mounts one component with none of the shell's
    // subscriptions behind it. Named here because a Vite build with no
    // `input` emits `index.html` alone, and the missing page would show up
    // only as a blank always-on-top rectangle in a bundled build --
    // `just dev` serves both from the filesystem and would never say so.
    rollupOptions: {
      input: {
        index: "index.html",
        capture: "capture.html",
      },
    },
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
    // One zone for every run, rather than whatever zone the person or the
    // runner happened to be in. `getTimezoneOffset` is ambient input, so an
    // unpinned suite tests a different thing on a laptop than on a runner:
    // #406 was an assertion that built `-0` and could only be wrong at an
    // offset of zero, green in Berlin for as long as no runner ran and red on
    // the first one that did. Pinning makes the zone a decision.
    //
    // Berlin and not UTC because it is the zone the app is developed and read
    // in: a fixture's `09:00` means on screen what it means in the test, and
    // the offset is positive, which is the sign the backend is given -- so the
    // ordinary case is the one under test rather than a degenerate one. The
    // cost is real and worth naming: no run now sits at an offset of zero, so
    // #406's own shape is witnessed by nothing except deliberately asking for
    // it with `TZ=UTC npx vitest run`.
    //
    // This is Vitest setting `process.env.TZ` in each worker and Node
    // re-reading it. `test-setup.ts` refuses to run if that ever stops
    // working, because the failure is otherwise silent.
    env: { TZ: "Europe/Berlin" },
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
    // Four forks, not the eleven Vitest picks on its own: `resolveMaxWorkers`
    // is `os.availableParallelism() - 1` outside watch mode, each fork boots
    // its own jsdom, and that is the whole box. The gate never has the whole
    // box. `front` runs before the cargo half of `just check`, so the build
    // beside these forks is always a neighbour's, and the working model's
    // concurrency section measures one `cargo test --workspace` at ~4.7
    // cores; eleven jsdoms beside that is how a fork times out before it
    // starts -- `Failed to start forks worker`, `Timeout waiting for worker
    // to respond`, a red gate with zero assertion failures (#249). Measured
    // on the otherwise idle 12-core box (baseline load 2-4, OrbStack):
    // 11 forks 7.3-9.0 s wall, 6 forks 9.5 s, 4 forks 12.0-12.3 s, 3 forks
    // 15.5 s. Four costs 3-5 s, and four forks plus the main process leave
    // seven cores, the ~4.7-core build with the baseline load of 2-4 beside
    // it; six would leave five, the build alone with nothing over for that
    // baseline. Measured under that load (a cold `cargo test --workspace`
    // in another target dir, load 6-19): three full `just check` runs, zero
    // worker-start failures, vitest 12-22 s. A number rather than a
    // percentage because the budget it has to fit is stated in absolute
    // cores (the working model's ~4.7 for a build on this box, and the
    // baseline measured above), so the cap is cores minus a neighbour, not
    // a share of cores; `"33%"` lands on 4 here only because
    // `Math.round(0.33 * 12)` happens to.
    maxWorkers: 4,
  },
  // Vitest must resolve Svelte's *browser* build, or `mount` runs the SSR
  // entry point and produces no DOM.
  resolve: { conditions: ["browser"] },
}));
