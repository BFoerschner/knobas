import { createRawSnippet, flushSync, mount, unmount } from "svelte";
import { afterEach, expect, test, vi } from "vitest";

import { isKey } from "./TfKey.svelte";
import TfScroller from "./TfScroller.svelte";

let view: ReturnType<typeof mount> | undefined;
let target: HTMLElement;

afterEach(() => {
  if (view) unmount(view);
  view = undefined;
  target.remove();
});

const words = createRawSnippet(() => ({ render: () => "<p>words</p>" }));

function draw(props: Record<string, unknown>) {
  target = document.createElement("div");
  document.body.append(target);
  view = mount(TfScroller, { target, props: { class: "tf-bmain", children: words, ...props } as never });
  flushSync();
}

test("the host holds the scroller, which holds the clip and the one body", () => {
  draw({});
  const body = target.querySelector(".tf-shost > .tf-scroller.tf-bmain > .tf-sclip > .tf-sbody");
  expect(body?.textContent).toBe("words");
});

test("attributes reach the scroller, not the host", () => {
  const onscroll = vi.fn();
  draw({ onscroll });
  target.querySelector(".tf-bmain")!.dispatchEvent(new Event("scroll"));
  expect(onscroll).toHaveBeenCalledOnce();
});

test("a key is told from a name by its shape", () => {
  for (const key of ["PAY-231", "OPS-77", "payout-service#143"]) expect(isKey(key), key).toBe(true);
  for (const name of ["Tickets", "Code", "Payout stack", "payout-service", "PR #142"]) expect(isKey(name), name).toBe(false);
});
