/**
 * A held view's line (ADR-0042): one line, the room it keeps once drawn, its whole words in its title where the line is
 * cut (the ux20 Windows pass, 2026-11-19: in a repository's tree column it wrapped to two and moved the tree).
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, expect, test } from "vitest";

import TfHeldNote from "./TfHeldNote.svelte";

let app: ReturnType<typeof mount> | null = null;
let target: HTMLElement;

afterEach(() => {
  if (app) unmount(app);
  app = null;
  target.remove();
});

function draw(props: { heldAt: string | null; source: string; failed?: string | null }) {
  target = document.createElement("div");
  document.body.appendChild(target);
  const state = $state(props);
  app = mount(TfHeldNote, { target, props: state });
  flushSync();
  return state;
}

test("the line is one line with its words in its title, and keeps its one-line room once the fresh answer lands", () => {
  const props = draw({ heldAt: new Date(Date.now() - 60_000).toISOString(), source: "Gitea (live)" });
  const line = target.querySelector<HTMLElement>(".tf-heldnote")!;
  expect(line.classList.contains("tf-held1")).toBe(true);
  expect(line.title).toBe(line.textContent);
  expect(line.title).toContain("Checking Gitea (live)");
  props.heldAt = null;
  flushSync();
  const room = target.querySelector<HTMLElement>(".tf-heldnote")!;
  expect(room.classList.contains("tf-held1")).toBe(true);
  expect(room.getAttribute("aria-hidden")).toBe("true");
});
