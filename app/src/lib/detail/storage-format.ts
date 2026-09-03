/**
 * Confluence **storage format** in, a tree of allow-listed nodes out (#285).
 *
 * The storage format is XHTML with Confluence's own `ac:` and `ri:` elements
 * mixed in, and the adapter keeps it verbatim in the payload (§3a) precisely
 * so this can re-project it without re-syncing. What the reader wants is the
 * page as it looks in Confluence: its headings, its lists and its tables, with
 * every macro shown as a labelled placeholder so they can see why a section
 * will refuse editing (spec #272, roadmap M3.2).
 *
 * # Why a tree and not sanitized HTML
 *
 * The obvious shape is "escape and allow-list into an HTML string, then
 * `{@html}` it". **`{@html}` is forbidden in this codebase**, and the rule is
 * enforced rather than remembered: `shell/house-rules.test.ts`'s *"no {@html}
 * — source text is untrusted (gotcha 7)"* scans every `.svelte` file. The rule
 * is right, too — `tauri dev` attaches no CSP at all, so an `{@html}` that
 * turns out to be reachable is invisible until a release build.
 *
 * So this produces **nodes**, and `StorageBody.svelte` renders them with real
 * elements. That is strictly stronger than a sanitized string: text arrives as
 * a text node, so nothing in a page's prose can be interpreted as markup at
 * any point in the pipeline, and an element exists only because this module
 * named it. There is no string for a later refactor to interpolate.
 *
 * # What can reach the DOM
 *
 * * The tags in {@link ALLOWED} and nothing else. An element that is not in it
 *   is **unwrapped** — its children survive, the element does not — so a
 *   `<span class="x">` costs its wrapper and no words.
 * * `href` on an `<a>`, and no other attribute on anything. Not a filter over
 *   attributes: there is no code path that carries one. `style`, `class`,
 *   `on*` and `srcdoc` are therefore unrepresentable rather than removed.
 * * Nothing at all from {@link DROPPED} — `script`, `style`, `iframe` and
 *   their kin are skipped *with their content*, so a `<script>` body is not
 *   even rendered as visible text.
 * * A `href` that is not `http:` or `https:` is dropped and the link renders
 *   as its words. `javascript:` and `data:` are the reason; the check is on
 *   the parsed scheme, after entity decoding, so `java&#115;cript:` does not
 *   slip past a substring test.
 *
 * # Order matters, and it is the security-relevant half
 *
 * Tags are read **before** entities are decoded, the same order and for the
 * same reason as `knobas_source_confluence::storage`: `&lt;script&gt;` in the
 * storage format is the *text* `<script>` and must come out as that text.
 * Decoding first would manufacture a tag out of somebody's prose.
 */

/** Which adapter's payloads carry a storage format (see {@link storageBodyOf}). */
export const STORAGE_FORMAT_ADAPTER = "confluence";

/** One node of a rendered page body. */
export type StorageNode =
  /** Somebody's words, entity-decoded. Rendered as a text node. */
  | { kind: "text"; text: string }
  /** An allow-listed element. `href` is set on `a` alone, and only when safe. */
  | {
      kind: "element";
      tag: AllowedTag;
      href: string | null;
      children: StorageNode[];
    }
  /**
   * A macro, in place of the element it replaces — content and all.
   *
   * The reader is told *what* it is rather than shown a hole, because that is
   * the sentence spec #272 asks for: a section holding one of these will
   * refuse a knobas-side edit, and a blank would leave that inexplicable.
   */
  | { kind: "macro"; label: string };

/**
 * The elements that may reach the DOM.
 *
 * Confluence's own body vocabulary, minus everything that carries behaviour or
 * remote content. `img` is **not** here: a page's images are `ac:image`
 * elements pointing at attachments this app has no credential to fetch from
 * the webview (`default-src 'self'`), so they become placeholders like any
 * other macro rather than broken boxes.
 */
const ALLOWED = [
  "h1",
  "h2",
  "h3",
  "h4",
  "h5",
  "h6",
  "p",
  "br",
  "hr",
  "ul",
  "ol",
  "li",
  "strong",
  "em",
  "b",
  "i",
  "u",
  "s",
  "del",
  "ins",
  "sub",
  "sup",
  "code",
  "pre",
  "blockquote",
  "a",
  "table",
  "thead",
  "tbody",
  "tfoot",
  "tr",
  "th",
  "td",
] as const;

