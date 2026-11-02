/**
 * Jira's wiki markup as Markdown (#2308), so a Jira ticket's description and
 * comments draw through the one Markdown component (`TfMarkdown`, #2261)
 * like every other source's prose. The source's text stays as Jira wrote it
 * in the mirror; this runs where it is drawn.
 *
 * What it reads: `h1.`–`h6.`, `#` and `*` lists (nested, mixed), `bq.`,
 * `{quote}`, `{code[:lang]}` and `{noformat}` blocks (a `{code:mermaid}`
 * block becomes a `mermaid` fence, which the registry draws), `||` tables,
 * `----`, `*bold*`, `_italic_`, `-struck-`, `{{mono}}`, `[text|url]`,
 * `[url]`, `[~account]` (as `@account`, the mention spelling the shell marks)
 * and `\\`. `{color}` and `{panel}` tags are dropped, their text kept.
 * `!name.ext|…!` is an image when the ticket's attachment list holds a file
 * of that name (`attachment:<id>`, which the Markdown component fills through
 * the surface, #2406); `(/)` and `(x)` at a list item's start are a done and
 * an open task box.
 * Anything else stays as written. A single line break is a line break, as
 * Jira draws it, and raw HTML is text, as Jira treats it: the Markdown
 * engine would otherwise parse it.
 */
import type { Markup } from "../ipc/sources";

/** A ticket's prose as its source wrote it, and how (#2308): what a ticket reader draws. */
export interface Prose {
  text: string;
  markup: Markup;
}

/** The Markdown to draw `text` with, by its source's `markup`; `null` for text, which is drawn as it is. */
export function proseMarkdown(text: string, markup: Markup, attachments: readonly NamedFile[] = []): string | null {
  switch (markup) {
    case "markdown":
      return text;
    case "jira_wiki":
      return jiraWikiToMarkdown(text, attachments);
    case "text":
      return null;
  }
}

/** A ticket's attachment as the wiki's `!name!` resolves it (#2406): by its file name, to its id. */
export interface NamedFile {
  id: string;
  name: string;
}

/** A line of the output: paragraph text gets a hard break before the next paragraph line. */
interface Line {
  text: string;
  prose: boolean;
}

const BLOCK_OPEN = /^\s*\{(code|noformat)(?::([^}]*))?\}\s*(.*)$/;
const HEADING = /^\s*h([1-6])\.\s+(.*)$/;
const LIST = /^\s*([*#-]+)\s+(.*)$/;
const QUOTE_LINE = /^\s*bq\.\s+(.*)$/;
const RULE = /^\s*-{4,}\s*$/;
const TABLE = /^\s*\|/;
/** `(/)` or `(x)` opening a list item's text, with more text after it. */
const TASK = /^\((\/|x)\)\s*(?=\S)/;

export function jiraWikiToMarkdown(wiki: string, attachments: readonly NamedFile[] = []): string {
  const files = new Map(attachments.map((file) => [file.name, file.id]));
  const inline = (text: string) => inlineMarkup(text, files);
  const input = wiki.replace(/\r\n?/g, "\n").split("\n");
  const out: Line[] = [];
  let quoted = false;
  let tableRows = 0;

  const push = (text: string, prose = false) => out.push({ text: quoted ? `> ${text}` : text, prose });

  for (let i = 0; i < input.length; i++) {
    const raw = input[i]!;

    const block = BLOCK_OPEN.exec(raw);
    if (block) {
      const tag = block[1]!;
      const lang = tag === "code" ? language(block[2] ?? "") : "";
      const close = `{${tag}}`;
      const body: string[] = [];
      let rest = block[3] ?? "";
      let after = "";
      // `{code}x{code}` on one line, or the body starting on the opening line.
      while (true) {
        const end = rest.indexOf(close);
        if (end >= 0) {
          if (end > 0) body.push(rest.slice(0, end));
          after = rest.slice(end + close.length);
          break;
        }
        if (rest !== "") body.push(rest);
        if (++i >= input.length) break;
        rest = input[i]!;
      }
      push("```" + lang);
      for (const line of body) push(line);
      push("```");
      if (after.trim() !== "") push(inline(literal(after.trim())), true);
      tableRows = 0;
      continue;
    }

    let line = raw.replace(/\{color(?::[^}]*)?\}/g, "").replace(/\{panel(?::[^}]*)?\}/g, "");

    if (/\{quote\}/.test(line)) {
      const parts = line.split("{quote}");
      for (let p = 0; p < parts.length; p++) {
        if (p > 0) quoted = !quoted;
        const part = parts[p]!;
        if (part.trim() !== "") push(inline(part), true);
      }
      tableRows = 0;
      continue;
    }

    if (TABLE.test(line)) {
      const header = /^\s*\|\|/.test(line);
      const cells = line
        .trim()
        .replace(/^\|\|?/, "")
        .replace(/\|\|?$/, "")
        .split(/\|\|?/)
        .map((cell) => inline(cell.trim()));
      push(`| ${cells.join(" | ")} |`);
      if (tableRows === 0) push(`| ${cells.map(() => "---").join(" | ")} |`);
      tableRows = header && tableRows === 0 ? 1 : tableRows + 1;
      continue;
    }
    tableRows = 0;

    if (line.trim() === "") {
      push("");
      continue;
    }
    if (RULE.test(line)) {
      push("---");
      continue;
    }
    const heading = HEADING.exec(line);
    if (heading) {
      push(`${"#".repeat(Number(heading[1]))} ${inline(heading[2]!)}`);
      continue;
    }
    const bq = QUOTE_LINE.exec(line);
    if (bq) {
      push(`> ${inline(bq[1]!)}`);
      continue;
    }
    const list = LIST.exec(line);
    if (list) {
      const marks = list[1]!;
      const indent = [...marks.slice(0, -1)].map((m) => (m === "#" ? "   " : "  ")).join("");
      let item = list[2]!;
      const task = TASK.exec(item);
      if (task) item = `${task[1] === "/" ? "[x]" : "[ ]"} ${item.slice(task[0].length)}`;
      push(`${indent}${marks.endsWith("#") ? "1." : "-"} ${inline(item)}`);
      continue;
    }
    line = literal(line).replace(/\\\\\s*/g, "  \n");
    push(inline(line), true);
  }

  return out
    .map((line, at) => (line.prose && out[at + 1]?.prose ? `${line.text}  ` : line.text))
    .join("\n");
}

