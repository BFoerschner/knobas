/**
 * The *Looking for changes…* note on a written item's page (#2615): after a write lands, knobas reads the item and its
 * linked tickets again on a bounded schedule (#2595), and the page of an item read so says it beside its *Refresh*,
 * in room it always holds, so nothing moves when the note comes or goes.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import TfDetailRefresh from "./TfDetailRefresh.svelte";
import { createDetailRefresh } from "./refresh.svelte";
import { detailOf, refreshProbe } from "./refresh.fixture";
import { forgetLooking, startLooking } from "./builds/looking.svelte";
import type { LookingAgain } from "../ipc/sources";

const WORDS = "Looking for changes…";

let target: HTMLElement;
let view: ReturnType<typeof mount> | undefined;
/** Tells `looking:again` as the backend would. */
let tell: (looking: LookingAgain) => void;

beforeEach(async () => {
  target = document.createElement("div");
  document.body.append(target);
  const handlers: ((event: { payload: unknown }) => void)[] = [];
  tell = (payload) => {
    handlers.forEach((handler) => handler({ payload }));
    flushSync();
  };
  startLooking(
    (_, handler) => {
      handlers.push(handler);
      return Promise.resolve(() => {});
    },
    () => Promise.resolve([]),
  );
  await Promise.resolve();
});

afterEach(() => {
  if (view) unmount(view);
  view = undefined;
  target.remove();
  forgetLooking();
});

function draw(props: { looking?: boolean; button?: boolean; page?: boolean } = {}): void {
  const refresh = createDetailRefresh(refreshProbe().ports);
  view = mount(TfDetailRefresh, { target, props: { detail: detailOf("jira:PAY-1"), refresh, reread: () => {}, page: true, looking: true, ...props } });
  flushSync();
}

/** The note's live region, and the words in it now. */
const region = () => target.querySelector(".tf-looking[role=status]");
const said = () => region()?.textContent?.trim() ?? null;

test("an item's page says Looking for changes… while its item is read again after a write, and not once it moved", () => {
  draw();
  expect(said(), "drawn empty before, so the words that come are announced").toBe("");
  tell({ scope: "jira:PAY-1", code: false, builds: false, items: ["jira:PAY-1", "jira:PAY-2"] });
  expect(said()).toBe(WORDS);
  tell({ scope: "jira:PAY-1", code: false, builds: false, items: ["jira:PAY-2"] });
  expect(said()).toBe("");
});

test("a linked ticket's page says so while the write's schedule reads it, and the schedule's end takes it away", () => {
  draw();
  tell({ scope: "jira:PAY-9", code: false, builds: false, items: ["jira:PAY-9", "jira:PAY-1"] });
  expect(said()).toBe(WORDS);
  tell({ scope: "jira:PAY-9", code: false, builds: false, items: [] });
  expect(said()).toBe("");
});

test("the note holds its room while it says nothing, so the Refresh beside it never moves", () => {
  draw();
  const note = region();
  expect(note?.classList.contains("tf-keepw")).toBe(true);
  expect(note?.getAttribute("data-keep")).toBe(WORDS);
  // Before the button: Refresh stays the header's last action.
  expect(note?.nextElementSibling?.classList.contains("tf-refresh")).toBe(true);
});

test("the ticket page, whose Refresh is the keybind's alone, still says it", () => {
  draw({ button: false });
  tell({ scope: "jira:PAY-1", code: false, builds: false, items: ["jira:PAY-1"] });
  expect(said()).toBe(WORDS);
  expect(target.querySelector(".tf-refresh")).toBeNull();
});

test("a detail that does not ask for the note draws none and holds no room", () => {
  draw({ looking: false });
  tell({ scope: "jira:PAY-1", code: false, builds: false, items: ["jira:PAY-1"] });
  expect(target.querySelector(".tf-looking")).toBeNull();
});
