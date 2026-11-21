/** Settings › Updates (ADR-0052, decisions D13): the box's rooms, the press rule on its forward button, the question. */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { UpdateState } from "../../ipc/update";
import { updateSample as fakeUpdate } from "./update-samples";
import { createLayers } from "../layers.svelte";
import { BY_POINTER_MS } from "../press-guard";
import TfUpdatesSection from "./TfUpdatesSection.svelte";
import { SECOND_WORDS } from "./updates";

let target: HTMLElement;
let view: ReturnType<typeof mount> | null = null;
let calls: string[];

/** A store whose state the test moves, recording each press. */
function store(first: UpdateState) {
  const box = $state({ state: first as UpdateState | null });
  const press = (name: string) => async () => void calls.push(name);
  return {
    get state() {
      return box.state;
    },
    set(next: UpdateState) {
      box.state = next;
    },
    check: press("check"),
    setCheck: async (on: boolean) => void calls.push(`check ${on}`),
    setChannel: async (channel: string) => {
      calls.push(`channel ${channel}`);
      box.state = { ...box.state!, channel: channel as UpdateState["channel"] };
    },
    download: press("download"),
    cancel: press("cancel"),
    install: press("install"),
    later: async () => {
      calls.push("later");
      box.state = { ...box.state!, marked: false };
    },
    notes: async (version: string) => ({ version, markdown: "## Notes", page_url: "https://example.invalid/r" }),
  };
}

beforeEach(() => {
  calls = [];
  target = document.createElement("div");
  target.className = "tf";
  document.body.appendChild(target);
});

afterEach(() => {
  if (view) unmount(view);
  view = null;
  target.remove();
  vi.useRealTimers();
});

async function settle() {
  for (let round = 0; round < 3; round += 1) {
    for (let i = 0; i < 10; i += 1) await Promise.resolve();
    flushSync();
  }
}

function draw(first: UpdateState, losses: () => Promise<string[]> = async () => []) {
  const s = store(first);
  view = mount(TfUpdatesSection, { target, props: { store: s, layers: createLayers(), losses, open: async () => {} } });
  flushSync();
  return s;
}

const second = () => target.querySelector<HTMLButtonElement>(".tf-upd-b2")!;
const firstButton = () => target.querySelector<HTMLButtonElement>(".tf-upd-b1")!;
const wordsOf = (button: HTMLElement) => [...button.querySelectorAll(".tf-upd-words > span")].map((span) => span.textContent).sort();

/**
 * The second button keeps its room across states: it is always drawn, and always holds every word it ever says (the
 * width is the widest's), the current one visible and the others hidden. jsdom has no layout; `just moves` measures the
 * boxes in Chrome.
 */
test("the_second_button_keeps_its_width_across_states", async () => {
  const s = draw(fakeUpdate("up-to-date"));
  const seen: string[][] = [];
  for (const name of ["up-to-date", "available", "downloading", "ready", "installing", "signature", "package"]) {
    s.set(fakeUpdate(name));
    await settle();
    const button = second();
    expect(button, name).not.toBeNull();
    const words = wordsOf(button);
    for (const word of SECOND_WORDS.filter((w) => !w.startsWith("Download "))) expect(words, name).toContain(word);
    expect(button.querySelectorAll(".tf-upd-words > span:not(.tf-upd-off)").length, name).toBe(1);
    seen.push(words.filter((word) => !/^Download( \d|$)/.test(word)));
    expect(words, name).toContain("Download 400 MB");
  }
  for (const words of seen) expect(words).toEqual(seen[0]);
  // Hidden by visibility, never removed.
  s.set(fakeUpdate("up-to-date"));
  await settle();
  expect(second().classList.contains("tf-unseen")).toBe(true);
});

