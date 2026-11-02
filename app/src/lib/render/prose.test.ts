import { describe, expect, it } from "vitest";

import { jiraWikiToMarkdown, proseMarkdown } from "./prose";

describe("jiraWikiToMarkdown (#2308)", () => {
  it("turns headings, numbered and bulleted lists into Markdown's", () => {
    expect(jiraWikiToMarkdown("h2. Why\n\n# first\n# second\n## nested\n* dot\n** deeper")).toBe(
      "## Why\n\n1. first\n1. second\n   1. nested\n- dot\n  - deeper",
    );
  });

  it("reads *bold*, _italic_, -struck-, {{mono}} and keeps words with hyphens and stars", () => {
    expect(jiraWikiToMarkdown("*Bold* and _it_ and -gone- and {{a_b*c}} in well-known 2*3*4")).toBe(
      "**Bold** and *it* and ~~gone~~ and `a_b*c` in well-known 2*3*4",
    );
  });

  it("makes a {code:lang} block a fence, its body untouched, so a Mermaid block is a mermaid fence", () => {
    expect(jiraWikiToMarkdown("Flow:\n{code:mermaid}\ngraph TD\n  A --> *B*\n{code}\nafter")).toBe(
      "Flow:\n```mermaid\ngraph TD\n  A --> *B*\n```\nafter",
    );
    expect(jiraWikiToMarkdown("{code:title=Foo.java|borderStyle=solid}\nint x;\n{code}")).toBe("```\nint x;\n```");
    expect(jiraWikiToMarkdown("{code:java|title=Foo}\nint x;\n{code}")).toBe("```java\nint x;\n```");
    expect(jiraWikiToMarkdown("{noformat}\nh1. not a heading\n{noformat}")).toBe("```\nh1. not a heading\n```");
  });

  it("reads links, mentions, quotes, rules and tables", () => {
    expect(jiraWikiToMarkdown("See [the runbook|https://wiki.example/run] or [https://x.example] and ask [~mara]")).toBe(
      "See [the runbook](https://wiki.example/run) or <https://x.example> and ask @mara",
    );
    expect(jiraWikiToMarkdown("bq. quoted\n{quote}\nlong\nquote\n{quote}\n----")).toBe(
      "> quoted\n> long  \n> quote\n---",
    );
    expect(jiraWikiToMarkdown("||Code||Retry||\n|AC04|no|\n|MS03|yes|")).toBe(
      "| Code | Retry |\n| --- | --- |\n| AC04 | no |\n| MS03 | yes |",
    );
  });

  it("keeps Jira's line breaks, and draws raw HTML as text", () => {
    expect(jiraWikiToMarkdown("one\ntwo\n\nthree <b>x</b>")).toBe("one  \ntwo\n\nthree &lt;b>x&lt;/b>");
  });
});

describe("jiraWikiToMarkdown: what Jira draws as text stays text (#2308)", () => {
  it("keeps the words after a block's closing tag", () => {
    expect(jiraWikiToMarkdown("{code}x{code} and *more*")).toBe("```\nx\n```\nand **more**");
  });

  it("escapes a line start Markdown would read as a list or a quote", () => {
    expect(jiraWikiToMarkdown("1. not a list\n> not a quote\n+ nor this")).toBe(
      "1\\. not a list  \n\\> not a quote  \n\\+ nor this",
    );
  });

  it("quotes a bq. line inside a {quote} once more", () => {
    expect(jiraWikiToMarkdown("{quote}\nbq. inner\n{quote}")).toBe("> > inner");
  });
});

describe("jiraWikiToMarkdown: inline images and task boxes (#2406)", () => {
  const files = [
    { id: "50000", name: "psp-409-trace.png" },
    { id: "50001", name: "retry timeline [v2].png" },
  ];

  it("makes !name.ext|…! an image of the attachment by that name, where the wiki put it", () => {
    expect(jiraWikiToMarkdown("See !psp-409-trace.png|thumbnail! here", files)).toBe(
      "See ![psp-409-trace.png](attachment:50000) here",
    );
    expect(jiraWikiToMarkdown("!psp-409-trace.png!", files)).toBe("![psp-409-trace.png](attachment:50000)");
    expect(jiraWikiToMarkdown("!retry timeline [v2].png|width=300,height=200!", files)).toBe(
      "![retry timeline \\[v2\\].png](attachment:50001)",
    );
  });

  it("leaves a name the attachment list does not hold as the text", () => {
    expect(jiraWikiToMarkdown("!missing.png|thumbnail! and !https://x.example/a.png!", files)).toBe(
      "!missing.png|thumbnail! and !https://x.example/a.png!",
    );
    expect(jiraWikiToMarkdown("!psp-409-trace.png!")).toBe("!psp-409-trace.png!");
    expect(jiraWikiToMarkdown("Wow! That *works*! Yes!", files)).toBe("Wow! That **works**! Yes!");
  });

  it("keeps an image name inside {{mono}} as code", () => {
    expect(jiraWikiToMarkdown("{{!psp-409-trace.png!}}", files)).toBe("`!psp-409-trace.png!`");
  });

  it("makes (/) and (x) at a list item's start a done and an open task box", () => {
    expect(jiraWikiToMarkdown("* (/) Retry the transient codes\n* (x) Cap the budget\n# (/)numbered")).toBe(
      "- [x] Retry the transient codes\n- [ ] Cap the budget\n1. [x] numbered",
    );
  });

  it("leaves (/) and (x) mid-line, outside a list or alone as the text", () => {
    expect(jiraWikiToMarkdown("* Retry (/) then (x)\n(/) not a list\n* (x)")).toBe(
      "- Retry (/) then (x)\n(/) not a list\n- (x)",
    );
  });
});

describe("proseMarkdown (#2308)", () => {
  it("answers the Markdown to draw, or null for text", () => {
    expect(proseMarkdown("# Title", "markdown")).toBe("# Title");
    expect(proseMarkdown("# item", "jira_wiki")).toBe("1. item");
    expect(proseMarkdown("# item", "text")).toBeNull();
  });
});
