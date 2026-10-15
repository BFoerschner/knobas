import { flushSync, mount, unmount } from "svelte";
import { expect, test, vi } from "vitest";

import ShellSection from "./ShellSection.svelte";

test("the rooms UI's settings switch to the new shell", () => {
  const target = document.createElement("div");
  document.body.append(target);
  const onswitch = vi.fn();
  const section = mount(ShellSection, { target, props: { onswitch } });
  flushSync();
  [...target.querySelectorAll("button")].find((b) => b.textContent === "Switch to the new shell")!.click();
  expect(onswitch).toHaveBeenCalledOnce();
  unmount(section);
  target.remove();
});
