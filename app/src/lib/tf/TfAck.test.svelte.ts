// An opening's chip as the shell draws it (ux21, 2026-11-19): where the window's last press was, in a live region, gone with its time.
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import { ACK_FADE_MS, ACK_MS, acknowledge, resetAcks } from "../shell/ack.svelte";
import TfAck from "./TfAck.svelte";

let target: HTMLElement;
let app: ReturnType<typeof mount> | null = null;

beforeEach(() => {
  vi.useFakeTimers();
  resetAcks();
  target = document.createElement("div");
  document.body.append(target);
  app = mount(TfAck, { target });
  flushSync();
});

afterEach(() => {
  if (app) unmount(app);
  app = null;
  target.remove();
  resetAcks();
  vi.useRealTimers();
});

const chip = () => target.querySelector<HTMLElement>(".tf-ack");

test("a pointer's press anywhere in the window puts the chip above and to the right of it, in a live region", () => {
  document.body.dispatchEvent(new PointerEvent("pointerdown", { clientX: 300, clientY: 400, bubbles: true }));
  acknowledge("Opening retry.rs in VS Code");
  flushSync();
  expect(chip()?.textContent).toBe("Opening retry.rs in VS Code");
  expect(chip()?.style.left).toBe("312px");
  expect(chip()?.style.bottom).toBe(`${window.innerHeight - 400 + 12}px`);
  expect(chip()?.closest("[role=status]")?.getAttribute("aria-live")).toBe("polite");
});

test("Enter on a control puts it above that control", () => {
  const button = document.createElement("button");
  document.body.append(button);
  button.getBoundingClientRect = () => ({ top: 200, bottom: 228, left: 640, right: 720, width: 80, height: 28, x: 640, y: 200, toJSON: () => ({}) });
  button.focus();
  button.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
  acknowledge("Opening in VS Code");
  flushSync();
  expect(chip()?.style.left).toBe("640px");
  expect(chip()?.style.bottom).toBe(`${window.innerHeight - 200 + 6}px`);
  button.remove();
});

test("answered, it fades and goes", () => {
  acknowledge("Opening in VS Code").settle();
  flushSync();
  vi.advanceTimersByTime(ACK_MS);
  flushSync();
  expect(chip()?.classList.contains("tf-ack-leaving")).toBe(true);
  vi.advanceTimersByTime(ACK_FADE_MS);
  flushSync();
  expect(chip()).toBeNull();
});

test("the shell's host going takes the chip and its timers with it", () => {
  acknowledge("Opening in VS Code");
  unmount(app!);
  app = null;
  expect(vi.getTimerCount()).toBe(0);
});
