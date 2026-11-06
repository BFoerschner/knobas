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

test("a row without the same control takes focus itself, so a second Enter presses nothing it didn't aim at", async () => {
  const { row } = page();
  buttonIn(row(0), "Hide").focus();
  const next = row(1);
  buttonIn(next, "Hide").remove();
  row(0).remove();
  await settle();
  expect(document.activeElement).toBe(next);
});

test("a control replaced in its row hands focus to the row, so a second Enter presses nothing it didn't aim at (F-447, F-340)", async () => {
  const { row } = page();
  const pause = buttonIn(row(0), "Pause");
  pause.focus();
  const resume = document.createElement("button");
  resume.textContent = "Resume";
  pause.replaceWith(resume);
  await settle();
  expect(document.activeElement).toBe(row(0));
  expect(row(0).getAttribute("tabindex")).toBe("-1");
});

test("a control that goes from a row that stays keeps focus in that row, and never hands it to a neighbour (F-646)", async () => {
  const { row } = page();
  const first = row(0);
  first.innerHTML = `<td>VS Code</td><td><button>Make default</button></td>`;
  row(1).innerHTML = `<td>Terminal</td><td><button>Make default</button></td>`;
  first.querySelector("button")!.focus();
  first.querySelector("button")!.remove();
  await settle();
  expect(document.activeElement).toBe(first);
});

test("a control that goes from a row with a link doesn't land on the link (F-447)", async () => {
  const { row } = page();
  const first = row(0);
  buttonIn(first, "Pause").remove();
  const hide = buttonIn(first, "Hide");
  hide.focus();
  hide.remove();
  await settle();
  expect(document.activeElement).toBe(first);
});

test("a row redrawn under its own key keeps focus, and a neighbour doesn't take it", async () => {
  const { row } = page();
  const key = row(0).querySelector("a")!.getAttribute("href")!;
  buttonIn(row(0), "Hide").focus();
  const again = row(0).cloneNode(true) as HTMLElement;
  buttonIn(again, "Hide").remove();
  row(0).replaceWith(again);
  await settle();
  expect(document.activeElement).toBe(again);
  expect(again.querySelector("a")!.getAttribute("href")).toBe(key);
});

/** Controls with no row: a bar under the page's heading. */
function bar(): { bar: HTMLElement; main: HTMLElement } {
  const main = document.createElement("main");
  main.className = "tf-main";
  const kept = document.createElement("div");
  kept.className = "tf-kept";
  kept.innerHTML = `<h1>Standup</h1><section><div class="bar"><button>Day after</button><a href="#/x">Open the page</a></div></section>`;
  main.append(kept);
  document.body.append(main);
  return { bar: kept.querySelector<HTMLElement>(".bar")!, main };
}

test("a control with no row, disabled where it stands, gives focus to what holds it and not to what follows it (F-340)", async () => {
  const { bar: holder } = bar();
  const press = holder.querySelector("button")!;
  press.focus();
  press.disabled = true;
  await settle();
  expect(document.activeElement).toBe(holder);
  expect(holder.getAttribute("tabindex")).toBe("-1");
});

test("a control with no row swapped for another with the same words goes to those words", async () => {
  const { bar: holder } = bar();
  const press = holder.querySelector("button")!;
  press.focus();
  const twin = document.createElement("button");
  twin.textContent = "Day after";
  press.replaceWith(twin);
  await settle();
  expect(document.activeElement).toBe(twin);
});

test("a control with no row whose whole group went gives focus to the nearest holder left, not the heading (F-340)", async () => {
  const { bar: holder } = bar();
  const section = holder.parentElement!;
  holder.querySelector("button")!.focus();
  holder.remove();
  await settle();
  expect(document.activeElement).toBe(section);
});

test("a control with no row and nothing left of what held it falls to the heading", async () => {
  const { bar: holder, main } = bar();
  holder.querySelector("button")!.focus();
  holder.parentElement!.remove();
  await settle();
  expect(document.activeElement).toBe(main.querySelector("h1"));
});

test("in a drawer, what's left of the drawer takes focus, and the × that would close it doesn't", async () => {
  const main = document.createElement("main");
  main.className = "tf-main";
  const drawer = document.createElement("div");
  drawer.className = "tf-drawer";
  drawer.innerHTML = `<button>×</button><ul><li><a href="#/a">A</a><button>Unlink</button></li></ul>`;
  document.body.append(main, drawer);
  drawer.querySelector<HTMLElement>("li button")!.focus();
  drawer.querySelector("ul")!.remove();
  await settle();
  expect(document.activeElement).toBe(drawer);
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

test("a dialog closed by key with nothing to give focus back to leaves it on the page's heading", async () => {
  const { heading } = page();
  const modal = document.createElement("div");
  modal.className = "tf-modal";
  modal.innerHTML = "<button>Close</button>";
  document.body.append(modal);
  await settle();
  modal.querySelector("button")!.focus();
  window.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter" }));
  modal.remove();
  await settle();
  expect(document.activeElement).toBe(heading);
});

test("Enter on a nav link lands on the new page's heading, so the next Tab starts on the page (F-052)", async () => {
  const { heading } = page();
  const nav = document.createElement("nav");
  nav.className = "tf-nav";
  const link = document.createElement("a");
  link.href = "#/inbox";
  nav.append(link);
  document.body.prepend(nav);
  link.focus();
  window.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter" }));
  window.dispatchEvent(new HashChangeEvent("hashchange"));
  await settle();
  expect(document.activeElement).toBe(heading);
  nav.remove();
});

test("a button whose whole group went gives focus to the heading rather than a pane's scroller", async () => {
  const { heading } = page(0);
  const scroller = document.createElement("section");
  scroller.className = "tf-scroller";
  const button = document.createElement("button");
  button.textContent = "Go";
  scroller.append(button);
  heading.after(scroller);
  button.focus();
  await settle();
  button.remove();
  await settle();
  expect(document.activeElement).toBe(heading);
  expect(scroller.hasAttribute("tabindex")).toBe(false);
});
