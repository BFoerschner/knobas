/** The built-in *Page* renderer's frame (ADR-0046): sanitized, sandboxed, served from the render scheme by a token. */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import type { FrameBridge } from "../../render/frame";
import TfHtmlPage from "./TfHtmlPage.svelte";

let target: HTMLElement;
let app: ReturnType<typeof mount> | null = null;

beforeEach(() => {
  target = document.createElement("div");
  target.className = "tf-fileview";
  document.body.append(target);
});

afterEach(() => {
  if (app) unmount(app);
  app = null;
  target.remove();
});

const HOSTILE = `<h1>Made</h1><script>parent.__TAURI_INTERNALS__.invoke("delete_source")</script><img src=x onerror="top.location='https://evil.example'"><a href="javascript:alert(1)">go</a>`;

async function settle() {
  for (let i = 0; i < 6; i++) await Promise.resolve();
  flushSync();
}

test("a page's script, handler and javascript: link are gone, and the frame is sandboxed on the render scheme", async () => {
  const kept: string[] = [];
  const tauri: FrameBridge = { isTauri: () => true, convertFileSrc: (p, s) => `${s}://localhost/${p}` };
  app = mount(TfHtmlPage, {
    target,
    props: { html: HOSTILE, title: "index.html, drawn as a page", frame: (doc: string) => (kept.push(doc), Promise.resolve("t1")), room: ".tf-fileview", bridge: tauri },
  });
  await settle();
  const frame = target.querySelector("iframe")!;
  expect(frame.getAttribute("sandbox")).toBe("");
  expect(frame.getAttribute("src")).toBe("knobas-render://localhost/folder.t1");
  expect(frame.hasAttribute("srcdoc")).toBe(false);
  expect(kept).toHaveLength(1);
  expect(kept[0]).toContain("<h1>Made</h1>");
  expect(kept[0]).not.toMatch(/<script|onerror|javascript:|__TAURI/i);
});

test("without the app's bridge the sanitized page is the frame's own document, still sandboxed", () => {
  const local: FrameBridge = { isTauri: () => false, convertFileSrc: (p, s) => `${s}://localhost/${p}` };
  app = mount(TfHtmlPage, { target, props: { html: HOSTILE, title: "page", frame: () => Promise.reject(new Error("not asked")), room: ".tf-fileview", bridge: local } });
  flushSync();
  const frame = target.querySelector("iframe")!;
  expect(frame.getAttribute("sandbox")).toBe("");
  expect(frame.getAttribute("srcdoc")).toContain("<h1>Made</h1>");
  expect(frame.getAttribute("srcdoc")).not.toMatch(/<script|onerror|javascript:/i);
});