/**
 * A paragraph line's start that Markdown reads as a block and Jira as text
 * (`1. `, `> `, `+ `), escaped so it stays text.
 */
function literal(line: string): string {
  return line.replace(/^(\s*)(\d+)([.)]\s)/, "$1$2\\$3").replace(/^(\s*)([>+]\s)/, "$1\\$2");
}

/** A `{code:…}` parameter list's language: its first parameter when that is not a `key=value`. */
function language(params: string): string {
  const first = params.split("|")[0]!.trim();
  return first.includes("=") ? "" : first;
}

/** One line's inline markup, `{{mono}}` spans kept verbatim. */
function inlineMarkup(text: string, files: ReadonlyMap<string, string>): string {
  return text
    .split(/(\{\{.*?\}\})/)
    .map((part) => (part.startsWith("{{") && part.endsWith("}}") ? mono(part.slice(2, -2)) : images(part, files)))
    .join("");
}

/** `!name!` or `!name|params!`: no space just inside either `!`. */
const IMAGE = /!(?=[^\s!|])([^!|\n]*[^\s!|])(?:\|[^!\n]*)?!/g;

/** A text's `!name!` images whose name the attachment list holds, as Markdown images; the rest, and anything else, as words. */
function images(text: string, files: ReadonlyMap<string, string>): string {
  let out = "";
  let from = 0;
  for (const match of text.matchAll(IMAGE)) {
    const id = files.get(match[1]!);
    if (id === undefined) continue;
    out += words(text.slice(from, match.index)) + `![${altText(match[1]!)}](attachment:${encodeURIComponent(id)})`;
    from = match.index + match[0].length;
  }
  return out + words(text.slice(from));
}

/** A file name as an image's alt text: the characters Markdown reads there escaped. */
function altText(name: string): string {
  return name.replace(/[\\[\]*_`<>]/g, (c) => (c === "<" ? "&lt;" : `\\${c}`));
}

function mono(code: string): string {
  const fence = code.includes("`") ? "``" : "`";
  return `${fence}${code}${fence}`;
}

/** A delimited span whose ends touch no word character: `*bold*` but not `2*3*4`. */
const span = (mark: string) => new RegExp(`(^|[^\\w${mark}])\\${mark}(?=\\S)(.*?\\S)\\${mark}(?![\\w${mark}])`, "g");
const BOLD = span("*");
const ITALIC = span("_");
const STRUCK = span("-");

function words(text: string): string {
  return text
    .replace(/</g, "&lt;")
    .replace(/\[~([^\]|]+)\]/g, "@$1")
    .replace(/\[([^\]|]+)\|([^\]]+)\]/g, (_, label: string, url: string) => `[${label}](${url.trim()})`)
    .replace(/\[((?:https?|mailto):[^\]\s]+)\]/g, "<$1>")
    .replace(BOLD, "$1**$2**")
    .replace(ITALIC, "$1*$2*")
    .replace(STRUCK, "$1~~$2~~");
}
