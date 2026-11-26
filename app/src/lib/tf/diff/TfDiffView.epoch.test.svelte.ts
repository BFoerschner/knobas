/**
 * The view's epoch (ux26): a diff with an end at the working tree reads again what it read of the old state when the
 * state moves: a rendered side is read again and replaced inside its stage, what was drawn staying until the answer
 * lands; the same epoch reads nothing.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import type { DiffFile } from "../../ipc/entity";
import { createDiffLayout } from "./diff-layout.svelte";
import { createDiffRendered } from "./diff-rendered.svelte";
import TfDiffView from "./TfDiffView.svelte";

const README: DiffFile = {
  path: "README.md",
  old_path: null,
  change: "modified",
  additions: 1,
  deletions: 1,
  hunks: [{ header: "@@ -1 +1 @@", lines: [{ kind: "del", old: 1, new: null, text: "# Old" }, { kind: "add", old: null, new: 1, text: "# New" }] }],
  truncated: false,
  threads: [],
};

let target: HTMLElement;
let view: ReturnType<typeof mount> | null = null;
beforeEach(() => {
  target = document.createElement("div");
  document.body.appendChild(target);
});
afterEach(() => {
  if (view) unmount(view);
  view = null;
  target.remove();
});
const settle = async () => {
  for (let n = 0; n < 8; n += 1) await Promise.resolve();
  flushSync();
};

test("a new epoch reads the rendered sides again and replaces them in their stage; the same epoch reads nothing", async () => {
  let state = 1;
  const asked: string[] = [];
  const pending: (() => void)[] = [];
  const context = {
    text: () => Promise.resolve(null),
    bytes: (_: DiffFile, side: "old" | "new") => {
      asked.push(`${side}@${state}`);
      const words = side === "old" ? "# Before" : `# After ${state}`;
      return new Promise<Uint8Array>((done) => pending.push(() => done(new TextEncoder().encode(words))));
    },
  };
  const props = $state({ files: [README], context, epoch: "s1", rendering: createDiffRendered(() => null), layout: createDiffLayout(() => null), defaults: { list: [] } });
  view = mount(TfDiffView, { target, props: props as never });
  flushSync();
  [...target.querySelectorAll<HTMLButtonElement>(".tf-fh .tf-seg button")].find((b) => b.textContent === "Rendered")!.click();
  flushSync();
  pending.splice(0).forEach((go) => go());
  await settle();
  const stage = target.querySelector(".tf-rstage")!;
  expect(stage.querySelector(".tf-rside-new h1")?.textContent).toBe("After 1");
  expect(asked).toEqual(["old@1", "new@1"]);

  props.epoch = "s1";
  flushSync();
  await settle();
  expect(asked, "the same epoch reads nothing").toHaveLength(2);

  state = 2;
  props.epoch = "s2";
  flushSync();
  expect(asked).toEqual(["old@1", "new@1", "old@2", "new@2"]);
  // Until the answers land, what was drawn stays, in the same stage.
  expect(target.querySelector(".tf-rstage")).toBe(stage);
  expect(stage.querySelector(".tf-rside-new h1")?.textContent).toBe("After 1");
  pending.splice(0).forEach((go) => go());
  await settle();
  expect(target.querySelector(".tf-rstage")).toBe(stage);
  expect(stage.querySelector(".tf-rside-new h1")?.textContent).toBe("After 2");
});
