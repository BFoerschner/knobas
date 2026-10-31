/**
 * What the ticket-first stylesheet has to keep (#809), checked against the
 * design system file rather than against a copy of itself: the colour and type
 * tables are read out of `docs/specs/2026-10-15-ticket-first-design-system.md`,
 * so a token that drifts from the spec fails here.
 */
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { brotliDecompressSync } from "node:zlib";

import { expect, test } from "vitest";

import { READER_DRAWER, READER_NARROW } from "./docs/docs";

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

/** `--token` → [light, dark], from the spec's colour table. */
const COLOURS: [string, string, string][] = [...spec.matchAll(/^\| `(--[a-z0-9-]+)`(?: \/ `(--[a-z0-9-]+)`)? \| `(#[0-9A-F]{6})`(?: \/ `(#[0-9A-F]{6})`)? \| `(#[0-9A-F]{6})`(?: \/ `(#[0-9A-F]{6})`)? \|/gm)].flatMap(
  (row) => {
    const pairs: [string, string, string][] = [[row[1]!, row[3]!, row[5]!]];
    if (row[2]) pairs.push([row[2], row[4]!, row[6]!]);
    return pairs;
  },
);

test("the spec's colour table was read, so the next two tests check something", () => {
  expect(COLOURS.length).toBeGreaterThanOrEqual(23);
});

test("light tokens are the spec's, on :root (#881)", () => {
  const light = block(code, ":root");
  const wrong = COLOURS.filter(([token, value]) => !light.includes(`${token}: ${value};`));
  expect(wrong).toEqual([]);
});

