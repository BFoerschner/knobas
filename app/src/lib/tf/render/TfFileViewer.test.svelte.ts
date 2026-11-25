/** The file viewer (#2415): XML or JSON text is drawn by the XML view's inline form and folds (D1); other text as lines. */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import TfFileViewer from "./TfFileViewer.svelte";
import { createLayers } from "../layers.svelte";

let target: HTMLElement;
let view: ReturnType<typeof mount> | null = null;
beforeEach(() => {
  target = document.createElement("div");
  document.body.appendChild(target);
  Element.prototype.scrollIntoView = function () {};
});
afterEach(() => {
  if (view) unmount(view);
  view = null;
  target.remove();
});

function show(name: string, text: string) {
  view = mount(TfFileViewer, { target, props: { name, kind: "TXT", content: { kind: "text", text, truncated: false }, download: () => {}, onclose: () => {}, layers: createLayers() } });
  flushSync();
}

test("a ticket's XML attachment folds in the viewer, a JSON one too; plain text stays numbered lines", async () => {
  show("order-17.xml", `<?xml version="1.0"?>\n<order>\n  <item>\n    <q>1</q>\n  </item>\n</order>`);
  await vi.waitFor(() => expect(document.querySelector(".tf-vdoc .tf-xv-inline .tf-xv-caret")).not.toBeNull());
  expect(document.querySelector(".tf-vstage > pre.tf-code")).toBeNull();
  unmount(view!);
  view = null;
  show("export.json", `{\n  "orders": [\n    1,\n    2\n  ]\n}`);
  await vi.waitFor(() => expect(document.querySelector(".tf-vdoc .tf-xv-inline .tf-xv-caret")).not.toBeNull());
  unmount(view!);
  view = null;
  show("notes.txt", "one\ntwo");
  expect(document.querySelector(".tf-vdoc")).toBeNull();
  expect([...document.querySelectorAll(".tf-vstage > pre.tf-code .tf-l")].map((l) => l.textContent)).toEqual(["1one", "2two"]);
});
