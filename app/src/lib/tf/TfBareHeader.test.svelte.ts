/**
 * A view's header while it has no item drawn (#2286, #2287, review of #2238):
 * the view's name and its *Sync now*, where every header puts it. Builds and
 * Docs draw it; it was a copy in each.
 */
import { createRawSnippet, flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import TfBareHeader from "./TfBareHeader.svelte";

const sync = createRawSnippet(() => ({ render: () => `<button type="button" class="tf-btn tf-syncnow">Sync now</button>` }));

let target: HTMLElement;
let view: ReturnType<typeof mount> | undefined;

beforeEach(() => {
  target = document.createElement("div");
  document.body.append(target);
});

afterEach(() => {
  if (view) unmount(view);
  view = undefined;
  target.remove();
});

test("the view's name on the left, its Sync now last in the header's actions", () => {
  view = mount(TfBareHeader, { target, props: { title: "Builds", sync } });
  flushSync();
  expect(target.querySelector("header.tf-fhead > div > h1")?.textContent).toBe("Builds");
  const acts = target.querySelector("header.tf-fhead > .tf-acts");
  expect(acts?.lastElementChild?.textContent).toBe("Sync now");
});

test("with no Sync now, no empty actions group", () => {
  view = mount(TfBareHeader, { target, props: { title: "Docs" } });
  flushSync();
  expect(target.querySelector("header.tf-fhead h1")?.textContent).toBe("Docs");
  expect(target.querySelector(".tf-acts")).toBeNull();
});
