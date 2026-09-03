/**
 * A Confluence page body, rendered (#285).
 *
 * **Every assertion here is on the DOM after mount**, and that is the point:
 * roadmap §4 gotcha 7 is about what the *webview* ends up holding, and a test
 * over the sanitizer's return value cannot witness it. A string assertion
 * would pass just as happily against a `{@html}` of the same string, which is
 * exactly the arrangement the rule forbids — so the questions asked below are
 * "is there a `<script>` element in this subtree", "is this a real `<table>`",
 * "what does the placeholder say", and never "what does the string look
 * like".
 *
 * The parser's own decisions that have no rendering — which URL schemes
 * survive, where a payload keeps its body — are asserted through the DOM too
 * wherever a rendering exists, and directly where none does.
 */
import { flushSync, mount, unmount } from "svelte";
import { expect, test } from "vitest";

import StorageBody from "./StorageBody.svelte";
import {
  pageCommentsOf,
  pageVersionOf,
  parseStorageFormat,
  storageBodyOf,
} from "./storage-format";

/** Mount a storage-format body and hand back its live DOM. */
function render(storage: string) {
  const target = document.createElement("div");
  document.body.append(target);
  const opened: string[] = [];
  const app = mount(StorageBody, {
    target,
    props: {
      nodes: parseStorageFormat(storage),
      onopenlink: (href) => opened.push(href),
    },
  });
  flushSync();
  return {
    target,
    opened,
    /** The rendered text, with template whitespace collapsed. */
    text: () => (target.textContent ?? "").replace(/\s+/g, " ").trim(),
    /** Every element in the rendered subtree, by lowercased tag name. */
    tags: () =>
      [...target.querySelectorAll("*")].map((node) =>
        node.tagName.toLowerCase(),
      ),
    done: () => {
      unmount(app);
      target.remove();
    },
  };
}

/**
 * **A `<script>` in a seeded body never reaches the DOM** — criterion 5, and
 * the whole reason this renderer exists.
 *
 * Three shapes in one body, because they fail differently. The literal
 * `<script>` element is the headline. The `onerror` is the attribute path: a
 * sanitizer that allow-lists tags but forwards attributes passes the first
 * assertion and fails the app. And the `javascript:` link is the third, which
 * no attribute filter catches because `href` is an attribute a link is
 * *supposed* to have.
 *
 * `querySelector("script")` and not a text search: a `<script>` element in a
 * jsdom tree is findable whether or not jsdom chose to execute it, and that is
 * the question — whether the element is there — rather than whether this test
 * runner happens to run it.
 */
test("a script tag, an event attribute and a javascript: link never reach the DOM", () => {
  const screen = render(
    '<p>before</p><script>alert(1)</script><p onclick="alert(2)">after</p>' +
      '<a href="javascript:alert(3)">press me</a>' +
      '<img src="x" onerror="alert(4)"/>' +
      "<style>body{display:none}</style>",
  );

  expect(
    screen.target.querySelector("script"),
    "a script element reached the webview",
  ).toBeNull();
  expect(
    screen.target.querySelector("style"),
    "a style element reached the webview",
  ).toBeNull();
  expect(screen.target.querySelector("img")).toBeNull();
  expect(screen.target.querySelector("[onclick]")).toBeNull();
  expect(screen.target.querySelector("[onerror]")).toBeNull();
  // Not one attribute of any kind survives on the elements that did render.
  for (const element of screen.target.querySelectorAll("*")) {
    expect(
      [...element.attributes].map((a) => a.name),
      `${element.tagName} kept attributes`,
    ).toEqual([]);
  }
  // The words the author actually wrote are all still there...
  expect(screen.text()).toContain("before");
  expect(screen.text()).toContain("after");
  expect(screen.text()).toContain("press me");
  // ...and the script's body is not shown as if it were some of them.
  expect(
    screen.text(),
    "a script body was rendered as visible prose",
  ).not.toContain("alert(1)");
  expect(screen.text()).not.toContain("display:none");
  screen.done();
});

/**
 * A `javascript:` link renders as its words and cannot be followed.
 *
 * The link is a `<button>` by design (see `StorageBody.svelte`), so "cannot be
 * followed" is literally "there is no control to press": the words are there,
 * the affordance is not.
 */
