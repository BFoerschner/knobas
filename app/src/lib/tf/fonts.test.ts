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
  MONO_KEY,
  codeRows,
  BUNDLED_CODE,
  CODE_LINES,
  KNOBS,
  knobSteps,
  LINES_KEY,
  MEASURE_KEY,
  RELEASE_KEY,
  ROLES,
  sizeKey,
  sizeStep,
  SIZES,
  unusedCodeFaces,
  CASCADIA_CODE_CSS,
  CASCADIA_MONO_CSS,
  shownName,
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
  expect(familyName("cascadia code")).toBeNull();
  expect(familyName("Cascadia Mono")).toBeNull();
  expect(familyName("   ")).toBeNull();
  expect(familyName(null)).toBeNull();
  expect(familyName("a\nb")).toBe("ab");
  expect(familyName("x".repeat(201))).toBeNull();
  expect(cssFamily('Odd "Name" \\ 2')).toBe('"Odd \\"Name\\" \\\\ 2"');
  // Each role's own bundled face (2026-11-20: Inter; for code Cascadia Mono since 2026-11-21, and Cascadia Code, which
  // the bundle declares too).
  expect(familyName("Inter", "ui")).toBeNull();
  expect(familyName("inter", "head")).toBeNull();
  expect(familyName("Inter", "read")).toBeNull();
  // Another role's bundled face, and the faces bundled before, are a choice like any.
  expect(familyName("Cascadia Code", "ui")).toBe("Cascadia Code");
  expect(familyName("Cascadia Mono", "read")).toBe("Cascadia Mono");
  expect(familyName("Inter", "code")).toBe("Inter");
  expect(familyName("Archivo", "ui")).toBe("Archivo");
  expect(familyName("Source Serif 4", "read")).toBe("Source Serif 4");
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
  // Inter's .546 (1118 of 2048) for the first three; Cascadia's .518 at its 105 % for Code.
  expect(BUNDLED_X_HEIGHT).toEqual({ ui: 1118 / 2048, head: 1118 / 2048, read: 1118 / 2048, code: (1060 / 2048) * 1.05 });
  // Georgia (.481) for Interface: .546 / .481 = 113 %.
  expect(evenStep("ui", 0.481)).toBe(115);
  // Courier New (.423) for Code: Cascadia's .518 at its 105 % is .544, and .544 / .423 = 129 %.
  expect(evenStep("code", 0.423)).toBe(130);
  // A face as tall as the bundled one: 100 (Archivo's .526 is 104 % off Inter's, on the scale at 105).
  expect(evenStep("read", 0.546)).toBe(100);
  expect(evenStep("ui", 0.526)).toBe(105);
  // Too far off either way, or not said.
  expect(evenStep("code", 0.3)).toBeNull();
  expect(evenStep("ui", 0.8)).toBeNull();
  expect(evenStep("ui", null)).toBeNull();
  expect(evenStep("ui", 0)).toBeNull();
});

test("nothing kept: the bundled faces at 100 %, Cascadia Mono with ligatures off, and nothing written on <html>", () => {
  const style = rootStyle();
  const fonts = createFonts(() => storage(), () => style);
  for (const role of ROLES) {
    expect(fonts.family(role)).toBeNull();
    expect(fonts.size(role)).toBe(100);
  }
  expect(fonts.ligatures).toBe(false);
  expect(fonts.bundled("code")).toBe("Cascadia Mono");
  expect(fonts.bundled("ui")).toBe("Inter");
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
    "--head-chosen": '"Avenir Next Condensed"',
    "--read-chosen": '"Georgia"',
    "--mono-chosen": '"Fira Code"',
    "--size-ui": "1.1",
    "--size-head": "0.9",
    "--size-read": "1.25",
    "--size-code": "1.1",
    "--mono-bundled": '"knobas Cascadia Code"',
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
    "knobas.code.mono": "1",
    // The chosen code face as measured when picked; jsdom lays nothing out, so it stands at Cascadia's line.
    "knobas.code.measure": '{"family":"Fira Code","line":1.2202,"bar":null}',
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
  expect([...kept.items]).toEqual([[MONO_KEY, "1"]]);
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
  // A chosen face keeps its ligatures as they drew (2026-11-21): on, kept as this build's `on`.
  expect(fonts.ligatures).toBe(true);
  expect(style.props.get("--mono-ligatures")).toBe("normal");
  expect(kept.items.get(LIGATURES_KEY)).toBe("on");
});

