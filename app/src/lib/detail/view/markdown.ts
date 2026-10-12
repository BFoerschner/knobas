/**
 * Markdown in, blocks out: what a text block holding Markdown (a README,
 * #663) is drawn from.
 *
 * # Why blocks and not HTML
 *
 * `{@html}` is forbidden app-wide (`shell/house-rules.test.ts`, gotcha 7), and
 * ADR-0020 rule 5 says a text block is source text, interpolated and never
 * rendered as markup. So this reads the Markdown's *structure* -- headings,
 * paragraphs, lists, code, quotes, rules, tables -- and `MarkdownText.svelte`
 * draws each piece with an element it names itself. Every string stays a
 * string: HTML a README embeds arrives on screen as the characters it is made
 * of, which is the honest drawing of it.
 *
 * # What it reads, and what it does not
 *
 * The Markdown READMEs are written in, not the whole of CommonMark: ATX and
 * setext headings, fenced and indented code, bullet and numbered lists
 * (nesting by indentation), block quotes, thematic breaks, GFM tables (kept
 * as their own aligned lines, drawn monospace), and inline code, strong,
 * emphasis, links and images. An image is its alt text: the webview has no
 * credential for the source's files. Anything else reads as text, which loses
 * formatting and never words.
 */

/** A run of text inside a block. */
export type Inline =
  | { kind: "text"; text: string }
  | { kind: "code"; text: string }
  | { kind: "strong"; text: string }
  | { kind: "em"; text: string }
  /** `href` is the source's; whether it is opened is the component's call. */
  | { kind: "link"; text: string; href: string }
  /** An image, as its alt text. */
  | { kind: "image"; text: string };

export interface ListItem {
  /** How far it is nested: 0 for a top-level item. */
  depth: number;
  inline: Inline[];
}

export type MdBlock =
  | { kind: "heading"; level: 1 | 2 | 3 | 4 | 5 | 6; inline: Inline[] }
  | { kind: "paragraph"; inline: Inline[] }
  | { kind: "list"; ordered: boolean; start: number; items: ListItem[] }
  | { kind: "code"; text: string }
  | { kind: "quote"; inline: Inline[] }
  | { kind: "rule" }
  /** A table's lines, verbatim. */
  | { kind: "table"; text: string };

const FENCE = /^ {0,3}(`{3,}|~{3,})/;
const ATX = /^ {0,3}(#{1,6})(?:[ \t]+(.*?))?(?:[ \t]+#+)?[ \t]*$/;
const SETEXT_1 = /^ {0,3}=+[ \t]*$/;
const SETEXT_2 = /^ {0,3}-+[ \t]*$/;
const RULE = /^ {0,3}([-*_])(?:[ \t]*\1){2,}[ \t]*$/;
const ITEM = /^([ \t]*)([-*+]|\d{1,9}[.)])[ \t]+(.*)$/;
const QUOTE = /^ {0,3}>[ \t]?(.*)$/;
const TABLE_RULE = /^[ \t]*\|?[ \t]*:?-+:?[ \t]*(\|[ \t]*:?-+:?[ \t]*)*\|?[ \t]*$/;
const INDENTED = /^(?: {4}|\t)(.*)$/;

const blank = (line: string) => line.trim() === "";

/** Columns of leading whitespace, a tab counting four. */
function indentOf(space: string): number {
  return [...space].reduce((n, c) => n + (c === "\t" ? 4 : 1), 0);
}

/** Parse `source` into the blocks it is made of. */
export function parseMarkdown(source: string): MdBlock[] {
  const lines = source.replace(/\r\n?/g, "\n").split("\n");
  const blocks: MdBlock[] = [];
  let paragraph: string[] = [];

  const flush = () => {
    if (paragraph.length > 0) {
      blocks.push({ kind: "paragraph", inline: inline(paragraph.join(" ")) });
      paragraph = [];
    }
  };

  let at = 0;
  while (at < lines.length) {
    const line = lines[at]!;

    if (blank(line)) {
      flush();
      at += 1;
      continue;
    }

    const fence = FENCE.exec(line);
    if (fence) {
      flush();
      const marker = fence[1]!;
      const body: string[] = [];
      at += 1;
      while (at < lines.length) {
        const close = lines[at]!.trim();
        if (close.startsWith(marker[0]!.repeat(marker.length)) && /^[`~]+$/.test(close)) break;
        body.push(lines[at]!);
        at += 1;
      }
      blocks.push({ kind: "code", text: body.join("\n") });
      at += 1;
      continue;
    }

    // A setext underline turns the paragraph above it into a heading. Tested
    // before the rule: `---` under text is a heading, not a break.
    if (paragraph.length > 0 && (SETEXT_1.test(line) || SETEXT_2.test(line))) {
      const level = SETEXT_1.test(line) ? 1 : 2;
      blocks.push({ kind: "heading", level, inline: inline(paragraph.join(" ")) });
      paragraph = [];
      at += 1;
      continue;
    }

    const atx = ATX.exec(line);
    if (atx) {
      flush();
      const level = atx[1]!.length as 1 | 2 | 3 | 4 | 5 | 6;
      blocks.push({ kind: "heading", level, inline: inline((atx[2] ?? "").trim()) });
      at += 1;
      continue;
    }

    if (RULE.test(line)) {
      flush();
      blocks.push({ kind: "rule" });
      at += 1;
      continue;
    }

    if (QUOTE.test(line)) {
      flush();
      const quoted: string[] = [];
      while (at < lines.length && QUOTE.test(lines[at]!)) {
        quoted.push(QUOTE.exec(lines[at]!)![1]!);
        at += 1;
      }
      blocks.push({ kind: "quote", inline: inline(quoted.filter((q) => !blank(q)).join(" ")) });
      continue;
    }

    if (line.includes("|") && at + 1 < lines.length && TABLE_RULE.test(lines[at + 1]!) && lines[at + 1]!.includes("-")) {
      flush();
      const rows: string[] = [];
      while (at < lines.length && !blank(lines[at]!) && lines[at]!.includes("|")) {
        rows.push(lines[at]!.trim());
        at += 1;
      }
      blocks.push({ kind: "table", text: rows.join("\n") });
      continue;
    }

    const item = ITEM.exec(line);
    if (item) {
      flush();
      const ordered = /\d/.test(item[2]!);
      const start = ordered ? Number.parseInt(item[2]!, 10) : 1;
      const base = indentOf(item[1]!);
      const items: { depth: number; raw: string }[] = [];
      while (at < lines.length) {
        const current = lines[at]!;
        const next = ITEM.exec(current);
        const last = items.at(-1);
        if (next) {
          // A marker of the other family at the top level starts a new list.
          if (indentOf(next[1]!) <= base && /\d/.test(next[2]!) !== ordered) break;
          const depth = Math.min(4, Math.floor(Math.max(0, indentOf(next[1]!) - base) / 2));
          items.push({ depth, raw: next[3]! });
        } else if (last && !blank(current) && /^[ \t]+/.test(current)) {
          last.raw = `${last.raw} ${current.trim()}`;
        } else {
          break;
        }
        at += 1;
      }
      blocks.push({
        kind: "list",
        ordered,
        start,
        items: items.map(({ depth, raw }) => ({ depth, inline: inline(raw) })),
      });
      continue;
    }

    if (paragraph.length === 0 && INDENTED.test(line)) {
      const body: string[] = [];
      while (at < lines.length && (INDENTED.test(lines[at]!) || blank(lines[at]!))) {
        body.push(blank(lines[at]!) ? "" : INDENTED.exec(lines[at]!)![1]!);
        at += 1;
      }
      while (body.length > 0 && body.at(-1) === "") body.pop();
      blocks.push({ kind: "code", text: body.join("\n") });
      continue;
    }

    paragraph.push(line.trim());
    at += 1;
  }
  flush();
  return blocks;
}