test("an unsafe link keeps its words and offers nothing to press", () => {
  const screen = render(
    '<p><a href="javascript:alert(1)">danger</a> and <a href="https://wiki.example/x">safe</a></p>',
  );

  const buttons = [...screen.target.querySelectorAll("button")];
  expect(buttons.map((button) => button.textContent?.trim())).toEqual(["safe"]);
  expect(screen.text()).toContain("danger");
  expect(
    screen.target.querySelector("a"),
    "an anchor could navigate the webview away",
  ).toBeNull();

  buttons[0]!.click();
  flushSync();
  expect(
    screen.opened,
    "a body link goes to the OS browser, like Open in browser",
  ).toEqual(["https://wiki.example/x"]);
  screen.done();
});

/**
 * The obfuscations that beat a substring test.
 *
 * Every one of these is a `javascript:` URL a browser has historically been
 * willing to follow, and every one of them passes
 * `href.startsWith("javascript:")`. The check is on the scheme parsed *after*
 * entities are decoded and control characters are stripped, which is why they
 * all land in the same place: no button.
 */
test("an unsafe scheme in disguise is still unsafe", () => {
  for (const href of [
    "javascript:alert(1)",
    "JaVaScRiPt:alert(1)",
    "java&#115;cript:alert(1)",
    "&#106;avascript:alert(1)",
    "\tjavascript:alert(1)",
    "java\nscript:alert(1)",
    "data:text/html,<script>alert(1)</script>",
    "vbscript:msgbox(1)",
    // Relative and protocol-relative: dull rather than dangerous, and dropped
    // for a dull reason -- the wiki's host is not knowable from here.
    "/display/ENG/Home",
    "//evil.example/x",
  ]) {
    const screen = render(`<p><a href="${href}">go</a></p>`);
    expect(
      screen.target.querySelector("button"),
      `${href} was offered as a link`,
    ).toBeNull();
    expect(screen.text()).toBe("go");
    screen.done();
  }
});

/**
 * **A macro renders as a labelled placeholder** — criterion 2, and spec #272's
 * sentence: the reader has to be able to see why a section will refuse
 * editing.
 *
 * The macro's own `ac:name` is what the placeholder says, because that is the
 * word the author chose in the editor. Its *content* goes with it: an `info`
 * macro's rich-text body is the macro, not prose beside it, and leaving the
 * text behind would show a paragraph knobas cannot edit as if it were one it
 * could.
 */
test("a macro is a placeholder naming the macro, and takes its content with it", () => {
  const screen = render(
    "<p>above</p>" +
      '<ac:structured-macro ac:name="info"><ac:rich-text-body><p>mind the gap</p>' +
      "</ac:rich-text-body></ac:structured-macro>" +
      "<p>below</p>",
  );

  const placeholders = [...screen.target.querySelectorAll(".ac")].map((node) =>
    node.textContent?.trim(),
  );
  expect(placeholders, "the placeholder lost the macro's name").toEqual([
    "info macro",
  ]);
  expect(screen.text()).toContain("above");
  expect(screen.text()).toContain("below");
  expect(
    screen.text(),
    "the macro's body was rendered as editable prose",
  ).not.toContain("mind the gap");
  screen.done();
});

/**
 * The macro shapes that are *not* a structured macro with a body.
 *
 * A self-closing macro (`<ac:image/>`), one that names no macro at all, and a
 * macro **nested inside another macro's** rich-text body — the last is the one
 * that decides whether the skip is depth-counted, because a scan to the first
 * `</ac:structured-macro>` ends the outer macro early and spills the rest of
 * the page out of it.
 */
test("a self-closing macro, an unnamed one and a nested one each place one placeholder", () => {
  const nested = render(
    '<ac:structured-macro ac:name="expand"><ac:rich-text-body>' +
      '<ac:structured-macro ac:name="code"><ac:plain-text-body>' +
      "<![CDATA[if (a < b) retry();]]></ac:plain-text-body></ac:structured-macro>" +
      "</ac:rich-text-body></ac:structured-macro><p>tail</p>",
  );
  expect(
    [...nested.target.querySelectorAll(".ac")].map((node) =>
      node.textContent?.trim(),
    ),
    "a macro inside a macro ended the outer one early",
  ).toEqual(["expand macro"]);
  expect(nested.text()).toContain("tail");
  expect(nested.text()).not.toContain("retry()");
  nested.done();

  const inline = render(
    '<p>see <ac:image ac:align="center"><ri:attachment ri:filename="x.png"/></ac:image> and ' +
      '<ac:link><ri:page ri:content-title="Home"/></ac:link>' +
      // Genuinely self-closing, which is the shape that has no close tag to
      // skip to: a subtree skip started here would swallow the rest of the
      // paragraph.
      ' and <ac:image ac:align="right"/> too</p>',
  );
  expect(
    [...inline.target.querySelectorAll(".ac")].map((node) =>
      node.textContent?.trim(),
    ),
    "an element with no ac:name is labelled by its own name",
  ).toEqual(["image", "link", "image"]);
  expect(inline.text()).toContain("see");
  expect(
    inline.text(),
    "a self-closing macro swallowed what followed it",
  ).toContain("too");
  inline.done();
});

