/** `row-room`: a value hides only if the row's group covers its words. */
import { afterEach, expect, test } from "vitest";

import { installRowRoom, measureRow } from "./row-room";

type Box = { left: number; right: number; top?: number; bottom?: number };

function rect({ left, right, top = 0, bottom = 20 }: Box): DOMRect {
  return { left, right, top, bottom, width: right - left, height: bottom - top, x: left, y: top, toJSON: () => ({}) } as DOMRect;
}

function row(group: Box, ...values: Box[]): HTMLElement {
  const tr = document.createElement("tr");
  const acts = document.createElement("span");
  acts.className = "tf-rowacts";
  acts.getBoundingClientRect = () => rect(group);
  const button = document.createElement("button");
  acts.append(button);
  tr.append(acts);
  for (const box of values) {
    const value = document.createElement("span");
    value.className = "tf-rowval";
    value.textContent = "2 h ago";
    value.getBoundingClientRect = () => rect(box);
    tr.append(value);
  }
  document.body.append(tr);
  return tr;
}

/** jsdom has no layout for a `Range`, so the words are the box here. */
const covered = (tr: Element) => [...tr.querySelectorAll(".tf-rowval")].map((value) => value.classList.contains("tf-covered"));

afterEach(() => {
  document.body.replaceChildren();
});

test("a value clear of the group stays, one it reaches hides whole, and a pixel's touch is not a reach", () => {
  const tr = row({ left: 300, right: 400 }, { left: 100, right: 200 }, { left: 250, right: 320 }, { left: 280, right: 301 });
  measureRow(tr);
  expect(covered(tr)).toEqual([false, true, false]);
});

test("a value on another line of the row, under the group's column, stays", () => {
  const tr = row({ left: 300, right: 400, top: 0, bottom: 20 }, { left: 320, right: 380, top: 24, bottom: 44 });
  measureRow(tr);
  expect(covered(tr)).toEqual([false]);
});

test("a measure takes back an old mark when the group has moved off it", () => {
  const tr = row({ left: 300, right: 400 }, { left: 320, right: 380 });
  measureRow(tr);
  expect(covered(tr)).toEqual([true]);
  tr.querySelector<HTMLElement>(".tf-rowacts")!.getBoundingClientRect = () => rect({ left: 500, right: 600 });
  measureRow(tr);
  expect(covered(tr)).toEqual([false]);
});

test("the shell's handler measures on the way into a row, and on focus into its group but not on its link", () => {
  const off = installRowRoom();
  const tr = row({ left: 300, right: 400 }, { left: 320, right: 380 });
  const link = document.createElement("a");
  link.href = "#/x";
  tr.prepend(link);

  link.dispatchEvent(new FocusEvent("focusin", { bubbles: true }));
  expect(covered(tr)).toEqual([false]);

  tr.querySelector("button")!.dispatchEvent(new FocusEvent("focusin", { bubbles: true }));
  expect(covered(tr)).toEqual([true]);

  tr.querySelector(".tf-rowval")!.classList.remove("tf-covered");
  link.dispatchEvent(new Event("pointerover", { bubbles: true }));
  expect(covered(tr)).toEqual([true]);
  off();
});

test("a row with no group is left alone", () => {
  const off = installRowRoom();
  const tr = document.createElement("tr");
  const value = document.createElement("span");
  value.className = "tf-rowval";
  tr.append(value);
  document.body.append(tr);
  value.dispatchEvent(new Event("pointerover", { bubbles: true }));
  expect(value.classList.contains("tf-covered")).toBe(false);
  off();
});
