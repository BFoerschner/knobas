/**
 * The diff view warmed while the app is idle (the forge-rounds pass, 2026-11-15): drawn once off screen, hidden and
 * inert, taking no keys, then taken down with nothing left behind.
 */
import { afterEach, expect, test } from "vitest";
import { diffLayout } from "./diff-layout.svelte";
import { forgetWarmth, warmDiffView, warmFiles } from "./warm";

afterEach(() => {
  forgetWarmth();
  diffLayout.reset();
  document.querySelectorAll(".tf-warm").forEach((node) => node.remove());
});

test("the diff is drawn off screen, hidden and inert, then taken down; once a session", () => {
  let takeDown: (() => void) | null = null;
  warmDiffView(
    (go) => go(),
    (go) => (takeDown = go),
  );
  const host = document.querySelector<HTMLElement>(".tf-warm");
  expect(host).not.toBeNull();
  expect(host!.getAttribute("aria-hidden")).toBe("true");
  expect(host!.inert).toBe(true);
  expect(host!.style.visibility).toBe("hidden");
  expect(host!.querySelectorAll(".tf-diff").length, "the made-up files' cards").toBe(warmFiles().length);

  // A key the diff view answers on a shown page does nothing here: its page is hidden.
  const before = diffLayout.chosen;
  window.dispatchEvent(new KeyboardEvent("keydown", { key: "s", bubbles: true }));
  expect(diffLayout.chosen).toBe(before);

  takeDown!();
  expect(document.querySelector(".tf-warm"), "nothing left behind").toBeNull();

  let again = 0;
  warmDiffView(() => (again += 1));
  expect(again, "once a session").toBe(0);
});
