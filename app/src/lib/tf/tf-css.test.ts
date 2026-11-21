/**
 * What the ticket-first stylesheet has to keep (#809), checked against the
 * design system file rather than against a copy of itself: the colour and type
 * tables are read out of `docs/specs/2026-10-15-ticket-first-design-system.md`,
 * so a token that drifts from the spec fails here.
 */
import { createHash } from "node:crypto";
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { brotliDecompressSync } from "node:zlib";

import { parse, parseCss } from "svelte/compiler";
import { expect, test } from "vitest";

import { ROLES } from "../render/roles";
import { EMBED_FRAME_BORDER, OUTLINE_ROOM, READER_DRAWER } from "./docs/docs";

const APP = process.cwd();
/**
 * The sheet as the browser sees it: `tf.css` with each `./css/<area>.css`
 * import replaced by that file, in import order, so the checks below read
 * the cascade and not one part of it. The font imports stay as written.
 */
const css = readFileSync(join(APP, "src/lib/tf/tf.css"), "utf8").replace(/^@import "(\.\/css\/[^"]+)";$/gm, (_, part: string) =>
  readFileSync(join(APP, "src/lib/tf", part), "utf8"),
);
/** What the components not yet ported still draw with (#881). */
const unportedCode = readFileSync(join(APP, "src/unported.css"), "utf8").replace(/\/\*[\s\S]*?\*\//g, "");
const spec = readFileSync(join(APP, "../docs/specs/2026-10-15-ticket-first-design-system.md"), "utf8");

/** `css` with its comments removed, so prose about a rule cannot satisfy it. */
const code = css.replace(/\/\*[\s\S]*?\*\//g, "");

/** The body of the first rule whose selector is exactly `selector`. */
function block(source: string, selector: string): string {
  const at = source.indexOf(`${selector} {`);
  expect(at, `no rule for ${selector}`).toBeGreaterThan(-1);
  return source.slice(at, source.indexOf("}", at));
}

test("a field is on `--field`: `--paper` and `--sheet` read as disabled", () => {
  const onPage = [...code.matchAll(/([^{}]+)\{([^{}]*)\}/g)].flatMap(([, selectors, body]) => {
    if (!/background:\s*var\(--(paper|sheet)\)/.test(body!)) return [];
    return selectors!
      .split(",")
      .map((selector) => selector.trim())
      .filter((selector) => /(^|[\s>+~])(input|textarea|select)\b[^\s>+~]*$/.test(selector) && !/checkbox|radio/.test(selector))
      // The branch box is a select in a row of buttons, so it is a button, on the button's fill (2026-11-21, the focus pass).
      .filter((selector) => selector !== ".tf-bsel select");
  });
  expect(onPage).toEqual([]);
});

/** `--token` → [light, dark], from the spec's colour table. */
const COLOURS: [string, string, string][] = [...spec.matchAll(/^\| `(--[a-z0-9-]+)`(?: \/ `(--[a-z0-9-]+)`)? \| `(#[0-9A-F]{6})`(?: \/ `(#[0-9A-F]{6})`)? \| `(#[0-9A-F]{6})`(?: \/ `(#[0-9A-F]{6})`)? \|/gm)].flatMap(
  (row) => {
    const pairs: [string, string, string][] = [[row[1]!, row[3]!, row[5]!]];
    if (row[2]) pairs.push([row[2], row[4]!, row[6]!]);
    return pairs;
  },
);

test("a held header's subline keeps its line both held and shown, so an empty one moves nothing as the header shows (2026-11-17)", () => {
  expect(css).toMatch(/\.tf-fhead\.tf-unread \.tf-sub, \.tf-fhead\.tf-holds \.tf-sub \{ min-height: 1lh; \}/);
});

test("Settings › Renderers › Decompiler stacks its cell and holds two lines for the Java line and its reason, so a late answer moves nothing (ux19 gate, 2026-11-19)", () => {
  expect(block(code, ".tf-stg-c.tf-dec-c")).toMatch(/flex-direction:\s*column/);
  const room = block(code, ".tf-dec-st");
  expect(room).toMatch(/line-height:\s*1\.5/);
  expect(room).toMatch(/[^-]height:\s*3em/);
  expect(room).toMatch(/overflow:\s*hidden/);
  expect(block(code, ".tf-dec-line")).toMatch(/line-clamp:\s*2/);
  expect(block(code, ".tf-dec-line.tf-dec-one")).toMatch(/line-clamp:\s*1/);
  expect(block(code, ".tf-dec-why")).toMatch(/white-space:\s*nowrap/);
});

test("a held view's line stays one line, the room it keeps, so a narrow column's tree does not move as it comes and goes (ux20 Windows pass, 2026-11-19)", () => {
  const line = block(code, ".tf-heldnote.tf-held1");
  expect(line).toMatch(/white-space:\s*nowrap/);
  expect(line).toMatch(/overflow:\s*hidden/);
  expect(line).toMatch(/text-overflow:\s*ellipsis/);
});

test("a toast is one line at most 680 px wide that gives up its middle, so no path or command line grows it (ux21, 2026-11-19)", () => {
  expect(block(code, ".tf-toast")).toMatch(/max-width:\s*min\(680px, calc\(100vw - 32px\)\)/);
  const line = block(code, ".tf-toast-line");
  expect(line).toMatch(/white-space:\s*pre/);
  expect(line).toMatch(/min-width:\s*0/);
  const head = block(code, ".tf-toast-head");
  expect(head).toMatch(/flex:\s*0 1 auto/);
  expect(head).toMatch(/overflow:\s*hidden/);
  expect(head).toMatch(/text-overflow:\s*ellipsis/);
  expect(block(code, ".tf-toast-tail")).toMatch(/flex:\s*none/);
});

test("an opening's chip floats where the press was, one line, nothing to press, and stills under reduced motion (ux21, 2026-11-19)", () => {
  const chip = block(code, ".tf-ack");
  expect(chip).toMatch(/position:\s*fixed/);
  expect(chip).toMatch(/max-width:\s*360px/);
  expect(chip).toMatch(/white-space:\s*nowrap/);
  expect(chip).toMatch(/pointer-events:\s*none/);
  expect(block(code, ".tf-acks")).toMatch(/pointer-events:\s*none/);
  // The layer takes no room: a fixed box over the whole window made WebView2 scroll the board and a Docs space on the
  // main thread (the ux21 Windows pass).
  const layer = block(code, ".tf-acks");
  expect(layer).not.toMatch(/inset:/);
  expect(layer).toMatch(/width:\s*0;/);
  expect(layer).toMatch(/height:\s*0;/);
  expect(code).toMatch(/@media \(prefers-reduced-motion: reduce\) \{\s*\.tf-ack \{ animation: none; transition: none; \}/);
});

test("the spec's colour table was read, so the next two tests check something", () => {
  expect(COLOURS.length).toBeGreaterThanOrEqual(23);
});

/**
 * A colour token as `tokens.css` writes it (ADR-0055): its side's variable with knobas's value behind it,
 * `--paper: var(--paper-light, #E4E8EC);`, or the plain value for a token no theme sets.
 */
const holds = (body: string, token: string, side: "light" | "dark", value: string) => body.includes(`${token}: var(${token}-${side}, ${value});`) || body.includes(`${token}: ${value};`);

test("light tokens are the spec's, on :root (#881)", () => {
  const light = block(code, ":root");
  const wrong = COLOURS.filter(([token, value]) => !holds(light, token, "light", value));
  expect(wrong).toEqual([]);
});

test("dark tokens are the spec's, by the OS unless forced light, and when forced dark", () => {
  const media = code.slice(code.indexOf("@media (prefers-color-scheme: dark)"));
  const byOs = block(media, ':root:not([data-theme="light"])');
  const forced = block(code, ':root[data-theme="dark"]');
  for (const dark of [byOs, forced]) {
    const wrong = COLOURS.filter(([token, , value]) => !holds(dark, token, "dark", value));
    expect(wrong).toEqual([]);
  }
});

test("the two dark blocks are one set of tokens, so an edit cannot reach only one", () => {
  const media = code.slice(code.indexOf("@media (prefers-color-scheme: dark)"));
  const tokens = (body: string) => body.slice(body.indexOf("{") + 1).replace(/\s+/g, " ").trim();
  expect(tokens(block(media, ':root:not([data-theme="light"])'))).toBe(tokens(block(code, ':root[data-theme="dark"]')));
});

test("the tokens are set once, on :root, and the unported sheet sets none of the new ones nor styles the shell", () => {
  expect(code.match(/(^|\n)\.tf \{[^}]*--paper\s*:/), "a second token block on the shell root").toBeNull();
  expect(unportedCode).not.toMatch(/\.tf\b|tf-/);
  const own = [...unportedCode.matchAll(/(--[a-z0-9-]+)\s*:/g)].map((match) => match[1]!);
  expect(own.filter((token) => COLOURS.some(([name]) => name === token))).toEqual([]);
});

test("the type scale is the spec's", () => {
  const scale = [...spec.matchAll(/^\| `(--t-[a-z0-9]+)` \| (\d+) px \|/gm)].map((row) => [row[1]!, row[2]!]);
  expect(scale.length).toBe(11);
  const root = block(code, ":root");
  expect(scale.filter(([token, px]) => !root.includes(`${token}: ${px}px;`))).toEqual([]);
});

test("every class is tf- prefixed, so app.css's bare class rules cannot reach the new shell", () => {
  const rules = code.replace(/"[^"]*"/g, '""');
  const classes = [...rules.matchAll(/\.(-?[_a-zA-Z][\w-]*)/g)].map((match) => match[1]!);
  expect(classes.length).toBeGreaterThan(20);
  expect([...new Set(classes.filter((name) => name !== "tf" && !name.startsWith("tf-")))]).toEqual([]);
});

/**
 * The window is the shell: a scrolling document would double-scroll every pane
 * inside it (carried over from the rooms UI's `app-css.test.ts`, #881).
 */
test("keeps the document from scrolling behind the shell", () => {
  expect(code).toMatch(/html,\s*body\s*\{[^}]*overflow:\s*hidden/);
});

/**
 * Spec §14's 4.5:1 for text, computed from the spec's own table rather than
 * asserted about. Since #881 the components not yet ported read these tokens
 * too (through `unported.css`), on the same three surfaces.
 */
test("every text token clears 4.5:1 on paper, sheet and sheet-2, light and dark", () => {
  const channel = (value: number) => {
    const c = value / 255;
    return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
  };
  const luminance = (hex: string) => {
    const [r, g, b] = [1, 3, 5].map((at) => channel(Number.parseInt(hex.slice(at, at + 2), 16)));
    return 0.2126 * r! + 0.7152 * g! + 0.0722 * b!;
  };
  const contrast = (a: string, b: string) => {
    const [high, low] = [luminance(a), luminance(b)].sort((x, y) => y - x);
    return (high! + 0.05) / (low! + 0.05);
  };
  const value = (token: string, dark: boolean) => {
    const row = COLOURS.find(([name]) => name === token);
    expect(row, `${token} is not in the spec's table`).toBeTruthy();
    return row![dark ? 2 : 1];
  };
  const failing: string[] = [];
  for (const dark of [false, true]) {
    for (const surface of ["--paper", "--sheet", "--sheet-2"]) {
      for (const text of ["--ink", "--ink-2", "--muted", "--ok", "--warn", "--err", "--info"]) {
        const ratio = contrast(value(text, dark), value(surface, dark));
        if (ratio < 4.5) failing.push(`${text} on ${surface} (${dark ? "dark" : "light"}): ${ratio.toFixed(2)}`);
      }
    }
  }
  expect(failing).toEqual([]);
});

/*
 * Width is `font-variation-settings: "wdth" N`, with `font-stretch: N%` beside it since Settings › Fonts (2026-11-18):
 * a chosen Headings face narrows through its own width axis or condensed members, which only `font-stretch` reaches.
 * The bundled Inter has neither and draws both at its normal width (2026-11-20).
 * Every `font-stretch` is a `"wdth"`'s twin, at the same width (`code-face.test.ts` holds that every narrowed rule has one).
 */
test("width comes from font-variation-settings, and font-stretch only as its twin at the same width", () => {
  expect(block(code, ".tf-jobkey")).toMatch(/font-variation-settings: "wdth" 62; font-stretch: 62%/);
  const stretches = [...code.matchAll(/font-stretch:\s*([^;}]+)/g)].map((m) => m[1]!.trim());
  const widths = [...code.matchAll(/font-variation-settings:\s*"wdth"\s*([\d.]+)\s*;\s*font-stretch:\s*([\d.]+)%/g)];
  expect(stretches.length).toBeGreaterThan(20);
  expect(widths).toHaveLength(stretches.length);
  for (const [, wdth, stretch] of widths) expect(stretch).toBe(wdth);
});

test("focus is 2px --focus, and --card-ink on a card surface", () => {
  expect(code).toMatch(/:root \{ --focus-w: 2px; --focus-off: 2px; --focus-in: -2px; \}/);
  expect(code).toMatch(/\.tf \{ --focus-c: var\(--focus\); --focus-o: var\(--focus-off\); \}/);
  expect(block(code, ".tf :focus-visible")).toMatch(/outline: var\(--focus-w\) solid var\(--focus-c\); outline-offset: var\(--focus-o\);/);
  expect(block(code, ".tf .tf-card")).toMatch(/--focus-c: var\(--card-ink\)/);
});

/**
 * The ring takes the shape of what it rings (2026-11-21; Björn: "the blue selection outline around elements might seem
 * weird"): the focus rule set `border-radius: 3px`, so a focused pill or switch went square and a square tab went round.
 */
test("no focus rule changes a control's shape: none sets a radius", () => {
  const focusRules = [...code.matchAll(/([^{}]+)\{([^{}]*)\}/g)].filter(([, selectors]) => /:focus(-visible|-within)?\b/.test(selectors!));
  expect(focusRules.length).toBeGreaterThan(20);
  for (const [, selectors, body] of focusRules) expect(body, selectors!.trim()).not.toMatch(/border-radius/);
});

/**
 * One place: a component sets `--focus-o` or `--focus-c`, and never writes the ring's literal width and colour. Every
 * stylesheet is read, those a component imports itself (`jar.css`, `openapi.css`) too, and every component's `<style>`.
 */
test("no component writes a ring of its own: every focus outline is drawn from the tokens", () => {
  const sheets = readdirSync(join(APP, "src/lib/tf/css"))
    .filter((name) => name.endsWith(".css"))
    .map((name) => [name, readFileSync(join(APP, "src/lib/tf/css", name), "utf8")] as const);
  const components = (readdirSync(join(APP, "src/lib"), { recursive: true }) as string[])
    .filter((name) => name.endsWith(".svelte"))
    .map((name) => [name, readFileSync(join(APP, "src/lib", name), "utf8").match(/<style[^>]*>([\s\S]*?)<\/style>/)?.[1] ?? ""] as const);
  expect(sheets.length).toBeGreaterThan(30);
  for (const [name, text] of [...sheets, ...components]) {
    const body = text.replace(/\/\*[\s\S]*?\*\//g, "");
    expect(body, name).not.toMatch(/outline:\s*2px solid var\(--focus\)/);
    const focusRules = [...body.matchAll(/([^{}]+)\{([^{}]*)\}/g)].filter(([, selectors]) => /:focus(-visible|-within)?\b/.test(selectors!));
    for (const [, selectors, rule] of focusRules) expect(rule, `${name}: ${selectors!.trim()}`).not.toMatch(/outline(-offset|-width)?:[^;]*\dpx/);
  }
});

test("a field rings inside, as its own border; a box that clips gives what it holds the inside ring", () => {
  expect(code).toMatch(/\.tf :is\(textarea, select, input:not\(\[type="checkbox"\], \[type="radio"\], \[type="range"\]\)\):focus-visible \{ outline-color: var\(--focus\); outline-offset: var\(--focus-in\); \}/);
  const clippers = /:is\(([^)]*)\) \{ --focus-o: var\(--focus-in\); \}/.exec(code);
  expect(clippers).not.toBeNull();
  for (const box of [".tf-nav", ".tf-tabs", ".tf-seg", ".tf-menu", ".tf-flist", ".tf-fence", ".tf-ftree", ".tf-tbl"]) expect(clippers![1]!.split(", ")).toContain(box);
});

test("a select pressed with the mouse draws no ring (`data-input`, input-modality.ts)", () => {
  expect(code).toMatch(/:root\[data-input="pointer"\] \.tf select:focus-visible \{ outline: none; \}/);
});

/** The branch box is a button that opens a list (2026-11-21; Björn: "the box from branch switching … different sized than the button above"). */
test("the branch box wears the button's face: height, fill, border, radius, padding, and as wide as its name", () => {
  const box = block(code, ".tf-bsel select");
  expect(box).toMatch(/appearance: none; height: var\(--control-h\); max-width: 282px; padding: 0 29px 0 33px;/);
  expect(box).toMatch(/border: 1px solid var\(--rule\); border-radius: 4px; background: var\(--sheet\)/);
  expect(box).toMatch(/font: 500 calc\(var\(--t-sm\) \* var\(--fs\)\) var\(--mono\)/);
  expect(box).toMatch(/field-sizing: content/);
  expect(block(code, ".tf-btn")).toMatch(/height: var\(--control-h\); padding: 0 11px; border: 1px solid var\(--rule\); border-radius: 4px;\s*background: var\(--sheet\)/);
  expect(block(code, ".tf .tf-bsel select:focus-visible")).toMatch(/outline-offset: var\(--focus-off\)/);
});

test("a disabled icon-only button is dimmed like a disabled button (#2288)", () => {
  expect(code).toMatch(/\.tf-iconbtn\[disabled\][^{]*\{ opacity: \.55; cursor: not-allowed; \}/);
});

test("reduced motion stops the pulse and every other animation in the shell", () => {
  expect(block(code, ".tf-dot.tf-run")).toMatch(/animation: tf-pulse 1\.4s/);
  const reduced = code.slice(code.indexOf("@media (prefers-reduced-motion: reduce)"));
  expect(reduced).toMatch(/\.tf \*[^{]*\{ transition: none !important; animation: none !important; \}/);
});

/**
 * The faces are the app's own files (2026-11-20, `fonts/fonts.css`): imported once, before every rule, never fetched,
 * and every file a face names is in its folder beside its licence (`OFL.txt`) and its `SOURCE.md`, whose SHA-256 is
 * the file's. Cascadia Code's licence reserves its name for Microsoft's files as they are, so a changed byte there is a
 * licence breach as much as a different font; Inter's is held the same way, so a new version is a deliberate change.
 */
test("the faces are the app's own files, each beside its licence, as SOURCE.md says they are", () => {
  expect(css).toMatch(/^@import "\.\/fonts\/fonts\.css";$/m);
  expect(code).not.toMatch(/@fontsource|https?:/);
  const fonts = join(APP, "src/lib/tf/fonts");
  const faces = readFileSync(join(fonts, "fonts.css"), "utf8");
  expect(faces).not.toMatch(/https?:/);
  const files = [...faces.matchAll(/url\("\.\/([\w-]+)\/([\w-]+\.woff2)"\)/g)].map(([, folder, file]) => ({ folder: folder!, file: file! }));
  expect(files.map(({ folder }) => folder).filter((folder, i, all) => all.indexOf(folder) === i)).toEqual(["inter", "cascadia-mono", "cascadia-code", "cascadia-2009"]);
  expect(files).toHaveLength(13);
  for (const { folder, file } of files) {
    const bytes = readFileSync(join(fonts, folder, file));
    expect(bytes.subarray(0, 4).toString("latin1"), file).toBe("wOF2");
    const source = readFileSync(join(fonts, folder, "SOURCE.md"), "utf8");
    expect(source, `${folder}/SOURCE.md names ${file}`).toContain(`| \`${file}\` |`);
    expect(source.split("\n").find((line) => line.includes(`\`${file}\``)), file).toContain(createHash("sha256").update(bytes).digest("hex"));
    const licence = readFileSync(join(fonts, folder, "OFL.txt"), "utf8");
    expect(licence, `${folder}/OFL.txt`).toContain("SIL OPEN FONT LICENSE Version 1.1");
  }
  // The copyright notice above the licence's text, where a reserved name is declared.
  const notice = (folder: string) => readFileSync(join(fonts, folder, "OFL.txt"), "utf8").split("This Font Software is licensed")[0]!;
  expect(notice("cascadia-code")).toMatch(/Microsoft Corporation,\s+with Reserved Font Name Cascadia Code\./);
  expect(notice("cascadia-mono")).toMatch(/Microsoft Corporation,\s+with Reserved Font Name Cascadia Code\./);
  expect(notice("cascadia-2009")).toMatch(/Microsoft Corporation,\s+with Reserved Font Name Cascadia Code\./);
  expect(notice("inter")).toMatch(/The Inter Project Authors/);
  expect(notice("inter")).not.toMatch(/Reserved Font Name/);
});

/**
 * The axes are in the font files, not synthesised: Inter's `opsz` (14 to 32), which draws headings in its display cut
 * by itself, and its `wght`; Cascadia Mono's and Code's `wght` (200 to 700), which code's 400 and 600 take.
 *
 * WOFF2 keeps its table data in one Brotli stream after a variable-length table directory; an `fvar` axis record is
 * the tag followed by min, default and max as 16.16 fixed.
 */
test("the bundled Inter has its optical size and weight axes, Cascadia Mono and Code their weight axis", () => {
  const font = (path: string) => readFileSync(join(APP, "src/lib/tf/fonts", path));
  const inter = font("inter/inter-latin-opsz-normal.woff2");
  expect(axes(inter, "opsz")).toContainEqual({ tag: "opsz", min: 14, max: 32 });
  expect(axes(inter, "wght")).toContainEqual({ tag: "wght", min: 100, max: 900 });
  expect(axes(font("cascadia-code/CascadiaCode.woff2"), "wght")).toContainEqual({ tag: "wght", min: 200, max: 700 });
  expect(axes(font("cascadia-code/CascadiaCodeItalic.woff2"), "wght")).toContainEqual({ tag: "wght", min: 200, max: 700 });
  expect(axes(font("cascadia-mono/CascadiaMono.woff2"), "wght")).toContainEqual({ tag: "wght", min: 200, max: 700 });
  expect(axes(font("cascadia-mono/CascadiaMonoItalic.woff2"), "wght")).toContainEqual({ tag: "wght", min: 200, max: 700 });
  // The older release, 2009.22 (2026-11-21): its two upright files, the same axis.
  expect(axes(font("cascadia-2009/CascadiaMono.woff2"), "wght")).toContainEqual({ tag: "wght", min: 200, max: 700 });
  expect(axes(font("cascadia-2009/CascadiaCode.woff2"), "wght")).toContainEqual({ tag: "wght", min: 200, max: 700 });
});

function axes(woff2: Buffer, tag: string): { tag: string; min: number; max: number }[] {
  const tables = woff2.readUInt16BE(12);
  const compressed = woff2.readUInt32BE(20);
  let at = 48;
  const base128 = () => {
    let value = 0;
    for (;;) {
      const byte = woff2[at++]!;
      value = value * 128 + (byte & 0x7f);
      if ((byte & 0x80) === 0) return value;
    }
  };
  for (let index = 0; index < tables; index += 1) {
    const flags = woff2[at++]!;
    const known = flags & 0x3f;
    if (known === 63) at += 4;
    base128();
    const version = flags >> 6;
    // glyf (10) and loca (11) are transformed at version 0; every other table at a non-zero one.
    const transformed = known === 10 || known === 11 ? version === 0 : version !== 0;
    if (transformed) base128();
  }
  const data = brotliDecompressSync(woff2.subarray(at, at + compressed));
  const found: { tag: string; min: number; max: number }[] = [];
  for (let index = data.indexOf(tag); index !== -1; index = data.indexOf(tag, index + 1)) {
    if (index + 16 <= data.length) found.push({ tag, min: data.readInt32BE(index + 4) / 65536, max: data.readInt32BE(index + 12) / 65536 });
  }
  return found;
}

test("on the Time view's strip, yellow fills the running timer's block and nothing else (#827)", () => {
  // Every rule of the Day tab that paints with the card's yellow.
  const yellow = [...code.matchAll(/([^{}]*)\{([^}]*)\}/g)]
    .filter(([, selector, body]) => selector!.includes("tf-tm-") && /var\(--card(-2)?\)/.test(body!))
    .map(([, selector]) => selector!.trim());
  expect(yellow).toEqual([".tf-tm-b.tf-tm-run rect", ".tf-tm-legend i.tf-tm-run"]);
});

test("a sync run's result wraps anywhere, so a request URL in an error cannot widen the table (#1478)", () => {
  expect(block(code, ".tf-stg-res")).toContain("overflow-wrap: anywhere;");
});

test("the top bar's timer target is capped with an ellipsis, a label is sans and a key mono, and the nav fits at 1100px (#1430)", () => {
  const key = block(code, ".tf-timer-key");
  expect(key).toMatch(/max-width:\s*22ch/);
  expect(key).toMatch(/text-overflow:\s*ellipsis/);
  expect(key).not.toMatch(/font-family/);
  expect(block(code, ".tf-timer-key.tf-mono")).toMatch(/font-family:\s*var\(--mono\)/);
  const at = code.indexOf("@media (max-width: 1240px) and (min-width: 901px)");
  expect(at, "no rule band between the narrow layout and 1240px").toBeGreaterThan(-1);
  const band = code.slice(at, code.indexOf("\n}", at));
  expect(band).toMatch(/\.tf-search\s*\{[^}]*min-width:\s*0/);
  expect(band).toMatch(/\.tf-search \.tf-label, \.tf-search kbd\s*\{[^}]*display:\s*none/);
  expect(band).toMatch(/\.tf-timer-key\s*\{[^}]*max-width:\s*12ch/);
  expect(band).toMatch(/\.tf-nav a\s*\{[^}]*padding:/);
  // A count is a state to act on (R-1.3): Inbox and Monitors keep theirs in the band.
  expect(band).not.toMatch(/\.tf-nav \.tf-count\s*\{[^}]*display:\s*none/);
});

/**
 * The ⌘K launcher on the shell's tokens (#1422). Its rules are one part of
 * `tf.css`, so the tf- prefix test above reads them; its components carry no
 * stylesheet of their own, so nothing outside this part can set a font there.
 */
const LAUNCHER = ["Launcher", "Board", "Chips", "Help", "QueryBox", "Results", "Row"].map((name) =>
  readFileSync(join(APP, `src/lib/launcher/${name}.svelte`), "utf8"),
);
const launcherCode = readFileSync(join(APP, "src/lib/tf/css/launcher.css"), "utf8").replace(/\/\*[\s\S]*?\*\//g, "");
/** `[selector, body]` for every rule of the launcher's part. */
const launcherRules = [...launcherCode.matchAll(/([^{}]+)\{([^}]*)\}/g)].map((rule) => [rule[1]!.trim(), rule[2]!] as const);

test("the launcher's rules are a part of tf.css, and its components have no stylesheet of their own (#1422)", () => {
  expect(readFileSync(join(APP, "src/lib/tf/tf.css"), "utf8")).toMatch(/^@import "\.\/css\/launcher\.css";$/m);
  expect(code).toContain(".tf-ln-res");
  expect(launcherRules.length).toBeGreaterThan(40);
  for (const source of LAUNCHER) {
    expect(source).not.toMatch(/<style\b/);
    expect(source).not.toMatch(/\btf-mono\b|--mono\b/);
  }
});

test("the launcher sets mono on a ticket's key cell, a code evidence line and the pane's files, and nowhere else (#1422, #2421, D10, D11)", () => {
  const mono = launcherRules.filter(([, body]) => /var\(--mono\)/.test(body)).map(([selector]) => selector);
  expect(mono).toEqual([".tf-ln-file-at", ".tf-ln-file-code", ".tf-ln-k.tf-ln-mono", ".tf-ln-why.tf-ln-code .tf-ln-why-t"]);
  // The key cell is drawn by the row, and only there.
  const row = LAUNCHER[6]!;
  // A forge's issue key is cut to its repository and number there, the whole in its title (the live run, 2026-11-09).
  expect(row).toMatch(/<span class="tf-ln-k" class:tf-ln-mono=\{mono\} title=\{[^}]*\}>\{mono \? shortKey\(cell\) : cell\}<\/span>/);
  expect(LAUNCHER.slice(0, 6).join("\n")).not.toMatch(/tf-ln-(k|mono)\b/);
});

test("the launcher's dialog does not clip to its rounded corners, so a wheel over its list scrolls on the compositor (2026-11-20)", () => {
  // `overflow: hidden` with `border-radius` on the dialog made Chromium hit-test each wheel over the list on the main thread.
  const rules = (selector: string) => launcherRules.filter(([s]) => s === selector).map(([, body]) => body).join(";");
  expect(rules(".tf-dialog.tf-launch")).toMatch(/overflow:\s*visible/);
  // Its footer's fill keeps to the corners the dialog no longer clips.
  expect(rules(".tf-launch > footer")).toMatch(/border-radius:\s*0 0 7px 7px/);
});

test("the launcher is prototype-6's dialog: an ink selection bar, a borderless 17px box, no capitals and no rooms tokens (#1422)", () => {
  const selected = launcherRules.find(([selector]) => selector.includes(".tf-ln-res.tf-ln-on"))?.[1] ?? "";
  expect(selected).toMatch(/background:\s*var\(--sheet-2\)/);
  expect(selected).toMatch(/box-shadow:\s*inset 3px 0 0 var\(--ink\)/);
  const box = launcherRules.find(([selector]) => selector === ".tf .tf-ln-q input")?.[1] ?? "";
  expect(box).toMatch(/font-size:\s*calc\(var\(--t-xl\) \* var\(--fs\)\)/);
  expect(box).toMatch(/border:\s*0/);
  expect(launcherRules.find(([selector]) => selector === ".tf-ln-q")?.[1]).toMatch(/border-bottom:\s*1px solid var\(--rule\)/);
  expect(launcherCode).not.toMatch(/text-transform/);
  // Only the design system's tokens: none of the names unported.css keeps for the rooms components.
  const used = [...launcherCode.matchAll(/var\((--[a-z0-9-]+)\)/g)].map((match) => match[1]!);
  const rooms = [...unportedCode.matchAll(/(--[a-z0-9-]+)\s*:/g)].map((match) => match[1]!);
  expect(used.filter((token) => rooms.includes(token))).toEqual([]);
});

/*
 * #1445: JetBrains Mono's contextual ligatures draw `->`, `=>`, `<=` and `==`
 * as single glyphs, so source text would show characters the file does not
 * hold. A `font:` shorthand resets `font-variant-ligatures`, so the property
 * has to sit in the same rule, after the shorthand. Since 2026-11-16 it is
 * `var(--mono-ligatures)`, `none` until *Ligatures in code* is turned on in
 * Settings › Appearance (`code-face.svelte.ts`).
 */
test("code, diff, editor and the mono token class draw ligatures only as Settings says", () => {
  for (const selector of [".tf-code", ".tf-diff", ".tf-editor", ".tf .tf-mono"]) {
    expect(block(code, selector), selector).toMatch(/font-variant-ligatures: var\(--mono-ligatures\)/);
  }
});

test("every rule that takes the mono font variable sets the ligature switch after its font declaration", () => {
  const rules = [...code.matchAll(/([^{}]+)\{([^{}]*)\}/g)].filter((m) => m[2]!.includes("var(--mono)"));
  expect(rules.length).toBeGreaterThan(30);
  const bare = rules
    .filter((m) => {
      const body = m[2]!;
      const lastFont = Math.max(body.lastIndexOf("font:"), body.lastIndexOf("font-family:"));
      return body.lastIndexOf("font-variant-ligatures: var(--mono-ligatures)") < lastFont;
    })
    .map((m) => m[1]!.trim());
  expect(bare).toEqual([]);
});

test("a Find match is on --mark and inherits its colour (#1462)", () => {
  const base = block(code, ".tf mark");
  expect(base).toMatch(/background:\s*var\(--mark\)/);
  expect(base).toMatch(/color:\s*inherit/);
});

test("the asset tree page is viewport-high, so its pane scrolls and the page does not (#1460)", () => {
  expect(block(code, ".tf-full:has(> .tf-asset-tree)")).toMatch(/(^|[{;\s])height:\s*100%/);
  expect(block(code, ".tf-asset-tree")).toMatch(/min-height:\s*0/);
});

test("the Docs reader: tree and page, the outline the page's sticky right column, folded on the component's own threshold (#2081)", () => {
  expect(block(code, ".tf-rd")).toMatch(/grid-template-columns:\s*var\(--side-docs-tree, 300px\) minmax\(0, 1fr\);/);
  expect(block(code, ".tf-rd-sheet")).toMatch(/grid-template-columns:\s*minmax\(0, 1fr\) var\(--side-docs-outline, 268px\);/);
  const outline = block(code, ".tf-rd-outline");
  expect(outline).toMatch(/position:\s*sticky/);
  // Where it starts, under the pinned crumb row and the sheet's padding, so its heading does not move as the page scrolls (F-443).
  expect(outline).toMatch(/top:\s*60px/);
  // The outline folds by the sheet's own width (`readerFit`), and no window width in the stylesheet decides it.
  expect(block(code, ".tf-rd-sheet")).toContain("column-gap: 56px");
  expect(OUTLINE_ROOM).toBe(268 + 56);
  expect(block(code, ".tf-rd-sheet.tf-rd-solo")).toMatch(/grid-template-columns:\s*minmax\(0, 1fr\)\s*;/);
  expect(code).not.toMatch(/@media \(max-width: 13\d\dpx\) \{\s*\.tf-rd-sheet/);
  // The tree narrows at the shell's own 1240px breakpoint.
  const rail = code.indexOf("@media (max-width: 1240px) {\n  .tf-rd {");
  expect(rail, "no 1240px band for the tree").toBeGreaterThan(-1);
  expect(code.slice(rail, code.indexOf("\n}", rail))).toMatch(/\.tf-rd\s*\{[^}]*grid-template-columns:\s*var\(--side-docs-tree, 220px\) minmax\(0, 1fr\)/);
});

test("beside the outline nothing in the Docs reader's page is capped in width, and folded its body stops at 730 px (#2081, F-432)", () => {
  const capped = [...code.matchAll(/([^{}]*)\{([^}]*)\}/g)]
    .filter(([, selector]) => /\.tf-(rd-page|rd-sheet|doc|cmts|sec-edit|compose|pre|callout|macro|rd-foot)\b/.test(selector!))
    .filter(([, , body]) => /max-width/.test(body!))
    .map(([, selector]) => selector!.trim());
  expect(capped).toEqual([".tf-rd-solo :is(.tf-doc, .tf-rd-fold)"]);
  expect(block(code, ".tf-rd-solo :is(.tf-doc, .tf-rd-fold)")).toMatch(/max-width:\s*730px/);
  expect(code).not.toContain("74ch");
});

test("below 900px the Docs tree is a drawer over the page, on the component's own threshold, and still under reduced motion (#2084)", () => {
  const at = code.indexOf(`@media ${READER_DRAWER} {`);
  expect(at, `no rule band for ${READER_DRAWER}`).toBeGreaterThan(-1);
  expect(code, "the old band put 900px on the narrow side").not.toContain("@media (max-width: 900px) {\n  .tf-rd");
  const band = code.slice(at, code.indexOf("\n}", at));
  const rule = (selector: string) => {
    const from = band.indexOf(`${selector} {`);
    expect(from, `no ${selector} in the band`).toBeGreaterThan(-1);
    return band.slice(from, band.indexOf("}", from));
  };
  // The tree leaves the grid and lies over the page, so the page does not reflow.
  expect(rule(".tf-rd")).toMatch(/grid-template-columns:\s*minmax\(0, 1fr\);/);
  expect(rule(".tf-rd")).toMatch(/position:\s*relative/);
  const tree = rule(".tf-rd-tree");
  expect(tree).toMatch(/position:\s*absolute/);
  expect(tree).toMatch(/transform:\s*translateX\(-100%\)/);
  expect(tree).toMatch(/visibility:\s*hidden/);
  expect(tree).toMatch(/transition:\s*transform/);
  expect(tree).not.toMatch(/display:\s*none/);
  const open = rule(".tf-rd-drawer .tf-rd-tree");
  expect(open).toMatch(/transform:\s*none/);
  expect(open).toMatch(/visibility:\s*visible/);
  const backdrop = rule(".tf-rd-backdrop");
  expect(backdrop).toMatch(/position:\s*absolute/);
  expect(backdrop).toMatch(/inset:\s*0/);
  expect(backdrop, "the scrim fades in, as the design system's Motion asks").toMatch(/animation:\s*tf-fade \.15s/);
  expect(Number(/z-index:\s*(\d+)/.exec(backdrop)![1])).toBeLessThan(Number(/z-index:\s*(\d+)/.exec(tree)![1]));
  // Reduced motion: no slide, open or closed, and the rule outranks the band's.
  const still = code.slice(code.indexOf("@media (prefers-reduced-motion: reduce) {", at));
  const stillRule = still.slice(0, still.indexOf("\n}"));
  expect(stillRule).toMatch(/\.tf-rd \.tf-rd-tree,\s*\.tf-rd\.tf-rd-drawer \.tf-rd-tree\s*\{\s*transition:\s*none;/);
  expect(stillRule).toMatch(/\.tf-rd-backdrop\s*\{\s*animation:\s*none;/);
});

/** The one-row bars that stick: never taller than the window, so they need no bound of their own (#2091). */
test("no page's scroller reserves room for its bar: the page is as wide as the window, and a rail's own list may keep its room", () => {
  const gutters = [...code.matchAll(/([^{}]*)\{[^}]*scrollbar-gutter[^}]*\}/g)].map(([, selector]) => selector!.trim().split("\n").at(-1)!.trim());
  // Builds' scroller keeps a gutter only to take it back with its clip (builds at 6x, 2026-11-15): the page keeps no
  // room for its bar, and its host is no size container.
  expect(gutters).toEqual([".tf-shost > .tf-bmain"]);
  // Tickets' views list keeps its bar's room by scrolling always, not by a kept gutter: with LCD text (Windows, Linux)
  // the gutter made Chromium hit-test each wheel over it on the main thread (`scrollers.mjs --strict`, 2026-11-20).
  expect(block(code, ".tf-vfind + .tf-vlist")).toMatch(/overflow-y:\s*scroll/);
  expect(code).toMatch(/\.tf-shost > \.tf-bmain > \.tf-sclip \{\s*margin-right:\s*-8px;\s*\}/);
  expect(block(code, ".tf-main")).toMatch(/overflow:\s*hidden auto/);
  expect(block(code, ".tf-sbody")).toMatch(/width:\s*100cqw/);
});

const STICKY_BARS = [".tf-mon-gaps thead th", ".tf-mon-head", ".tf-fbar", ".tf-findbar", ".tf-findhold", ".tf-rd-crumbrow", ".tf-ln-h", ".tf-wtabs", ".tf-logbar", ".tf-pdf-bar", ".tf-board-wrap.tf-lanes .tf-sbody > :is(.tf-fhead, .tf-fbar, .tf-crumbs)", ".tf-board-wrap:not(.tf-lanes) .tf-col h3", ".tf-ticketcard .tf-go", ".tf-diff .tf-fh", ".tf-list .tf-tbl th", ".tf-board-wrap.tf-lanes > :is(.tf-fhead, .tf-fbar, .tf-crumbs)", ".tf-tm-week :is(th:first-child, td.tf-tm-t)", ".tf-tm-week :is(th.tf-tm-tot, td.tf-tm-tot)", ".tf-laned > h2", ".tf-lanes-more", ".tf-newruns", ".tf-dtool.tf-dband", ".tf-prfiles.tf-with-tool > .tf-pickbar"];

test("every sticky side column is bounded by the window and scrolls on its own, so its last entry is reachable (#2091)", () => {
  const sticky = [...code.matchAll(/([^{}]*)\{([^}]*)\}/g)]
    .filter(([, , body]) => /position:\s*sticky/.test(body!))
    .map(([, selector, body]) => [selector!.trim(), body!] as const);
  const columns = sticky.filter(([selector]) => !STICKY_BARS.includes(selector));
  // The columns this test was written against: it keeps finding them.
  expect(columns.map(([selector]) => selector)).toEqual(
    expect.arrayContaining([".tf-ftreebox", ".tf-flist", ".tf-rd-outline"]),
  );
  // A column whose head stays put scrolls in an inner box instead (#2257: the file tree's rows under its head).
  const INNER: Record<string, string> = { ".tf-ftreebox": ".tf-ftree", ".tf-rbv > .tf-rbrail": ".tf-pl-list", ".tf-dstick": ".tf-flist" };
  const scrolls = (selector: string, body: string) =>
    INNER[selector] === undefined
      ? /overflow(-y)?:\s*auto/.test(body)
      : // The column clips, or (the file tree's, 2026-11-20: a rounded clip around its scroller put every wheel's hit test on
        // the main thread) lets its inner box shrink to the room left (`min-height: 0`), which bounds it as well.
        (/overflow:\s*hidden/.test(body) || /min-height:\s*0/.test(block(code, INNER[selector]!))) &&
        /display:\s*flex/.test(body) &&
        /overflow(-y)?:\s*auto/.test(block(code, INNER[selector]!));
  const unbounded = columns
    .filter(([selector, body]) => !/max-height:\s*calc\(100vh - \d+px\)/.test(body) || !scrolls(selector, body))
    .map(([selector]) => selector);
  expect(unbounded).toEqual([]);
  // Below the open find bar the outline sticks lower, and is shorter by as much.
  const lower = block(code, ".tf-findbar ~ .tf-rd-sheet .tf-rd-outline");
  const by = (body: string) => Number(/max-height:\s*calc\(100vh - (\d+)px\)/.exec(body)?.[1]);
  const top = (body: string) => Number(/(?<![\w-])top:\s*(\d+)/.exec(body)?.[1]);
  const outline = block(code, ".tf-rd-outline");
  expect(by(lower) - by(outline)).toBe(top(lower) - top(outline));
});

test("the changed files list sticks: its box is as tall as the diffs beside it (#2091)", () => {
  // A sticky element moves only inside its parent, and a grid item kept to its content's height leaves it no room.
  expect(block(code, ".tf-dnav")).toMatch(/align-self:\s*stretch/);
});

test("an Inbox row has no content-visibility: the windowed list measures drawn rows, and a skipped row measures as its placeholder (#2209)", () => {
  expect(block(code, ".tf-ib")).not.toContain("content-visibility");
});

test("the Tickets page's row is bounded by the page, so a preview change does not re-lay out the list; stacked below 900px it is content-high (perf audit after #2200)", () => {
  expect(block(code, ".tf-tickets")).toMatch(/grid-template-rows:\s*minmax\(0, 1fr\);/);
  const narrow = code.slice(code.indexOf("@media (max-width: 900px) {\n  .tf-tickets"));
  expect(block(narrow, "  .tf-tickets")).toMatch(/grid-template-rows:\s*none;[^}]*height:\s*auto;/);
});

test("each role's colour reads its slot's variable in the right block, with knobas's own as the fallback (ADR-0051)", () => {
  const mockup = readFileSync(join(APP, "../mockups/code-area/base.css"), "utf8").replace(/\/\*[\s\S]*?\*\//g, "");
  /** Each `--hl-*` token a block sets, as written: light on `:root`, dark by the OS and when forced. */
  const blocks = (source: string) => {
    const media = source.slice(source.indexOf("@media (prefers-color-scheme: dark)"));
    const hl = (body: string) => Object.fromEntries([...body.matchAll(/(--hl-[a-z-]+):\s*([^;]+);/g)].map((m) => [m[1]!, m[2]!.trim()]));
    return {
      light: hl(block(source, ":root")),
      os: hl(block(media, ':root:not([data-theme="light"])')),
      forced: hl(block(source, ':root[data-theme="dark"]')),
    };
  };
  const ours = blocks(code);
  expect(Object.keys(ours.light).sort()).toEqual([...ROLES.map((role) => `--hl-${role}`), "--hl-com-style"].sort());
  expect(ours.os).toEqual(ours.forced);
  const read = (value: string, side: string, role: string) => {
    const m = new RegExp(`^var\\(--hl-${role}-${side}, (.+)\\)$`).exec(value);
    expect(m, `--hl-${role} reads --hl-${role}-${side}`).not.toBeNull();
    return m![1]!;
  };
  // The fallbacks are the mockup's six colours, spread over the seventeen roles: nothing drawn today changes colour
  // but what the new roles split off.
  const before = blocks(mockup);
  for (const [side, now, was] of [["light", ours.light, before.light], ["dark", ours.os, before.os]] as const) {
    const fallback = Object.fromEntries(ROLES.map((role) => [role, read(now[`--hl-${role}`]!, side, role)]));
    const old = (role: string) => was[`--hl-${role}`] ?? before.light[`--hl-${role}`];
    for (const role of ["kw", "str", "num", "com", "fn", "ty"]) expect(fallback[role]).toBe(old(role));
    for (const role of ["esc", "con", "attr"]) expect(fallback[role]).toBe(old("num"));
    for (const role of ["key", "tag"]) expect(fallback[role]).toBe(old("ty"));
    expect(fallback.head).toBe(old("kw"));
    expect(fallback.link).toBe(old("fn"));
    for (const role of ["prop", "var", "op", "punct"]) expect(fallback[role]).toBe("var(--ink)");
    expect(now["--hl-com-style"]).toBe(`var(--hl-com-style-${side}, italic)`);
  }
});

test("every role is a class that draws in its variable through --hl (ADR-0051)", () => {
  for (const role of ROLES) expect(block(code, `.tf-hl-${role}`)).toMatch(new RegExp(`--hl:\\s*var\\(--hl-${role}\\);\\s*color:\\s*var\\(--hl\\)`));
  expect(block(code, ".tf-hl-com")).toMatch(/font-style:\s*var\(--hl-com-style\)/);
});

/** `--control-*` → px, from the Controls section's token table (#2238). */
const CONTROL_TOKENS = [...spec.matchAll(/^\| `(--control-[a-z-]+)` \| (\d+) px \|/gm)].map((row) => [row[1]!, row[2]!] as const);

test("the control tokens are the Controls section's, on :root (#2280)", () => {
  expect(CONTROL_TOKENS.map(([token]) => token)).toEqual(["--control-h", "--control-h-sm", "--control-h-lg", "--control-gap"]);
  const root = block(code, ":root");
  expect(CONTROL_TOKENS.filter(([token, px]) => !root.includes(`${token}: ${px}px;`))).toEqual([]);
});

/**
 * Three sizes, each a fixed height from its token and horizontal padding only
 * (#2280), so neither line-height nor a `<kbd>` inside can make one button of
 * a row taller than its neighbour. Radius is the design system's 4px for every size.
 */
test.each([
  [".tf-btn", "--control-h"],
  [".tf-btn.tf-sm", "--control-h-sm"],
  [".tf-btn.tf-lg", "--control-h-lg"],
])("%s is var(%s) high with horizontal padding only (#2280)", (selector, token) => {
  const rule = block(code, selector);
  expect(rule).toMatch(new RegExp(`(^|[;{\\s])height:\\s*var\\(${token}\\);`));
  expect(rule).toMatch(/padding:\s*0 \d+px;/);
  expect(rule).not.toMatch(/min-height|line-height|padding-(top|bottom)/);
});

test("a button's radius is 4px, and no size rule sets its own (#2280)", () => {
  expect(block(code, ".tf-btn")).toMatch(/border-radius:\s*4px;/);
  // No size rule outside the three may set a radius of its own on a button.
  expect(code.match(/\.tf-btn\.tf-(sm|lg) \{[^}]*border-radius/g)).toBeNull();
});

test("one danger class: tf-danger-t is merged into tf-danger (#2280)", () => {
  expect(code).not.toMatch(/tf-danger-t\b/);
  expect(block(code, ".tf-btn.tf-danger")).toMatch(/color:\s*var\(--err\);/);
});

test("the link review's decide buttons take their size from .tf-lg, not a padding of their own (#2280)", () => {
  expect(code).not.toMatch(/\.tf-decide \.tf-btn \{/);
});

test("on the Tickets board and list, yellow is the chosen job's card and row, and selection is a two-pixel ink outline with the lift (spec #2396, #2398)", () => {
  // Every rule of the board and the list that paints with the card's yellow or floats: keyed on the chosen job's
  // `.tf-card`, never on the selection's `.tf-sel`; and the selection's rules, never on the yellow.
  const rules = [...code.matchAll(/([^{}]*)\{([^}]*)\}/g)]
    .map(([, selector, body]) => ({ selector: selector!.trim(), body: body! }))
    .filter(({ selector }) => /\.tf-(tcard|row)\b/.test(selector));
  const yellow = rules.filter(({ body }) => /var\(--card(-2)?\)/.test(body)).map(({ selector }) => selector);
  expect(yellow).toEqual([".tf-tcard.tf-card", ".tf .tf-tcard.tf-card:hover", ".tf-row.tf-card td"]);
  const lifted = rules.filter(({ body }) => body.includes("var(--shadow)")).map(({ selector }) => selector);
  expect(lifted).toEqual([".tf-tcard.tf-sel", ".tf-tcard.tf-card.tf-sel"]);

  // The outline is a ring, not `outline`, so the focus ring (`.tf :focus-visible`) never replaces it.
  expect(block(code, ".tf-tcard.tf-sel")).toMatch(/box-shadow:\s*0 0 0 2px var\(--ink\), var\(--shadow\);/);
  expect(block(code, ".tf-tcard.tf-card.tf-sel")).toMatch(/box-shadow:\s*0 0 0 2px var\(--card-ink\), 0 0 0 4px var\(--sel-halo\), var\(--shadow\);/);
  expect(block(code, ".tf-tcard.tf-card")).not.toContain("box-shadow");
  // A row's outline is drawn inside its cells, two pixels on each edge of the row; on the yellow row, in its ink.
  expect(block(code, ".tf-row.tf-sel td")).toMatch(/--tf-sel-ink:\s*var\(--ink\);[^}]*box-shadow:\s*inset 0 2px 0 var\(--tf-sel-ink\), inset 0 -2px 0 var\(--tf-sel-ink\);/);
  expect(block(code, ".tf-row.tf-sel td:first-child")).toMatch(/box-shadow:\s*inset 2px 0 0 var\(--tf-sel-ink\), inset 0 2px 0 var\(--tf-sel-ink\), inset 0 -2px 0 var\(--tf-sel-ink\);/);
  expect(block(code, ".tf-row.tf-sel td:last-child")).toMatch(/box-shadow:\s*inset -2px 0 0 var\(--tf-sel-ink\), inset 0 2px 0 var\(--tf-sel-ink\), inset 0 -2px 0 var\(--tf-sel-ink\);/);
  expect(block(code, ".tf-row.tf-card.tf-sel td")).toMatch(/--tf-sel-ink:\s*var\(--card-ink\);/);
});

test("every yellow card is a light surface in both themes: the page card's, the board card's and the list row's dots and chips take the light tone values, the blocked chip too (spec #2396 story 56, #2400)", () => {
  const TONES = ["--ok", "--ok-bg", "--warn", "--warn-bg", "--err", "--err-bg", "--info", "--info-bg", "--muted", "--muted-bg"];
  // The light values are the spec's colour table's, not a copy of the stylesheet.
  const light = new Map(COLOURS.map(([token, value]) => [token, value]));
  expect(TONES.every((token) => light.has(token)), "the spec's table names every tone").toBe(true);
  // The bodies of every rule whose selector list names `selector` exactly, so a grouped rule counts for each.
  const rulesNaming = (selector: string) =>
    [...code.matchAll(/([^{}]*)\{([^}]*)\}/g)]
      .filter(([, list]) => list!.split(",").some((one) => one.trim() === selector))
      .map(([, , body]) => body)
      .join("\n");
  // Each card's tone reads the card's own token (ADR-0055), whose value in both of knobas's themes is the light tone's.
  const wrong = [".tf-ticketcard.tf-card", ".tf-tcard.tf-card", ".tf-row.tf-card"].flatMap((selector) => {
    const card = rulesNaming(selector);
    return TONES.filter((token) => !card.includes(`${token}: var(--card-${token.slice(2)});`)).map((token) => `${selector} ${token}`);
  });
  expect(wrong).toEqual([]);
  const media = code.slice(code.indexOf("@media (prefers-color-scheme: dark)"));
  const sides = [["light", block(code, ":root")], ["dark", block(media, ':root:not([data-theme="light"])')], ["dark", block(code, ':root[data-theme="dark"]')]] as const;
  for (const [side, body] of sides)
    for (const token of TONES) {
      // Dark's Unknown dot on the card is a step darker than light's, so it is 3:1 on dark's card (ADR-0055, 3.2).
      const value = side === "dark" && token === "--muted" ? "#5D6773" : light.get(token)!;
      expect(body, `${side} --card-${token.slice(2)}`).toContain(`--card-${token.slice(2)}: var(--card-${token.slice(2)}-${side}, ${value});`);
    }
  // Every dot and chip draws from these tokens, so none needs a value of its own on the card.
  expect(block(code, ".tf-dot.tf-err")).toContain("var(--err)");
  expect(block(code, ".tf-jobcard .tf-chip.tf-err")).toContain("var(--err)");
  // The rule is the design system's, in its words.
  expect(spec).toContain("The yellow card is a light surface in both themes");
});

