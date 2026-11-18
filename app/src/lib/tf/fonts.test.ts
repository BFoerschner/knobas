/**
 * Each role's face and size (Settings › Fonts, 2026-11-18, `fonts.svelte.ts`): what a name and a size are, what is kept
 * across a restart, what lands on <html> before the first draw, and who hears a size change.
 */
import { readFileSync } from "node:fs";
import { join } from "node:path";

import { expect, test, vi } from "vitest";

import {
  BUNDLED_X_HEIGHT,
  createFonts,
  cssFamily,
  evenStep,
  familyName,
  fontKey,
  LIGATURES_KEY,
  onCodeSize,
  ROLES,
  sizeKey,
  sizeStep,
  SIZES,
  type RootStyle,
} from "./fonts.svelte";
import type { KeptStorage } from "./kept-value";

/** A `localStorage` of its own, kept across "restarts" (a second `createFonts` over it). */
function storage(): KeptStorage & { items: Map<string, string> } {
  const items = new Map<string, string>();
  return {
    items,
    getItem: (key) => items.get(key) ?? null,
    setItem: (key, value) => void items.set(key, value),
    removeItem: (key) => void items.delete(key),
  };
}

/** <html>'s inline style, as the settings write it. */
function rootStyle(): RootStyle & { props: Map<string, string> } {
  const props = new Map<string, string>();
  return {
    props,
    setProperty: (name: string, value: string | null) => void props.set(name, value ?? ""),
    removeProperty: (name: string) => {
      const was = props.get(name) ?? "";
      props.delete(name);
      return was;
    },
  };
}

test("a family is a trimmed name, its CSS quotes dropped, and the role's bundled face or nothing is no choice", () => {
  expect(familyName("  Fira Code ")).toBe("Fira Code");
  expect(familyName('"Iosevka Term"')).toBe("Iosevka Term");
  expect(familyName("'Menlo'")).toBe("Menlo");
  expect(familyName("jetbrains mono")).toBeNull();
  expect(familyName("   ")).toBeNull();
  expect(familyName(null)).toBeNull();
  expect(familyName("a\nb")).toBe("ab");
  expect(familyName("x".repeat(201))).toBeNull();
  expect(cssFamily('Odd "Name" \\ 2')).toBe('"Odd \\"Name\\" \\\\ 2"');
  // Each role's own bundled face, by its name or its family's.
  expect(familyName("Archivo", "ui")).toBeNull();
  expect(familyName("archivo variable", "head")).toBeNull();
  expect(familyName("Source Serif 4", "read")).toBeNull();
  expect(familyName("Source Serif 4 Variable", "read")).toBeNull();
  // Another role's bundled face is a choice like any.
  expect(familyName("JetBrains Mono", "ui")).toBe("JetBrains Mono");
  expect(familyName("Archivo", "code")).toBe("Archivo");
});

test("a size is one of the ten steps, 85 to 130 in fives, the nearest one, 100 for anything else", () => {
  expect(SIZES).toEqual([85, 90, 95, 100, 105, 110, 115, 120, 125, 130]);
  expect(sizeStep(115)).toBe(115);
  expect(sizeStep("112")).toBe(110);
  expect(sizeStep(113)).toBe(115);
  expect(sizeStep(40)).toBe(85);
  expect(sizeStep(400)).toBe(130);
  expect(sizeStep("big")).toBe(100);
  expect(sizeStep(null)).toBe(100);
  expect(sizeStep(Number.NaN)).toBe(100);
});

test("the even step draws a face's lower case as tall as the bundled face's, when that is on the scale", () => {
  expect(BUNDLED_X_HEIGHT).toEqual({ ui: 0.526, head: 0.526, read: 0.475, code: 0.55 });
  // Georgia (.481) for Interface: .526 / .481 = 109 %.
  expect(evenStep("ui", 0.481)).toBe(110);
  // Courier New (.423) for Code: .55 / .423 = 130 %.
  expect(evenStep("code", 0.423)).toBe(130);
  // A face as tall as the bundled one: 100.
  expect(evenStep("read", 0.475)).toBe(100);
  // Too far off either way, or not said.
  expect(evenStep("code", 0.3)).toBeNull();
  expect(evenStep("ui", 0.8)).toBeNull();
  expect(evenStep("ui", null)).toBeNull();
  expect(evenStep("ui", 0)).toBeNull();
});

