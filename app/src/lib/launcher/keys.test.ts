import { expect, test } from "vitest";

import { chordOf, heldOf, kindOf, refusalOf } from "./keys";

const press = (code: string, mods: Partial<Record<"metaKey" | "ctrlKey" | "altKey" | "shiftKey", boolean>> = {}, key = "") =>
  new KeyboardEvent("keydown", { code, key, ...mods });

test("a chord is a modifier and one letter or digit, read by code and spelled ⌘ ⌃ ⌥ ⇧ then the key", () => {
  expect(chordOf(press("KeyO", { metaKey: true, shiftKey: true }, "O"))).toBe("⌘⇧O");
  expect(chordOf(press("Digit1", { metaKey: true }, "1"))).toBe("⌘1");
  expect(chordOf(press("KeyO", { altKey: true }, "ø")), "the code, not the character ⌥ types").toBe("⌥O");
  expect(chordOf(press("KeyZ", { metaKey: true }, "y")), "a German board's Y sits on KeyZ").toBe("⌘Z");
  expect(chordOf(press("Digit9", { metaKey: true, ctrlKey: true, altKey: true, shiftKey: true }))).toBe("⌘⌃⌥⇧9");
});

test("typing, ⇧ alone, a bare modifier and a key that is no letter or digit are no chord", () => {
  expect(chordOf(press("KeyO", {}, "o"))).toBeNull();
  expect(chordOf(press("KeyO", { shiftKey: true }, "O"))).toBeNull();
  expect(chordOf(press("MetaLeft", { metaKey: true }, "Meta"))).toBeNull();
  expect(chordOf(press("Slash", { metaKey: true }, "/"))).toBeNull();
  expect(chordOf(press("Enter", { metaKey: true }, "Enter"))).toBeNull();
});

test("the modifiers held are spelled in the same order, for the panel to draw while a chord is pressed", () => {
  expect(heldOf(press("ShiftLeft", { shiftKey: true, metaKey: true }, "Shift"))).toEqual(["⌘", "⇧"]);
  expect(heldOf(press("KeyA"))).toEqual([]);
});

test("the box's own chords are refused with a message: ⌘K, ⌘S, ⌘B, ⌘D and ⌘?", () => {
  for (const [code, key] of [
    ["KeyK", "k"],
    ["KeyS", "s"],
    ["KeyB", "b"],
    ["KeyD", "d"],
  ] as const) {
    const refused = refusalOf(press(code, { metaKey: true }, key));
    expect(refused, code).toMatch(/^⌘[KSBD] is the launcher's own$/);
  }
  expect(refusalOf(press("Slash", { metaKey: true, shiftKey: true }, "?"))).toBe("⌘? is the launcher's own");
  // Off the Mac Ctrl is the box's ⌘, and another modifier on top changes nothing.
  expect(refusalOf(press("KeyK", { ctrlKey: true }, "k"))).toBe("⌃K is the launcher's own");
  expect(refusalOf(press("KeyS", { metaKey: true, shiftKey: true }, "S"))).toBe("⌘⇧S is the launcher's own");
});

test("the shell's and the system's chords are refused too, and every other chord is free", () => {
  expect(refusalOf(press("KeyT", { metaKey: true }, "t"))).toBe("⌘T is the shell's own");
  expect(refusalOf(press("KeyR", { metaKey: true }, "r"))).toBe("⌘R is the shell's own");
  expect(refusalOf(press("KeyC", { metaKey: true }, "c"))).toBe("⌘C is the system's own");
  expect(refusalOf(press("KeyV", { metaKey: true }, "v"))).toBe("⌘V is the system's own");
  expect(refusalOf(press("KeyO", { metaKey: true, shiftKey: true }, "O"))).toBeNull();
  expect(refusalOf(press("KeyK", { altKey: true }, "˚")), "⌥K is nobody's").toBeNull();
  expect(refusalOf(press("Digit1", { metaKey: true }, "1"))).toBeNull();
});

test("a key is kept per kind: an entity's own kind, else the row's", () => {
  expect(kindOf({ kind: "entity", entityId: "gitea:r", entityKind: "repo", title: "r", sourceId: "gitea", path: null })).toBe("repo");
  expect(kindOf({ kind: "list", id: "l", label: "L", text: "#t", refusal: null })).toBe("list");
  expect(kindOf({ kind: "view", id: "v", title: "V" })).toBe("view");
});
