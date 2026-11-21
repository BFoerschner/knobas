/** The crumb of *Move…* and *Add a route…* (F-546): the first and the last two whole, the middle one plain `…` with the path in its title. */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, expect, test, vi } from "vitest";

import type { AssetRow } from "../ipc/assets";
import PickerCrumb from "./PickerCrumb.svelte";

const steps = (...names: string[]) => names.map((name) => ({ id: `asset:${name}`, name }) as AssetRow);

let view: ReturnType<typeof mount> | null = null;
afterEach(() => {
  if (view) void unmount(view);
  view = null;
  document.body.replaceChildren();
});

function draw(crumb: AssetRow[], onstand: (path: AssetRow[]) => void = () => {}) {
  view = mount(PickerCrumb, { target: document.body, props: { crumb, onstand } });
  flushSync();
}
const words = () => [...document.querySelectorAll(".crumb button, .crumb .fold")].map((part) => part.textContent?.trim());

test("deep, it keeps the first and the last two whole, and the fold has the path in its title and no press", () => {
  draw(steps("estate", "host", "vm", "engine"));
  expect(words()).toEqual(["Estate", "…", "vm", "engine"]);
  const fold = document.querySelector<HTMLElement>(".crumb .fold")!;
  expect(fold.title).toBe("estate / host");
  expect(fold.tagName).toBe("SPAN");
});

test("three levels draw all three: a fold for one name says less than the name", () => {
  draw(steps("estate", "host", "vm"));
  expect(words()).toEqual(["Estate", "estate", "host", "vm"]);
});

test("a press walks to the level it names, and the level you stand on takes no press", () => {
  const onstand = vi.fn();
  const crumb = steps("estate", "host", "vm", "engine");
  draw(crumb, onstand);
  const [top, , foot] = document.querySelectorAll<HTMLButtonElement>(".crumb button");
  [...document.querySelectorAll<HTMLButtonElement>(".crumb button")].find((button) => button.textContent?.trim() === "vm")!.click();
  expect(onstand).toHaveBeenLastCalledWith(crumb.slice(0, 3));
  top!.click();
  expect(onstand).toHaveBeenLastCalledWith([]);
  expect(foot!.disabled).toBe(true);
});