test("ligatures are off until turned on, kept as on, and on draws Cascadia Code (2026-11-21)", () => {
  const kept = storage();
  const style = rootStyle();
  const fonts = createFonts(() => kept, () => style);
  expect(fonts.ligatures).toBe(false);
  fonts.setLigatures(true);
  expect(fonts.ligatures).toBe(true);
  expect(fonts.bundled("code")).toBe("Cascadia Code");
  expect(kept.items.get(LIGATURES_KEY)).toBe("on");
  expect(Object.fromEntries(style.props)).toEqual({ "--mono-bundled": '"knobas Cascadia Code"', "--mono-ligatures": "normal" });
  // The next start keeps it on.
  const after = rootStyle();
  expect(createFonts(() => kept, () => after).ligatures).toBe(true);
  expect(Object.fromEntries(after.props)).toEqual({ "--mono-bundled": '"knobas Cascadia Code"', "--mono-ligatures": "normal" });
  fonts.setLigatures(false);
  expect(kept.items.has(LIGATURES_KEY)).toBe(false);
  expect([...style.props]).toEqual([]);
  expect(fonts.bundled("code")).toBe("Cascadia Mono");
});

test.each([
  // What earlier builds kept, read once on the first start since 2026-11-21. The bundled face draws Cascadia Mono
  // whatever was kept: ligatures on by default (2026-11-20), turned off ("off"), or an "on" from before 2026-11-20.
  [null, null, false],
  [null, "off", false],
  [null, "on", false],
  // A chosen face keeps its ligatures as they drew: on unless turned off.
  ["Fira Code", null, true],
  ["Fira Code", "on", true],
  ["Fira Code", "off", false],
  // A chosen Cascadia becomes the bundled face, and draws Mono.
  ["Cascadia Code", null, false],
])("a kept face %s with ligatures kept %s draws ligatures: %s, and the next start reads what the first left", (family, was, on) => {
  const kept = storage();
  if (family !== null) kept.items.set(fontKey("code"), family);
  if (was !== null) kept.items.set(LIGATURES_KEY, was);
  const fonts = createFonts(() => kept, () => rootStyle());
  expect(fonts.ligatures).toBe(on);
  expect(kept.items.get(LIGATURES_KEY) ?? null).toBe(on ? "on" : null);
  expect(kept.items.get(MONO_KEY)).toBe("1");
  expect(createFonts(() => kept, () => rootStyle()).ligatures).toBe(on);
});

test("after the first start an off kept by an earlier build is nothing, and the switch's own on stays", () => {
  const kept = storage();
  kept.items.set(MONO_KEY, "1");
  kept.items.set(LIGATURES_KEY, "off");
  expect(createFonts(() => kept, () => rootStyle()).ligatures).toBe(false);
  kept.items.set(LIGATURES_KEY, "on");
  expect(createFonts(() => kept, () => rootStyle()).ligatures).toBe(true);
  // Done once: a bundled face's `on` is not taken away again.
  expect(kept.items.get(LIGATURES_KEY)).toBe("on");
});

