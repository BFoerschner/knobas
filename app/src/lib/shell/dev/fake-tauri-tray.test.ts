/**
 * The wire, not the endpoints: the real `SuggestionTray` over the real
 * `invoke`, answered by the fixture (#237).
 *
 * The unit tests beside this one prove the handler table; this one proves the
 * thing the ticket was actually about, which is what a room *shows* under
 * `?fake-ipc`. A fixture whose handlers are all present but answer a shape the
 * tray cannot draw would pass every test in `fake-tauri.test.ts` and still put
 * a red line on the room, so the assertions here are against the rendered
 * text: no failure line, a heading that counts, both badges, and a count that
 * drops when a row is answered.
 *
 * Its own file so the fixture's session state starts fresh and the counts can
 * be literal. `@tauri-apps/api`'s `invoke` and `listen` are the real modules:
 * both resolve through `window.__TAURI_INTERNALS__`, which is exactly what
 * `installFakeTauri` defines.
 */
import { flushSync, mount, unmount } from "svelte";
import { beforeEach, expect, test, vi } from "vitest";

import SuggestionTray from "../SuggestionTray.svelte";
import { demoHandlers, installFakeTauri } from "./fake-tauri";

beforeEach(() => {
  document.body.innerHTML = "";
});

function render() {
  const target = document.createElement("div");
  document.body.append(target);
  const app = mount(SuggestionTray, { target, props: { sources: [], onopen: () => {} } });
  flushSync();
  return { target, app };
}

/** Wait for the tray's heading to read `text`, flushing Svelte each poll. */
async function heading(target: HTMLElement, text: string) {
  await vi.waitFor(() => {
    flushSync();
    expect(target.querySelector(".cnt")?.textContent).toBe(text);
  });
}

test("a room under ?fake-ipc draws proposals, not a red line, and answering one drops the count", async () => {
  installFakeTauri(demoHandlers());
  const { target, app } = render();

  await heading(target, "3 waiting");
  expect(target.querySelector(".fail")).toBeNull();
  expect(target.querySelectorAll(".row.sug")).toHaveLength(3);
  const badges = [...target.querySelectorAll(".cls")].map((badge) => badge.textContent?.trim());
  expect(badges).toContain("exact");
  expect(badges).toContain("guess");
  for (const why of target.querySelectorAll(".why")) {
    expect(why.textContent?.replace(/\s+/g, " ").trim()).toMatch(/(exact|guess) \S/);
  }

  const first = target.querySelector(".row.sug")!;
  (first.querySelector("button.pri") as HTMLButtonElement).click();
  await heading(target, "2 waiting");
  expect(target.querySelector(".fail")).toBeNull();
  expect(target.querySelectorAll(".row.sug")).toHaveLength(2);

  const next = target.querySelector(".row.sug")!;
  (next.querySelector("button.ghost") as HTMLButtonElement).click();
  await heading(target, "1 waiting");
  expect(target.querySelectorAll(".row.sug")).toHaveLength(1);

  unmount(app);
  target.remove();
});