/** The press rule: *Downloading…* turned into *Restart to update* by itself takes no press for 700 ms. Mutant: drop it. */
test("a_press_within_700_ms_of_the_download_ending_restarts_nothing", async () => {
  vi.useFakeTimers({ toFake: ["performance", "Date"] });
  const s = draw(fakeUpdate("downloading"));
  s.set(fakeUpdate("ready"));
  await settle();
  second().click();
  await settle();
  expect(calls).toEqual([]);
  vi.advanceTimersByTime(BY_POINTER_MS + 10);
  second().click();
  await settle();
  expect(calls).toEqual(["install"]);
});

test("a press on a state the page opened in goes through at once", async () => {
  draw(fakeUpdate("available"));
  second().click();
  await settle();
  expect(calls).toEqual(["download"]);
});

test("later_takes_the_dot_off_the_cog_and_keeps_the_download", async () => {
  const s = draw(fakeUpdate("ready"));
  firstButton().click();
  await settle();
  expect(calls).toEqual(["later"]);
  expect(s.state?.marked).toBe(false);
  expect(s.state?.phase.phase).toBe("ready");
});

test("the_question_opens_only_with_something_to_lose", async () => {
  draw(fakeUpdate("ready"));
  second().click();
  await settle();
  expect(calls).toEqual(["install"]);
  expect(document.querySelector(".tf-upd-lost")).toBeNull();
  unmount(view!);
  view = null;
  calls = [];

  draw(fakeUpdate("ready"), async () => ["2 drafts you have not sent", "The timer stops now, at 1:42:10"]);
  second().click();
  await settle();
  expect(calls).toEqual([]);
  const items = [...document.querySelectorAll(".tf-upd-lost li")].map((li) => li.textContent);
  expect(items).toEqual(["2 drafts you have not sent", "The timer stops now, at 1:42:10"]);
  const go = [...document.querySelectorAll("button")].find((button) => button.textContent === "Restart and update")!;
  go.click();
  await settle();
  expect(calls).toEqual(["install"]);
});

/** Choosing a channel keeps the row's rooms: the line under the buttons has the longest reading's room either way. */
test("choosing_a_channel_keeps_the_rows_height", async () => {
  draw(fakeUpdate("up-to-date"));
  const line = () => target.querySelector(".tf-upd-chan")!;
  expect(line().classList.contains("tf-warn")).toBe(true);
  const releases = [...target.querySelectorAll(".tf-seg button")].find((button) => button.textContent === "Releases") as HTMLButtonElement;
  releases.click();
  await settle();
  expect(calls).toEqual(["channel releases"]);
  expect(line().textContent).toBe("Finished releases only.");
  // The room is the CSS's three lines, the same element in both readings.
  expect(target.querySelectorAll(".tf-upd-chan").length).toBe(1);
});

test("the page draws in the frame it opens, with nothing read but the state", () => {
  draw(fakeUpdate("available"));
  expect(target.querySelector(".tf-upd-v")?.textContent).toBe("3.2.9-ux27");
  expect(target.querySelector(".tf-upd-says")?.textContent).toMatch(/ready to download/);
});

test("a build without keys says updates aren't set up and offers nothing", async () => {
  draw(fakeUpdate("not-set-up"));
  await settle();
  expect(target.querySelector(".tf-upd-says")?.textContent?.trim()).toBe("Updates aren't set up for this build.");
  expect(second().classList.contains("tf-unseen")).toBe(true);
  expect(target.querySelector<HTMLButtonElement>(".tf-upd-check")?.disabled).toBe(true);
});

/** The trips' finding: a download's progress, landing four times a second, must not read the notes again each time. */
test("the notes are read once per version, not on every update of the state", async () => {
  const s = draw(fakeUpdate("downloading"));
  let reads = 0;
  const notes = s.notes;
  s.notes = async (version: string) => {
    reads += 1;
    return notes(version);
  };
  await settle();
  const before = reads;
  for (const received of [50, 60, 70]) {
    s.set({ ...fakeUpdate("downloading"), phase: { phase: "downloading", received: received * 1_000_000, total: 89_000_000 } });
    await settle();
  }
  expect(reads).toBe(before);
});
