/**
 * The section rule, on its own (issue #286).
 *
 * Everything asserted here is a pure function over a storage-format string, so
 * these are the tests that decide whether a reader is offered an edit at all.
 * The component that renders the offer is `Detail.svelte`; what may be edited
 * is decided here.
 */
import { describe, expect, it } from "vitest";

import { pageSections, replaceSectionBody, toStorage } from "./page-sections";

/** The seeded fixture page's shape: a heading, prose, a macro further down. */
const PAGE =
  "<p>preamble nobody titled</p>" +
  "<h2>Backoff policy</h2><p>base 30 s, factor 2.</p>" +
  "<h2>Rollout</h2><p>behind a flag.</p>" +
  '<h2>Runbook</h2><ac:structured-macro ac:name="info"><ac:rich-text-body><p>call ops</p>' +
  "</ac:rich-text-body></ac:structured-macro>";

describe("pageSections", () => {
  it("cuts a section at the next heading of the same level", () => {
    const sections = pageSections(PAGE);
    expect(sections.map((s) => s.heading)).toEqual(["Backoff policy", "Rollout", "Runbook"]);
    expect(sections.map((s) => s.level)).toEqual([2, 2, 2]);
    expect(sections[0]!.text).toBe("base 30 s, factor 2.");
    expect(sections[1]!.text).toBe("behind a flag.");
  });

  /**
   * A preamble has no heading to name it, so there is no section to aim an
   * *Edit* at -- and the first section must not swallow it, or an edit would
   * move somebody's words under a heading they were never under.
   */
  it("gives text before the first heading to no section", () => {
    const first = pageSections(PAGE)[0]!;
    expect(PAGE.slice(0, first.start)).toBe("<p>preamble nobody titled</p>");
    expect(first.text).not.toContain("preamble");
  });

  /**
   * "Until the next heading of the same or **higher** level": an `h2` section
   * runs through the `h3`s under it and stops at the next `h2` **and** at an
   * `h1`, which is the clause a rule written as "the next heading of the same
   * level" would get wrong.
   */
  it("runs a section through deeper headings and stops at a shallower one", () => {
    const storage =
      "<h1>Payments</h1><p>the area.</p>" +
      "<h2>Backoff</h2><p>base 30 s.</p><h3>Jitter</h3><p>full.</p>" +
      "<h1>Ledger</h1><p>elsewhere.</p>";
    const sections = pageSections(storage);
    expect(sections.map((s) => `${s.level}:${s.heading}`)).toEqual([
      "1:Payments",
      "2:Backoff",
      "3:Jitter",
      "1:Ledger",
    ]);
    // The h1 swallows everything under it, up to the next h1 and no further.
    expect(sections[0]!.text).toBe("the area.\n\nBackoff\n\nbase 30 s.\n\nJitter\n\nfull.");
    // The h2 stops at the next h1 rather than running to the end.
    expect(sections[1]!.text).toBe("base 30 s.\n\nJitter\n\nfull.");
    expect(sections[3]!.text).toBe("elsewhere.");
  });

  /** Four to six are content inside a section, not the start of one. */
  it("opens a section for h1 to h3 and for nothing deeper", () => {
    const storage = "<h3>Jitter</h3><p>full.</p><h4>Caveat</h4><p>on retries.</p>";
    const sections = pageSections(storage);
    expect(sections.map((s) => s.heading)).toEqual(["Jitter"]);
    expect(sections[0]!.text).toBe("full.\n\nCaveat\n\non retries.");
  });
});

