import "./lib/shell/own-frame-guard";

import { mount } from "svelte";

import App from "./App.svelte";
// Each role's face and size as chosen in Settings › Fonts, on <html> before anything draws (`fonts.svelte.ts`).
import "./lib/tf/fonts.svelte";
import "./lib/tf/tf.css";
import { zoom } from "./lib/tf/zoom.svelte";
import "./unported.css";

const target = document.getElementById("app");
if (!target) {
  throw new Error("index.html is missing the #app mount point");
}

// The kept zoom on the webview before the first draw (`zoom.svelte.ts`): one call, and none at 100 %.
await zoom.boot();

export default mount(App, { target });
