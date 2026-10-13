/**
 * This runner's jsdom has no `localStorage` (see `SourcesView.test`), and
 * Node's own global answers the lookup with an `ExperimentalWarning` instead.
 * Saying *none* before anything is evaluated keeps the review drafts' lookup
 * (#709) quiet; a suite that needs a storage installs one, as
 * `SourcesView.test` does. Imported first by `test-setup.ts`.
 */
Object.defineProperty(window, "localStorage", { value: undefined, configurable: true, writable: true });
