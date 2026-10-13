/**
 * The Logs settings section (issue #747): the button asks to open the folder,
 * and a refusal is said rather than swallowed.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import LogsSection from "./LogsSection.svelte";

let target: HTMLDivElement;
let app: Record<string, unknown> | undefined;

beforeEach(() => {
  target = document.createElement("div");
  document.body.append(target);
});

afterEach(() => {
  if (app) void unmount(app);
  app = undefined;
  target.remove();
});

function press() {
  const button = target.querySelector<HTMLButtonElement>('button[aria-label="Show logs"]');
  expect(button).not.toBeNull();
  button!.click();
  flushSync();
}

test("Show logs opens the folder", async () => {
  let opened = 0;
  app = mount(LogsSection, {
    target,
    props: {
      ports: {
        openLogs: () => {
          opened += 1;
          return Promise.resolve();
        },
      },
    },
  });
  press();
  await Promise.resolve();
  expect(opened).toBe(1);
});

test("a refused open says why", async () => {
  app = mount(LogsSection, {
    target,
    props: { ports: { openLogs: () => Promise.reject(new Error("not allowed")) } },
  });
  press();
  await new Promise((r) => setTimeout(r, 0));
  flushSync();
  expect(target.textContent).toContain("not allowed");
});