test("choosing Cascadia Code or Cascadia Mono by name is the bundled face, and sets the switch", () => {
  const kept = storage();
  const style = rootStyle();
  const fonts = createFonts(() => kept, () => style);
  fonts.setFamily("code", "Cascadia Code");
  expect(fonts.family("code")).toBeNull();
  expect(fonts.ligatures).toBe(true);
  expect(style.props.has("--mono-chosen")).toBe(false);
  fonts.setFamily("code", '"cascadia mono"');
  expect(fonts.family("code")).toBeNull();
  expect(fonts.ligatures).toBe(false);
  // Going back to the bundled face (an empty name) leaves the switch alone, and another role's Cascadia is a choice.
  fonts.setLigatures(true);
  fonts.setFamily("code", null);
  expect(fonts.ligatures).toBe(true);
  fonts.setFamily("ui", "Cascadia Mono");
  expect(fonts.ligatures).toBe(true);
  expect(fonts.family("ui")).toBe("Cascadia Mono");
});

test("an installed Cascadia is its own choice: a stored plain name, or the name with (installed) after it (2026-11-21)", () => {
  const kept = storage();
  kept.items.set(MONO_KEY, "1");
  // Stored since the bundled faces have names of their own: the machine's font of that name.
  kept.items.set(fontKey("code"), "Cascadia Code");
  const style = rootStyle();
  const fonts = createFonts(() => kept, () => style);
  expect(fonts.family("code")).toBe("Cascadia Code");
  expect(style.props.get("--mono-chosen")).toBe('"Cascadia Code"');
  expect(fonts.ligatures).toBe(false);
  // Picked from the list or typed with the suffix: the installed copy, and the switch is left as it is.
  fonts.setFamily("code", null);
  fonts.setFamily("code", "Cascadia Mono (installed)");
  expect(fonts.family("code")).toBe("Cascadia Mono");
  expect(kept.items.get(fontKey("code"))).toBe("Cascadia Mono");
  expect(fonts.ligatures).toBe(false);
  // Typed bare, or by its CSS name, it is the bundled face again.
  fonts.setFamily("code", "knobas Cascadia Code");
  expect(fonts.family("code")).toBeNull();
  expect(fonts.ligatures).toBe(true);
  expect(fonts.bundledCss("code")).toBe(CASCADIA_CODE_CSS);
  fonts.setFamily("code", "Cascadia Mono");
  expect(fonts.family("code")).toBeNull();
  expect(fonts.bundledCss("code")).toBe(CASCADIA_MONO_CSS);
});

test.each([
  ["Cascadia Mono", "Cascadia Mono (installed)"],
  ["cascadia code", "cascadia code (installed)"],
  ["Fira Code", "Fira Code"],
  ["Cascadia Code NF", "Cascadia Code NF"],
])("an installed %s shows as %s", (family, shown) => {
  expect(shownName(family)).toBe(shown);
  expect(familyName(shown)).toBe(family);
});

test("the switch turns to a face once it has landed, and the last press wins", async () => {
  const landed: Record<string, (() => void)[]> = {};
  const asked: string[] = [];
  const style = rootStyle();
  const fonts = createFonts(() => storage(), () => style, (family) => {
    asked.push(family);
    return new Promise<void>((done) => (landed[family] ??= []).push(done));
  });
  const land = async (family: string) => {
    for (const done of landed[family]!.splice(0)) done();
    await Promise.resolve();
    await Promise.resolve();
  };
  fonts.setLigatures(true);
  // The switch reads on at once; code turns to Cascadia Code when its files have loaded.
  expect(fonts.ligatures).toBe(true);
  expect(asked).toEqual(["knobas Cascadia Code"]);
  expect([...style.props]).toEqual([]);
  await land("knobas Cascadia Code");
  expect(style.props.get("--mono-bundled")).toBe('"knobas Cascadia Code"');
  // Off, on and off again before either lands: Mono lands, then Code, which changes nothing.
  fonts.setLigatures(false);
  fonts.setLigatures(true);
  fonts.setLigatures(false);
  await land("knobas Cascadia Mono");
  expect([...style.props]).toEqual([]);
  await land("knobas Cascadia Code");
  expect([...style.props]).toEqual([]);
});

test("a face that fails to load still turns code to it, drawn in its stand-ins", async () => {
  const style = rootStyle();
  const fonts = createFonts(() => storage(), () => style, () => Promise.reject(new Error("gone")));
  fonts.setLigatures(true);
  await Promise.resolve();
  await Promise.resolve();
  expect(style.props.get("--mono-ligatures")).toBe("normal");
});

