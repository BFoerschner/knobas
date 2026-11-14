/**
 * How many dialogs are open (`TfDialog`), for what draws differently under one: the toasts sit lower and show only the
 * newest. A count the dialogs keep, not `body:has(.tf-modal)`: a `:has()` on the body made Chromium restyle the page's
 * rows each time one entered a list (PR #2479).
 */
export const modals = $state({ open: 0 });
