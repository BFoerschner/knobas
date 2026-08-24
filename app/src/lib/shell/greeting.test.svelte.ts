import { flushSync, mount, unmount } from "svelte";
import { expect, test } from "vitest";

import Greeting from "./greeting.svelte";

test("renders its prop and reacts to a change", () => {
  const target = document.createElement("div");
  document.body.append(target);
  const props = $state({ name: "knobas" });
  const app = mount(Greeting, { target, props });

  expect(target.textContent).toBe("hello knobas");
  props.name = "Tidewater";
  flushSync();
  expect(target.textContent).toBe("hello Tidewater");

  unmount(app);
  target.remove();
});