test("the idle loader leaves the bundled Cascadias the release and the switch do not draw", () => {
  const fonts = createFonts(() => storage(), () => rootStyle());
  expect(unusedCodeFaces(fonts)).toEqual(["knobas Cascadia Code", "knobas Cascadia Mono 2009", "knobas Cascadia Code 2009"]);
  fonts.setLigatures(true);
  expect(unusedCodeFaces(fonts)).toEqual(["knobas Cascadia Mono", "knobas Cascadia Mono 2009", "knobas Cascadia Code 2009"]);
  fonts.setFamily("code", "Cascadia Mono 2009.22");
  expect(unusedCodeFaces(fonts)).toEqual(["knobas Cascadia Mono", "knobas Cascadia Code", "knobas Cascadia Code 2009"]);
});
test.each([
  // Björn's: Cascadia installed and set to 105 % is the bundled Cascadia at its 100 % (and an installed Cascadia Mono at
  // 105 %, which Settings marked as the even step, is the bundled Mono at 100 %).
  ["Cascadia Code", "105", null],
  ["CaskaydiaCove Nerd Font Mono", "105", null],
  ["Cascadia Mono", "105", null],
  ["cascadia mono", "110", "105"],
  // A Cascadia kept at 100 % keeps its drawn size: 100 over 1.05 is 95.
  ['"Cascadia Code PL"', null, "95"],
  ["Cascadia Code NF", "130", "125"],
])("a kept %s at %s %% becomes the bundled Cascadia, its size divided by 1.05 onto a step", (family, size, now) => {
  const kept = storage();
  kept.items.set(fontKey("code"), family);
  if (size !== null) kept.items.set(sizeKey("code"), size);
  const style = rootStyle();
  const fonts = createFonts(() => kept, () => style);
  expect(fonts.family("code")).toBeNull();
  expect(kept.items.has(fontKey("code"))).toBe(false);
  expect(kept.items.get(sizeKey("code")) ?? null).toBe(now);
  expect(fonts.size("code")).toBe(now === null ? 100 : Number(now));
  // Once: a second start reads what the first left.
  const again = createFonts(() => kept, () => rootStyle());
  expect(again.size("code")).toBe(fonts.size("code"));
});

test("the Cascadia migration runs once: a Cascadia chosen after it stays chosen, at its size", () => {
  const kept = storage();
  createFonts(() => kept, () => rootStyle());
  kept.items.set(fontKey("code"), "CaskaydiaCove Nerd Font Mono");
  kept.items.set(sizeKey("code"), "105");
  const again = createFonts(() => kept, () => rootStyle());
  expect(again.family("code")).toBe("CaskaydiaCove Nerd Font Mono");
  expect(again.size("code")).toBe(105);
});

test("another kept code face keeps its size, and Cascadia chosen for another role is that role's choice", () => {
  const kept = storage();
  kept.items.set(fontKey("code"), "Fira Code");
  kept.items.set(sizeKey("code"), "105");
  kept.items.set(fontKey("ui"), "Cascadia Code");
  const fonts = createFonts(() => kept, () => rootStyle());
  expect(fonts.family("code")).toBe("Fira Code");
  expect(fonts.size("code")).toBe(105);
  expect(fonts.family("ui")).toBe("Cascadia Code");
});

test("a kept size off the steps is read onto the nearest, and a kept bundled name is no choice", () => {
  const kept = storage();
  kept.items.set(sizeKey("ui"), "137");
  kept.items.set(sizeKey("code"), "nonsense");
  kept.items.set(fontKey("read"), "Inter");
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
  expect(fonts.ligatures).toBe(false);
  expect([...style.props]).toEqual([]);
  expect([...kept.items]).toEqual([[MONO_KEY, "1"]]);
  fonts.setSize("read", 110);
  expect(heard).not.toHaveBeenCalled();
});

