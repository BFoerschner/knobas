/**
 * The style gate (#2281): the design system's Controls section, enforced.
 *
 * The scan reads every stylesheet and component under `app/src/` off disk,
 * as the house rules do, and fails when
 *
 * - (a) a rule outside the controls stylesheet (`lib/tf/css/chrome.css`) sets
 *   a button's `height`, `min-height`, `padding`, `font-size` or
 *   `border-radius` (a `font` shorthand counts, it sets the size), or a disabled control's
 *   `opacity` or `cursor` (the disabled look is one rule in that stylesheet);
 * - (b) the unported button classes (`btn`, `tb-btn`, the old `x`) appear in a
 *   stylesheet or a component;
 * - (c) a row of buttons (an actions container: `.tf-acts`, a local `…acts` row or the bare `.acts`,
 *   `.tf-trail`, a dialog footer; or a row that holds controls under another name: the filter bar
 *   `.tf-fbar`, the pill toggles `.tf-fchips`, the large decision `.tf-decide`, a `…-btns` row, a dialog's
 *   title-line cluster `.tf-dctl`) has a
 *   `gap` other than `var(--control-gap)`;
 * - (f) a field in a row of buttons (the filter bar's `.tf-fsearch`, or a `select` or `input` in a
 *   row of buttons or a `.tf-fsel`) is sized by vertical padding or a literal height: it takes the
 *   row's height from `var(--control-h…)`;
 * - (g) an actions container other than the page header's (`.tf-fhead`, `.tf-stg-h`) is pushed to the
 *   end and laid out as a flex row by hand (`margin-left: auto` with `display: flex`): a strip's or a
 *   row's trailing group is `.tf-trail`;
 * - (h) a `font-size` (or a `font` shorthand's size) is not a `--t-*` token, apart from the reading face and code, which
 *   keep their own sizes (a rule that sets `--serif` or `--mono`, the `.tf-rd-title`, and the PDF's text layer);
 * - (i) a pill toggle is drawn outside `.tf-fchips`: a class round enough to be a pill (`border-radius` of 9 px or more)
 *   that also answers `aria-pressed`;
 * - (j) a grid's label column is a literal `132px`: it is `var(--tf-label)`;
 * - (k) `*` is the subject of a rule that sets a width, a height, `overflow` or `position`: a shared layout rule names
 *   what it lays out;
 * - (l) `role="menu"` appears outside `TfMenu.svelte`: a menu is `TfMenu`;
 * - (m) a component draws the missing line by hand inside a `TfDialog` (it is `TfDialog`'s `why`), outside one in any
 *   markup but `<p class="tf-dwhy" role="status">`, or says *(needed)* (a needed field is a `.tf-req` *required*);
 * - (n) a `TfDialog`'s footer has a primary disabled for more than a send in flight, and the dialog passes no `why`;
 * - (o) a box that narrows or finds (a placeholder or label starting *Filter* or *Find*) has no `filterKeys`: `Esc` means one
 *   thing in all of them (the key table);
 * - (p) a rule hides a row's value on `:focus-within`: focus on a row's link reveals nothing, and only `row-room.ts` knows
 *   what the group covers;
 * - (d) an allowlist entry has no reason;
 * - (e) an allowlist entry matches nothing.
 *
 * The allowlist is `style-gate-exceptions/`, one `<ticket>.txt` per cleanup
 * ticket, so tickets that run side by side each delete their own file and
 * never touch another's. Each line is
 *
 *     <rule> <file> <selector or class>  <reason>
 *
 * with `<file>` relative to `app/src/`, two spaces before the reason, and `#`
 * starting a comment, as in `test-layout-exceptions.txt`.
 */
import { existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, relative } from "node:path";

import { expect, test } from "vitest";

/** `app/src/`, off `cwd` as in the house rules: Vitest's root is `app/`. */
const ROOT = join(process.cwd(), "src");
const ALLOWLIST = join(ROOT, "lib/tf/controls/style-gate-exceptions");
/** The one stylesheet that sizes controls. */
const CONTROLS_SHEET = "lib/tf/css/chrome.css";
const SECTION = "docs/specs/2026-10-15-ticket-first-design-system.md, section Controls";

type Rule = "a" | "b" | "c" | "f" | "g" | "h" | "i" | "j" | "k" | "l" | "m" | "n" | "o" | "p";
/** One breach: the rule, the file under `app/src/`, and the selector (a, c) or class (b) it is keyed on. */
type Violation = { rule: Rule; file: string; key: string; detail: string };
type Entry = { list: string; line: number; rule: string; file: string; key: string; reason: string };

/**
 * Classes that name a control (rule a). `tf-refresh` is the shared Refresh button
 * (`TfDetailRefresh`), always a `.tf-btn`; views resized it under its own name.
 * `tf-chipbtn` is a chip's own button, drawn as the chip's text (spec #2463).
 */