export type AllowedTag = (typeof ALLOWED)[number];

const ALLOWED_SET: ReadonlySet<string> = new Set<string>(ALLOWED);

/** Elements with no closing tag, so a frame is never opened for them. */
const VOID_TAGS: ReadonlySet<string> = new Set(["br", "hr"]);

/**
 * Elements skipped **with their content**.
 *
 * Unwrapping a `<script>` would be safe — its body would arrive as a text node
 * and text cannot execute — but it would put `alert(1)` on screen as if the
 * author had written it. Content that is not prose is not shown as prose.
 */
const DROPPED: ReadonlySet<string> = new Set([
  "script",
  "style",
  "iframe",
  "object",
  "embed",
  "template",
  "noscript",
  "svg",
  "math",
  "form",
  "input",
  "textarea",
  "select",
  "option",
  "button",
  "link",
  "meta",
  "base",
  "head",
  "title",
  "img",
  "audio",
  "video",
  "source",
  "canvas",
  "applet",
  "frame",
  "frameset",
  "portal",
]);

/**
 * The `ac:` elements that are page **structure** rather than content.
 *
 * A Confluence page written in the two-column layout wraps its *whole* body in
 * these, so replacing them with a placeholder would replace the page with the
 * word "layout". They are unwrapped instead: the columns are lost, every word
 * survives. Everything else beginning `ac:` is a placeholder.
 */
const TRANSPARENT_AC: ReadonlySet<string> = new Set([
  "ac:layout",
  "ac:layout-section",
  "ac:layout-cell",
]);

/**
 * How deep the rendered tree may nest before a wrapper is dropped.
 *
 * **`StorageBody.svelte` recurses**, one component instance per level, so a
 * body's nesting depth is a JavaScript stack depth at render time — and a page
 * body is untrusted input. Measured: a body of ~150 nested `<b>` elements
 * overflows the stack under the test runner, and the throw happens inside
 * Svelte's flush where nothing catches it, so one adversarial page would take
 * the window down rather than merely render badly. The parser itself is
 * iterative and has no such limit; this cap is the renderer's.
 *
 * Past the cap an allow-listed element is **unwrapped** — the same treatment a
 * `<span>` gets, so every word survives and only structure nobody wrote on
 * purpose is lost. 64 is far above anything prose reaches (a table inside a
 * list inside a quote is about six) and far below where the stack complains.
 */
const MAX_ELEMENT_DEPTH = 64;

/** Containers in which only elements are legal, so stray whitespace is noise. */
const ELEMENT_ONLY: ReadonlySet<string> = new Set([
  "ul",
  "ol",
  "table",
  "thead",
  "tbody",
  "tfoot",
  "tr",
]);

/**
 * The page body, as nodes.
 *
 * Total: every input produces a tree, malformed markup included. A body that
 * would not parse is a page a reader cannot read, and the storage format is
 * whatever a wiki has accumulated over ten years — so an unclosed tag closes
 * itself at the end and a stray `<` is a word.
 */
