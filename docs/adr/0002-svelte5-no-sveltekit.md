---
status: accepted
---

# Svelte 5 + Vite, without SvelteKit

The frontend is a Tauri 2 webview that ports a behaviour-rich HTML/CSS mockup region by region. Decided 2026-08-24 (Claude, under delegation), ratified 2026-08-27 (Björn): **Svelte 5 runes on plain Vite, no SvelteKit.** Runes are fine-grained signals, so the mockup's imperative state logic maps about 1:1; `mount()` ports regions as islands; the mockup's CSS carries over as a global stylesheet.

## Rejected

- React. Runtime and StrictMode friction for an app that is mostly imperative state transitions.
- Solid. 2.0 was at RC when the choice was made.
- SvelteKit. Routing and SSR machinery a desktop webview never uses; the app's router is a hash router over entity addresses.
