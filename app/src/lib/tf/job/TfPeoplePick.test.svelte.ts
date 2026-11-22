/**
 * The people picker (ADR-0054, section 25): it opens at once with *Me*, *Unassigned* and the current assignee, the
 * source's people join below when the list lands with nothing drawn moving, typing ranks by name and account, and the
 * keys pick, clear and close.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import { createLayers } from "../layers.svelte";
import TfPeoplePick, { ASK_AFTER_MS, initials } from "./TfPeoplePick.svelte";
import type { FieldOption } from "../../ipc/entity";

let target: HTMLElement;
let opener: HTMLButtonElement;
let view: ReturnType<typeof mount> | null = null;

beforeEach(() => {
  target = document.createElement("div");
  opener = document.createElement("button");
  document.body.append(opener, target);
});

afterEach(() => {
  if (view) void unmount(view);
  view = null;
  target.remove();
  opener.remove();
  vi.useRealTimers();
});

const PEOPLE: FieldOption[] = [
  { value: "olu", label: "Olu Adeyemi", hint: "olu" },
  { value: "lena", label: "Lena Hoffmann", hint: "lena" },
  { value: "mara", label: "Mara Lindqvist", hint: "mara" },
  { value: "ben", label: "Ben Okafor", hint: "ben" },
];

function props(over: Record<string, unknown> = {}) {
  const state = $state({
    open: true,
    opener,
    label: "Assignee",
    current: { value: "ben", shown: "Ben Okafor" },
    me: "mara",
    options: [] as FieldOption[],
    complete: false,
    asking: true,
    failed: null as string | null,
    sourceName: "Jira",
    scope: "PAY",
    onpick: vi.fn(),
    onsearch: vi.fn(),
    layers: createLayers(),
    ...over,
  });
  return state;
}

const names = () => [...document.querySelectorAll(".tf-pick-opt .tf-pick-name")].map((n) => n.textContent);
const field = () => document.querySelector<HTMLInputElement>(".tf-pick input")!;
function key(name: string) {
  field().dispatchEvent(new KeyboardEvent("keydown", { key: name, bubbles: true, cancelable: true }));
  flushSync();
}
function type(text: string) {
  field().value = text;
  field().dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
}

/** Mutant: the list's landing sorting *Me* away from the top. */
test("it opens with three rows before the list lands, and keeps their places after", () => {
  const p = props();
  view = mount(TfPeoplePick, { target, props: p });
  flushSync();
  expect(names()).toEqual(["Me (mara)", "Unassigned", "Ben Okafor"]);
  const before = [...document.querySelectorAll(".tf-pick-opt")].map((row) => row.textContent);
  p.options = PEOPLE;
  p.complete = true;
  p.asking = false;
  flushSync();
  const after = names();
  expect(after.slice(0, 3)).toEqual(["Me (Mara Lindqvist)", "Unassigned", "Ben Okafor"]);
  expect(after.slice(3)).toEqual(["Lena Hoffmann", "Olu Adeyemi"]);
  expect(before).toHaveLength(3);
  expect(document.querySelector(".tf-pick-foot")?.textContent).toBe("4 people who can be assigned in PAY");
});

test("a required field leads with no Unassigned", () => {
  view = mount(TfPeoplePick, { target, props: props({ required: true }) });
  flushSync();
  expect(names()).not.toContain("Unassigned");
});

test("typing ranks by name and account, marks the letters, and Enter picks and closes", () => {
  const p = props({ options: PEOPLE, complete: true, asking: false });
  view = mount(TfPeoplePick, { target, props: p });
  flushSync();
  type("le");
  expect(names()[0]).toBe("Lena Hoffmann");
  expect(document.querySelector(".tf-pick-opt mark")?.textContent).toBe("Le");
  key("Enter");
  expect(p.onpick).toHaveBeenCalledWith("lena", "Lena Hoffmann");
  expect(p.open).toBe(false);
  expect(document.activeElement).toBe(opener);
});

test("Unassigned picks nobody, and ↓ moves to it", () => {
  const p = props({ options: PEOPLE, complete: true, asking: false });
  view = mount(TfPeoplePick, { target, props: p });
  flushSync();
  key("ArrowDown");
  key("Enter");
  expect(p.onpick).toHaveBeenCalledWith(null, "Unassigned");
});

test("an Enter on arrival, the opener's second press, picks nobody; one after the hold picks the row", () => {
  vi.useFakeTimers();
  const p = props({ options: PEOPLE, complete: true, asking: false });
  view = mount(TfPeoplePick, { target, props: p });
  flushSync();
  key("Enter");
  expect(p.onpick).not.toHaveBeenCalled();
  vi.advanceTimersByTime(800);
  key("Enter");
  expect(p.onpick).toHaveBeenCalledTimes(1);
});

test("Esc clears the words, then closes", () => {
  const p = props({ options: PEOPLE, complete: true, asking: false });
  view = mount(TfPeoplePick, { target, props: p });
  flushSync();
  type("ol");
  key("Escape");
  expect(field().value).toBe("");
  expect(p.open).toBe(true);
  key("Escape");
  expect(p.open).toBe(false);
});

test("a list that is not whole asks the source 150 ms after the last key, once", () => {
  vi.useFakeTimers();
  const p = props({ options: PEOPLE, complete: false, asking: false });
  view = mount(TfPeoplePick, { target, props: p });
  flushSync();
  type("a");
  vi.advanceTimersByTime(ASK_AFTER_MS - 50);
  type("al");
  vi.advanceTimersByTime(ASK_AFTER_MS - 1);
  expect(p.onsearch).not.toHaveBeenCalled();
  vi.advanceTimersByTime(1);
  expect(p.onsearch).toHaveBeenCalledTimes(1);
  expect(p.onsearch).toHaveBeenCalledWith("al");
});

test("a whole list asks the source nothing as a person types", () => {
  vi.useFakeTimers();
  const p = props({ options: PEOPLE, complete: true, asking: false });
  view = mount(TfPeoplePick, { target, props: p });
  flushSync();
  type("al");
  vi.advanceTimersByTime(1000);
  expect(p.onsearch).not.toHaveBeenCalled();
});

test("a failed read leaves Me, Unassigned and the current assignee, with a line saying why", () => {
  view = mount(TfPeoplePick, { target, props: props({ asking: false, failed: "Jira did not answer" }) });
  flushSync();
  expect(names()).toEqual(["Me (mara)", "Unassigned", "Ben Okafor"]);
  expect(document.querySelector(".tf-pick-foot")?.textContent).toBe("Jira's list could not be read: Jira did not answer");
});

test("initials are a name's first and last letters", () => {
  expect(initials("Mara Lindqvist")).toBe("ML");
  expect(initials("Me (Ben Okafor)")).toBe("BO");
  expect(initials("olu")).toBe("OL");
});