/** `app/src/`, off `cwd`: Vitest's root is `app/`. */
const source = (file: string) => readFileSync(join(process.cwd(), "src", file), "utf8");

test("the lists of code's lines hear code's size, and the text measured in script takes its role's size", () => {
  // A run's log, a file's source and a diff's lines: rows that are line boxes, held at the new size (Fable's ruling 3).
  for (const file of ["lib/tf/builds/TfRunLog.svelte", "lib/tf/diff/TfDiffCard.svelte", "lib/tf/render/TfJarCode.svelte", "lib/tf/render/TfOasAnswer.svelte"]) {
    expect(source(file), file).toMatch(/const rowsOf = codeRows\((13, 1\.6|12, 1\.55|12, 1\.6)\);\s*const win = new Windowed<\w+>\(\{[^}]*estimate: rowsOf\.estimate[^}]*textSize: rowsOf\.textSize[^}]*\}\)/);
  }
  // A file's source is drawn by `TfLines` (Source and the XML view share it): its rows hear code's size unless the view
  // holds one height of its own.
  expect(source("lib/tf/code/TfLines.svelte")).toMatch(/const rowsOf = codeRows\(13, 1\.6\);[\s\S]*?const win = new Windowed<\w+>\(\{[^}]*estimate: fixed \?\? rowsOf\.estimate[^}]*textSize: fixed === null \? rowsOf\.textSize : null[^}]*\}\)/);
  expect(source("lib/tf/repo/TfRepoFiles.svelte")).toMatch(/<TfLines\b/);
  // A run's step names and the day strip's labels are fitted by a canvas: at the role's size, again when it changes.
  expect(source("lib/tf/builds/TfRun.svelte")).toMatch(/const px = \(\(command \? 11 : 12\) \* fonts\.size\(role\)\) \/ 100;\s*void fonts\.family\(role\);/);
  expect(source("lib/tf/time/TfDay.svelte")).toMatch(/const scale = fonts\.size\(role\) \/ 100;\s*void fonts\.family\(role\);[\s\S]*getPropertyValue\("--t-2xs"\)\) \* scale/);
  // And the interface's weight and letter spacing, after every character (2026-11-21).
  expect(source("lib/tf/builds/TfRun.svelte")).toMatch(/const weight = command \? 400 : fonts\.knob\("ui", "wt"\);\s*const track = command \? 0 : \(fonts\.knob\("ui", "ls"\) \/ 100\) \* px;[\s\S]*measureText\(text\)\.width \+ \[\.\.\.text\]\.length \* track/);
  expect(source("lib/tf/time/TfDay.svelte")).toMatch(/const track = plain \? \(fonts\.knob\("ui", "ls"\) \/ 100\) \* px : 0;[\s\S]*measureText\(text\)\.width \+ \[\.\.\.text\]\.length \* track/);
});