/**
 * A macro **inside a table cell** — the shape where the two features meet.
 *
 * A placeholder that escaped its cell would break the row, and a subtree skip
 * that overran would eat the rest of the table; both are invisible in a test
 * that puts the macro in a paragraph of its own.
 */
test("a macro inside a table cell stays in its cell and the table stays a table", () => {
  const screen = render(
    "<table><tbody><tr><td>Runbook</td><td>" +
      '<ac:structured-macro ac:name="jira"><ac:parameter ac:name="key">PAY-231' +
      "</ac:parameter></ac:structured-macro></td></tr>" +
      "<tr><td>Owner</td><td>payments</td></tr></tbody></table>",
  );

  const table = screen.target.querySelector("table")!;
  expect(table.rows, "the table lost a row to the macro").toHaveLength(2);
  const cell = table.rows[0]!.cells[1]!;
  expect(cell.querySelector(".ac")?.textContent?.trim()).toBe("jira macro");
  expect(
    cell.textContent,
    "the macro's parameters leaked into the cell",
  ).not.toContain("PAY-231");
  expect([...table.rows[1]!.cells].map((c) => c.textContent)).toEqual([
    "Owner",
    "payments",
  ]);
  screen.done();
});

/**
 * The three layout containers are **not** macros: they are the wrapper a
 * two-column Confluence page puts around its entire body, so a placeholder
 * there would replace the page with the word "layout".
 */
test("a page written in a layout renders its body rather than a layout placeholder", () => {
  const screen = render(
    '<ac:layout><ac:layout-section ac:type="two_equal"><ac:layout-cell>' +
      "<p>left column</p></ac:layout-cell><ac:layout-cell><p>right column</p>" +
      "</ac:layout-cell></ac:layout-section></ac:layout>",
  );

  expect(screen.target.querySelectorAll(".ac")).toHaveLength(0);
  expect(screen.text()).toContain("left column");
  expect(screen.text()).toContain("right column");
  expect(screen.tags().filter((tag) => tag === "p")).toHaveLength(2);
  screen.done();
});

/**
 * **A table renders as a table** — criterion 2's other half.
 *
 * Asserted through the DOM's own table interface (`rows`, `cells`) rather than
 * by counting `<td>` elements: what the criterion asks for is a table a reader
 * can read across, and a `<td>` that ended up outside a `<tr>` would satisfy a
 * tag count and render as a run-on line.
 */
test("a table renders as a table, with its header row", () => {
  const screen = render(
    "<table><thead><tr><th>Code</th><th>Meaning</th></tr></thead><tbody>" +
      "<tr><td>503</td><td>retry</td></tr><tr><td>409</td><td>manual review</td></tr>" +
      "</tbody></table>",
  );

  const table = screen.target.querySelector("table");
  expect(table, "a table did not render as a table").not.toBeNull();
  expect(table!.rows).toHaveLength(3);
  expect(
    [...table!.rows[0]!.cells].map((cell) => cell.tagName.toLowerCase()),
  ).toEqual(["th", "th"]);
  expect([...table!.rows[2]!.cells].map((cell) => cell.textContent)).toEqual([
    "409",
    "manual review",
  ]);
  screen.done();
});

/**
 * The body vocabulary that has to survive, and the wrappers that must not.
 *
 * A `<span class="x">` is unwrapped — wrapper gone, words kept — which is the
 * direction that costs formatting rather than content. A heading is a heading
 * and a list is a list, because that is what "as it looks in Confluence"
 * means.
 */
