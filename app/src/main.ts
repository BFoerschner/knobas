import "./lib/shell/own-frame-guard";

import { mount } from "svelte";

import App from "./App.svelte";
// Each role's face and size as chosen in Settings › Fonts, on <html> before anything draws (`fonts.svelte.ts`).
import "./lib/tf/fonts.svelte";
// The window's and code's themes on <html> (`themes.svelte.ts`); the head's script wrote the last ones before the first paint.
import "./lib/tf/themes.svelte";
import "./lib/tf/highlighter.svelte";
import "./lib/tf/theme.svelte";
import "./lib/tf/tf.css";
import { zoom } from "./lib/tf/zoom.svelte";
import { theme as windowMode } from "./lib/tf/theme.svelte";
import { themes as windowThemes } from "./lib/tf/themes.svelte";
import { watchWindowLook } from "./lib/tf/window-look.svelte";
import "./unported.css";

const target = document.getElementById("app");
if (!target) {
  throw new Error("index.html is missing the #app mount point");
}

// The kept zoom on the webview before the first draw (`zoom.svelte.ts`): one call, and none at 100 %.
await zoom.boot();

// The bare window keeps the theme's page colour for the next start (ADR-0055).
watchWindowLook(windowThemes, windowMode);

export default mount(App, { target });