export function parseStorageFormat(storage: string): StorageNode[] {
  const root: StorageNode[] = [];
  /** Open elements, innermost last. `node === null` is an unwrapped frame. */
  const stack: {
    name: string;
    node: ElementNode | null;
    children: StorageNode[];
  }[] = [];
  const into = () =>
    stack.length > 0 ? stack[stack.length - 1]!.children : root;
  /** Open *element* frames — the depth of the tree the renderer will recurse. */
  let depth = 0;

  let at = 0;
  while (at < storage.length) {
    const open = storage.indexOf("<", at);
    if (open === -1) {
      pushText(storage.slice(at));
      break;
    }
    if (open > at) pushText(storage.slice(at, open));

    if (storage.startsWith("<![CDATA[", open)) {
      // A code macro's body. Verbatim, because it is inside CDATA precisely so
      // that it may contain `<` — and never entity-decoded, for the same
      // reason. It is only reachable when a caller parses a fragment that is
      // already inside a macro; a whole page's CDATA sits under the
      // `ac:structured-macro` this replaces with a placeholder.
      const end = storage.indexOf("]]>", open);
      const inside = storage.slice(open + 9, end === -1 ? storage.length : end);
      if (inside !== "") into().push({ kind: "text", text: inside });
      at = end === -1 ? storage.length : end + 3;
      continue;
    }
    if (storage.startsWith("<!--", open)) {
      const end = storage.indexOf("-->", open);
      at = end === -1 ? storage.length : end + 3;
      continue;
    }

    const tag = readTag(storage, open);
    if (!tag) {
      // A `<` that never closes: the rest of the document is text.
      pushText(storage.slice(open));
      break;
    }
    if (tag.name === "") {
      // A `<` that closes but names no element -- `3 < 4` in somebody's prose,
      // where the `>` a scan finds belongs to the *next* tag. Only the bracket
      // is text; the tag after it is still a tag, which is what keeps
      // `<p>3 < 4</p>` one paragraph rather than a paragraph and a literal
      // `</p>`.
      pushText("<");
      at = open + 1;
      continue;
    }
    at = tag.after;

    if (tag.closing) {
      close(tag.name);
      continue;
    }
    if (tag.name.startsWith("ac:") && !TRANSPARENT_AC.has(tag.name)) {
      into().push({ kind: "macro", label: macroLabel(tag) });
      if (!tag.selfClosing) at = skipSubtree(storage, at, tag.name);
      continue;
    }
    // `DROPPED` is consulted **before** `ALLOWED`, deliberately: the two lists
    // are disjoint today, and this order is what keeps a future edit that adds
    // a name to both from turning a skip into a rendered element. Refusal wins
    // over permission.
    if (DROPPED.has(tag.name)) {
      if (!tag.selfClosing) at = skipSubtree(storage, at, tag.name);
      continue;
    }
    if (ALLOWED_SET.has(tag.name) && depth < MAX_ELEMENT_DEPTH) {
      const allowed = tag.name as AllowedTag;
      if (VOID_TAGS.has(allowed) || tag.selfClosing) {
        into().push({
          kind: "element",
          tag: allowed,
          href: null,
          children: [],
        });
        continue;
      }
      const node: ElementNode = {
        kind: "element",
        tag: allowed,
        href: allowed === "a" ? safeHref(tag.attributes["href"]) : null,
        children: [],
      };
      stack.push({ name: tag.name, node, children: node.children });
      depth += 1;
      continue;
    }
    // Anything else — a `<span>`, a `<div>`, an `ac:layout`, a namespace this
    // app has never heard of, and an allow-listed element past
    // {@link MAX_ELEMENT_DEPTH} — is unwrapped: the wrapper goes, the words
    // stay. An unwrapped frame costs no render recursion, so nesting below the
    // cap is bounded however deep the markup goes.
    if (!tag.selfClosing)
      stack.push({ name: tag.name, node: null, children: [] });
  }

  // An unclosed tag closes itself at the end of the document.
  while (stack.length > 0) close(stack[stack.length - 1]!.name);
  return root;

  function pushText(raw: string) {
    const text = inPre()
      ? decodeEntities(raw)
      : decodeEntities(raw).replace(/\s+/g, " ");
    if (text === "") return;
    // Whitespace between a `<tr>` and its `<td>` is indentation, not prose.
    if (
      text.trim() === "" &&
      ELEMENT_ONLY.has(stack[stack.length - 1]?.name ?? "")
    )
      return;
    into().push({ kind: "text", text });
  }

  function inPre(): boolean {
    return stack.some((frame) => frame.name === "pre");
  }

  /**
   * Close `name`, and everything left open inside it.
   *
   * A close tag with no open frame is ignored rather than being an error:
   * `</b>` on its own is something a wiki body really contains, and the
   * alternative is a page that renders as one long refusal.
   */
  function close(name: string) {
    // A reverse scan rather than `findLastIndex`, which this build's
    // `lib` target does not have: the *innermost* frame of that name is the
    // one being closed, so `<b><b>x</b></b>` nests rather than unwinding both.
    let found = -1;
    for (let index = stack.length - 1; index >= 0; index -= 1) {
      if (stack[index]!.name === name) {
        found = index;
        break;
      }
    }
    if (found === -1) return;
    while (stack.length > found) {
      const frame = stack.pop()!;
      const parent =
        stack.length > 0 ? stack[stack.length - 1]!.children : root;
      if (frame.node) {
        parent.push(frame.node);
        depth -= 1;
      } else {
        // One at a time rather than a spread: a frame's children are a page's,
        // and `push(...huge)` is an argument list, which has a limit.
        for (const child of frame.children) parent.push(child);
      }
    }
  }
}