const CONTROL = /\.(?:tf-btn|tf-sm|tf-lg|tf-iconbtn|tf-chipbtn|tf-refresh)(?![\w-])/;
/** The segmented control and the pill toggles: their `button`s are controls. */
const BUTTON_GROUP = /\.(?:tf-seg|tf-fchips)(?![\w-])/;
const SIZING = /^(?:height|min-height|padding(?:-[a-z-]+)?|font-size|font|border(?:-[a-z]+)*-radius)$/;
/** A disabled control's subject, and the properties that draw it disabled (rule a). */
const DISABLED = /\[disabled\]|:disabled|\[aria-disabled/;
const DISABLED_LOOK = /^(?:opacity|cursor)$/;
/** The unported button classes (rule b). */
const UNPORTED = ["btn", "tb-btn", "x"];
/**
 * Actions containers (rule c): `.tf-acts` and every local `…acts` row (`.tf-stg-acts`,
 * `.tf-wq-acts`, and `.tf-appacts`, `.tf-logacts` without the hyphen, #2343), a strip's
 * `.tf-trail`, the unported `.dlg-f`, and a dialog's `footer`. A name ending in `facts`
 * (`.tf-facts`, `.artifacts`) is not one. The bare `.acts` of the unported styles and the
 * boot screen is one too (#2292).
 */
const ACTIONS_CLASS = /\.(?:(?:[\w-]+(?<![fF]))?acts|tf-trail|dlg-f)(?![\w-])/;
const DIALOG = /\.(?:[\w-]*dialog[\w-]*|tf-steps)(?![\w-])/;
/**
 * Rows of controls not named for actions (rule c, review of #2238): the filter bar, the pill
 * toggles, the link review's large decision, a `…-btns` row (the view editor's `.tf-vw-btns`) and a
 * dialog's title-line cluster of actions and Close (`.tf-dctl`, #2415).
 */
const BUTTON_ROW = /\.(?:tf-fbar|tf-fchips|tf-decide|tf-dctl|[\w-]+-btns)(?![\w-])/;

/** The rooms UI's sheet, which keeps its own type until it goes (rule h does not read it). */
const OLD_SHEET = "unported.css";
/** What keeps its own sizes (rule h): the reading face's title, the PDF's text layer, and a box measured off-screen. */
const OWN_TYPE = /\.(?:tf-rd-title|tf-pdf-text|tf-diagram-scratch)(?![\w-])/;

/** The size a rule sets that is not on the type scale (rule h), or `null`. */
function offScale(selector: string, decls: [string, string][]): string | null {
  const face = decls.some(([prop, value]) => /^font(?:-family)?$/.test(prop) && /var\(--(?:serif|mono)\)/.test(value));
  if (face || OWN_TYPE.test(selector)) return null;
  for (const [prop, value] of decls) {
    if (prop === "font-size" && !/^(?:var\(--t-|inherit$)/.test(value)) return `font-size: ${value}`;
    if (prop === "font" && !/var\(--t-|^inherit$/.test(value) && /\d/.test(value)) return `font: ${value}`;
  }
  return null;
}

/** The classes on a selector's subject. */
function subjectClasses(selector: string): string[] {
  return [...(compounds(selector).at(-1) ?? "").matchAll(/\.([\w-]+)/g)].map((m) => m[1]!);
}

/** Is the rule's radius a pill's? */
function roundRadius(decls: [string, string][]): boolean {
  return decls.some(([prop, value]) => prop === "border-radius" && /^(?:9|[1-9]\d+)px$/.test(value));
}

/** Every `.css` and `.svelte` file under `root`, relative, sorted. */
function files(root: string): string[] {
  const found: string[] = [];
  const walk = (dir: string) => {
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      const path = join(dir, entry.name);
      if (entry.isDirectory()) walk(path);
      else if (/\.(css|svelte)$/.test(entry.name)) found.push(relative(root, path));
    }
  };
  walk(root);
  return found.sort();
}

/** Split on `sep` outside parentheses and brackets. */
function splitTop(text: string, sep: RegExp): string[] {
  const parts: string[] = [];
  let depth = 0;
  let start = 0;
  for (let i = 0; i < text.length; i++) {
    const ch = text[i]!;
    if (ch === "(" || ch === "[") depth++;
    else if (ch === ")" || ch === "]") depth--;
    else if (depth === 0 && sep.test(ch)) {
      parts.push(text.slice(start, i));
      start = i + 1;
    }
  }
  parts.push(text.slice(start));
  return parts;
}

/** A selector's compounds, combinators dropped. */
function compounds(selector: string): string[] {
  return splitTop(selector.replace(/\s*([>+~])\s*/g, " "), /\s/).filter((part) => part !== "");
}

/** Every style rule in `css` as [selectors, declarations]; at-rule groups are entered, comments dropped. */
function rules(css: string): { selectors: string[]; decls: [string, string][] }[] {
  const text = css.replace(/\/\*[\s\S]*?\*\//g, "");
  const found: { selectors: string[]; decls: [string, string][] }[] = [];
  let start = 0;
  for (let i = 0; i < text.length; i++) {
    const ch = text[i];
    if (ch === ";" || ch === "}") start = i + 1;
    else if (ch === "{") {
      const prelude = text.slice(start, i).trim();
      start = i + 1;
      if (prelude.startsWith("@")) {
        // A group (`@media`, `@supports`) holds rules; `@keyframes` and `@font-face` hold no controls.
        if (/^@(keyframes|font-face)/.test(prelude)) i = text.indexOf("}", text.indexOf("}", i) + 1) - 1;
        continue;
      }
      const end = text.indexOf("}", i);
      const decls = text
        .slice(i + 1, end)
        .split(";")
        .map((decl) => decl.split(":"))
        .filter((pair) => pair.length >= 2)
        .map(([prop, ...value]) => [prop!.trim().toLowerCase(), value.join(":").trim()] as [string, string]);
      const selectors = splitTop(prelude, /,/).map((s) => s.trim().replace(/\s+/g, " ")).filter((s) => s !== "");
      found.push({ selectors, decls });
      i = end;
      start = end + 1;
    }
  }
  return found;
}

/** Does the rule's subject (its last compound) draw a control? */
function targetsControl(selector: string): boolean {
  const parts = compounds(selector);
  const subject = parts.at(-1) ?? "";
  if (CONTROL.test(subject)) return true;
  return /^button(?![\w-])/.test(subject) && parts.slice(0, -1).some((part) => BUTTON_GROUP.test(part));
}

/** Is the rule's subject an actions container? */
function isActions(selector: string): boolean {
  const parts = compounds(selector);
  const subject = parts.at(-1) ?? "";
  if (ACTIONS_CLASS.test(subject)) return true;
  return /^footer(?![\w-])/.test(subject) && parts.slice(0, -1).some((part) => DIALOG.test(part));
}

/** Is the rule's subject a field in a row of buttons: the filter bar's search, or a `select`/`input` inside such a row or a `.tf-fsel`? */
function isRowField(selector: string): boolean {
  const parts = compounds(selector);
  const subject = parts.at(-1) ?? "";
  if (/\.tf-fsearch(?![\w-])/.test(subject)) return true;
  return (
    /^(?:select|input)(?![\w-])/.test(subject) &&
    parts.slice(0, -1).some((part) => ACTIONS_CLASS.test(part) || BUTTON_ROW.test(part) || /\.tf-fsel(?![\w-])/.test(part))
  );
}

/** What sizes a field other than its token: a literal height, or vertical padding. */
function fieldSizing(decls: [string, string][]): string[] {
  const zero = (v: string | undefined) => v === undefined || /^0(px)?$/.test(v);
  return decls.flatMap(([prop, value]) => {
    if (/^(?:min-)?height$/.test(prop)) return value.startsWith("var(--control-h") ? [] : [`${prop}: ${value}`];
    if (/^padding-(?:top|bottom|block(?:-start|-end)?)$/.test(prop)) return zero(value.split(/\s+/)[0]) ? [] : [`${prop}: ${value}`];
    if (prop === "padding") {
      const parts = value.split(/\s+/);
      return zero(parts[0]) && zero(parts[2] ?? parts[0]) ? [] : [`${prop}: ${value}`];
    }
    return [];
  });
}

/** The page headers, whose `.tf-acts` is the canonical header's and not a trailing group. */
const PAGE_HEADER = /\.(?:tf-fhead|tf-stg-h)(?![\w-])/;

/** Does the rule rebuild `.tf-trail`: an actions container outside a page header, pushed to the end as a flex row? */
function rebuildsTrail(selector: string, decls: [string, string][]): boolean {
  const parts = compounds(selector);
  if (!isActions(selector) || /\.tf-trail(?![\w-])/.test(parts.at(-1) ?? "")) return false;
  if (parts.slice(0, -1).some((part) => PAGE_HEADER.test(part))) return false;
  const has = (prop: string, value: RegExp) => decls.some(([p, v]) => p === prop && value.test(v));
  return has("margin-left", /^auto$/) && has("display", /^(?:inline-)?flex$/);
}

/** Is the rule's subject a row of buttons: an actions container, or a row of controls under another name? */
function isButtonRow(selector: string): boolean {
  return isActions(selector) || BUTTON_ROW.test(compounds(selector).at(-1) ?? "");
}

/** The stylesheet text of `file`: the file itself, or a component's `<style>` blocks. */
function stylesheet(file: string, text: string): string {
  if (file.endsWith(".css")) return text;
  return [...text.matchAll(/<style[^>]*>([\s\S]*?)<\/style>/g)].map((m) => m[1]).join("\n");
}

/** `expr` with every template literal's `${…}` holes blanked, so the literal's fixed text reads as words. */
function holesBlanked(expr: string): string {
  let prev: string;
  do {
    prev = expr;
    expr = expr.replace(/\$\{[^{}]*\}/g, " ");
  } while (expr !== prev);
  return expr;
}

/** The expression in each `class={…}`, up to its balancing brace (a template literal's `${…}` nests). */
function classExpressions(markup: string): string[] {
  return [...markup.matchAll(/\bclass=\{/g)].map((m) => {
    const start = m.index + m[0].length;
    let depth = 1;
    let i = start;
    for (; i < markup.length && depth > 0; i++) {
      if (markup[i] === "{") depth++;
      else if (markup[i] === "}") depth--;
    }
    return markup.slice(start, i - 1);
  });
}

/** Class names a component's markup sets: `class="…"`, `class={…}` string and template literals, `class:name`. */
function markupClasses(text: string): Set<string> {
  const markup = text.replace(/<(script|style)[^>]*>[\s\S]*?<\/\1>/g, "");
  const names = new Set<string>();
  const add = (words: string) => words.split(/\s+/).forEach((word) => word && names.add(word));
  for (const [, value] of markup.matchAll(/\bclass="([^"]*)"/g)) {
    add(value!.replace(/\{[^}]*\}/g, (expr) => [...expr.matchAll(/(["'`])([^"'`]*)\1/g)].map((m) => m[2]).join(" ")));
  }
  for (const expr of classExpressions(markup)) {
    for (const [, , literal] of holesBlanked(expr).matchAll(/(["'`])([^"'`]*)\1/g)) add(literal!);
  }
  for (const [, name] of markup.matchAll(/\bclass:([\w-]+)/g)) names.add(name!);
  return names;
}

/** The one component that may draw a `role="menu"` (rule l). */
const MENU_COMPONENT = "lib/tf/TfMenu.svelte";
/** The one component that draws a dialog's missing line (rule m). */
const DIALOG_COMPONENT = "lib/tf/settings/TfDialog.svelte";
/** What a shared layout rule may not set on `*` (rule k). */
const LAYOUT = /^(?:(?:min-|max-)?(?:width|height)|overflow(?:-[xy])?|position)$/;

/** Where the tag at `start` ends: its first `>` outside any `{…}`, so an arrow function in an attribute doesn't end it. */
function tagEnd(markup: string, start: number): number {
  let depth = 0;
  for (let i = start; i < markup.length; i++) {
    if (markup[i] === "{") depth++;
    else if (markup[i] === "}") depth--;
    else if (markup[i] === ">" && depth === 0) return i;
  }
  return markup.length;
}

/** What may disable a dialog's primary with no line: a send in flight, which its label or the dialog already shows. */
const IN_FLIGHT = /^!?(?:(?:queue\.)?(?:busy|flushing)|sending|writing|saving|deleting|publishing|signingIn|testing)$/;

/** The `disabled={…}` of each primary in `block`, as written. */
function disabledPrimaries(block: string): string[] {
  return [...block.matchAll(/<button\b/g)].flatMap((m) => {
    const tag = block.slice(m.index, tagEnd(block, m.index));
    if (!/\btf-primary\b/.test(tag) || /\bclass:tf-primary\b/.test(tag)) return [];
    const at = tag.search(/(?<![\w-])disabled=\{/);
    if (at === -1) return [];
    let depth = 0;
    for (let i = at + "disabled=".length; i < tag.length; i++) {
      if (tag[i] === "{") depth++;
      else if (tag[i] === "}" && --depth === 0) return [tag.slice(at + "disabled={".length, i)];
    }
    return [];
  });
}

/** Rules (m) and (n): the missing line is `TfDialog`'s, and a disabled primary has one. */
function missingLine(file: string, markup: string, add: (v: Violation) => void): void {
  if (/\(needed\)/.test(markup)) add({ rule: "m", file, key: "(needed)", detail: "says (needed); a needed field is a `.tf-req` *required* in its label" });
  const blocks = [...markup.matchAll(/<TfDialog\b[\s\S]*?<\/TfDialog>/g)].map((m) => m[0]);
  for (const block of blocks) {
    if (/\btf-dwhy\b/.test(block)) add({ rule: "m", file, key: "tf-dwhy in a dialog", detail: "draws the missing line by hand; it is `TfDialog`'s `why`" });
    const passes = /\bwhy=/.test(block.slice(0, tagEnd(block, 0)));
    for (const expr of disabledPrimaries(block)) {
      const inFlight = expr.split("||").every((part) => IN_FLIGHT.test(part.trim()));
      if (!inFlight && !passes) add({ rule: "n", file, key: expr, detail: `a primary is disabled={${expr}} and its dialog passes no \`why\`` });
    }
  }
  const outside = blocks.reduce((rest, block) => rest.replace(block, ""), markup);
  for (const [tag] of outside.matchAll(/<[a-z]+\b[^>]*\btf-dwhy\b[^>]*>/g)) {
    if (tag !== '<p class="tf-dwhy" role="status">') add({ rule: "m", file, key: tag, detail: 'the missing line has one markup, `<p class="tf-dwhy" role="status">`' });
  }
}

/** Every breach of rules (a), (b) and (c) under `root`, one per (rule, file, key). */
function scan(root: string): Violation[] {
  const found = new Map<string, Violation>();
  const add = (v: Violation) => found.set(`${v.rule} ${v.file} ${v.key}`, found.get(`${v.rule} ${v.file} ${v.key}`) ?? v);
  for (const file of files(root)) {
    const text = readFileSync(join(root, file), "utf8");
    const sheet = rules(stylesheet(file, text));
    const pressable = new Set(sheet.flatMap(({ selectors }) => selectors.filter((s) => /\[aria-pressed/.test(compounds(s).at(-1) ?? "")).flatMap(subjectClasses)));
    for (const { selectors, decls } of sheet) {
      for (const selector of selectors) {
        const size = file === OLD_SHEET ? null : offScale(selector, decls);
        if (size !== null) add({ rule: "h", file, key: selector, detail: `sets ${size}, which is not on the type scale; use a \`--t-*\` token` });
        if (roundRadius(decls) && subjectClasses(selector).some((name) => pressable.has(name))) {
          add({ rule: "i", file, key: selector, detail: "draws a pill toggle outside `.tf-fchips`; a pill is a `button` in `.tf-fchips`" });
        }
        if (decls.some(([prop, value]) => prop === "grid-template-columns" && /(?<![\d.])132px/.test(value))) {
          add({ rule: "j", file, key: selector, detail: "sets a literal 132px label column; use `var(--tf-label)`" });
        }
        if (/:focus-within/.test(selector) && /\.tf-rowval/.test(selector)) {
          add({ rule: "p", file, key: selector, detail: "hides a row's value on focus; `row-room.ts` measures what the group covers, and chrome.css hides `.tf-covered`" });
        }
        const laidOut = decls.filter(([prop]) => LAYOUT.test(prop)).map(([prop]) => prop);
        if (file !== OLD_SHEET && laidOut.length > 0 && /^\*(?![\w-])/.test(compounds(selector).at(-1) ?? "")) {
          add({ rule: "k", file, key: selector, detail: `sets ${laidOut.join(", ")} on \`*\`; a shared layout rule names what it lays out` });
        }
        const disabled = DISABLED.test(compounds(selector).at(-1) ?? "");
        const sizing = decls.filter(([prop]) => SIZING.test(prop) || (disabled && DISABLED_LOOK.test(prop))).map(([prop]) => prop);
        if (file !== CONTROLS_SHEET && sizing.length > 0 && targetsControl(selector)) {
          add({ rule: "a", file, key: selector, detail: `sets ${sizing.join(", ")} on a control outside ${CONTROLS_SHEET}` });
        }
        for (const name of UNPORTED) {
          if (new RegExp(`(?<![\\w-])\\.${name}(?![\\w-])`).test(selector)) {
            add({ rule: "b", file, key: name, detail: `styles the unported button class .${name}` });
          }
        }
        if (rebuildsTrail(selector, decls)) {
          add({ rule: "g", file, key: selector, detail: "rebuilds the trailing group of actions; use `.tf-trail`" });
        }
        const fieldSize = fieldSizing(decls);
        if (fieldSize.length > 0 && isRowField(selector)) {
          add({
            rule: "f",
            file,
            key: selector,
            detail: `sizes a field in a row of buttons by ${fieldSize.join(", ")}; it takes the row's height, \`height: var(--control-h…)\``,
          });
        }
        const gaps = decls.filter(([prop, value]) => /^(?:gap|column-gap)$/.test(prop) && value !== "var(--control-gap)");
        if (gaps.length > 0 && isButtonRow(selector)) {
          add({ rule: "c", file, key: selector, detail: `sets ${gaps.map(([p, v]) => `${p}: ${v}`).join(", ")} on a row of buttons` });
        }
      }
    }
    if (file.endsWith(".svelte")) {
      const used = markupClasses(text);
      for (const name of UNPORTED.filter((n) => used.has(n))) {
        add({ rule: "b", file, key: name, detail: `uses the unported button class ${name}` });
      }
      const markup = text.replace(/<(script|style)[^>]*>[\s\S]*?<\/\1>/g, "").replace(/<!--[\s\S]*?-->/g, "");
      if (file !== MENU_COMPONENT && /\brole=["']menu["']/.test(markup)) {
        add({ rule: "l", file, key: 'role="menu"', detail: "draws a menu by hand; a menu is `TfMenu`, which draws in the float host" });
      }
      if (file !== DIALOG_COMPONENT) missingLine(file, markup, add);
      for (const box of markup.matchAll(/<input\b/g)) {
        const tag = markup.slice(box.index, tagEnd(markup, box.index));
        const named = /\b(?:placeholder|aria-label)="(?:Filter|Find)\b/.test(tag) || /\bdata-filter\b/.test(tag);
        if (named && !/filterKeys\(/.test(tag)) add({ rule: "o", file, key: tag.slice(0, 60), detail: "a box that narrows or finds answers `Esc` with `{@attach filterKeys()}`" });
      }
    }
  }
  return [...found.values()];
}

/** Every entry in every `<ticket>.txt` under `dir`. A line that can't be read is an entry with no reason. */
function allowlist(dir: string): Entry[] {
  if (!existsSync(dir)) return [];
  return readdirSync(dir)
    .filter((name) => name.endsWith(".txt"))
    .sort()
    .flatMap((list) =>
      readFileSync(join(dir, list), "utf8")
        .split("\n")
        .flatMap((raw, index) => {
          const line = raw.trimEnd();
          if (/^\s*(#|$)/.test(line)) return [];
          const [head = "", ...rest] = line.split(/ {2,}|\t+/);
          const [rule = "", file = "", ...key] = head.trim().split(" ");
          return [{ list, line: index + 1, rule, file, key: key.join(" "), reason: rest.join("  ").trim() }];
        }),
    );
}

/** What fails the gate, one line per failure: unlisted breaches, entries without a reason (d), stale entries (e). */
function gate(violations: Violation[], entries: Entry[]): string[] {
  const failures: string[] = [];
  const listed = new Map<string, Entry>();
  for (const entry of entries) {
    const at = `style-gate-exceptions/${entry.list}:${entry.line}`;
    const id = `${entry.rule} ${entry.file} ${entry.key}`;
    if (!/^\d+\.txt$/.test(entry.list)) {
      failures.push(`${at}: ${id} sits in a file that names no ticket; entries go in the cleanup ticket's \`<ticket>.txt\``);
    } else if (!/^[abcfghijklmn]$/.test(entry.rule) || entry.file === "" || entry.key === "") {
      failures.push(`${at}: "${id}" is not \`<rule a|b|c|f|g|h|i|j|k|l> <file> <selector or class>  <reason>\``);
    } else if (entry.reason === "") {
      failures.push(`(d) ${at}: ${id} has no reason; every exception says why it is one (${SECTION})`);
    } else if (listed.has(id)) {
      const first = listed.get(id)!;
      failures.push(`${at}: ${id} is already listed in style-gate-exceptions/${first.list}:${first.line}; a violation goes in one file`);
    } else {
      listed.set(id, entry);
    }
  }
  const seen = new Set<string>();
  for (const v of violations) {
    const id = `${v.rule} ${v.file} ${v.key}`;
    seen.add(id);
    if (!listed.has(id)) failures.push(`(${v.rule}) ${v.file} \`${v.key}\` ${v.detail}. See ${SECTION}.`);
  }
  for (const [id, entry] of listed) {
    if (!seen.has(id)) {
      failures.push(`(e) style-gate-exceptions/${entry.list}:${entry.line}: ${id} matches nothing; delete the line`);
    }
  }
  return failures;
}

test("the app's stylesheets and components keep the control rules, or their allowlist says which ticket removes the breach", () => {
  expect(gate(scan(ROOT), allowlist(ALLOWLIST))).toEqual([]);
});

test("the scan reads the stylesheets and the components", () => {
  const walked = files(ROOT);
  // Floors, not a census: a narrowed walk would pass every rule.
  expect(walked).toContain(CONTROLS_SHEET);
  expect(walked).toContain("unported.css");
  expect(walked.filter((f) => f.endsWith(".css")).length).toBeGreaterThanOrEqual(30);
  expect(walked.filter((f) => f.endsWith(".svelte")).length).toBeGreaterThanOrEqual(150);
});

/** Run the gate over a planted `app/src/` and its allowlist, both in a temporary directory. */
function planted(sources: Record<string, string>, lists: Record<string, string> = {}): string[] {
  const dir = mkdtempSync(join(tmpdir(), "style-gate-"));
  try {
    const put = (path: string, text: string) => {
      mkdirSync(dirname(path), { recursive: true });
      writeFileSync(path, text);
    };
    put(join(dir, "src", CONTROLS_SHEET), ".tf-btn { height: var(--control-h); padding: 0 11px; }\n");
    for (const [path, text] of Object.entries(sources)) put(join(dir, "src", path), text);
    mkdirSync(join(dir, "allow"));
    for (const [name, text] of Object.entries(lists)) put(join(dir, "allow", name), text);
    return gate(scan(join(dir, "src")), allowlist(join(dir, "allow")));
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

test("(k) a rule that lays out `*` fails, and one that names its subject or sets something else passes", () => {
  for (const css of [
    ".tf-main > .tf-kept > * { width: 100vw; }",
    ".tf-x > * { overflow: hidden; }",
    "@media (max-width: 900px) { .tf-x * { position: absolute; } }",
    ".tf-x > *:first-child { min-height: 10px; }",
  ]) {
    const failures = planted({ "lib/tf/css/view.css": css });
    expect(failures, css).toHaveLength(1);
    expect(failures[0], css).toMatch(/^\(k\) lib\/tf\/css\/view\.css /);
  }
  expect(
    planted({
      "lib/tf/css/view.css": [
        ".tf-keptpage { width: 100vw; }",
        ".tf-x > * { flex: none; }",
        ".tf-x > * + * { margin-top: 4px; }",
        ".tf-x > .tf-y { overflow: hidden; }",
      ].join("\n"),
    }),
  ).toEqual([]);
});

test("(l) a `role=\"menu\"` outside `TfMenu` fails; `TfMenu`'s own and a word about one in a comment pass", () => {
  const failures = planted({ "lib/tf/x/TfX.svelte": '<div class="tf-menu" role="menu"></div>' });
  expect(failures).toHaveLength(1);
  expect(failures[0]).toMatch(/^\(l\) lib\/tf\/x\/TfX\.svelte /);
  expect(
    planted({
      [MENU_COMPONENT]: '<div class="tf-menu" role="menu"></div>',
      "lib/tf/x/TfX.svelte": '<!-- a `role="menu"` is TfMenu -->\n<button role="menuitem">x</button>',
    }),
  ).toEqual([]);
});

/** The rules a component alone breaches. */
const rulesFor = (file: string, markup: string): string[] => planted({ [file]: markup }).map((failure) => failure.slice(1, 2));

test("(o) a filter or find box with no `filterKeys` fails, and one with it, or any other field, passes", () => {
  expect(rulesFor("lib/x/Y.svelte", '<input placeholder="Filter changed files" />')).toEqual(["o"]);
  expect(rulesFor("lib/x/Y.svelte", '<input aria-label="Find in log" value={typed} />')).toEqual(["o"]);
  expect(rulesFor("lib/x/Y.svelte", '<input placeholder="Filter changed files" {@attach filterKeys()} />')).toEqual([]);
  expect(rulesFor("lib/x/Y.svelte", '<input placeholder="Name" />')).toEqual([]);
});

test("(p) a value hidden on focus within fails; the group's own hover and the marked value pass", () => {
  const rule = (css: string) => rulesFor("lib/tf/css/x.css", css);
  expect(rule("tr:is(:hover, :focus-within) .tf-rowval { visibility: hidden; }")).toEqual(["p"]);
  expect(rule("tr:hover .tf-rowval.tf-covered { visibility: hidden; }")).toEqual([]);
});

test("(m) a hand-drawn missing line in a dialog, a second markup and *(needed)* fail; the page's one markup and `TfDialog`'s own pass", () => {
  const dialog = (footer: string, open = "<TfDialog>") => `${open}{#snippet footer()}${footer}{/snippet}</TfDialog>`;
  const m = (markup: string, file = "lib/x/Y.svelte") => {
    expect(rulesFor(file, markup).filter((rule) => rule === "m").length, markup).toBeGreaterThan(0);
  };
  m(dialog('<p class="tf-dwhy" role="status">Name it first.</p><button class="tf-btn tf-primary">Add</button>'));
  m('<span class="tf-dwhy">Name it first.</span>');
  m("<label>Name (needed)</label>");
  expect(rulesFor("lib/x/Y.svelte", '<p class="tf-dwhy" role="status">Name it first.</p>')).toEqual([]);
  expect(rulesFor("lib/tf/settings/TfDialog.svelte", dialog('<p class="tf-dwhy" role="status">{why}</p>'))).toEqual([]);
});

test("(n) a dialog's primary disabled for more than a send needs `why`; a send in flight does not, nor a primary that can't be disabled", () => {
  const dialog = (open: string, disabled: string) =>
    `${open}{#snippet footer()}<button class="tf-btn tf-primary" onclick={() => { go(); }} disabled={${disabled}}>Add</button>{/snippet}</TfDialog>`;
  expect(rulesFor("lib/x/Y.svelte", dialog("<TfDialog>", "!ready"))).toEqual(["n"]);
  expect(rulesFor("lib/x/Y.svelte", dialog("<TfDialog>", 'writing || name === ""'))).toEqual(["n"]);
  expect(rulesFor("lib/x/Y.svelte", dialog('<TfDialog why={ready ? null : "Name it."}>', "!ready"))).toEqual([]);
  expect(rulesFor("lib/x/Y.svelte", dialog("<TfDialog>", "writing"))).toEqual([]);
  expect(rulesFor("lib/x/Y.svelte", dialog("<TfDialog>", "sending || queue.busy"))).toEqual([]);
  expect(rulesFor("lib/x/Y.svelte", '<TfDialog>{#snippet footer()}<button class="tf-btn tf-primary">Add</button>{/snippet}</TfDialog>')).toEqual([]);
  expect(rulesFor("lib/x/Y.svelte", '<button class="tf-btn tf-primary" disabled={!ready}>Add</button>')).toEqual([]);
});

/** The dialogs that arrive on a select, and why: picking it is what each is opened for. Any other marks its field `data-arrive`. */
const ARRIVES_ON_A_SELECT: Record<string, string> = {
  "lib/assets/ImportDialog.svelte": "*Import from* decides which fields follow, so it is the first question",
  "lib/tf/builds/TfRunDialog.svelte": "*Start* chooses what the rest of the form holds",
};

test("a dialog that starts on a select marks the field it is for, or is listed as choosing to (F-453)", () => {
  const onSelect = files(ROOT).flatMap((file) => {
    if (!file.endsWith(".svelte")) return [];
    const blocks = readFileSync(join(ROOT, file), "utf8").match(/<TfDialog\b[\s\S]*?<\/TfDialog>/g) ?? [];
    return blocks.some((block) => {
      const first = [...block.matchAll(/<(select|textarea|input)\b([\s\S]*?)\/?>/g)].find((field) => !/type="(checkbox|radio|hidden)"/.test(field[2]!));
      return first?.[1] === "select" && !block.includes("data-arrive");
    })
      ? [file]
      : [];
  });
  expect(onSelect.sort()).toEqual(Object.keys(ARRIVES_ON_A_SELECT).sort());
});

test("(a) a control sized outside the controls stylesheet fails, and the same rule in it passes", () => {
  for (const css of [
    ".tf-pr-head .tf-btn { padding: 2px 8px; }",
    ".tf-x .tf-btn.tf-sm { height: 24px; }",
    "@media (max-width: 900px) { .tf-x > .tf-iconbtn { min-height: 30px; } }",
    ".tf-x .tf-lg { font: 500 var(--t-xs) var(--sans); }",
    ".tf-x .tf-seg button { font-size: var(--t-2xs); }",
    ".tf-rvq .tf-fchips button { padding: 1px 9px; }",
    ".tf-y, .tf-x .tf-btn { border-radius: 2px; }",
    ".tf-rd-acts .tf-refresh { padding: 3px 8px; }",
    ".tf-sc-chip .tf-chipbtn { padding: 0 2px; }",
  ]) {
    const failures = planted({ "lib/tf/css/view.css": css });
    expect(failures, css).toHaveLength(1);
    expect(failures[0], css).toMatch(/^\(a\) lib\/tf\/css\/view\.css `[^`]*tf-[a-z]+[^`]*` sets .* See docs\/specs\/2026-10-15-ticket-first-design-system\.md, section Controls\.$/);
  }
  expect(planted({ "lib/tf/css/view.css": ".tf-y, .tf-x .tf-btn { border-radius: 2px; }" })[0]).toContain("`.tf-x .tf-btn`");
  // The clean twins: in the controls stylesheet, not a sizing property, or not the control itself.
  expect(planted({ [CONTROLS_SHEET]: ".tf-btn.tf-sm { height: var(--control-h-sm); padding: 0 8px; font-size: var(--t-sm); }" })).toEqual([]);
  expect(
    planted({
      "lib/tf/css/view.css": [
        ".tf-x .tf-btn { margin-left: auto; color: red; }",
        ".tf-iconbtn svg { width: 16px; height: 16px; }",
        ".tf-btn kbd { font-size: var(--t-2xs); padding: 0 3px; }",
        ".tf-seg .tf-jobk { font-size: var(--t-2xs); }",
        ".tf-btns { padding: 4px; }",
        ".tf-x button { padding: 4px; }",
        "/* .tf-x .tf-btn { padding: 2px; } */",
      ].join("\n"),
    }),
  ).toEqual([]);
});

test("(a) a disabled control's look set outside the controls stylesheet fails", () => {
  for (const css of [
    ".tf-ftreebox > header .tf-iconbtn[disabled] { opacity: .45; cursor: default; background: none; }",
    ".tf-x .tf-btn:disabled { cursor: default; }",
    ".tf-x .tf-btn[aria-disabled=\"true\"] { opacity: .3; }",
  ]) {
    const failures = planted({ "lib/tf/css/view.css": css });
    expect(failures, css).toHaveLength(1);
    expect(failures[0], css).toMatch(/^\(a\) lib\/tf\/css\/view\.css `[^`]+` sets (opacity|cursor).* on a control outside lib\/tf\/css\/chrome\.css\. See /);
  }
  // The clean twins: in the controls stylesheet, a disabled control's placement, an enabled control's cursor.
  expect(planted({ [CONTROLS_SHEET]: ".tf-btn[disabled], .tf-iconbtn[disabled] { opacity: .55; cursor: not-allowed; }" })).toEqual([]);
  expect(planted({ "lib/tf/css/view.css": ".tf-x .tf-btn[disabled] { margin-left: auto; }\n.tf-x .tf-btn { cursor: grab; }" })).toEqual([]);
});

test("(h) a size off the type scale fails, and the reading face, code and tokens pass", () => {
  for (const css of [".tf-x { font-size: 12.5px; }", ".tf-x { font-size: .85em; }", ".tf-x { font: 600 12px var(--sans); }"]) {
    const failures = planted({ "lib/tf/css/view.css": css });
    expect(failures, css).toHaveLength(1);
    expect(failures[0], css).toMatch(/^\(h\) lib\/tf\/css\/view\.css `\.tf-x` sets font/);
  }
  expect(
    planted({
      "lib/tf/css/view.css": [
        ".tf-x { font-size: var(--t-sm); }",
        ".tf-x { font: 600 var(--t-xs) var(--mono); }",
        ".tf-x { font: inherit; }",
        ".tf-doc { font-family: var(--serif); font-size: 17.5px; }",
        ".tf-x code { font: .88em var(--mono); font-size: round(.88em, 1px); }",
        ".tf .tf-rd-title { font-size: 30px; }",
      ].join("\n"),
    }),
  ).toEqual([]);
  expect(planted({ "unported.css": ".row .k { font-size: 12px; }" })).toEqual([]);
});

test("(i) a pill toggle outside .tf-fchips fails, and the pill toggles themselves and a round badge pass", () => {
  const failures = planted({ "lib/tf/css/view.css": '.tf-x-val { border-radius: 12px; }\n.tf-x-val[aria-pressed="true"] { color: red; }' });
  expect(failures).toHaveLength(1);
  expect(failures[0]).toMatch(/^\(i\) lib\/tf\/css\/view\.css `\.tf-x-val` draws a pill toggle/);
  expect(planted({ "lib/tf/css/view.css": ".tf-count { border-radius: 9px; }" })).toEqual([]);
  expect(planted({ "lib/tf/css/view.css": '.tf-x-val { border-radius: 4px; }\n.tf-x-val[aria-pressed="true"] { color: red; }' })).toEqual([]);
  expect(planted({ [CONTROLS_SHEET]: '.tf-fchips button { border-radius: 12px; }\n.tf-fchips button[aria-pressed="true"] { color: red; }' })).toEqual([]);
});

test("(j) a literal 132px label column fails, and the token and a wider measure pass", () => {
  const failures = planted({ "lib/tf/css/view.css": ".tf-x { grid-template-columns: 132px minmax(0, 1fr); }" });
  expect(failures).toHaveLength(1);
  expect(failures[0]).toMatch(/^\(j\) lib\/tf\/css\/view\.css `\.tf-x` sets a literal 132px label column/);
  expect(planted({ "lib/tf/css/view.css": ".tf-x { grid-template-columns: var(--tf-label) minmax(0, 1fr); }\n.tf-y { grid-template-columns: 1132px 1fr; width: 132px; }" })).toEqual([]);
});

test("(a) a component's <style> block is a stylesheet too", () => {
  const component = '<button class="tf-btn">Go</button>\n<style>\n  .row :global(.tf-btn) { height: 20px; }\n</style>\n';
  expect(planted({ "lib/view/Row.svelte": component })).toEqual([
    "(a) lib/view/Row.svelte `.row :global(.tf-btn)` sets height on a control outside lib/tf/css/chrome.css. See docs/specs/2026-10-15-ticket-first-design-system.md, section Controls.",
  ]);
  expect(planted({ "lib/view/Row.svelte": '<button class="tf-btn">Go</button>\n<style>\n  .row { height: 20px; }\n</style>\n' })).toEqual([]);
});

test("(b) the unported button classes fail in a stylesheet and in a component's markup", () => {
  expect(planted({ "unported.css": ".btn{height:24px}\n.btn.sm{height:20px}\n.tb-btn{gap:7px}\n.dlg-h .x{margin-left:auto}\n" })).toEqual([
    "(b) unported.css `btn` styles the unported button class .btn. See docs/specs/2026-10-15-ticket-first-design-system.md, section Controls.",
    "(b) unported.css `tb-btn` styles the unported button class .tb-btn. See docs/specs/2026-10-15-ticket-first-design-system.md, section Controls.",
    "(b) unported.css `x` styles the unported button class .x. See docs/specs/2026-10-15-ticket-first-design-system.md, section Controls.",
  ]);
  for (const markup of [
    '<button class="btn sm">Go</button>',
    '<button class="btn pri" disabled={busy}>Go</button>',
    '<button class="tb-btn imp">Import</button>',
    '<button class="x" aria-label="Close"></button>',
    "<button class=\"row {open ? 'btn' : ''}\">Go</button>",
    "<button class={['btn', wide && 'wide']}>Go</button>",
    "<button class:btn={true}>Go</button>",
    "<button class={`btn ${kind}`}>Go</button>",
    "<button class={`row ${open ? 'on' : ''} btn`}>Go</button>",
  ]) {
    const failures = planted({ "lib/assets/Dialog.svelte": markup });
    expect(failures, markup).toHaveLength(1);
    expect(failures[0], markup).toMatch(/^\(b\) lib\/assets\/Dialog\.svelte `(btn|tb-btn|x)` uses the unported button class /);
  }
  // The clean twins: the ported classes, an `x` that is an attribute or a variable, the words in a script.
  expect(
    planted({
      "lib/assets/Dialog.svelte": [
        "<script>const btn = 'btn'; let x = 1;</script>",
        '<button class="tf-btn tf-sm">Go</button>',
        '<line class="tf-tm-hr" x1={x} x2={x} />',
        "<span class=\"row {x ? 'on' : ''}\">{x}</span>",
        "<rect class={`tf-bar tf-run-${tone(x)}`} x={x} />",
        "<style>.tf-btns { color: red; } .tf-x { width: 1.5x; }</style>",
      ].join("\n"),
      "lib/tf/css/view.css": ".tf-btn.tf-sm { color: red; } .tf-tbx { color: red; }",
    }),
  ).toEqual([]);
});

test("(c) a row of buttons with a gap other than the control gap fails", () => {
  for (const css of [
    ".tf-fhead .tf-acts { margin-left: auto; display: flex; gap: 6px; }",
    ".tf-strip .tf-trail { gap: 8px; }",
    ".tf-dialog footer { display: flex; gap: 8px; }",
    ".tf-vw-dialog > footer { column-gap: 4px; }",
    ".tf-steps footer { gap: 8px; }",
    ".dlg-f { gap: 6px; }",
    ".tf-stg-acts { display: flex; gap: 8px; }",
    // A row named `…acts` without the hyphen is one too (#2343).
    ".tf-approw .tf-appacts { display: flex; gap: 4px; }",
    ".tf-logacts { gap: 8px; }",
    // The bare `.acts` of the unported styles and the boot screen too (#2292).
    ".sec-h .acts{display:flex;gap:4px}",
    ".acts { margin-top: 16px; display: flex; gap: 6px; }",
    // A row of buttons that is not called actions: the filter bar, the pill toggles, the large decision, a `…-btns` row.
    ".tf-fbar { display: flex; gap: 8px 10px; }",
    ".tf-fchips { display: inline-flex; gap: 4px; }",
    ".tf-decide { display: flex; gap: 10px; }",
    ".tf-vw-btns { display: flex; gap: 8px; }",
    // A dialog's title-line cluster of actions and Close (#2415).
    ".tf-dhead .tf-dctl { display: flex; gap: 6px; }",
  ]) {
    const failures = planted({ "lib/tf/css/view.css": css });
    expect(failures, css).toHaveLength(1);
    expect(failures[0], css).toMatch(/^\(c\) lib\/tf\/css\/view\.css `[^`]+` sets (column-)?gap: \d+px( \d+px)? on a row of buttons\. See /);
  }
  // In the controls stylesheet too: the gap rule has no home where it may be broken.
  expect(planted({ [CONTROLS_SHEET]: ".tf-trail { gap: 4px; }" })).toHaveLength(1);
  // The clean twins: the token, a container that is not one of actions, a child of one.
  expect(
    planted({
      "lib/tf/css/view.css": [
        ".tf-fhead .tf-acts { gap: var(--control-gap); }",
        ".tf-dialog footer { gap: var(--control-gap); }",
        ".tf-launch > footer { gap: 14px; }",
        ".tf-fchips { gap: var(--control-gap); }",
        // The select's label and the select are not two buttons.
        ".tf-fsel { gap: 5px; }",
        ".tf-acts .tf-btn { gap: 6px; }",
        ".tf-fbar .tf-fcount { gap: 4px; }",
        ".tf-actsx { gap: 8px; }",
        ".acts .tf-btn { gap: 6px; }",
        ".actsx { gap: 8px; }",
        ".facts { gap: 8px; }",
        // Facts are not actions (#2343).
        ".tf-facts { gap: 8px; }",
        ".tf-item-facts { gap: 8px; }",
        ".tf-runfacts { gap: 8px; }",
        ".artifacts { gap: 8px; }",
        ".codeFacts { gap: 8px; }",
        ".tf-refresh-failed { padding: 8px; }",
      ].join("\n"),
    }),
  ).toEqual([]);
});

test("(f) a field in a row of buttons sized by padding or a literal height fails", () => {
  for (const css of [
    ".tf-fsearch { display: flex; padding: 4px 8px; }",
    ".tf-fsel select { border: 1px solid; padding: 2px 6px; }",
    ".tf-fbar select { height: 24px; }",
    ".tf-acts input { min-height: 30px; }",
    ".tf-dl-head .tf-fsearch { padding: 6px 9px; }",
    ".tf-trail select { padding-top: 3px; }",
  ]) {
    const failures = planted({ "lib/tf/css/view.css": css });
    expect(failures, css).toHaveLength(1);
    expect(failures[0], css).toMatch(/^\(f\) lib\/tf\/css\/view\.css `[^`]+` sizes a field in a row of buttons by .+; it takes the row's height, `height: var\(--control-h…\)`\. See /);
  }
  // The clean twins: the token and horizontal padding, the text box inside the field, a field in a form.
  expect(
    planted({
      "lib/tf/css/view.css": [
        ".tf-fsearch { height: var(--control-h-sm); padding: 0 8px; }",
        ".tf-fsel select { height: var(--control-h-sm); padding: 0 6px; }",
        ".tf-fsearch input { padding: 1px 0; }",
        ".tf-formgrid input { padding: 5px 8px; }",
        ".tf-fsearch.tf-wide { padding-left: 4px; max-width: none; }",
        ".tf-acts select { height: var(--control-h); }",
      ].join("\n"),
    }),
  ).toEqual([]);
});

test("the filter bar's search and select, and the file tree's Go to file, take the small control height", () => {
  const height = (sheet: string, selector: string) =>
    rules(readFileSync(join(ROOT, "lib/tf/css", sheet), "utf8"))
      .find((rule) => rule.selectors.includes(selector))
      ?.decls.find(([prop]) => prop === "height")?.[1];
  expect(height("filter.css", ".tf-fsearch")).toBe("var(--control-h-sm)");
  expect(height("filter.css", ".tf-fsel select")).toBe("var(--control-h-sm)");
  expect(height("code.css", ".tf-ftreebox > header .tf-gotofile")).toBe("var(--control-h-sm)");
});

test("(g) a trailing group of actions rebuilt by hand fails; it is .tf-trail", () => {
  for (const css of [
    ".tf-banner .tf-acts { margin-left: auto; display: flex; gap: var(--control-gap); }",
    ".tf-logacts { margin-left: auto; display: flex; gap: var(--control-gap); }",
    ".sec-h .acts{margin-left:auto;display:flex;gap:var(--control-gap)}",
    ".tf-diff .tf-fh-acts { margin-left: auto; display: flex; align-items: center; }",
  ]) {
    const failures = planted({ "lib/tf/css/view.css": css });
    expect(failures, css).toHaveLength(1);
    expect(failures[0], css).toMatch(/^\(g\) lib\/tf\/css\/view\.css `[^`]+` rebuilds the trailing group of actions; use `\.tf-trail`\. See /);
  }
  // The clean twins: the page header's own rule, a placement-only margin, the trail itself and a tweak of it.
  expect(
    planted({
      "lib/tf/css/view.css": [
        ".tf-fhead .tf-acts { margin-left: auto; display: flex; gap: var(--control-gap); }",
        ".tf-stg-h .tf-acts { margin-left: auto; display: flex; gap: var(--control-gap); }",
        ".tf-suggest .tf-acts { margin-left: auto; }",
        ".tf-mon-acts .tf-trail { justify-content: flex-end; flex-wrap: nowrap; }",
        ".tf-ext-h .tf-btn { margin-left: auto; }",
      ].join("\n"),
      [CONTROLS_SHEET]: ".tf-btn { height: var(--control-h); padding: 0 11px; }\n.tf-trail { margin-left: auto; display: flex; gap: var(--control-gap); }",
    }),
  ).toEqual([]);
});

const BREACH = { "lib/tf/css/pr.css": ".tf-pr-head .tf-btn { padding: 2px 8px; }" };

test("an allowlist entry with its reason lets a breach through", () => {
  expect(planted(BREACH, { "2295.txt": "# Code\n\na lib/tf/css/pr.css .tf-pr-head .tf-btn  the PR header's local small size\n" })).toEqual([]);
  // Whitespace inside the selector is normalised on both sides.
  expect(planted({ "lib/tf/css/pr.css": ".tf-pr-head   >  .tf-btn { padding: 2px 8px; }" }, { "2295.txt": "a lib/tf/css/pr.css .tf-pr-head > .tf-btn  the PR header\n" })).toEqual([]);
});

test("(d) an allowlist entry with no reason fails, and lets nothing through", () => {
  expect(planted(BREACH, { "2295.txt": "a lib/tf/css/pr.css .tf-pr-head .tf-btn\n" })).toEqual([
    "(d) style-gate-exceptions/2295.txt:1: a lib/tf/css/pr.css .tf-pr-head .tf-btn has no reason; every exception says why it is one (docs/specs/2026-10-15-ticket-first-design-system.md, section Controls)",
    "(a) lib/tf/css/pr.css `.tf-pr-head .tf-btn` sets padding on a control outside lib/tf/css/chrome.css. See docs/specs/2026-10-15-ticket-first-design-system.md, section Controls.",
  ]);
});

test("(e) a stale allowlist entry fails and names the file it sits in", () => {
  const list = { "2284.txt": "# Settings\na lib/tf/css/settings.css .tf-set .tf-btn  the settings rows' local size\n" };
  expect(planted({}, list)).toEqual([
    "(e) style-gate-exceptions/2284.txt:2: a lib/tf/css/settings.css .tf-set .tf-btn matches nothing; delete the line",
  ]);
  expect(planted({ "lib/tf/css/settings.css": ".tf-set .tf-btn { height: 24px; }" }, list)).toEqual([]);
});

test("an entry listed in two tickets' files, or not in the entry format, fails", () => {
  const entry = "a lib/tf/css/pr.css .tf-pr-head .tf-btn  the PR header\n";
  expect(planted(BREACH, { "2295.txt": entry, "2296.txt": entry })).toEqual([
    "style-gate-exceptions/2296.txt:1: a lib/tf/css/pr.css .tf-pr-head .tf-btn is already listed in style-gate-exceptions/2295.txt:1; a violation goes in one file",
  ]);
  expect(planted({}, { "2295.txt": "lib/tf/css/pr.css .tf-pr-head .tf-btn  no rule letter\n" })).toHaveLength(1);
});

test("an entry in a file that names no ticket fails, and lets nothing through", () => {
  expect(planted(BREACH, { "misc.txt": "a lib/tf/css/pr.css .tf-pr-head .tf-btn  the PR header\n" })).toEqual([
    "style-gate-exceptions/misc.txt:1: a lib/tf/css/pr.css .tf-pr-head .tf-btn sits in a file that names no ticket; entries go in the cleanup ticket's `<ticket>.txt`",
    "(a) lib/tf/css/pr.css `.tf-pr-head .tf-btn` sets padding on a control outside lib/tf/css/chrome.css. See docs/specs/2026-10-15-ticket-first-design-system.md, section Controls.",
  ]);
});

test("the ticket page card's copy button and foot button keep the three sizes: the copy small, the foot large, no stylesheet sizing either (spec #2396)", () => {
  const card = readFileSync(join(ROOT, "lib/tf/job/TfJobCard.svelte"), "utf8").replace(/<(script|style)[^>]*>[\s\S]*?<\/\1>/g, "");
  /** The class words on the first element whose class attribute matches `marker`. */
  const classesOf = (marker: RegExp) => {
    const value = [...card.matchAll(/\bclass="([^"]*)"/g)].map(([, words]) => words!.split(/\s+/)).find((words) => words.some((word) => marker.test(word)));
    expect(value, `no element with a class matching ${marker}`).toBeDefined();
    return value!;
  };
  expect(classesOf(/^tf-tkcopy$/)).toEqual(expect.arrayContaining(["tf-iconbtn", "tf-sm"]));
  // The foot is the one button inside `.tf-go`.
  const foot = card.match(/<div class="tf-go">\s*<button[^>]*\bclass="([^"]*)"/);
  expect(foot, "no button inside the card's .tf-go").not.toBeNull();
  expect(foot![1]!.split(/\s+/)).toEqual(expect.arrayContaining(["tf-btn", "tf-lg"]));
  // Their own classes name no control, so (a) does not see them: no rule whose subject is the copy button, or a
  // button in the card's foot, sizes it either.
  const sized = files(ROOT).flatMap((file) =>
    rules(stylesheet(file, readFileSync(join(ROOT, file), "utf8"))).flatMap(({ selectors, decls }) =>
      selectors
        .filter((selector) => {
          const parts = compounds(selector);
          const subject = parts.at(-1) ?? "";
          return /\.tf-tkcopy(?![\w-])/.test(subject) || (parts.slice(0, -1).some((part) => /\.tf-go(?![\w-])/.test(part)) && /\.tf-btn|^button/.test(subject));
        })
        .filter(() => decls.some(([prop]) => SIZING.test(prop)))
        .map((selector) => `${file} ${selector}`),
    ),
  );
  expect(sized).toEqual([]);
  // The two lists of rows (`.tf-tksecs`, `.tf-tkacts`) are links and buttons drawn as rows, not `.tf-btn`: they take
  // no control size, so the gate does not size them (design system, *The ticket page's card*).
});