test("headings, lists and inline emphasis survive, and an unknown wrapper is unwrapped", () => {
  const screen = render(
    "<h2>Backoff policy</h2><p>base <strong>30 s</strong>, factor <em>2</em>, " +
      '<code>MAX=5</code></p><ul><li>first</li><li><span class="x">second</span></li></ul>' +
      "<div><blockquote>quoted</blockquote></div>",
  );

  expect(screen.target.querySelector("h2")?.textContent).toBe("Backoff policy");
  expect(screen.target.querySelectorAll("ul > li")).toHaveLength(2);
  expect(screen.target.querySelector("strong")?.textContent).toBe("30 s");
  expect(screen.target.querySelector("em")?.textContent).toBe("2");
  expect(screen.target.querySelector("code")?.textContent).toBe("MAX=5");
  expect(screen.target.querySelector("blockquote")?.textContent).toBe("quoted");
  expect(
    screen.tags(),
    "a wrapper knobas does not know reached the DOM",
  ).not.toContain("span");
  expect(screen.tags()).not.toContain("div");
  expect(screen.text()).toContain("second");
  screen.done();
});

/**
 * gotcha 7 from the other side: **escaped markup in the storage format is
 * prose**, and has to come out as the prose it is.
 *
 * The order the parser reads in is what makes this true — tags first, entities
 * after — so `&lt;script&gt;` is four words about a script tag and not a
 * script tag. And it still is not one after decoding: it arrives as a text
 * node, so the DOM has the characters and no element.
 */
test("escaped markup is somebody's words, and stays words", () => {
  const screen = render(
    "<p>write &lt;script&gt;alert(1)&lt;/script&gt; in the box</p>",
  );

  expect(screen.target.querySelector("script")).toBeNull();
  expect(screen.target.querySelectorAll("p")).toHaveLength(1);
  expect(screen.text()).toBe("write <script>alert(1)</script> in the box");
  screen.done();
});

/**
 * Malformed markup renders, because a wiki body is whatever ten years put in
 * it and a page nobody can read is worse than a page that lost a `<b>`.
 */
test("an unclosed tag, a stray close and a bare angle bracket all still render", () => {
  const unclosed = render("<p>unfinished");
  expect(unclosed.target.querySelector("p")?.textContent).toBe("unfinished");
  unclosed.done();

  const stray = render("<p>a</b>b</p>");
  expect(stray.text()).toBe("ab");
  stray.done();

  const bare = render("<p>a &lt; b and 3 < 4</p>");
  expect(bare.text()).toBe("a < b and 3 < 4");
  bare.done();

  const empty = render("");
  expect(empty.target.querySelectorAll("*")).toHaveLength(0);
  empty.done();
});

/**
 * An attribute value containing `>` is one tag, not one and a half. A scan to
 * the first `>` spills the rest of the attribute into the page as text — and
 * on an `<a>` it would spill the *href* it was in the middle of reading.
 */
test("a greater-than inside an attribute does not end the tag", () => {
  const screen = render(
    "<p><a href=\"https://wiki.example/a?x=1&amp;y=2\" title='a>b'>link</a></p>",
  );

  const button = screen.target.querySelector("button");
  expect(screen.text()).toBe("link");
  button!.click();
  flushSync();
  expect(screen.opened).toEqual(["https://wiki.example/a?x=1&y=2"]);
  screen.done();
});

// -- the payload reads (#284's shapes), and their miss direction -------------

/**
 * Where a page keeps its body and its comments, and the **gate**: the read
 * answers for the Confluence adapter and for nothing else.
 *
 * The same payload under another adapter's name is a miss, not a rendering.
 * That is the ADR-0007 discipline this interim read is under — a `page` from a
 * second adapter is not obliged to be XHTML, and guessing from the shape of
 * the payload is the guess the ADR forbids.
 */
test("the body and the comments are read at Confluence's own paths, for Confluence alone", () => {
  const payload = {
    id: "98307",
    body: { storage: { value: "<p>hello</p>", representation: "storage" } },
    children: {
      comment: {
        results: [
          {
            id: "98320",
            body: { storage: { value: "<p>@Mara can you add the SLA?</p>" } },
          },
          { id: "98321", body: { storage: { value: "<p>on it</p>" } } },
        ],
      },
    },
  };

  expect(storageBodyOf("confluence", payload)).toBe("<p>hello</p>");
  expect(pageCommentsOf("confluence", payload).map((c) => c.id)).toEqual([
    "98320",
    "98321",
  ]);

  for (const other of ["jira", "gitea", "mock", "", null, undefined]) {
    expect(
      storageBodyOf(other, payload),
      `${other} was read as storage format`,
    ).toBeNull();
    expect(pageCommentsOf(other, payload)).toEqual([]);
  }
});