type ElementNode = Extract<StorageNode, { kind: "element" }>;

/**
 * One parsed tag.
 *
 * Exported with {@link readTag} for `page-sections.ts`, which scans the *same*
 * markup for heading boundaries and must read a tag the same way this parser
 * does -- quoted attribute values and all. A second scanner with its own idea
 * of where a tag ends is how `<img alt="a > b"/>` becomes two different
 * documents to two readers of one page.
 */
export interface Tag {
  name: string;
  closing: boolean;
  selfClosing: boolean;
  attributes: Record<string, string>;
  /** Index just past the `>`. */
  after: number;
}

/**
 * Read `<…>` starting at `open`, respecting quoted attribute values.
 *
 * `<img alt="a > b"/>` is one tag; a scan to the first `>` would cut it in
 * half and spill `b"/>` into the page as text. `null` for a `<` that never
 * closes.
 */
export function readTag(source: string, open: number): Tag | null {
  let quote: string | null = null;
  let end = -1;
  for (let at = open + 1; at < source.length; at += 1) {
    const char = source[at]!;
    if (quote !== null) {
      if (char === quote) quote = null;
      continue;
    }
    if (char === '"' || char === "'") quote = char;
    else if (char === ">") {
      end = at;
      break;
    }
  }
  if (end === -1) return null;

  const inside = source.slice(open + 1, end);
  const closing = inside.startsWith("/");
  const selfClosing = inside.endsWith("/");
  const body = inside.replace(/^\//, "").replace(/\/$/, "");
  // The empty string for a `<` that names nothing; the caller decides what
  // that means, because "not a tag" and "no closing bracket at all" are two
  // different recoveries.
  const name = (body.match(/^[^\s/>]+/)?.[0] ?? "").toLowerCase();
  return {
    name,
    closing,
    selfClosing,
    attributes: closing ? {} : readAttributes(body.slice(name.length)),
    after: end + 1,
  };
}

/**
 * The attributes of a tag body, lowercased names to raw values.
 *
 * Read in full rather than only the two that are wanted, because "the one I
 * looked for" is how an attribute grammar gets mis-read: a value containing
 * `href=` inside it would otherwise be found by a search for `href`. Nothing
 * downstream may reach this map except by name — see the module docs.
 */
function readAttributes(rest: string): Record<string, string> {
  const attributes: Record<string, string> = {};
  const pattern = /([^\s=/>]+)\s*(?:=\s*("[^"]*"|'[^']*'|[^\s/>]+))?/g;
  for (const match of rest.matchAll(pattern)) {
    const value = match[2] ?? "";
    attributes[match[1]!.toLowerCase()] = value.replace(/^["']|["']$/g, "");
  }
  return attributes;
}

/**
 * What a macro placeholder says.
 *
 * The macro's **own name** where Confluence gave one — `ac:name` on an
 * `ac:structured-macro`, which is the word the author chose in the editor
 * (`info`, `code`, `jira`) and therefore the word they will recognise. The
 * element's local name otherwise, so `ac:image` reads *image* and `ac:link`
 * reads *link* rather than both reading "macro".
 */
function macroLabel(tag: Tag): string {
  // Decoded, because the attribute is raw: `ac:name` is the author's word and
  // `a &amp; b` is not the word they typed. Safe to decode here and nowhere
  // else in this function's output, since a label leaves as a `text` node --
  // there is no path by which a decoded `<` becomes an element (#286).
  const named = decodeEntities(tag.attributes["ac:name"] ?? "").trim();
  if (named !== "") return `${named} macro`;
  return tag.name.slice("ac:".length).replace(/-/g, " ");
}

/**
 * Index just past the close tag matching an element opened at `from`.
 *
 * Depth-counted, so a macro nested inside a macro's rich-text body does not
 * end the outer one early. An unclosed element swallows the rest of the
 * document, which is the safe direction: the alternative is a macro's innards
 * spilling into the page as prose.
 */
function skipSubtree(source: string, from: number, name: string): number {
  let depth = 1;
  let at = from;
  while (at < source.length) {
    const open = source.indexOf("<", at);
    if (open === -1) return source.length;
    if (source.startsWith("<![CDATA[", open)) {
      const end = source.indexOf("]]>", open);
      at = end === -1 ? source.length : end + 3;
      continue;
    }
    if (source.startsWith("<!--", open)) {
      const end = source.indexOf("-->", open);
      at = end === -1 ? source.length : end + 3;
      continue;
    }
    const tag = readTag(source, open);
    if (!tag) return source.length;
    if (tag.name === "") {
      // A bare `<` in the macro's own prose. Stepping past the `>` a scan
      // found would step past the macro's *close* tag with it, and the rest
      // of the page would vanish into the macro.
      at = open + 1;
      continue;
    }
    at = tag.after;
    if (tag.name !== name || tag.selfClosing) continue;
    if (tag.closing) {
      depth -= 1;
      if (depth === 0) return at;
    } else depth += 1;
  }
  return source.length;
}

/**
 * Characters stripped from a URL before its scheme is read.
 *
 * Every ASCII control character and the space. `java script:` and
 * `\njavascript:` are URLs browsers have historically been willing to follow,
 * and both would pass a naive `startsWith("javascript:")` test.
 */
const URL_NOISE = /[\u0000-\u0020]/g;

/**
 * A link's target, or `null` where it may not be followed.
 *
 * `http:` and `https:` only, matching what `openExternal` will actually accept
 * — the app hands a body link to the OS browser exactly as *Open in browser*
 * hands over an item's own URL, and a scheme the backend refuses would be a
 * link that looks live and does nothing. Everything else is `null` and renders
 * as its words: `javascript:` and `data:` are the reason the check exists, and
 * a relative `/display/ENG/Home` is dropped for a duller one — it is a path on
 * a wiki whose host is not knowable from here.
 *
 * The scheme is read **after** entity decoding and after control characters
 * are stripped, so nothing reaches a substring test in disguise.
 */
function safeHref(raw: string | undefined): string | null {
  if (raw === undefined) return null;
  const href = decodeEntities(raw).replace(URL_NOISE, "");
  if (href === "") return null;
  const scheme = href.match(/^([A-Za-z][A-Za-z0-9+.-]*):/)?.[1]?.toLowerCase();
  return scheme === "http" || scheme === "https" ? href : null;
}

/** The named entities a storage-format body carries, plus numeric references. */
const ENTITIES: Record<string, string> = {
  amp: "&",
  lt: "<",
  gt: ">",
  quot: '"',
  apos: "'",
  nbsp: " ",
};

/**
 * XML entities, decoded.
 *
 * An entity this app does not know is left **exactly as written**, the
 * decision `knobas_source_confluence::storage` makes: inventing a character
 * for `&hellip;` would put a guess in the page, and the literal text is worse
 * only cosmetically.
 *
 * `&nbsp;` becomes U+00A0 here and an ordinary space in the Rust half, and the
 * two are right for their own jobs: this is a rendering, where the author asked
 * for a hard space, and that one feeds a full-text index nobody will ever type
 * one into.
 */
function decodeEntities(raw: string): string {
  return raw.replace(
    /&(#[Xx]?[0-9A-Fa-f]+|[A-Za-z][A-Za-z0-9]{1,10});/g,
    (whole, body: string) => {
      if (!body.startsWith("#")) return ENTITIES[body.toLowerCase()] ?? whole;
      const digits = body.slice(1);
      const code =
        digits.startsWith("x") || digits.startsWith("X")
          ? Number.parseInt(digits.slice(1), 16)
          : Number.parseInt(digits, 10);
      if (!Number.isFinite(code) || code <= 0 || code > 0x10ffff) return whole;
      try {
        return String.fromCodePoint(code);
      } catch {
        return whole;
      }
    },
  );
}

/**
 * The verbatim storage format in a page's payload, or `null`.
 *
 * A **payload read outside an adapter**, so ADR-0007's interim discipline
 * applies and is worth stating: it is *one* named read, it is gated on the
 * adapter kind rather than guessed from the shape of the payload, and it
 * misses to `null` — a page whose record carries no readable body renders the
 * normalized `body_text` the mirror already holds, which is what every other
 * kind renders. Absence, never a wrong rendering.
 *
 * The gate is `adapterKind` and not the entity kind: `page` is a word two
 * adapters may both emit, and a Gitea wiki page's body is Markdown. #277's
 * `KindPaths` cannot express this yet — it has no slot shaped like "a body in
 * this markup dialect" — so adding one would be a `crates/knobas-source/src/**`
 * change and a §10.8 conversation. When such a slot is ratified this read
 * expires into it.
 */
export function storageBodyOf(
  adapterKind: string | null | undefined,
  payload: unknown,
): string | null {
  if (adapterKind !== STORAGE_FORMAT_ADAPTER) return null;
  const value = at(at(at(payload, "body"), "storage"), "value");
  return typeof value === "string" && value.trim() !== "" ? value : null;
}

/** One comment on a page, as its record carries it. */
export interface PageComment {
  /** Confluence's own content id for the comment, so `{#each}` can key on it. */
  id: string;
  /** The comment's storage format, verbatim. */
  storage: string;
  /**
   * Who wrote it, in the source's own spelling, or `null`.
   *
   * The same rule the adapter applies to a *page*'s author
   * (`knobas_source_confluence::map`): the person who made this version, and
   * the creator for a comment nobody has edited since. Two answers to one
   * question would be worse than one that sometimes misses.
   */
  author: string | null;
  /** When it was last written, as the record spells the instant, or `null`. */
  when: string | null;
}

/**
 * A page's comments, in the order the record carries them.
 *
 * `children.comment.results`, which is where the adapter keeps them — a
 * comment is not a kind of its own, it rides in its page's payload the way a
 * Jira comment rides in its issue's (#284). Same gate and same miss direction
 * as {@link storageBodyOf}: a record with nothing readable there has no
 * comments rather than an empty section.
 */
export function pageCommentsOf(
  adapterKind: string | null | undefined,
  payload: unknown,
): PageComment[] {
  if (adapterKind !== STORAGE_FORMAT_ADAPTER) return [];
  const results = at(at(at(payload, "children"), "comment"), "results");
  if (!Array.isArray(results)) return [];
  return results.flatMap((entry, index) => {
    const storage = at(at(at(entry, "body"), "storage"), "value");
    if (typeof storage !== "string" || storage.trim() === "") return [];
    const id = at(entry, "id");
    return [
      {
        id: typeof id === "string" ? id : `comment-${index}`,
        storage,
        author:
          text(at(at(at(entry, "version"), "by"), "username")) ??
          text(at(at(at(entry, "history"), "createdBy"), "username")),
        when: text(at(at(entry, "version"), "when")),
      },
    ];
  });
}

/**
 * The **version number** a page's record stands at, or `null`.
 *
 * What a section edit is made *against*: `WriteOp::UpdatePage` carries it as
 * `base_version`, the adapter sends `base_version + 1`, and Confluence aborts
 * the write if somebody else got there first. A page whose record does not say
 * what version it is therefore gets **no edit offered** rather than an edit
 * sent against a guess -- which is this read's stated failure direction and the
 * reason it answers `null` instead of `1`.
 *
 * The same interim payload-read discipline as {@link storageBodyOf}, for the
 * same reason and with the same expiry: one named read, gated on the adapter
 * kind rather than guessed from the payload's shape, missing to `null`. #277's
 * `KindPaths` has no slot shaped like "the record's own revision number"
 * either, and adding one is a `crates/knobas-source/src/**` change with its own
 * §10.8 conversation; this read expires into it when there is one.
 *
 * An integer, and checked to be one: `version.number` arriving as a string
 * would otherwise flow into `base_version + 1` as concatenation on the wire.
 */
export function pageVersionOf(
  adapterKind: string | null | undefined,
  payload: unknown,
): number | null {
  if (adapterKind !== STORAGE_FORMAT_ADAPTER) return null;
  const value = at(at(payload, "version"), "number");
  return typeof value === "number" && Number.isSafeInteger(value) && value > 0
    ? value
    : null;
}

/** A non-blank string at that path, or `null`. Every other shape is a miss. */
function text(value: unknown): string | null {
  return typeof value === "string" && value.trim() !== "" ? value.trim() : null;
}

/** One step into an object, or `undefined`. Arrays and `null` are misses. */
function at(value: unknown, key: string): unknown {
  if (typeof value !== "object" || value === null || Array.isArray(value))
    return undefined;
  return (value as Record<string, unknown>)[key];
}
