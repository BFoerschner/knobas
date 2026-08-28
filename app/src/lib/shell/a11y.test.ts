/**
 * The keyboard-and-screen-reader rules, scanned instead of walked.
 *
 * Spec §14 asks for a shell that is keyboard-complete with visible focus. A
 * walk with the mouse unplugged is what actually establishes that, and it was
 * done — the record is in this task's PR body. What is here is the subset a
 * walk cannot keep true: the rules that a *new* component, added next month by
 * someone who never read the walk, has to obey too.
 *
 * Scanned off disk rather than by mounting, so a rule applies to markup no
 * test happens to render.
 */
import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";

import { expect, test } from "vitest";

const ROOT = join(process.cwd(), "src") + "/";

/** Every `.svelte` file under `app/src/`, absolute, sorted. */
function components(): string[] {
  const found: string[] = [];
  const walk = (dir: string) => {
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      const path = join(dir, entry.name);
      if (entry.isDirectory()) walk(path);
      else if (entry.name.endsWith(".svelte")) found.push(path);
    }
  };
  walk(ROOT);
  return found.sort();
}

/** A component's template: everything outside `<script>` and `<style>`. */
function markup(text: string): string {
  return text
    .replace(/<script\b[\s\S]*?<\/script>/g, "")
    .replace(/<style\b[\s\S]*?<\/style>/g, "")
    .replace(/<!--[\s\S]*?-->/g, "");
}

/**
 * One element's opening tag, found by scanning rather than by regex.
 *
 * **This is the whole reason there is a scanner here.** The obvious
 * `<button\b([\s\S]*?)>` ends the tag at the first `>` in the source, and in
 * Svelte the first `>` is very often inside an attribute:
 * `onclick={() => router.go("#/sources")}`. Every button in this codebase has
 * one. The regex version therefore treated the arrow's `>` as the end of the
 * tag, took the rest of the handler as the element's *content*, and concluded
 * that no button was icon-only — a lint that reported zero offenders because
 * it was reading the wrong halves of every element. It passed on the first
 * run, and it survived deleting an `aria-label`, which is how it was caught.
 *
 * So: scan forward from `<tag`, tracking quote and brace depth, and stop at
 * the first `>` that is at depth zero and outside a string.
 *
 * @returns the attribute text and the index just past the `>`, or `null` for
 * an unterminated tag.
 */
function openTag(text: string, from: number): { attributes: string; end: number } | null {
  let index = from;
  let depth = 0;
  let quote: string | null = null;

  while (index < text.length) {
    const c = text[index]!;
    if (quote) {
      if (c === quote) quote = null;
    } else if (c === '"' || c === "'" || c === "`") {
      quote = c;
    } else if (c === "{") {
      depth += 1;
    } else if (c === "}") {
      depth -= 1;
    } else if (c === ">" && depth === 0) {
      return { attributes: text.slice(from, index), end: index + 1 };
    }
    index += 1;
  }
  return null;
}

/** Every `<tag …>` in the markup, with its attribute text. */
function openTags(text: string, tags: string[]): { tag: string; attributes: string }[] {
  const template = markup(text);
  const found: { tag: string; attributes: string }[] = [];
  const pattern = new RegExp(`<(${tags.join("|")})(?=[\\s/>])`, "g");
  for (const match of template.matchAll(pattern)) {
    const open = openTag(template, match.index + match[0].length);
    if (open) found.push({ tag: match[1]!, attributes: open.attributes });
  }
  return found;
}

/**
 * Every `<button …>…</button>` in the markup, as `{ open, inner }`.
 *
 * Nested buttons are invalid HTML, so the content runs to the next
 * `</button>`; the *opening* tag is found by {@link openTag} for the reason
 * documented there.
 */
function buttons(text: string): { open: string; inner: string }[] {
  const template = markup(text);
  const found: { open: string; inner: string }[] = [];
  for (const match of template.matchAll(/<button(?=[\s/>])/g)) {
    const open = openTag(template, match.index + match[0].length);
    if (!open) continue;
    const close = template.indexOf("</button>", open.end);
    found.push({
      open: open.attributes,
      inner: close === -1 ? "" : template.slice(open.end, close),
    });
  }
  return found;
}