test("a list of code's lines hears its rows' height change with the new height over the old, and code's tracking at 1", () => {
  const fonts = createFonts(() => storage(), () => rootStyle());
  const file = codeRows(13, 1.6, fonts);
  const log = codeRows(12, 1.55, fonts);
  // Joined: Cascadia's own line, rounded to whole pixels: 16 px at 13 px, 15 at 12.
  expect(file.estimate).toBe(16);
  expect(log.estimate).toBe(15);
  const heardFile = vi.fn();
  const heardLog = vi.fn();
  const stop = file.textSize(heardFile);
  log.textSize(heardLog);
  fonts.setSize("ui", 120);
  fonts.setKnob("ui", "ls", 2);
  expect(heardFile).not.toHaveBeenCalled();
  // Open: each area's own number, 20.8 and 18.6, unrounded.
  fonts.setLines("open");
  expect(heardFile).toHaveBeenLastCalledWith(20.8 / 16);
  expect(heardLog).toHaveBeenLastCalledWith(18.6 / 15);
  fonts.setSize("code", 125);
  expect(heardFile).toHaveBeenLastCalledWith(1.25);
  // A notch is one ratio in every area, rounded: 1.40 at 16.25 px is 22.75, 23 px.
  fonts.setLines(140);
  expect(fonts.codeRow(13, 1.6)).toBe(23);
  expect(heardFile).toHaveBeenLastCalledWith(23 / 26);
  // Code's letter spacing: the same height, heard at 1 so wrapped rows are read again.
  heardFile.mockClear();
  fonts.setKnob("code", "ls", 3);
  expect(heardFile).toHaveBeenCalledExactlyOnceWith(1);
  stop();
  fonts.setLines("joined");
  expect(heardFile).toHaveBeenCalledTimes(1);
  expect(heardLog).toHaveBeenLastCalledWith(Math.round(15 * 1.2202) / Math.round(15 * 1.4));
});
test("main.ts loads the fonts before it mounts, so the first draw is in the kept faces and sizes", () => {
  const main = source("main.ts");
  const loaded = main.indexOf('import "./lib/tf/fonts.svelte";');
  expect(loaded).toBeGreaterThan(-1);
  expect(loaded).toBeLessThan(main.indexOf("mount(App"));
});

test("each role's knobs are kept as whole numbers, written on <html> off their default, and put on their notches", () => {
  const kept = storage();
  const style = rootStyle();
  const fonts = createFonts(() => kept, () => style);
  // Every knob at its default writes nothing: a default draws what it drew.
  expect([...style.props.keys()].filter((key) => /^--(ls|wt|lh)-/.test(key))).toEqual([]);
  expect(KNOBS).toEqual({ ui: ["ls", "wt"], head: ["ls"], read: ["lh", "ls", "wt"], code: ["ls", "wt"] });
  fonts.setKnob("ui", "ls", 2);
  fonts.setKnob("head", "ls", -3);
  fonts.setKnob("read", "lh", 181);
  fonts.setKnob("read", "wt", 320);
  fonts.setKnob("code", "wt", 470);
  fonts.setKnob("code", "ls", 9);
  // A knob a role does not have is no knob.
  fonts.setKnob("head", "wt", 450);
  expect(Object.fromEntries([...style.props].filter(([key]) => /^--(ls|wt|lh)-/.test(key)))).toEqual({
    "--ls-ui": "0.02em",
    "--ls-head": "-0.03em",
    "--lh-read": "1.8",
    "--wt-read": "350",
    "--wt-code": "450",
    "--ls-code": "0.06em",
  });
  expect(Object.fromEntries([...kept.items].filter(([key]) => /\.(ls|wt|lh)$/.test(key)))).toEqual({
    "knobas.ui.ls": "2",
    "knobas.head.ls": "-3",
    "knobas.read.lh": "180",
    "knobas.read.wt": "350",
    "knobas.code.wt": "450",
    "knobas.code.ls": "6",
  });
  const again = createFonts(() => kept, () => rootStyle());
  expect(again.knob("read", "lh")).toBe(180);
  expect(again.knob("head", "wt")).toBe(400);
  // Back to the default: nothing kept, nothing written.
  fonts.setKnob("ui", "ls", 0);
  fonts.setKnob("read", "lh", 165);
  expect(kept.items.has("knobas.ui.ls")).toBe(false);
  expect(style.props.has("--ls-ui")).toBe(false);
  expect(style.props.has("--lh-read")).toBe(false);
  expect(knobSteps("ui", "wt")).toEqual([350, 400, 450]);
  expect(knobSteps("code", "wt")).toEqual([300, 350, 400, 450, 500]);
  expect(knobSteps("code", "ls")).toEqual([-3, -2, -1, 0, 1, 2, 3, 4, 5, 6]);
  expect(knobSteps("read", "lh")).toEqual([140, 145, 150, 155, 160, 165, 170, 175, 180, 185, 190]);
  fonts.reset();
  expect([...style.props.keys()].filter((key) => /^--(ls|wt|lh)-/.test(key))).toEqual([]);
});