test("nothing kept: the bundled faces at 100 %, no ligatures, and nothing written on <html>", () => {
  const style = rootStyle();
  const fonts = createFonts(() => storage(), () => style);
  for (const role of ROLES) {
    expect(fonts.family(role)).toBeNull();
    expect(fonts.size(role)).toBe(100);
  }
  expect(fonts.ligatures).toBe(false);
  expect([...style.props]).toEqual([]);
});

test("every role's family and size are kept across a restart and written on <html> before anything draws", () => {
  const kept = storage();
  const style = rootStyle();
  const fonts = createFonts(() => kept, () => style);
  fonts.setFamily("ui", "Helvetica Neue");
  fonts.setFamily("head", "Avenir Next Condensed");
  fonts.setFamily("read", "Georgia");
  fonts.setFamily("code", "Fira Code");
  fonts.setSize("ui", 110);
  fonts.setSize("head", 90);
  fonts.setSize("read", 125);
  fonts.setSize("code", 112);
  fonts.setLigatures(true);
  const written = {
    "--ui-chosen": '"Helvetica Neue"',
    "--ui-letter": "normal",
    "--ui-word": "normal",
    "--head-chosen": '"Avenir Next Condensed"',
    "--read-chosen": '"Georgia"',
    "--mono-chosen": '"Fira Code"',
    "--size-ui": "1.1",
    "--size-head": "0.9",
    "--size-read": "1.25",
    "--size-code": "1.1",
    "--mono-ligatures": "normal",
  };
  expect(Object.fromEntries(style.props)).toEqual(written);
  expect(Object.fromEntries(kept.items)).toEqual({
    "knobas.ui.font": "Helvetica Neue",
    "knobas.head.font": "Avenir Next Condensed",
    "knobas.read.font": "Georgia",
    "knobas.code.font": "Fira Code",
    "knobas.ui.size": "110",
    "knobas.head.size": "90",
    "knobas.read.size": "125",
    "knobas.code.size": "110",
    "knobas.code.ligatures": "on",
  });

  // The next run: written as the module loads, so the first draw is already in it.
  const after = rootStyle();
  const again = createFonts(() => kept, () => after);
  expect(again.family("read")).toBe("Georgia");
  expect(again.size("read")).toBe(125);
  expect(again.ligatures).toBe(true);
  expect(Object.fromEntries(after.props)).toEqual(written);

  for (const role of ROLES) {
    again.setFamily(role, null);
    again.setSize(role, 100);
  }
  again.setLigatures(false);
  expect([...after.props]).toEqual([]);
  expect([...kept.items]).toEqual([]);
});

test("the code font kept before Settings › Fonts is still read under its key", () => {
  const kept = storage();
  kept.items.set("knobas.code.font", "Menlo");
  kept.items.set("knobas.code.ligatures", "on");
  expect(fontKey("code")).toBe("knobas.code.font");
  expect(LIGATURES_KEY).toBe("knobas.code.ligatures");
  const style = rootStyle();
  const fonts = createFonts(() => kept, () => style);
  expect(fonts.family("code")).toBe("Menlo");
  expect(style.props.get("--mono-chosen")).toBe('"Menlo"');
  expect(style.props.get("--mono-ligatures")).toBe("normal");
});

test("a kept size off the steps is read onto the nearest, and a kept bundled name is no choice", () => {
  const kept = storage();
  kept.items.set(sizeKey("ui"), "137");
  kept.items.set(sizeKey("code"), "nonsense");
  kept.items.set(fontKey("read"), "Source Serif 4");
  const style = rootStyle();
  const fonts = createFonts(() => kept, () => style);
  expect(fonts.size("ui")).toBe(130);
  expect(fonts.size("code")).toBe(100);
  expect(fonts.family("read")).toBeNull();
  expect(Object.fromEntries(style.props)).toEqual({ "--size-ui": "1.3" });
});

