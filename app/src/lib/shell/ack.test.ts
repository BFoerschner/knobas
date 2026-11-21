import { afterEach, beforeEach, expect, test, vi } from "vitest";

import { ACK_AFTER_MS, ACK_FADE_MS, ACK_GIVE_UP_MS, ACK_MS, acknowledge, acknowledged, acks, cutMiddle, lastName, notePress, openingWords, placeFor, resetAcks } from "./ack.svelte";

beforeEach(() => {
  vi.useFakeTimers();
  resetAcks();
});

afterEach(() => {
  resetAcks();
  vi.useRealTimers();
});

const pointer = (x: number, y: number) => notePress(new PointerEvent("pointerdown", { clientX: x, clientY: y }), Date.now());

test("the words name the application and at most the item's last name, never a path or a command line", () => {
  expect(openingWords("VS Code")).toBe("Opening in VS Code");
  expect(openingWords("VS Code", lastName("src/sepa/retry.rs"))).toBe("Opening retry.rs in VS Code");
  expect(lastName("C:\\Users\\someone\\source\\repos\\payout-service\\")).toBe("payout-service");
  expect(openingWords("Terminal", "")).toBe("Opening in Terminal");
});

test("a long name gives up its middle and keeps more of its end", () => {
  expect(cutMiddle("PaymentRetryOrchestratorService.java", 28)).toBe("PaymentRet…ratorService.java");
  expect(cutMiddle("PaymentRetryOrchestratorService.java", 28)).toHaveLength(28);
  expect(cutMiddle("retry.rs", 28)).toBe("retry.rs");
  expect(openingWords("IntelliJ IDEA Ultimate with the team's flags on", "retry.rs")).toBe("Opening retry.rs in IntelliJ IDE…the team's flags on");
});

test("a pointer's press puts the chip above and to the right of the pointer", () => {
  const now = 10_000;
  expect(placeFor({ at: now, x: 400, y: 300, box: null }, 1440, 868, now)).toEqual({ left: 412, bottom: 868 - 300 + 12 });
});

test("near the right edge it hangs to the left of the pointer, and near the top it stands below", () => {
  const now = 10_000;
  expect(placeFor({ at: now, x: 1300, y: 300, box: null }, 1440, 868, now)).toEqual({ right: 1440 - 1300 + 12, bottom: 868 - 300 + 12 });
  expect(placeFor({ at: now, x: 400, y: 20, box: null }, 1440, 868, now)).toEqual({ left: 412, top: 32 });
});

test("a key's press puts it above the control that had focus, its left edge on the control's", () => {
  const now = 10_000;
  const box = { top: 200, bottom: 228, left: 600, right: 700 };
  expect(placeFor({ at: now, x: 600, y: 200, box }, 1440, 868, now)).toEqual({ left: 600, bottom: 868 - 200 + 6 });
  expect(placeFor({ at: now, x: 1300, y: 200, box: { ...box, left: 1300, right: 1400 } }, 1440, 868, now)).toEqual({ right: 40, bottom: 868 - 200 + 6 });
});

test("with no press, or one long ago, it stands where a toast would", () => {
  expect(placeFor(null, 1440, 868, 10_000)).toEqual({ centre: true, bottom: 44 });
  expect(placeFor({ at: 0, x: 400, y: 300, box: null }, 1440, 868, 10_000)).toEqual({ centre: true, bottom: 44 });
});

test("the chip stands where the last press was", () => {
  pointer(400, 300);
  acknowledge("Opening in VS Code");
  expect(acks.item?.text).toBe("Opening in VS Code");
  expect(acks.item?.place).toEqual({ left: 412, bottom: window.innerHeight - 300 + 12 });
});

test("answered at once, it stands its second, fades, and goes", () => {
  acknowledge("Opening in VS Code").settle();
  vi.advanceTimersByTime(ACK_MS - 1);
  expect(acks.item?.leaving).toBe(false);
  vi.advanceTimersByTime(1);
  expect(acks.item?.leaving).toBe(true);
  vi.advanceTimersByTime(ACK_FADE_MS);
  expect(acks.item).toBeNull();
});

test("answered late, it stands a moment past the answer, so a slow program's chip doesn't blink out", () => {
  const ack = acknowledge("Opening #142 in VS Code");
  vi.advanceTimersByTime(ACK_MS + 2000);
  expect(acks.item?.leaving).toBe(false);
  ack.settle();
  vi.advanceTimersByTime(ACK_AFTER_MS - 1);
  expect(acks.item?.leaving).toBe(false);
  vi.advanceTimersByTime(1);
  expect(acks.item?.leaving).toBe(true);
});

test("refused, it goes at once", () => {
  acknowledge("Opening in VS Code").fail();
  expect(acks.item).toBeNull();
});

test("an answer that never comes doesn't keep it for good", () => {
  acknowledge("Opening in VS Code");
  vi.advanceTimersByTime(ACK_GIVE_UP_MS);
  expect(acks.item?.leaving).toBe(true);
  vi.advanceTimersByTime(ACK_FADE_MS);
  expect(acks.item).toBeNull();
});

test("a second opening takes the first one's place, and the first one's answer no longer touches it", () => {
  const first = acknowledge("Opening in VS Code");
  acknowledge("Opening in Terminal");
  first.fail();
  expect(acks.item?.text).toBe("Opening in Terminal");
});

test("acknowledged runs the opening, and a refusal still reaches its caller", async () => {
  await expect(acknowledged("Opening in the browser", () => Promise.resolve(7))).resolves.toBe(7);
  expect(acks.item?.text).toBe("Opening in the browser");
  await expect(acknowledged("Opening in the browser", () => Promise.reject(new Error("refused")))).rejects.toThrow("refused");
  expect(acks.item).toBeNull();
});
