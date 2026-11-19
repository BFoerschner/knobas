// A toast draws as one line that gives up its middle (ux21, 2026-11-19): what the line's spans hold, and its title.
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import { push, reset } from "../shell/toasts.svelte";
import TfToasts from "./TfToasts.svelte";

let target: HTMLElement;
let app: ReturnType<typeof mount> | null = null;

beforeEach(() => {
  reset();
  target = document.createElement("div");
  document.body.append(target);
  app = mount(TfToasts, { target });
});

afterEach(() => {
  if (app) unmount(app);
  app = null;
  target.remove();
  reset();
});

const WHY = "'open -a \"Visual Studio Code\" {path}' could not be run: No such file or directory (os error 2)";

test("a long failure is a head that shrinks and a tail that stays, the whole line in its title", () => {
  push({ text: `Could not start VS Code: ${WHY}`, tone: "err" });
  flushSync();
  const line = target.querySelector<HTMLElement>(".tf-toast-line")!;
  expect(line.title).toBe(`Could not start VS Code: ${WHY}`);
  expect(line.querySelector(".tf-toast-head")!.textContent).toBe("Could not start VS Code: 'open -a \"Visual Studio Code\" {path}' could not be run: ");
  expect(line.querySelector(".tf-toast-tail")!.textContent).toBe("No such file or directory (os error 2)");
  // A reader hears the whole line.
  expect(line.textContent).toBe(`Could not start VS Code: ${WHY}`);
});

test("a short one is all head, with no title to repeat it", () => {
  push({ text: "Copied" });
  flushSync();
  const line = target.querySelector<HTMLElement>(".tf-toast-line")!;
  expect(line.hasAttribute("title")).toBe(false);
  expect(line.querySelector(".tf-toast-tail")).toBeNull();
  expect(line.textContent).toBe("Copied");
});

test("a key in the line is still a key", () => {
  push({ text: "On PAY-240 now" });
  flushSync();
  expect(target.querySelector(".tf-toast-line .tf-key")?.textContent).toBe("PAY-240");
});
