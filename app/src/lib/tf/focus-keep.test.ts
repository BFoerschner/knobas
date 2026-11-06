/** `focus-keep`: a control that goes away doesn't drop the reader's place on the body. */
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import { installFocusKeep } from "./focus-keep";
import { restorePlace } from "./return-place";

let off: () => void = () => {};

/** A page in `.tf-main`: a heading and a table whose rows each have a link and two buttons. */
function page(rows = 3): { body: HTMLElement; heading: HTMLElement; row: (n: number) => HTMLElement } {
  const main = document.createElement("main");
  main.className = "tf-main";
  main.tabIndex = -1;
  const kept = document.createElement("div");
  kept.className = "tf-kept";
  const heading = document.createElement("h1");
  heading.textContent = "Monitors";
  const table = document.createElement("table");
  const body = document.createElement("tbody");
  table.append(body);
  for (let n = 0; n < rows; n += 1) {
    const tr = document.createElement("tr");
    tr.innerHTML = `<td><a href="#/row/${n}">Row ${n}</a></td><td><button>Pause</button><button>Hide</button></td>`;
    body.append(tr);
  }
  kept.append(heading, table);
  main.append(kept);
  document.body.append(main);
  return { body, heading, row: (n) => body.children[n] as HTMLElement };
}

const buttonIn = (row: Element, words: string): HTMLButtonElement => [...row.querySelectorAll("button")].find((b) => b.textContent === words)!;

/** The mutation's callback, and the frame it asks for. */
async function settle(): Promise<void> {
  await Promise.resolve();
  await vi.advanceTimersByTimeAsync(40);
}

beforeEach(() => {
  vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout", "requestAnimationFrame", "cancelAnimationFrame", "performance"] });
  off = installFocusKeep();
});

afterEach(() => {
  off();
  vi.useRealTimers();
  document.body.replaceChildren();
});

test("a row that leaves hands focus to the same control in the row that took its place", async () => {
  const { row } = page();
  buttonIn(row(0), "Hide").focus();
  const next = row(1);
  row(0).remove();
  await settle();
  expect(document.activeElement).toBe(buttonIn(next, "Hide"));
});

test("the last row hands focus to the row before it", async () => {
  const { row } = page();
  buttonIn(row(2), "Hide").focus();
  const before = row(1);
  row(2).remove();
  await settle();
  expect(document.activeElement).toBe(buttonIn(before, "Hide"));
});

test("a row without the same control hands focus to its first control", async () => {
  const { row } = page();
  buttonIn(row(0), "Hide").focus();
  const next = row(1);
  buttonIn(next, "Hide").remove();
  row(0).remove();
  await settle();
  expect(document.activeElement).toBe(next.querySelector("a"));
});

test("a control replaced in its row hands focus to what stands at its place there", async () => {
  const { row } = page();
  const pause = buttonIn(row(0), "Pause");
  pause.focus();
  const resume = document.createElement("button");
  resume.textContent = "Resume";
  pause.replaceWith(resume);
  await settle();
  expect(document.activeElement).toBe(resume);
});

test("a control that is disabled hands focus on, and doesn't wait to be enabled", async () => {
  const { row } = page();
  const pause = buttonIn(row(0), "Pause");
  pause.focus();
  pause.disabled = true;
  await settle();
  expect(document.activeElement).not.toBe(pause);
  expect(row(0).contains(document.activeElement)).toBe(true);
});

test("the list emptied, focus goes to the page's heading, which is no stop on the way through", async () => {
  const { row, body, heading } = page(1);
  buttonIn(row(0), "Hide").focus();
  body.replaceChildren();
  await settle();
  expect(document.activeElement).toBe(heading);
  expect(heading.getAttribute("tabindex")).toBe("-1");
});

test("a reader's own blur is left alone: nothing comes back when the page next changes", async () => {
  const { row } = page();
  const hide = buttonIn(row(0), "Hide");
  hide.focus();
  hide.blur();
  await settle();
  row(0).remove();
  await settle();
  expect(document.activeElement).toBe(document.body);
});

test("focus the page moved itself is left where it is", async () => {
  const { row } = page();
  buttonIn(row(0), "Hide").focus();
  const link = row(2).querySelector("a")!;
  row(0).remove();
  link.focus();
  await settle();
  expect(document.activeElement).toBe(link);
});

test("it stays out of an open dialog, and takes over once the dialog has gone with what opened it", async () => {
  const { row } = page();
  buttonIn(row(0), "Hide").focus();
  const next = row(1);
  const modal = document.createElement("div");
  modal.className = "tf-modal";
  modal.innerHTML = "<button>Cancel</button>";
  document.body.append(modal);
  modal.querySelector("button")!.focus();
  row(0).remove();
  await settle();
  expect(document.activeElement).toBe(modal.querySelector("button"));
  modal.remove();
  await settle();
  expect(document.activeElement).toBe(buttonIn(next, "Hide"));
});

test("a press a webview gave no focus still counts: when the dialog it opened goes, focus goes back to the control", async () => {
  const { row } = page();
  const hide = buttonIn(row(0), "Hide");
  hide.click();
  await settle();
  expect(document.activeElement).toBe(document.body);
  const modal = document.createElement("div");
  modal.className = "tf-modal";
  document.body.append(modal);
  await settle();
  modal.remove();
  await settle();
  expect(document.activeElement).toBe(hide);
});

test("a press on nothing leaves no place to come back to", async () => {
  const { row } = page();
  buttonIn(row(0), "Hide").click();
  document.querySelector("h1")!.dispatchEvent(new Event("pointerdown", { bubbles: true }));
  row(0).remove();
  await settle();
  expect(document.activeElement).toBe(document.body);
});

test("a move made by key with no place to return to focuses the new page's heading", async () => {
  const { heading } = page();
  window.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter" }));
  window.dispatchEvent(new HashChangeEvent("hashchange"));
  await settle();
  expect(document.activeElement).toBe(heading);
});

test("a move made by the pointer leaves focus alone", async () => {
  page();
  window.dispatchEvent(new Event("pointerdown"));
  window.dispatchEvent(new HashChangeEvent("hashchange"));
  await settle();
  expect(document.activeElement).toBe(document.body);
});

test("a move by key waits for a heading its page draws later, and for a launcher to finish closing", async () => {
  const main = document.createElement("main");
  main.className = "tf-main";
  document.body.append(main);
  const launcher = document.createElement("div");
  launcher.className = "tf-modal";
  document.body.append(launcher);
  window.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter" }));
  window.dispatchEvent(new HashChangeEvent("hashchange"));
  await settle();
  expect(document.activeElement).toBe(document.body);
  launcher.remove();
  await settle();
  expect(document.activeElement).toBe(document.body);
  const heading = document.createElement("h1");
  main.append(heading);
  await settle();
  expect(document.activeElement).toBe(heading);
});

test("a return to a place keeps focus its own until it is over", async () => {
  const { heading, row } = page();
  const main = document.querySelector(".tf-main")!;
  const kept = document.querySelector(".tf-kept")!;
  const link = row(1).querySelector("a")!;
  const end = restorePlace({ scrolls: [], pressed: link, to: "#/row/1", said: { text: "Row 1", nth: 0 } }, kept);
  window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
  // A key ends a return's hold, so the return here is ended by hand, after the keeper has looked.
  window.dispatchEvent(new HashChangeEvent("hashchange"));
  await settle();
  expect(main.contains(document.activeElement)).toBe(true);
  expect(document.activeElement).not.toBe(heading);
  end();
});
