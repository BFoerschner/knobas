/**
 * What jsdom is missing that the shell asks for on mount.
 *
 * Loaded by `vite.config.ts`'s `test.setupFiles`, so every suite gets it
 * without importing anything.
 */

// jsdom has no `matchMedia`, and the shell asks it about reduced motion the
// moment a `Flap` mounts. Without this a component test fails inside Svelte's
// own effect runner, which reports it as an unrelated mount error.
if (!window.matchMedia) {
  Object.defineProperty(window, "matchMedia", {
    writable: true,
    configurable: true,
    value: (query: string) => ({
      matches: false,
      media: query,
      onchange: null,
      addListener: () => {},
      removeListener: () => {},
      addEventListener: () => {},
      removeEventListener: () => {},
      dispatchEvent: () => false,
    }),
  });
}