describe("the refusal rule", () => {
  it("refuses a section holding a macro", () => {
    const sections = pageSections(PAGE);
    expect(sections[2]!.refusal).toBe("macro");
    expect(sections.map((s) => s.refusal)).toEqual([null, null, "macro"]);
  });

  /**
   * The other half of the rule, and the one a mutant that dropped it would
   * pass every macro test: a table has no line-oriented text form either.
   */
  it("refuses a section holding a table", () => {
    const storage =
      "<h2>Limits</h2><table><tbody><tr><td>30 s</td></tr></tbody></table>" +
      "<h2>Prose</h2><p>nothing but words.</p>";
    expect(pageSections(storage).map((s) => s.refusal)).toEqual(["table", null]);
  });

  /**
   * The refusal covers the **whole** section, headings under it included: an
   * `h2` whose `h3` holds a macro is a section an edit would rewrite through
   * text, so the macro would be gone.
   */
  it("refuses when the macro is under a deeper heading inside the section", () => {
    const storage =
      "<h2>Runbook</h2><p>steps.</p><h3>Escalate</h3>" +
      '<ac:structured-macro ac:name="jira"/><h2>After</h2><p>done.</p>';
    const sections = pageSections(storage);
    expect(sections[0]!.refusal).toBe("macro");
    expect(sections[1]!.refusal).toBe("macro");
    expect(sections[2]!.refusal).toBe(null);
  });

  /** The heading itself is inside its own section. */
  it("refuses when the macro is in the heading rather than the body", () => {
    const storage = '<h2>Runbook <ac:emoticon ac:name="warning"/></h2><p>steps.</p>';
    expect(pageSections(storage)[0]!.refusal).toBe("macro");
  });

  /**
   * A `<h2>` inside a code macro's CDATA is somebody's example of markup, not
   * a heading of this page -- and a commented-out table is not a table.
   */
  it("reads neither CDATA nor a comment as markup", () => {
    const storage =
      "<h2>Example</h2><p>like this:</p><ac:plain-text-body><![CDATA[<h2>not mine</h2>]]>" +
      "</ac:plain-text-body>";
    expect(pageSections(storage).map((s) => s.heading)).toEqual(["Example"]);

    const commented = "<h2>Limits</h2><!-- <table><tr><td>x</td></tr></table> --><p>words.</p>";
    expect(pageSections(commented)[0]!.refusal).toBe(null);
  });

  /**
   * A heading inside a table cell opens a section whose slice reaches the
   * table's own close tag, so it is refused. Stated as a test because it is
   * the one place the flat scan and a tree walk would answer differently, and
   * the flat scan's answer is the safe one.
   */
  it("refuses a heading that lives inside a table", () => {
    const storage = "<table><tbody><tr><td><h2>In a cell</h2><p>words.</p></td></tr></tbody></table>";
    expect(pageSections(storage)[0]!.refusal).toBe("table");
  });

  /**
   * Not a refusal, a label: a list survives being read but not being written
   * back, and the surface has to be able to say so *before* the reader
   * commits.
   */
  it("flags a section whose block structure a text edit will not rebuild", () => {
    const listy = "<h2>Steps</h2><ul><li>one</li><li>two</li></ul>";
    expect(pageSections(listy)[0]!.flattens).toBe(true);
    expect(pageSections(listy)[0]!.refusal).toBe(null);
    const prose = "<h2>Steps</h2><p>one, then two.</p>";
    expect(pageSections(prose)[0]!.flattens).toBe(false);
  });
});