test("code's line spacing is Joined until chosen, kept as open or hundredths, and written as the line height drawn", () => {
  const kept = storage();
  const style = rootStyle();
  const fonts = createFonts(() => kept, () => style);
  expect(fonts.lines).toBe("joined");
  expect(style.props.has("--code-lh")).toBe(false);
  expect(style.props.has("--code-lh-face")).toBe(false);
  fonts.setLines("open");
  expect(style.props.get("--code-lh")).toBe("initial");
  expect(kept.items.get(LINES_KEY)).toBe("open");
  fonts.setLines(143);
  expect(fonts.lines).toBe(145);
  expect(kept.items.get(LINES_KEY)).toBe("145");
  expect(style.props.has("--code-lh")).toBe(false);
  expect(style.props.get("--code-lh-face")).toBe("1.45");
  expect(CODE_LINES).toEqual([110, 115, 120, 125, 130, 135, 140, 145, 150, 155, 160, 165, 170, 175, 180]);
  expect(createFonts(() => kept, () => rootStyle()).lines).toBe(145);
  fonts.setLines("joined");
  expect(kept.items.has(LINES_KEY)).toBe(false);
  expect(style.props.has("--code-lh-face")).toBe(false);
  // Anything else kept is Joined.
  kept.items.set(LINES_KEY, "tall");
  expect(createFonts(() => kept, () => rootStyle()).lines).toBe("joined");
});

test("an installed code face is measured once as it is picked, and Joined is its own line from then on", () => {
  const kept = storage();
  const style = rootStyle();
  const measured: string[] = [];
  const measure = (family: string) => (measured.push(family), family === "Menlo" ? { line: 1.17, bar: 1.3 } : null);
  const fonts = createFonts(() => kept, () => style, () => null, measure);
  const heard = vi.fn();
  fonts.onCodeRows(heard);
  fonts.setFamily("code", "Menlo");
  expect(measured).toEqual(["Menlo"]);
  expect(fonts.faceLine).toBe(1.17);
  expect(style.props.get("--code-lh-face")).toBe("1.17");
  expect(heard).toHaveBeenCalledExactlyOnceWith("rows");
  expect(JSON.parse(kept.items.get(MEASURE_KEY)!)).toEqual({ family: "Menlo", line: 1.17, bar: 1.3 });
  // Chosen again (the field left as it was chooses what it holds), it is not measured again.
  fonts.setFamily("code", "Menlo");
  expect(measured).toEqual(["Menlo"]);
  // The next start draws from what was kept, and measures again only once the window is idle, for the start after.
  const idle: (() => void)[] = [];
  const again = createFonts(() => kept, () => rootStyle(), () => null, () => ({ line: 1.2, bar: 1.3 }), undefined, (go) => idle.push(go));
  expect(again.faceLine).toBe(1.17);
  idle.forEach((go) => go());
  expect(again.faceLine).toBe(1.17);
  expect(JSON.parse(kept.items.get(MEASURE_KEY)!).line).toBe(1.2);
  // Another role's choice measures nothing; the bundled face is Cascadia's line, and its measure is gone.
  fonts.setFamily("ui", "Menlo");
  expect(measured).toEqual(["Menlo"]);
  fonts.setFamily("code", null);
  expect(fonts.faceLine).toBe(1.2202);
  expect(style.props.has("--code-lh-face")).toBe(false);
  expect(kept.items.has(MEASURE_KEY)).toBe(false);
  // A face that cannot be measured stands at Cascadia's line.
  fonts.setFamily("code", "Iosevka");
  expect(fonts.faceLine).toBe(1.2202);
  // A measure kept for another family is not this one's.
  kept.items.set(fontKey("code"), "Fira Code");
  expect(createFonts(() => kept, () => rootStyle(), () => null, () => null).faceLine).toBe(1.2202);
});

