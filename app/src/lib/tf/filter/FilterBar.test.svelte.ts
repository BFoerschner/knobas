/**
 * The filter bar (#810): typing keeps focus and the caret while the view
 * below re-renders, `/` focuses the field, and chips, dropdowns, the scope
 * and the count are what the config says.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import type { FilterConfig, FilterState } from "./filter";
import { toQuery } from "./filter";
import FilterBar from "./FilterBar.svelte";
import { sessionFilter } from "./filter-state.svelte";

let target: HTMLElement;
let bar: ReturnType<typeof mount> | null = null;

beforeEach(() => {
  target = document.createElement("div");
  document.body.appendChild(target);
});

afterEach(() => {
  if (bar) unmount(bar);
  bar = null;
  target.remove();
});

const config: FilterConfig = {
  id: "tickets-all",
  placeholder: "Filter by key, summary, epic or person",
  noun: "tickets",
  base: "",
  scopes: [
    { id: "mine", label: "Mine", query: "assignee:@me" },
    { id: "all", label: "All", query: "" },
  ],
  chips: [{ key: "issuetype", label: "Type", options: ["Story", "Bug", "Task"].map((v) => ({ value: v, label: v })) }],
  dropdowns: [{ key: "assignee", label: "Assignee", options: [{ value: "@me", label: "Me" }] }],
};

/** A bar whose `shown` follows every change, so the parent re-renders on each keystroke. */
function mountBar(state: FilterState) {
  const props = $state({ shown: 75, total: 75, changes: 0 });
  bar = mount(FilterBar, {
    target,
    props: {
      config,
      filter: state,
      get shown() {
        return props.shown;
      },
      get total() {
        return props.total;
      },
      onchange: () => {
        props.changes += 1;
        props.shown = Math.max(0, 75 - state.text.length * 10);
      },
    },
  });
  flushSync();
  return props;
}

function field(): HTMLInputElement {
  return target.querySelector<HTMLInputElement>("input[data-filter]")!;
}

/** One typed character at the caret, as the browser does it: value, caret, then `input`. */
function type(input: HTMLInputElement, char: string) {
  const at = input.selectionStart ?? input.value.length;
  input.value = input.value.slice(0, at) + char + input.value.slice(at);
  input.setSelectionRange(at + 1, at + 1);
  input.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
}

test("typing several characters keeps focus and the caret where they were typed", () => {
  const state = sessionFilter(config.id);
  const props = mountBar(state);
  const input = field();
  input.focus();

  for (const char of "sepa") type(input, char);
  expect(state.text).toBe("sepa");
  expect(document.activeElement).toBe(input);
  expect(input.selectionStart).toBe(4);
  expect(props.changes).toBe(4);
  expect(target.querySelector(".tf-fcount")!.textContent).toBe("35 of 75 tickets");

  // Into the middle: the caret stays after what was typed, not at the end.
  input.setSelectionRange(2, 2);
  type(input, "x");
  type(input, "y");
  expect(input.value).toBe("sexypa");
  expect(state.text).toBe("sexypa");
  expect(document.activeElement).toBe(input);
  expect(input.selectionStart).toBe(4);
  expect(input.selectionEnd).toBe(4);
});

test("the bar claims no / and draws no / hint: / is the shell's, and opens the launcher with the section's filter (#2425)", () => {
  mountBar(sessionFilter(config.id));
  const event = new KeyboardEvent("keydown", { key: "/", bubbles: true, cancelable: true });
  document.body.dispatchEvent(event);
  expect(document.activeElement).not.toBe(field());
  expect(event.defaultPrevented).toBe(false);
  expect(target.querySelector("kbd")).toBeNull();
});

test("with search off the bar draws no text field, and keeps its chips and scope (#2425)", () => {
  const quiet: FilterConfig = { ...config, id: "tickets-quiet", search: false };
  bar = mount(FilterBar, { target, props: { config: quiet, filter: sessionFilter(quiet.id), shown: 1, total: 1 } });
  flushSync();
  expect(target.querySelector("input")).toBeNull();
  expect(target.querySelector(".tf-fsearch")).toBeNull();
  expect(target.querySelectorAll('[aria-label="Scope"] button')).toHaveLength(2);
  expect(target.querySelectorAll('[aria-label="Type"] button').length).toBeGreaterThan(0);
});

test("the field is the one the Esc ladder clears, and the clear button empties it in place", () => {
  const state = sessionFilter(config.id);
  mountBar(state);
  const input = field();
  expect(input.hasAttribute("data-filter")).toBe(true);
  input.focus();
  type(input, "s");
  target.querySelector<HTMLButtonElement>('button[aria-label="Clear filter"]')!.click();
  flushSync();
  expect(state.text).toBe("");
  expect(input.value).toBe("");
  expect(document.activeElement).toBe(input);
});

test("chips, a dropdown and the scope write the state the view's query is built from", () => {
  const state = sessionFilter(config.id);
  mountBar(state);
  const chip = (label: string) =>
    [...target.querySelectorAll<HTMLButtonElement>('[aria-label="Type"] button')].find((b) => b.textContent === label)!;
  chip("Bug").click();
  chip("Story").click();
  flushSync();
  expect(chip("Bug").getAttribute("aria-pressed")).toBe("true");
  expect(chip("Task").getAttribute("aria-pressed")).toBe("false");

  const select = target.querySelector<HTMLSelectElement>(".tf-fsel select")!;
  expect(select.options[0]!.textContent).toBe("Any");
  select.value = "@me";
  select.dispatchEvent(new Event("change", { bubbles: true }));
  flushSync();
  expect(select.classList.contains("tf-set")).toBe(true);

  const scope = [...target.querySelectorAll<HTMLButtonElement>('[aria-label="Scope"] button')];
  expect(scope.map((b) => b.getAttribute("aria-pressed"))).toEqual(["true", "false"]);
  scope[1]!.click();
  flushSync();

  expect(toQuery(config, state)).toBe("issuetype:Bug issuetype:Story assignee:@me");
  chip("Bug").click();
  flushSync();
  expect(toQuery(config, state)).toBe("issuetype:Story assignee:@me");
});

test("a view's filter is kept for the session: a second bar for it opens where the first left it", () => {
  const state = sessionFilter(config.id);
  mountBar(state);
  field().focus();
  type(field(), "p");
  unmount(bar!);
  bar = null;

  mountBar(sessionFilter(config.id));
  expect(field().value).toBe("p");
  expect(sessionFilter("tickets-other").text).toBe("");
});

test("the count holds the room its longest text needs, and stays with the bar's selects when the row wraps", () => {
  mountBar(sessionFilter("tail-with-selects"));
  const count = target.querySelector<HTMLElement>(".tf-fcount")!;
  expect(count.dataset.room).toBe("74 of 75 tickets");
  expect(count.closest(".tf-ftail")?.querySelector("select[aria-label=Assignee]")).not.toBeNull();
  expect(target.querySelector(".tf-ftail .tf-fchips"), "pills stay ahead of a select").toBeNull();
});

test("with no select, the last pills keep the count company, so it never wraps alone", () => {
  const pills: FilterConfig = { ...config, id: "tail-with-pills", dropdowns: [] };
  bar = mount(FilterBar, { target, props: { config: pills, filter: sessionFilter(pills.id), shown: 1, total: 2 } });
  flushSync();
  expect(target.querySelector(".tf-ftail .tf-fchips")?.getAttribute("aria-label")).toBe("Type");
  expect(target.querySelectorAll(".tf-fchips")).toHaveLength(1);
});
