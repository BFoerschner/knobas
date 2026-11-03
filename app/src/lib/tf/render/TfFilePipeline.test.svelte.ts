/**
 * The file pipeline's step grip (`TfFilePipeline.svelte`): a drag moves a
 * step, and a drag that ends without a drop holds nothing afterwards.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { PipeStep } from "../../ipc/entity";
import TfFilePipeline from "./TfFilePipeline.svelte";

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

const STEPS: PipeStep[] = [{ step: "gunzip" }, { step: "jq", filter: "." }];

function draw() {
  const onchange = vi.fn<(steps: PipeStep[]) => void>();
  view = mount(TfFilePipeline, { target, props: { steps: STEPS, path: "a.json.gz", shownAs: "JSON", onchange, onreset: () => {} } });
  flushSync();
  return onchange;
}

const stepEl = (i: number) => target.querySelectorAll<HTMLElement>(".tf-pstep")[i]!;
const fire = (el: HTMLElement, type: string) => el.dispatchEvent(new Event(type, { bubbles: true, cancelable: true }));

test("a dragged step dropped on another moves there", () => {
  const onchange = draw();
  fire(stepEl(0), "dragstart");
  fire(stepEl(1), "drop");
  expect(onchange).toHaveBeenCalledWith([STEPS[1], STEPS[0]]);
});

test("a drag that ends without a drop leaves nothing held: a later drop from elsewhere moves nothing", () => {
  const onchange = draw();
  fire(stepEl(0), "dragstart");
  fire(stepEl(0), "dragend");
  fire(stepEl(1), "drop");
  expect(onchange).not.toHaveBeenCalled();
});