test("a size change is heard with its ratio, once, by everyone listening until they stop", () => {
  const fonts = createFonts(() => storage(), () => rootStyle());
  const heard = vi.fn();
  const stop = fonts.onSize(heard);
  fonts.setSize("code", 120);
  expect(heard).toHaveBeenCalledExactlyOnceWith("code", 1.2);
  fonts.setSize("code", 120);
  expect(heard).toHaveBeenCalledTimes(1);
  fonts.setSize("code", 90);
  expect(heard).toHaveBeenLastCalledWith("code", 0.75);
  // A family's change is no size change.
  fonts.setFamily("code", "Menlo");
  expect(heard).toHaveBeenCalledTimes(2);
  stop();
  fonts.setSize("code", 100);
  expect(heard).toHaveBeenCalledTimes(2);
});

test("storage that throws costs the memory, not the choice", () => {
  const broken: KeptStorage = {
    getItem: () => {
      throw new Error("blocked");
    },
    setItem: () => {
      throw new Error("blocked");
    },
    removeItem: () => {
      throw new Error("blocked");
    },
  };
  const style = rootStyle();
  const fonts = createFonts(() => broken, () => style);
  fonts.setFamily("head", "Futura");
  fonts.setSize("head", 95);
  expect(fonts.family("head")).toBe("Futura");
  expect(style.props.get("--head-chosen")).toBe('"Futura"');
  expect(style.props.get("--size-head")).toBe("0.95");
});

test("reset is the bundled faces at 100 % with nothing kept and nobody listening", () => {
  const kept = storage();
  const style = rootStyle();
  const fonts = createFonts(() => kept, () => style);
  const heard = vi.fn();
  fonts.onSize(heard);
  fonts.setFamily("ui", "Avenir");
  fonts.setSize("read", 120);
  fonts.setLigatures(true);
  heard.mockClear();
  fonts.reset();
  expect([...style.props]).toEqual([]);
  expect([...kept.items]).toEqual([]);
  fonts.setSize("read", 110);
  expect(heard).not.toHaveBeenCalled();
});

/** `app/src/`, off `cwd`: Vitest's root is `app/`. */
const source = (file: string) => readFileSync(join(process.cwd(), "src", file), "utf8");

test("the lists of code's lines hear code's size, and the text measured in script takes its role's size", () => {
  // A run's log, a file's source and a diff's lines: rows that are line boxes, held at the new size (Fable's ruling 3).
  for (const file of ["lib/tf/builds/TfRunLog.svelte", "lib/tf/repo/TfRepoFiles.svelte", "lib/tf/diff/TfDiffCard.svelte"]) {
    expect(source(file), file).toMatch(/new Windowed<\w+>\(\{[^}]*textSize: onCodeSize[^}]*\}\)/);
  }
  // A run's step names and the day strip's labels are fitted by a canvas: at the role's size, again when it changes.
  expect(source("lib/tf/builds/TfRun.svelte")).toMatch(/const px = \(\(isCommand\(name\) \? 11 : 12\) \* fonts\.size\(role\)\) \/ 100;\s*void fonts\.family\(role\);/);
  expect(source("lib/tf/time/TfDay.svelte")).toMatch(/const scale = fonts\.size\(role\) \/ 100;\s*void fonts\.family\(role\);[\s\S]*getPropertyValue\("--t-2xs"\)\) \* scale/);
});

test("onCodeSize hears Code's size alone", () => {
  const fonts = createFonts(() => storage(), () => rootStyle());
  const heard = vi.fn();
  const stop = onCodeSize(heard, fonts);
  fonts.setSize("ui", 120);
  fonts.setSize("read", 90);
  expect(heard).not.toHaveBeenCalled();
  fonts.setSize("code", 125);
  expect(heard).toHaveBeenCalledExactlyOnceWith(1.25);
  stop();
  fonts.setSize("code", 100);
  expect(heard).toHaveBeenCalledTimes(1);
});

test("main.ts loads the fonts before it mounts, so the first draw is in the kept faces and sizes", () => {
  const main = source("main.ts");
  const loaded = main.indexOf('import "./lib/tf/fonts.svelte";');
  expect(loaded).toBeGreaterThan(-1);
  expect(loaded).toBeLessThan(main.indexOf("mount(App"));
});
