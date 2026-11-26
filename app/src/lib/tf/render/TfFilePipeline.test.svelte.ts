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
const grip = (i: number) => stepEl(i).querySelector<HTMLElement>(".tf-pgrip")!;

/** A pointer event as a person's: the primary pointer, its main button. */
function pointer(type: string, x: number, y: number): Event {
  const event = new MouseEvent(type, { bubbles: true, cancelable: true, clientX: x, clientY: y, button: 0 });
  Object.defineProperties(event, { pointerId: { value: 1 }, isPrimary: { value: true } });
  return event;
}

/** Press `handle`, move past a tremble to where the hit test answers `onto`, and let go there. */
function dragOnto(handle: Element, onto: Element | null) {
  document.elementsFromPoint = () => (onto === null ? [] : [onto]);
  // As a real pointer: past its first move it is over the other step, not the grip.
  const elsewhere = onto ?? document.body;
  handle.dispatchEvent(pointer("pointerdown", 10, 10));
  elsewhere.dispatchEvent(pointer("pointermove", 40, 12));
  elsewhere.dispatchEvent(pointer("pointerup", 40, 12));
}

test("a step's grip dragged by pointer onto another step moves it there (HTML5 drag never drops on Windows)", () => {
  const onchange = draw();
  dragOnto(grip(0), stepEl(1).querySelector(".tf-pname") ?? stepEl(1));
  expect(onchange).toHaveBeenCalledWith([STEPS[1], STEPS[0]]);
  expect(stepEl(0).classList.contains("tf-lifted")).toBe(false);
});

test("a press that travels no more than a tremble, a release on nothing, and Esc move nothing", () => {
  const onchange = draw();
  document.elementsFromPoint = () => [stepEl(1)];
  grip(0).dispatchEvent(pointer("pointerdown", 10, 10));
  grip(0).dispatchEvent(pointer("pointermove", 13, 11));
  grip(0).dispatchEvent(pointer("pointerup", 13, 11));
  dragOnto(grip(0), null);
  grip(0).dispatchEvent(pointer("pointerdown", 10, 10));
  grip(0).dispatchEvent(pointer("pointermove", 40, 12));
  window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
  grip(0).dispatchEvent(pointer("pointerup", 40, 12));
  expect(onchange).not.toHaveBeenCalled();
});

test("no step can be dragged by HTML5 drag, which Tauri swallows on Windows", () => {
  draw();
  expect(target.querySelector("[draggable=true]")).toBeNull();
});