test("a description without markup keeps its line breaks in the reading column's panel (#2399)", () => {
  expect(block(code, ".tf-tkdesc p.tf-asks")).toMatch(/white-space:\s*pre-line;/);
});

test("the ticket page's columns take the card's width, and below 900px they stack (#2402)", () => {
  expect(block(code, ".tf-ticketpage .tf-ws")).toContain("grid-template-columns: var(--side-ticket-card, 320px) minmax(0, 1fr)");
  const narrow = code.slice(code.indexOf("@media (max-width: 900px) {\n  .tf-ticketpage {"));
  const body = narrow.slice(0, narrow.indexOf("\n}"));
  expect(body).toContain(".tf-ticketpage .tf-ws { grid-template-columns: minmax(0, 1fr); }");
});

test("an embed's frame has the border its height counts, inside a border-box (#2459)", () => {
  // TfDocEmbed adds this border to the reported document height; a border the
  // height does not count makes a fitting document scroll.
  const rule = block(code, ".tf-embed iframe");
  expect(rule).toMatch(new RegExp(`(^|[;{\\s])border: ${EMBED_FRAME_BORDER}px solid `));
  expect(rule).not.toMatch(/box-sizing|(^|[;{\s])(border-width|border-top|border-bottom|padding)\b/);
  expect(code).toMatch(/^\* \{ box-sizing: border-box;/m);
});

// The Windows pass on ux13 (2026-11-15): Builds' scroller keeps its bar's gutter (`scrollbar-gutter: stable`), and with
// it WebView2 judged the scroller not opaque and scrolled it on the main thread: its runs and a pipeline's history
// moved at 73 and 47 pictures a second under a wheel at 4x on Björn's PC, 26 and 27 at 6x, where ux12 held 229 and 222.
// Painting the clip inside it in the page's colour gives the compositor the scroll back (235 at 4x) and keeps ClearType.
test("a scroller that keeps its bar's gutter paints its clip in --paper, so WebView2 scrolls it on the compositor", () => {
  const code = css.replace(/\/\*[\s\S]*?\*\//g, "");
  const rules = [...code.matchAll(/([^{}]+)\{([^{}]*)\}/g)].map(([, selector, body]) => ({ selectors: selector!.split(",").map((s) => s.trim()), body: body! }));
  // `TfScroller`'s scrollers, whose body sits in a clip (`.tf-sclip`): Tickets' views rail keeps a gutter with no clip.
  const clips = (scroller: string) => rules.filter((rule) => rule.selectors.includes(`${scroller} > .tf-sclip`));
  const gutters = rules
    .filter((rule) => /scrollbar-gutter:\s*stable/.test(rule.body))
    .flatMap((rule) => rule.selectors)
    .filter((scroller) => clips(scroller).length > 0);
  expect(gutters, "Builds' scroller keeps a gutter, so this test checks something").toContain(".tf-shost > .tf-bmain");
  for (const scroller of gutters) {
    expect(clips(scroller).some((rule) => /background(-color)?:\s*var\(--paper\)/.test(rule.body)), `${scroller} > .tf-sclip`).toBe(true);
  }
});

// The Windows pass on ux16 (2026-11-17): every clip became positioned for its own paint layer (#2508), and Builds'
// painted clip under a kept gutter, positioned, made WebView2 hit-test each wheel event on the main thread
// (`FindScrollTargetOnMainThread` for 40 of 40): its runs and a pipeline's history moved at 71 and 81 pictures a
// second under a wheel at 4x on Björn's PC, 30 and 31 at 6x. Unpositioned, 0 of 40, and 236 and 237 at 4x again.
test("a scroller that keeps its bar's gutter keeps its painted clip unpositioned, so WebView2 hit-tests its wheel off the main thread", () => {
  const code = css.replace(/\/\*[\s\S]*?\*\//g, "");
  const rules = [...code.matchAll(/([^{}]+)\{([^{}]*)\}/g)].map(([, selector, body]) => ({ selectors: selector!.split(",").map((s) => s.trim()), body: body! }));
  const clips = (scroller: string) => rules.filter((rule) => rule.selectors.includes(`${scroller} > .tf-sclip`));
  const gutters = rules
    .filter((rule) => /scrollbar-gutter:\s*stable/.test(rule.body))
    .flatMap((rule) => rule.selectors)
    .filter((scroller) => clips(scroller).length > 0);
  expect(gutters, "Builds' scroller keeps a gutter, so this test checks something").toContain(".tf-shost > .tf-bmain");
  // The rule that positions every clip is less specific than a scroller's own, so the scroller's own decides.
  expect(rules.some((rule) => rule.selectors.includes(".tf-sclip") && /position:\s*relative/.test(rule.body)), "clips are positioned elsewhere").toBe(true);
  for (const scroller of gutters) {
    expect(clips(scroller).some((rule) => /(^|;)\s*position:\s*static\s*(;|$)/.test(rule.body)), `${scroller} > .tf-sclip`).toBe(true);
  }
});

/** A selector list split at its top-level commas, not those inside `:not(…)`. */
function topLevel(list: string): string[] {
  const out: string[] = [];
  let depth = 0;
  let start = 0;
  for (let i = 0; i < list.length; i += 1) {
    if (list[i] === "(") depth += 1;
    else if (list[i] === ")") depth -= 1;
    else if (list[i] === "," && depth === 0) {
      out.push(list.slice(start, i));
      start = i + 1;
    }
  }
  out.push(list.slice(start));
  return out;
}

/** Every rule of the sheet whose selector names `data-diff-unchanged`, as selector and body. */
function quietRules(): { selector: string; body: string }[] {
  const flat = code.replace(/@supports[^{]*\{([\s\S]*?\})\s*\}/g, "");
  return [...flat.matchAll(/([^{}]+)\{([^{}]*)\}/g)]
    .filter(([, selectors]) => selectors!.includes("data-diff-unchanged"))
    .flatMap(([, selectors, body]) => topLevel(selectors!).map((s) => ({ selector: s.trim(), body: body!.trim() })))
    .filter(({ selector }) => selector !== "");
}

test("a diff's unchanged lines draw quiet, grey or in full; changed lines and word marks never change (ADR-0051)", () => {
  const rules = quietRules();
  expect(rules.length).toBeGreaterThan(0);
  const doc = document.implementation.createHTMLDocument("");
  doc.body.innerHTML =
    '<div class="tf-diff"><div class="tf-ln" id="ctx"><span class="tf-hl-kw" id="ctx-kw">if</span></div>' +
    '<div class="tf-ln tf-add" id="add"><span class="tf-hl-kw" id="add-kw">if</span><span class="tf-wd"><span class="tf-hl-str" id="wd">x</span></span></div>' +
    '<div class="tf-ln tf-del" id="del"><span class="tf-hl-kw" id="del-kw">if</span></div></div>';
  const colourOf = (mode: string, id: string) => {
    doc.documentElement.setAttribute("data-diff-unchanged", mode);
    const el = doc.getElementById(id)!;
    const hit = rules.filter(({ selector }) => el.matches(selector));
    return hit.length === 0 ? "own" : /color:\s*([^;]+)/.exec(hit.at(-1)!.body)![1]!.trim();
  };
  expect(colourOf("quiet", "ctx-kw")).toBe("color-mix(in oklab, var(--hl) 40%, var(--ink-2))");
  expect(colourOf("grey", "ctx-kw")).toBe("var(--ink-2)");
  expect(colourOf("full", "ctx-kw")).toBe("own");
  expect(colourOf("quiet", "ctx")).toBe("var(--ink-2)");
  for (const mode of ["full", "quiet", "grey"])
    for (const id of ["add", "add-kw", "del", "del-kw", "wd"]) expect(colourOf(mode, id), `${mode} ${id}`).toBe("own");
  // The tints are the diff's own tokens, which a theme may set.
  expect(block(code, ".tf-diff .tf-add")).toMatch(/background:\s*var\(--diff-add\)/);
  expect(block(code, ".tf-diff .tf-del")).toMatch(/background:\s*var\(--diff-del\)/);
});


// ---------- every colour a token (ADR-0055) ----------

type CssNode = { type: string; property?: string; value?: string; prelude?: CssNode | string; name?: string; block?: CssNode | null; children?: CssNode[]; start: number; end: number };

/** Every stylesheet under `app/src` and every component's `<style>`, read as the style gate reads them (`style-cost.test.ts`). */
function everySheet(): { file: string; source: string; sheet: CssNode }[] {
  const root = join(APP, "src");
  const files = (readdirSync(root, { recursive: true }) as string[]).sort();
  const out: { file: string; source: string; sheet: CssNode }[] = [];
  for (const file of files) {
    if (file.endsWith(".css")) {
      const source = readFileSync(join(root, file), "utf8");
      out.push({ file, source, sheet: parseCss(source) as unknown as CssNode });
    } else if (file.endsWith(".svelte")) {
      const source = readFileSync(join(root, file), "utf8");
      if (!source.includes("<style")) continue;
      const sheet = (parse(source, { modern: true }) as unknown as { css: CssNode | null }).css;
      if (sheet) out.push({ file, source, sheet });
    }
  }
  return out;
}

/** Each declaration of a sheet with the selector of the rule it is in (an at-rule's prelude where it is in none). */
function declarations(source: string, sheet: CssNode): { selector: string; property: string; value: string }[] {
  const out: { selector: string; property: string; value: string }[] = [];
  const visit = (node: CssNode | null | undefined, selector: string) => {
    if (!node) return;
    let here = selector;
    if (node.type === "Rule" && typeof node.prelude === "object") here = source.slice(node.prelude.start, node.prelude.end).replace(/\s+/g, " ").trim();
    if (node.type === "Declaration") out.push({ selector, property: node.property ?? "", value: node.value ?? "" });
    for (const child of node.children ?? []) visit(child, here);
    if (node.block) visit(node.block, here);
  };
  visit(sheet, "");
  return out;
}

/** A colour written as a literal: hex, a colour function, or a named colour. */
const LITERAL = /#(?:[0-9a-f]{8}|[0-9a-f]{6}|[0-9a-f]{3,4})(?![0-9a-z_-])|\b(?:rgba?|hsla?|hwb|lab|lch|oklab|oklch|color)\(|(?<![-\w.#$])(?:white|black|red|green|blue|yellow|orange|purple|gray|grey|silver|navy|teal|pink|gold|magenta|cyan|brown)(?![-\w(])/i;
/** The three token blocks of `tokens.css`: the palette's own definitions. */
const TOKEN_BLOCKS = new Set([":root", ':root:not([data-theme="light"])', ':root[data-theme="dark"]']);

function literalColours(): string[] {
  const found: string[] = [];
  for (const { file, source, sheet } of everySheet())
    for (const { selector, property, value } of declarations(source, sheet)) {
      if (file === "lib/tf/css/tokens.css" && TOKEN_BLOCKS.has(selector)) continue;
      // A string or a `url()` is not a colour, whatever its letters spell.
      const bare = value.replace(/"[^"]*"|'[^']*'/g, '""').replace(/url\([^)]*\)/g, "url()");
      if (LITERAL.test(bare)) found.push(`${file} | ${selector} | ${property}: ${value}`);
    }
  return found;
}

const ALLOWED = readFileSync(join(APP, "src/lib/tf/theme/literal-colours.txt"), "utf8")
  .split("\n")
  .filter((line) => line.trim() !== "" && !line.startsWith("#"))
  .map((line) => {
    const [file, selector, reason] = line.split(" | ").map((part) => part.trim());
    return { file: file!, selector: selector!, reason: reason ?? "" };
  });

test("every colour the app draws is a token, or is on theme/literal-colours.txt with its reason (ADR-0055)", () => {
  const found = literalColours();
  expect(found.length, "the four kept literals are found, so the search reads the sheets").toBeGreaterThanOrEqual(4);
  const stray = found.filter((hit) => !ALLOWED.some(({ file, selector }) => hit.startsWith(`${file} | ${selector} | `)));
  expect(stray).toEqual([]);
  for (const { file, selector, reason } of ALLOWED) expect(reason.length, `${file} ${selector} says why`).toBeGreaterThan(20);
});

test("every line of theme/literal-colours.txt still matches a literal, so the list cannot outlive what it excuses", () => {
  const found = literalColours();
  const unused = ALLOWED.filter(({ file, selector }) => !found.some((hit) => hit.startsWith(`${file} | ${selector} | `)));
  expect(unused).toEqual([]);
});

test("the literal search catches a white background in chrome.css", () => {
  const source = ".tf-btn { background: #fff; }";
  const hits = declarations(source, parseCss(source) as unknown as CssNode).filter(({ value }) => LITERAL.test(value));
  expect(hits).toEqual([{ selector: ".tf-btn", property: "background", value: "#fff" }]);
  expect(LITERAL.test("var(--paper)")).toBe(false);
  expect(LITERAL.test("color-mix(in srgb, var(--card-shade) 12%, transparent)")).toBe(false);
  expect(LITERAL.test("rgba(0,0,0,.12)")).toBe(true);
});

/**
 * Text on a fill is `--on-fill`, on the primary button `--on-primary` (ADR-0055): a surface colour as text held only
 * while the fill was the ink, and a theme with a blue button on a dark panel would get dark words on blue.
 */
test("no rule writes text in a surface colour on what is selected or on the primary button", () => {
  const wrong: string[] = [];
  for (const { file, source, sheet } of everySheet()) {
    const rules = new Map<string, Record<string, string>>();
    for (const { selector, property, value } of declarations(source, sheet)) rules.set(selector, { ...rules.get(selector), [property]: value.trim() });
    for (const [selector, body] of rules) {
      const ground = body.background ?? body["background-color"] ?? "";
      if (/var\(--(sheet|paper|field)\)/.test(body.color ?? "") && /var\(--(fill|primary)\)/.test(ground)) wrong.push(`${file} ${selector}`);
    }
  }
  expect(wrong).toEqual([]);
  expect(block(code, '.tf-seg button[aria-pressed="true"]')).toMatch(/color: var\(--on-fill\)/);
  expect(block(code, ".tf-btn.tf-primary")).toMatch(/background: var\(--primary\); color: var\(--on-primary\)/);
});

/** No colour fades (ADR-0055, 3.16): a fading surface would draw half a theme for the length of the fade. */
test("no transition names a colour, but the site bar's key hint", () => {
  const COLOURED = /\b(all|color|background(-color)?|border(-[a-z]+)?-color|border|fill|stroke|outline-color)\b/;
  const wrong: string[] = [];
  for (const { file, source, sheet } of everySheet())
    for (const { selector, property, value } of declarations(source, sheet)) {
      if (property !== "transition" && property !== "transition-property") continue;
      if (value.trim() === "none") continue;
      if (file === "lib/tf/css/site.css" && selector === ".tf-sitebar kbd") continue;
      for (const part of value.split(",")) {
        const [name, duration] = part.trim().split(/\s+/);
        // A colour that changes at once after a delay (`0s .12s`, the side edge's hover) does not fade.
        if (COLOURED.test(name!) && !/^0m?s$/.test(duration ?? "")) wrong.push(`${file} ${selector}: ${part.trim()}`);
      }
    }
  expect(wrong).toEqual([]);
});
