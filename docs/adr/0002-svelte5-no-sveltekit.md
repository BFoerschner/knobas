---
status: accepted
---

# Svelte 5 + Vite, without SvelteKit

The frontend is a Tauri 2 desktop webview porting an existing, behaviour-rich HTML/CSS mockup region by region. Decided 2026-08-24 (Claude, under delegation), ratified 2026-08-27 (Björn): Svelte 5 (runes) with plain Vite — no SvelteKit. Runes are fine-grained signals, so the mockup's "mutate this when that changes" logic maps roughly 1:1; `mount()` lets regions port incrementally as islands; the mockup's CSS carries over wholesale as a global stylesheet.

## Considered options

- **React** — rejected: runtime + StrictMode friction for an app that is mostly imperative state transitions.
- **Solid** — rejected on timing: 2.0 was at RC when the choice was made.
- **SvelteKit on top of Svelte** — rejected: routing/SSR machinery a desktop webview never uses; the app's router is a hash router over stable entity addresses.