/**
 * A button whose only content is an icon has no accessible name unless it is
 * given one — and a screen reader then announces it as "button", which is the
 * single most common way a working control becomes unusable.
 *
 * "Only an icon" means: with every `<svg>` removed, nothing but whitespace is
 * left. A `{#if}` around a label counts as content, deliberately — a button
 * that is sometimes labelled is not the case this rule is about, and treating
 * it as one would push people towards a redundant `aria-label` that overrides
 * the visible text.
 */
test("every icon-only button has an accessible name", () => {
  const offenders: string[] = [];

  for (const file of components()) {
    const text = readFileSync(file, "utf8");
    for (const { open, inner } of buttons(text)) {
      const withoutIcons = inner.replace(/<svg\b[\s\S]*?<\/svg>/g, "").trim();
      if (withoutIcons !== "") continue;
      const named = /\baria-label(?:ledby)?\s*=/.test(open) || /\btitle\s*=/.test(open);
      if (!named) offenders.push(`${file.slice(ROOT.length)}: <button${open.trim()}>`);
    }
  }

  expect(offenders, "an icon-only button announces as \"button\" and nothing else").toEqual([]);
});

/**
 * A decorative `<svg>` inside a labelled control is announced twice without
 * this: once as the control's name and once as an unnamed graphic.
 */
test("decorative icons are hidden from the accessibility tree", () => {
  const offenders: string[] = [];

  for (const file of components()) {
    const text = readFileSync(file, "utf8");
    for (const { attributes } of openTags(text, ["svg"])) {
      const described =
        /\baria-hidden\s*=/.test(attributes) ||
        /\baria-label(?:ledby)?\s*=/.test(attributes) ||
        /\brole\s*=\s*["']img["']/.test(attributes);
      if (!described) offenders.push(`${file.slice(ROOT.length)}: <svg ${attributes.trim()}>`);
    }
  }

  expect(offenders).toEqual([]);
});

/**
 * A form control with no label is a box a screen reader reads as "edit text",
 * and the sources cockpit is almost entirely form controls.
 *
 * The accepted forms are the ones that actually work: a wrapping `<label>`, a
 * `for`/`id` pair, an `aria-label`, or an `aria-labelledby`. Because a
 * `for`/`id` pair spans two elements, the check is per file rather than per
 * element: a control carrying an `id` is taken as labelled when some `<label
 * for=…>` exists in the same component.
 */
test("every text input, select and textarea can be named", () => {
  const offenders: string[] = [];

  for (const file of components()) {
    const text = readFileSync(file, "utf8");
    const template = markup(text);
    const hasLabelFor = /<label\b[^>]*\bfor\s*=/.test(template);

    for (const { tag, attributes } of openTags(text, ["input", "select", "textarea"])) {
      // A checkbox or radio is conventionally wrapped by its label, and this
      // codebase does wrap them; a wrapping label gives the name without an
      // `id`, and there is no reliable way to see the wrapper from here.
      if (/\btype\s*=\s*["'](checkbox|radio|hidden)["']/.test(attributes)) continue;
      const named =
        /\baria-label(?:ledby)?\s*=/.test(attributes) ||
        (/\bid\s*=/.test(attributes) && hasLabelFor);
      if (!named) offenders.push(`${file.slice(ROOT.length)}: <${tag} ${attributes.trim()}>`);
    }
  }

  expect(offenders, "an unlabelled field announces as \"edit text\"").toEqual([]);
});

/** The scans have to be looking at something. */
test("the scan reaches the whole app", () => {
  const files = components();
  expect(files.length).toBeGreaterThan(20);
  const all = files.map((file) => readFileSync(file, "utf8")).join("\n");
  expect(buttons(all).length, "no buttons were found at all").toBeGreaterThan(30);
});