test("the ring is the tallest notch at which the drawn face's │ still joins, on this screen", () => {
  const fonts = createFonts(() => storage(), () => rootStyle());
  // 2407.24's │ is 1.246 of the size: 1.20 joins at 12, 13 and 14 px; 1.25 leaves 12 px a row of 15.
  expect(fonts.joinsUpTo).toBe(120);
  fonts.setFamily("code", "Cascadia Code 2009.22");
  expect(fonts.joinsUpTo).toBe(160);
  // An installed face whose │ was not measured has no ring.
  const other = createFonts(() => storage(), () => rootStyle(), () => null, () => ({ line: 1.2, bar: null }));
  other.setFamily("code", "Menlo");
  expect(other.joinsUpTo).toBeNull();
});

test("the device scale rounds code rows to its pixels, written on <html> off 1", () => {
  let now = 1;
  const heard: (() => void)[] = [];
  const style = rootStyle();
  const fonts = createFonts(() => storage(), () => style, () => null, () => null, { now: () => now, on: (go) => (heard.push(go), () => {}) });
  const rows = vi.fn();
  fonts.onCodeRows(rows);
  expect(style.props.has("--dpr")).toBe(false);
  // 12 px at 1.2202 is 14.64: 15 px at 1x, 18 device pixels (14.4 px) at 1.25.
  expect(fonts.codeRow(12, 1.55)).toBe(15);
  now = 1.25;
  heard.forEach((go) => go());
  expect(style.props.get("--dpr")).toBe("1.25");
  expect(fonts.codeRow(12, 1.55)).toBe(14.4);
  expect(rows).toHaveBeenCalledExactlyOnceWith("scale");
});

test("the older Cascadia is picked by name: its release is kept, it loads alone, and comments stand upright in it", async () => {
  const kept = storage();
  const style = rootStyle();
  const loads: string[] = [];
  const fonts = createFonts(() => kept, () => style, (family) => (loads.push(family), Promise.resolve()));
  expect(BUNDLED_CODE.map((face) => face.name)).toEqual(["Cascadia Mono", "Cascadia Code", "Cascadia Mono 2009.22", "Cascadia Code 2009.22"]);
  expect(familyName("Cascadia Mono 2009.22")).toBeNull();
  expect(familyName("knobas cascadia code 2009")).toBeNull();
  fonts.setFamily("code", "Cascadia Mono 2009.22");
  expect(fonts.release).toBe("2009.22");
  expect(fonts.ligatures).toBe(false);
  expect(fonts.family("code")).toBeNull();
  expect(fonts.bundled("code")).toBe("Cascadia Mono 2009.22");
  expect(loads).toEqual(["knobas Cascadia Mono 2009"]);
  expect(kept.items.get(RELEASE_KEY)).toBe("2009.22");
  // Drawn once it has landed.
  expect(style.props.has("--mono-bundled")).toBe(false);
  await Promise.resolve();
  expect(style.props.get("--mono-bundled")).toBe('"knobas Cascadia Mono 2009"');
  expect(style.props.get("--mono-synth")).toBe("none");
  // The switch flips Mono and Code within the release.
  fonts.setLigatures(true);
  await Promise.resolve();
  expect(style.props.get("--mono-bundled")).toBe('"knobas Cascadia Code 2009"');
  expect(style.props.get("--mono-ligatures")).toBe("normal");
  // The next start draws it at once.
  const after = rootStyle();
  createFonts(() => kept, () => after);
  expect(after.props.get("--mono-bundled")).toBe('"knobas Cascadia Code 2009"');
  expect(after.props.get("--mono-synth")).toBe("none");
  // An installed face chosen over it draws its own slant; the current release is picked back by name.
  fonts.setFamily("code", "Menlo");
  expect(style.props.has("--mono-synth")).toBe(false);
  fonts.setFamily("code", "Cascadia Mono");
  await Promise.resolve();
  expect(fonts.release).toBe("2407.24");
  expect(kept.items.has(RELEASE_KEY)).toBe(false);
  expect(style.props.has("--mono-bundled")).toBe(false);
  expect(style.props.has("--mono-synth")).toBe(false);
});