/**
 * Absence, never a wrong rendering: every unusable shape at either path is a
 * miss, so the panel falls back to the normalized text the mirror holds.
 */
test("a payload with nothing readable at either path misses rather than guessing", () => {
  for (const payload of [
    null,
    undefined,
    "a string",
    [],
    {},
    { body: null },
    { body: { storage: null } },
    { body: { storage: { value: 42 } } },
    { body: { storage: { value: "   " } } },
    { body: [{ storage: { value: "<p>x</p>" } }] },
  ]) {
    expect(
      storageBodyOf("confluence", payload),
      JSON.stringify(payload) ?? "undefined",
    ).toBeNull();
  }

  for (const payload of [
    {},
    { children: { comment: null } },
    { children: { comment: { results: {} } } },
    { children: { comment: { results: [null, 7, "x"] } } },
    {
      children: {
        comment: { results: [{ id: "1", body: { storage: { value: "" } } }] },
      },
    },
  ]) {
    expect(pageCommentsOf("confluence", payload)).toEqual([]);
  }

  // A comment the record gave no id is still a comment, and gets a key it can
  // be rendered under rather than being dropped.
  expect(
    pageCommentsOf("confluence", {
      children: {
        comment: { results: [{ body: { storage: { value: "<p>x</p>" } } }] },
      },
    }),
  ).toEqual([
    { id: "comment-0", storage: "<p>x</p>", author: null, when: null },
  ]);
});

/**
 * **Who wrote a comment and when** (#286).
 *
 * `api.rs`'s `EXPAND` asked for a comment's body and nothing else until this
 * ticket, so the payload carried no author and no timestamp and this section
 * could render the words alone. The two expansions it now asks for are the
 * ones the completion path already used, and this is the read of them.
 *
 * The author rule is the adapter's own for a *page*: the person who made this
 * version, and the creator for a comment nobody has edited since. Two answers
 * to one question would be worse than one that sometimes misses.
 */
test("a comment carries who wrote it and when, and misses to null", () => {
  const payload = {
    children: {
      comment: {
        results: [
          {
            id: "98320",
            body: { storage: { value: "<p>@Mara can you add the SLA?</p>" } },
            version: {
              number: 2,
              when: "2026-08-22T12:41:00.000Z",
              by: { username: "knobas" },
            },
            history: { createdBy: { username: "mara.lindqvist" } },
          },
          {
            id: "98321",
            body: { storage: { value: "<p>on it</p>" } },
            version: { number: 1, when: "2026-08-22T13:00:00.000Z" },
            history: { createdBy: { username: "mara.lindqvist" } },
          },
          {
            id: "98322",
            body: { storage: { value: "<p>no expansions</p>" } },
          },
        ],
      },
    },
  };
  expect(
    pageCommentsOf("confluence", payload).map((c) => [c.author, c.when]),
  ).toEqual([
    ["knobas", "2026-08-22T12:41:00.000Z"],
    // Nobody named on the version: its creator, which is #284's fallback.
    ["mara.lindqvist", "2026-08-22T13:00:00.000Z"],
    // A server that would not expand this deeply: no author, no instant, and
    // the comment still renders. Absence, never a guess.
    [null, null],
  ]);

  // A blank author is the same absence as a missing one -- it would otherwise
  // render as an empty byline beside somebody's words.
  expect(
    pageCommentsOf("confluence", {
      children: {
        comment: {
          results: [
            {
              id: "1",
              body: { storage: { value: "<p>x</p>" } },
              version: { when: "  ", by: { username: "   " } },
            },
          ],
        },
      },
    })[0],
  ).toEqual({ id: "1", storage: "<p>x</p>", author: null, when: null });
});

/**
 * **The version a section edit is made against** (#286).
 *
 * `pageVersionOf` is what fills `WriteOp::UpdatePage`'s `base_version`, and the
 * failure direction is stated rather than hoped: a record that does not say
 * what version it is yields `null`, and the surface offers **no edit** rather
 * than one sent against a guess. A string `"3"` is a miss for the same reason
 * -- it would reach the adapter and become `"3" + 1` on the wire.
 */
