/**
 * The Time view's address (#1504): `#/time/standup/<date>` opens the Standup
 * at that day, the way `#/time/week/<date>` opens a week. Nothing is read:
 * the database is not ready, so the tab draws its day and asks nothing.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import TfTime from "./TfTime.svelte";

let target: HTMLDivElement;
let view: ReturnType<typeof mount> | null = null;

beforeEach(() => {
  target = document.createElement("div");
  document.body.append(target);
});
afterEach(() => {
  if (view) unmount(view);
  view = null;
  target.remove();
});

function open(rest: string[]) {
  view = mount(TfTime, { target, props: { ready: false, rest, timer: { current: null }, ontimer: undefined } });
  flushSync();
}

test("#/time/standup/<date> opens the Standup tab on that day's protocol", () => {
  open(["standup", "2026-09-03"]);
  expect(target.querySelector(".tf-tabs [aria-current='page']")?.textContent).toBe("Standup");
  expect(target.querySelector(".tf-su-day")?.textContent).toBe("Thursday 3 September");
  expect(target.querySelector(".tf-fhead .tf-sub")?.textContent).toContain("Your standup for Thursday 3 September");
});

test("a word that is not a date after standup is today's Standup", () => {
  open(["standup", "yesterday"]);
  expect(target.querySelector(".tf-tabs [aria-current='page']")?.textContent).toBe("Standup");
  expect(target.querySelector(".tf-su-day")?.textContent).not.toBe("Thursday 3 September");
});
