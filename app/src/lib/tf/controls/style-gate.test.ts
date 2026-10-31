/**
 * The style gate (#2281): the design system's Controls section, enforced.
 *
 * The scan reads every stylesheet and component under `app/src/` off disk,
 * as the house rules do, and fails when
 *
 * - (a) a rule outside the controls stylesheet (`lib/tf/css/chrome.css`) sets
 *   a button's `height`, `min-height`, `padding`, `font-size` or
 *   `border-radius` (a `font` shorthand counts, it sets the size);
 * - (b) the unported button classes (`btn`, `tb-btn`, the old `x`) appear in a
 *   stylesheet or a component;
 * - (c) an actions container (`.tf-acts` or a local `…-acts` row, `.tf-trail`, a dialog footer) has a
 *   `gap` other than `var(--control-gap)`;
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

type Rule = "a" | "b" | "c";
/** One breach: the rule, the file under `app/src/`, and the selector (a, c) or class (b) it is keyed on. */
type Violation = { rule: Rule; file: string; key: string; detail: string };
type Entry = { list: string; line: number; rule: string; file: string; key: string; reason: string };

/**
 * Classes that name a control (rule a). `tf-refresh` is the shared Refresh button
 * (`TfDetailRefresh`), always a `.tf-btn`; views resized it under its own name.
 */
const CONTROL = /\.(?:tf-btn|tf-sm|tf-lg|tf-iconbtn|tf-refresh)(?![\w-])/;
/** The segmented control and the pill toggles: their `button`s are controls. */
const BUTTON_GROUP = /\.(?:tf-seg|tf-fchips)(?![\w-])/;
const SIZING = /^(?:height|min-height|padding(?:-[a-z-]+)?|font-size|font|border(?:-[a-z]+)*-radius)$/;
/** The unported button classes (rule b). */
const UNPORTED = ["btn", "tb-btn", "x"];
/**
 * Actions containers (rule c): `.tf-acts` and every local `…-acts` row (`.tf-stg-acts`,
 * `.tf-wq-acts`), a strip's `.tf-trail`, the unported `.dlg-f`, and a dialog's `footer`.
 */
const ACTIONS_CLASS = /\.(?:[\w-]+-acts|tf-trail|dlg-f)(?![\w-])/;
const DIALOG = /\.(?:[\w-]*dialog[\w-]*|tf-steps)(?![\w-])/;

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