describe("replaceSectionBody", () => {
  /**
   * The claim `WriteOp::UpdatePage` rests on: Confluence's content `PUT`
   * replaces the record, so what goes out is the **whole** body -- with the
   * macro section this module refuses to touch surviving byte for byte.
   */
  it("re-assembles the whole body and leaves everything else byte for byte", () => {
    const sections = pageSections(PAGE);
    const next = replaceSectionBody(PAGE, sections[1]!, "behind two flags.");
    expect(next).toBe(
      "<p>preamble nobody titled</p>" +
        "<h2>Backoff policy</h2><p>base 30 s, factor 2.</p>" +
        "<h2>Rollout</h2><p>behind two flags.</p>" +
        '<h2>Runbook</h2><ac:structured-macro ac:name="info"><ac:rich-text-body><p>call ops</p>' +
        "</ac:rich-text-body></ac:structured-macro>",
    );
    // The untouched halves are the original's own characters, not a re-render.
    expect(next.startsWith(PAGE.slice(0, sections[1]!.bodyStart))).toBe(true);
    expect(next.endsWith(PAGE.slice(sections[1]!.end))).toBe(true);
  });

  /** A section edit changes a section's prose, never its name. */
  it("keeps the heading outside the replaced range", () => {
    const sections = pageSections(PAGE);
    expect(replaceSectionBody(PAGE, sections[0]!, "new prose.")).toContain(
      "<h2>Backoff policy</h2><p>new prose.</p>",
    );
  });

  /** What a reader types is character data, not markup. */
  it("escapes what the reader typed", () => {
    const sections = pageSections("<h2>H</h2><p>old.</p>");
    expect(replaceSectionBody("<h2>H</h2><p>old.</p>", sections[0]!, "3 < 4 && <b>")).toBe(
      "<h2>H</h2><p>3 &lt; 4 &amp;&amp; &lt;b&gt;</p>",
    );
  });

  /** An emptied section is an empty section, not a page of empty paragraphs. */
  it("writes nothing for text that is only whitespace", () => {
    const storage = "<h2>H</h2><p>old.</p><h2>Next</h2><p>kept.</p>";
    const sections = pageSections(storage);
    expect(replaceSectionBody(storage, sections[0]!, "  \n\n ")).toBe(
      "<h2>H</h2><h2>Next</h2><p>kept.</p>",
    );
  });

  /**
   * The round trip a reader actually makes: read the section as text, change
   * one word, write it back, read it again. Entities survive it, which is what
   * says the escape and the decode agree.
   */
  it("round-trips a section's text through an edit", () => {
    const storage = "<h2>H</h2><p>3 &lt; 4</p><p>and &amp; more</p>";
    const before = pageSections(storage)[0]!;
    expect(before.text).toBe("3 < 4\n\nand & more");
    const next = replaceSectionBody(storage, before, before.text);
    expect(pageSections(next)[0]!.text).toBe(before.text);
  });
});

describe("toStorage", () => {
  it("makes a paragraph of a blank line and a break of a newline", () => {
    expect(toStorage("first\nsecond\n\nthird")).toBe("<p>first<br/>second</p><p>third</p>");
    expect(toStorage("a\r\n\r\nb")).toBe("<p>a</p><p>b</p>");
    expect(toStorage("\n\n  \na\n\n\n\nb\n\n")).toBe("<p>a</p><p>b</p>");
  });

  /** `&` first, or the escapes this writes get escaped again. */
  it("escapes ampersands before angle brackets", () => {
    expect(toStorage("<b>")).toBe("<p>&lt;b&gt;</p>");
    expect(toStorage("<b>")).not.toContain("&amp;lt;");
  });

  it("produces nothing for text that is only whitespace", () => {
    expect(toStorage("")).toBe("");
    expect(toStorage("   \n\n \t \n")).toBe("");
  });
});

describe("malformed markup", () => {
  /**
   * Total, like every other reader of this markup: a body that would not parse
   * is a page a reader still has to be able to open.
   */
  it("gives an unclosed heading a section that runs to the end", () => {
    const sections = pageSections("<h2>Unclosed<p>words.</p>");
    expect(sections).toHaveLength(1);
    expect(sections[0]!.level).toBe(2);
  });

  /**
   * `3 < 4` in prose: the `>` a naive scan finds belongs to the *next* tag, so
   * stepping past it would step past that tag and lose the section boundary
   * behind it.
   */
  it("reads a bare angle bracket as prose without losing the next heading", () => {
    const sections = pageSections("<h2>A</h2><p>3 < 4</p><h2>B</h2><p>words.</p>");
    expect(sections.map((s) => s.heading)).toEqual(["A", "B"]);
    expect(sections[0]!.text).toBe("3 < 4");
  });

  /** An attribute value holding a `>` is one tag, not two. */
  it("does not cut a tag in half at a quoted angle bracket", () => {
    const storage = '<h2>A</h2><p><a href="https://x.example/?a=1&gt;2">link</a></p><h2>B</h2>';
    expect(pageSections(storage).map((s) => s.heading)).toEqual(["A", "B"]);
  });

  it("has no sections at all for a body with no headings", () => {
    expect(pageSections("<p>just prose.</p>")).toEqual([]);
    expect(pageSections("")).toEqual([]);
  });
});