test("the page version is read as a positive integer or not at all", () => {
  expect(pageVersionOf("confluence", { version: { number: 3 } })).toBe(3);
  for (const version of [
    { number: "3" },
    { number: 0 },
    { number: -1 },
    { number: 1.5 },
    {},
  ]) {
    expect(
      pageVersionOf("confluence", { version }),
      JSON.stringify(version),
    ).toBeNull();
  }
  expect(pageVersionOf("confluence", {})).toBeNull();
  // Gated on the adapter, like every other read here: `version.number` is a
  // path some other source could carry meaning something else entirely.
  expect(pageVersionOf("jira", { version: { number: 3 } })).toBeNull();
});

/**
 * Two macro shapes #285 left unwitnessed, closed here (#286).
 *
 * A macro whose `ac:name` carries an entity is the author's word with an `&`
 * in it, and the placeholder must say the word rather than the escape. It is
 * safe to decode: a label leaves this module as a `text` node, so a decoded
 * `<` has no path to becoming an element -- which the second half asserts on
 * the DOM rather than on the string.
 *
 * An `ac:name` of nothing but whitespace is the same absence as a missing one,
 * and falls back to the element's own local name.
 */
test("a macro label decodes its name, and whitespace is no name at all", () => {
  const screen = render(
    '<ac:structured-macro ac:name="drawio &amp; friends"/>' +
      '<ac:structured-macro ac:name="   "/>' +
      '<ac:structured-macro ac:name="&lt;script&gt;alert(1)&lt;/script&gt;"/>',
  );
  const labels = [...screen.target.querySelectorAll(".ac")].map((n) =>
    n.textContent?.trim(),
  );
  expect(labels).toEqual([
    "drawio & friends macro",
    // No name: the element's own local name, so it reads as what it is.
    "structured macro",
    "<script>alert(1)</script> macro",
  ]);
  // The decoded label is text and stays text: no element came out of it.
  expect(
    screen.target.querySelector("script"),
    "a macro label became markup",
  ).toBeNull();
  screen.done();
});

/**
 * `ri:` elements **outside** any `ac:` parent — the other shape #285 left
 * unwitnessed.
 *
 * Inside a macro they are skipped with its subtree, which is asserted above.
 * Outside one the storage format is malformed, and the rule that applies is
 * the one every unknown element gets: **unwrapped**. The wrapper goes, every
 * word survives, and no attribute of it reaches the DOM — `ri:userkey` and
 * `ri:value` are exactly the attributes a naive renderer would have carried.
 */
test("an ri: element outside a macro is unwrapped, words and all", () => {
  const screen = render(
    '<p>see <ri:url ri:value="https://x.example">the runbook</ri:url> and ' +
      '<ri:user ri:userkey="ff8080"/>.</p>',
  );
  expect(screen.text()).toBe("see the runbook and .");
  expect(
    screen.target.querySelector("[ri\\:value]"),
    "an ri: attribute reached the DOM",
  ).toBeNull();
  expect(screen.target.innerHTML).not.toContain("ri:");
  screen.done();
});

/**
 * **A body nested past the cap renders, rather than taking the window down.**
 *
 * `StorageBody` recurses once per level, so a page's nesting depth is a
 * JavaScript stack depth at render time, and a page body is untrusted input.
 * Before `MAX_ELEMENT_DEPTH` this mount threw a stack overflow at around 150
 * nested elements — inside Svelte's flush, where this app has no boundary to
 * catch it, so one page written to do it would blank the window for whoever
 * opened it.
 *
 * Asserted on the DOM after mount, like everything else here, and asserted in
 * both directions: the words all arrive, and the tree the renderer had to walk
 * is bounded. The wrappers past the cap are unwrapped exactly as a `<span>` is
 * — structure nobody wrote on purpose is the only thing lost.
 */
test("a body nested deeper than a page ever is still renders its words", () => {
  const depth = 4000;
  const screen = render(`${"<b>".repeat(depth)}deep${"</b>".repeat(depth)}`);

  expect(screen.text(), "the words of a deeply nested body were lost").toBe(
    "deep",
  );
  const nesting = (node: Element): number =>
    1 + Math.max(0, ...[...node.children].map((child) => nesting(child)));
  expect(
    nesting(screen.target),
    "the rendered tree is not bounded",
  ).toBeLessThanOrEqual(80);
  screen.done();
});