/** Every breach of rules (a), (b) and (c) under `root`, one per (rule, file, key). */
function scan(root: string): Violation[] {
  const found = new Map<string, Violation>();
  const add = (v: Violation) => found.set(`${v.rule} ${v.file} ${v.key}`, found.get(`${v.rule} ${v.file} ${v.key}`) ?? v);
  for (const file of files(root)) {
    const text = readFileSync(join(root, file), "utf8");
    for (const { selectors, decls } of rules(stylesheet(file, text))) {
      for (const selector of selectors) {
        const sizing = decls.filter(([prop]) => SIZING.test(prop)).map(([prop]) => prop);
        if (file !== CONTROLS_SHEET && sizing.length > 0 && targetsControl(selector)) {
          add({ rule: "a", file, key: selector, detail: `sets ${sizing.join(", ")} on a control outside ${CONTROLS_SHEET}` });
        }
        for (const name of UNPORTED) {
          if (new RegExp(`(?<![\\w-])\\.${name}(?![\\w-])`).test(selector)) {
            add({ rule: "b", file, key: name, detail: `styles the unported button class .${name}` });
          }
        }
        const gaps = decls.filter(([prop, value]) => /^(?:gap|column-gap)$/.test(prop) && value !== "var(--control-gap)");
        if (gaps.length > 0 && isActions(selector)) {
          add({ rule: "c", file, key: selector, detail: `sets ${gaps.map(([p, v]) => `${p}: ${v}`).join(", ")} on an actions container` });
        }
      }
    }
    if (file.endsWith(".svelte")) {
      const used = markupClasses(text);
      for (const name of UNPORTED.filter((n) => used.has(n))) {
        add({ rule: "b", file, key: name, detail: `uses the unported button class ${name}` });
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
    } else if (!/^[abc]$/.test(entry.rule) || entry.file === "" || entry.key === "") {
      failures.push(`${at}: "${id}" is not \`<rule a|b|c> <file> <selector or class>  <reason>\``);
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

test("(a) a control sized outside the controls stylesheet fails, and the same rule in it passes", () => {
  for (const css of [
    ".tf-pr-head .tf-btn { padding: 2px 8px; }",
    ".tf-x .tf-btn.tf-sm { height: 24px; }",
    "@media (max-width: 900px) { .tf-x > .tf-iconbtn { min-height: 30px; } }",
    ".tf-x .tf-lg { font: 500 12px var(--sans); }",
    ".tf-x .tf-seg button { font-size: 11px; }",
    ".tf-rvq .tf-fchips button { padding: 1px 9px; }",
    ".tf-y, .tf-x .tf-btn { border-radius: 2px; }",
    ".tf-rd-acts .tf-refresh { padding: 3px 8px; }",
  ]) {
    const failures = planted({ "lib/tf/css/view.css": css });
    expect(failures, css).toHaveLength(1);
    expect(failures[0], css).toMatch(/^\(a\) lib\/tf\/css\/view\.css `[^`]*tf-[a-z]+[^`]*` sets .* See docs\/specs\/2026-10-15-ticket-first-design-system\.md, section Controls\.$/);
  }
  expect(planted({ "lib/tf/css/view.css": ".tf-y, .tf-x .tf-btn { border-radius: 2px; }" })[0]).toContain("`.tf-x .tf-btn`");
  // The clean twins: in the controls stylesheet, not a sizing property, or not the control itself.
  expect(planted({ [CONTROLS_SHEET]: ".tf-btn.tf-sm { height: var(--control-h-sm); padding: 0 8px; font-size: 13px; }" })).toEqual([]);
  expect(
    planted({
      "lib/tf/css/view.css": [
        ".tf-x .tf-btn { margin-left: auto; color: red; }",
        ".tf-iconbtn svg { width: 16px; height: 16px; }",
        ".tf-btn kbd { font-size: 11px; padding: 0 3px; }",
        ".tf-seg .tf-jobk { font-size: 11px; }",
        ".tf-btns { padding: 4px; }",
        ".tf-x button { padding: 4px; }",
        "/* .tf-x .tf-btn { padding: 2px; } */",
      ].join("\n"),
    }),
  ).toEqual([]);
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
        "<style>.tf-btns { gap: 4px; } .tf-x { width: 1.5x; }</style>",
      ].join("\n"),
      "lib/tf/css/view.css": ".tf-btn.tf-sm { color: red; } .tf-tbx { color: red; }",
    }),
  ).toEqual([]);
});

test("(c) an actions container's gap other than the control gap fails", () => {
  for (const css of [
    ".tf-fhead .tf-acts { margin-left: auto; display: flex; gap: 6px; }",
    ".tf-strip .tf-trail { gap: 8px; }",
    ".tf-dialog footer { display: flex; gap: 8px; }",
    ".tf-vw-dialog > footer { column-gap: 4px; }",
    ".tf-steps footer { gap: 8px; }",
    ".dlg-f { gap: 6px; }",
    ".tf-stg-acts { display: flex; gap: 8px; }",
  ]) {
    const failures = planted({ "lib/tf/css/view.css": css });
    expect(failures, css).toHaveLength(1);
    expect(failures[0], css).toMatch(/^\(c\) lib\/tf\/css\/view\.css `[^`]+` sets (column-)?gap: \d+px on an actions container\. See /);
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
        ".tf-fchips { gap: 4px; }",
        ".tf-acts .tf-btn { gap: 6px; }",
        ".tf-actsx { gap: 8px; }",
        ".tf-refresh-failed { padding: 8px; }",
      ].join("\n"),
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