/**
 * A link's or an image's `(target "title")`. The target may hold one level of
 * balanced parentheses (`Foo_(bar)`); the title runs to the closing paren.
 * No piece can contain a bracket or a paren it is not looking for, so a scan
 * that fails stops at the next one instead of at the end of the text: with
 * `[^\]]*` and `[^)]*` a 100,000-character README of unclosed `![` or `[a](`
 * took seconds to minutes to read (#663 review).
 */
const TARGET = String.raw`\(((?:[^()\s]|\([^()\s]*\))*)(?:\s[^()]*)?\)`;
const SKIP_TARGET = String.raw`\((?:[^()\s]|\([^()\s]*\))*(?:\s[^()]*)?\)`;

/**
 * Code first, so nothing inside backticks is read as anything else; then an
 * image inside a link (a README's badge, `[![alt](svg)](href)`, which is a
 * link labelled with the alt text); then images before links (an image is a
 * link with a `!`); then strong before emphasis. Underscore emphasis needs a
 * non-word character on both sides, so `snake_case_name` stays text.
 */
const INLINE = new RegExp(
  [
    "(`+)([^`]|[^`][\\s\\S]*?[^`])\\1(?!`)",
    String.raw`\[!\[([^\[\]]*)\]${SKIP_TARGET}\]${TARGET}`,
    String.raw`!\[([^\[\]]*)\]${SKIP_TARGET}`,
    String.raw`\[([^\[\]]+)\]${TARGET}`,
    String.raw`\*\*([^*]+?)\*\*|__([^_]+?)__`,
    String.raw`\*([^*\s][^*]*?)\*|(?<![\p{L}\p{N}_])_([^_\s][^_]*?)_(?![\p{L}\p{N}_])`,
  ].join("|"),
  "gu",
);

/** Read one block's text into runs. */
export function inline(text: string): Inline[] {
  const runs: Inline[] = [];
  let at = 0;
  const push = (run: Inline) => {
    const last = runs.at(-1);
    if (run.kind === "text" && last?.kind === "text") last.text += run.text;
    else runs.push(run);
  };
  for (const match of text.matchAll(INLINE)) {
    const index = match.index;
    if (index > at) push({ kind: "text", text: text.slice(at, index) });
    const [, ticks, code, badge, badgeHref, alt, label, href, strong1, strong2, em1, em2] = match;
    if (ticks !== undefined) push({ kind: "code", text: code!.trim() === "" ? code! : code!.trim() });
    else if (badge !== undefined) push({ kind: "link", text: badge || "image", href: badgeHref ?? "" });
    else if (alt !== undefined) push({ kind: "image", text: alt });
    else if (label !== undefined) push({ kind: "link", text: label, href: href ?? "" });
    else if (strong1 !== undefined || strong2 !== undefined)
      push({ kind: "strong", text: (strong1 ?? strong2)! });
    else push({ kind: "em", text: (em1 ?? em2)! });
    at = index + match[0].length;
  }
  if (at < text.length) push({ kind: "text", text: text.slice(at) });
  return runs;
}
