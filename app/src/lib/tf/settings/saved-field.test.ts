import { afterEach, expect, test, vi } from "vitest";

import { SavedWord, savedField } from "./saved-field.svelte";

afterEach(() => {
  vi.useRealTimers();
  document.body.replaceChildren();
});

function field(stored = "2000") {
  const state = { stored };
  const input = document.createElement("input");
  input.value = stored;
  document.body.append(input);
  const word = new SavedWord();
  const restored = vi.fn();
  const save = vi.fn(async (at: HTMLInputElement) => {
    if (at.value === state.stored) return false;
    state.stored = at.value;
    return true;
  });
  const stop = savedField({ saved: () => state.stored, save, word, restored })(input) as () => void;
  return { state, input, word, restored, save, stop };
}

function press(input: HTMLInputElement, key: string): KeyboardEvent {
  const event = new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true });
  input.dispatchEvent(event);
  return event;
}

const settled = () => new Promise((done) => setTimeout(done));

test("Esc puts the stored value back, answers the key and says nothing was saved (F-641)", async () => {
  const { input, save, word, restored } = field();
  input.value = "20009";
  input.focus();
  expect(press(input, "Escape").defaultPrevented).toBe(true);
  expect(input.value).toBe("2000");
  expect(document.activeElement).toBe(input);
  expect(restored).toHaveBeenCalledOnce();
  await settled();
  expect(save).not.toHaveBeenCalled();
  expect(word.shown).toBe(false);
});

test("Esc on an untouched field is the shell's", () => {
  const { input, restored } = field();
  expect(press(input, "Escape").defaultPrevented).toBe(false);
  expect(restored).not.toHaveBeenCalled();
});

test("Enter saves and says so for about two seconds; a second Enter writes nothing", async () => {
  vi.useFakeTimers();
  const { input, save, word } = field();
  input.value = "1500";
  expect(press(input, "Enter").defaultPrevented).toBe(true);
  await vi.advanceTimersByTimeAsync(0);
  expect(word.shown).toBe(true);
  press(input, "Enter");
  await vi.advanceTimersByTimeAsync(0);
  expect(save).toHaveBeenCalledTimes(2);
  expect(save).toHaveLastReturnedWith(expect.any(Promise));
  await vi.advanceTimersByTimeAsync(2100);
  expect(word.shown).toBe(false);
});

test("leaving saves, by a change or by the field going away with words in it", async () => {
  const left = field();
  left.input.value = "1500";
  left.input.dispatchEvent(new Event("change", { bubbles: true }));
  await settled();
  expect(left.state.stored).toBe("1500");

  const gone = field();
  gone.input.value = "1700";
  gone.stop();
  await settled();
  expect(gone.state.stored).toBe("1700");
});

test("a field that goes away holding what is stored writes nothing", async () => {
  const { save, stop } = field();
  stop();
  await settled();
  expect(save).not.toHaveBeenCalled();
});

test("a refused save says nothing", async () => {
  const input = document.createElement("input");
  const word = new SavedWord();
  savedField({ saved: () => "1", save: () => false, word })(input);
  input.value = "2";
  input.dispatchEvent(new Event("change"));
  await settled();
  expect(word.shown).toBe(false);
});