test("dark tokens are the spec's, by the OS unless forced light, and when forced dark", () => {
  const media = code.slice(code.indexOf("@media (prefers-color-scheme: dark)"));
  const byOs = block(media, ':root:not([data-theme="light"])');
  const forced = block(code, ':root[data-theme="dark"]');
  for (const dark of [byOs, forced]) {
    const wrong = COLOURS.filter(([token, , value]) => !dark.includes(`${token}: ${value};`));
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

test("width comes from font-variation-settings only, never font-stretch", () => {
  expect(code).not.toMatch(/font-stretch/);
  expect(block(code, ".tf-jobkey")).toMatch(/font-variation-settings: "wdth" 62/);
});

test("focus is 2px --focus, and --card-ink on a card surface", () => {
  expect(block(code, ".tf :focus-visible")).toMatch(/outline: 2px solid var\(--focus\)/);
  expect(code).toMatch(/\.tf \.tf-card:focus-visible, \.tf \.tf-card :focus-visible \{ outline-color: var\(--card-ink\); \}/);
});

test("reduced motion stops the pulse and every other animation in the shell", () => {
  expect(block(code, ".tf-dot.tf-run")).toMatch(/animation: tf-pulse 1\.4s/);
  const reduced = code.slice(code.indexOf("@media (prefers-reduced-motion: reduce)"));
  expect(reduced).toMatch(/\.tf \*[^{]*\{ transition: none !important; animation: none !important; \}/);
});

test("the three families are vendored from npm, Archivo from its width-axis file", () => {
  expect(code).toMatch(/@import "@fontsource-variable\/archivo\/wdth\.css";/);
  expect(code).toMatch(/@import "@fontsource-variable\/source-serif-4\//);
  expect(code).toMatch(/@import "@fontsource\/jetbrains-mono\//);
  expect(code).not.toMatch(/https?:/);
});

/**
 * The axis is in the font file, not synthesised: the latin Archivo the
 * stylesheet imports has an `fvar` axis record `wdth` whose minimum is 62.0.
 *
 * WOFF2 keeps its table data in one Brotli stream after a variable-length
 * table directory; an `fvar` axis record is the tag followed by min, default
 * and max as 16.16 fixed. The default-only file (`index.css`) carries `wght`
 * alone and fails this.
 */
test("the vendored Archivo carries a wdth axis from 62", () => {
  const face = readFileSync(join(APP, "node_modules/@fontsource-variable/archivo/wdth.css"), "utf8");
  const latin = face.match(/archivo-latin-wdth-normal \*\/[\s\S]*?url\(\.\/(files\/[^)]+)\)/);
  expect(latin, "no latin face in wdth.css").not.toBeNull();
  const font = readFileSync(join(APP, "node_modules/@fontsource-variable/archivo", latin![1]!));
  expect(font.subarray(0, 4).toString("latin1")).toBe("wOF2");
  expect(axes(font)).toContainEqual({ tag: "wdth", min: 62 });
});

function axes(woff2: Buffer): { tag: string; min: number }[] {
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
  const found: { tag: string; min: number }[] = [];
  for (let index = data.indexOf("wdth"); index !== -1; index = data.indexOf("wdth", index + 1)) {
    if (index + 16 <= data.length) found.push({ tag: "wdth", min: data.readInt32BE(index + 4) / 65536 });
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
  expect(band).toMatch(/\.tf-nav \.tf-count\s*\{[^}]*display:\s*none/);
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
  for (const source of [...LAUNCHER, readFileSync(join(APP, "src/lib/shell/StateChip.svelte"), "utf8")]) {
    expect(source).not.toMatch(/<style\b/);
    expect(source).not.toMatch(/\btf-mono\b|--mono\b/);
  }
});

test("the launcher sets mono on a result row's key and path and nowhere else (#1422)", () => {
  const mono = launcherRules.filter(([, body]) => /var\(--mono\)/.test(body)).map(([selector]) => selector);
  expect(mono).toEqual([".tf-ln-k", ".tf-ln-pa"]);
  // Both spans are drawn by the row, and only there.
  const row = LAUNCHER[6]!;
  expect(row).toMatch(/<span class="tf-ln-k">\{key\}<\/span>/);
  expect(row).toMatch(/<span class="tf-ln-pa">\{path\}<\/span>/);
  expect(LAUNCHER.slice(0, 6).join("\n")).not.toMatch(/tf-ln-(k|pa)\b/);
});

test("the launcher is prototype-6's dialog: an ink selection bar, a borderless 17px box, no capitals and no rooms tokens (#1422)", () => {
  const selected = launcherRules.find(([selector]) => selector.includes(".tf-ln-res.tf-ln-on"))?.[1] ?? "";
  expect(selected).toMatch(/background:\s*var\(--sheet-2\)/);
  expect(selected).toMatch(/box-shadow:\s*inset 3px 0 0 var\(--ink\)/);
  const box = launcherRules.find(([selector]) => selector === ".tf .tf-ln-q input")?.[1] ?? "";
  expect(box).toMatch(/font-size:\s*var\(--t-xl\)/);
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
 * has to sit in the same rule, after the shorthand.
 */
test("code, diff, editor and the mono token class draw without ligatures", () => {
  for (const selector of [".tf-code", ".tf-diff", ".tf-editor", ".tf .tf-mono"]) {
    expect(block(code, selector), selector).toMatch(/font-variant-ligatures: none/);
  }
});

test("every rule that takes the mono font variable turns ligatures off after its font declaration", () => {
  const rules = [...code.matchAll(/([^{}]+)\{([^{}]*)\}/g)].filter((m) => m[2]!.includes("var(--mono)"));
  expect(rules.length).toBeGreaterThan(30);
  const bare = rules
    .filter((m) => {
      const body = m[2]!;
      const lastFont = Math.max(body.lastIndexOf("font:"), body.lastIndexOf("font-family:"));
      return body.lastIndexOf("font-variant-ligatures: none") < lastFont;
    })
    .map((m) => m[1]!.trim());
  expect(bare).toEqual([]);
});

test("a Find match in a broken Dependencies tile is --ink on --mark; elsewhere it inherits (#1462)", () => {
  const rule = block(code, ".tf-dp-t.tf-dp-err mark");
  expect(rule).toMatch(/color:\s*var\(--ink\)/);
  expect(rule).not.toMatch(/background/);
  const base = block(code, ".tf mark");
  expect(base).toMatch(/background:\s*var\(--mark\)/);
  expect(base).toMatch(/color:\s*inherit/);
});

test("the asset tree page is viewport-high, so its pane scrolls and the page does not (#1460)", () => {
  expect(block(code, ".tf-full:has(> .tf-asset-tree)")).toMatch(/(^|[{;\s])height:\s*100%/);
  expect(block(code, ".tf-asset-tree")).toMatch(/min-height:\s*0/);
});

test("the Docs reader: tree and page, the outline the page's sticky right column, folded on the component's own threshold (#2081)", () => {
  expect(block(code, ".tf-rd")).toMatch(/grid-template-columns:\s*300px minmax\(0, 1fr\);/);
  expect(block(code, ".tf-rd-sheet")).toMatch(/grid-template-columns:\s*minmax\(0, 1fr\) 268px;/);
  const outline = block(code, ".tf-rd-outline");
  expect(outline).toMatch(/position:\s*sticky/);
  expect(outline).toMatch(/top:\s*0/);
  // The band the component reads through `matchMedia` is the stylesheet's own.
  const at = code.indexOf(`@media ${READER_NARROW} {`);
  expect(at, `no rule band for ${READER_NARROW}`).toBeGreaterThan(-1);
  const band = code.slice(at, code.indexOf("\n}", at));
  expect(band).toMatch(/\.tf-rd\s*\{[^}]*grid-template-columns:\s*220px minmax\(0, 1fr\)/);
  expect(band).toMatch(/\.tf-rd-sheet\s*\{[^}]*grid-template-columns:\s*minmax\(0, 1fr\)\s*;/);
});

test("nothing in the Docs reader's page is capped in width (#2081)", () => {
  const capped = [...code.matchAll(/([^{}]*)\{([^}]*)\}/g)]
    .filter(([, selector]) => /\.tf-(rd-page|rd-sheet|doc|cmts|sec-edit|compose|pre|callout|macro|rd-foot)\b/.test(selector!))
    .filter(([, , body]) => /max-width/.test(body!))
    .map(([, selector]) => selector!.trim());
  expect(capped).toEqual([]);
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
const STICKY_BARS = [".tf-fbar", ".tf-findbar", ".tf-ln-h", ".tf-wtabs", ".tf-logbar", ".tf-pdf-bar"];

test("every sticky side column is bounded by the window and scrolls on its own, so its last entry is reachable (#2091)", () => {
  const sticky = [...code.matchAll(/([^{}]*)\{([^}]*)\}/g)]
    .filter(([, , body]) => /position:\s*sticky/.test(body!))
    .map(([, selector, body]) => [selector!.trim(), body!] as const);
  const columns = sticky.filter(([selector]) => !STICKY_BARS.includes(selector));
  // The columns this test was written against: it keeps finding them.
  expect(columns.map(([selector]) => selector)).toEqual(
    expect.arrayContaining([".tf-ftreebox", ".tf-flist", ".tf-rd-outline", ".tf-dp-rail", ".tf-dp-panel"]),
  );
  // A column whose head stays put scrolls in an inner box instead (#2257: the file tree's rows under its head).
  const INNER: Record<string, string> = { ".tf-ftreebox": ".tf-ftree" };
  const scrolls = (selector: string, body: string) =>
    INNER[selector] === undefined
      ? /overflow(-y)?:\s*auto/.test(body)
      : /overflow:\s*hidden/.test(body) && /display:\s*flex/.test(body) && /overflow(-y)?:\s*auto/.test(block(code, INNER[selector]!));
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

test("the highlighting themes are the Code area mockup's --hl-* tokens, light on :root and dark in both dark blocks (#2254)", () => {
  const mockup = readFileSync(join(APP, "../mockups/code-area/base.css"), "utf8").replace(/\/\*[\s\S]*?\*\//g, "");
  /** Each `--hl-*` token a theme sets: light on `:root`, dark by the OS and when forced. */
  const themes = (source: string) => {
    const media = source.slice(source.indexOf("@media (prefers-color-scheme: dark)"));
    const hl = (body: string) => Object.fromEntries([...body.matchAll(/(--hl-[a-z]+):\s*([^;]+);/g)].map((m) => [m[1]!, m[2]!.trim()]));
    return {
      light: hl(block(source, ":root")),
      os: hl(block(media, ':root:not([data-theme="light"])')),
      forced: hl(block(source, ':root[data-theme="dark"]')),
    };
  };
  const ours = themes(code);
  expect(Object.keys(ours.light).sort()).toEqual(["--hl-com", "--hl-fn", "--hl-kw", "--hl-num", "--hl-str", "--hl-ty"]);
  expect(ours).toEqual(themes(mockup));
  // The two literal colours differ by theme; the rest follow --ok, --warn, --muted and --info.
  expect(ours.os["--hl-kw"]).not.toBe(ours.light["--hl-kw"]);
  expect(ours.os["--hl-ty"]).not.toBe(ours.light["--hl-ty"]);
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
