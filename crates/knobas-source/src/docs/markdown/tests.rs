//! The converter's rules, one block at a time, in the frame's vocabulary
//! (ported from `knobas_core::markdown`'s tests, #952).

use super::*;

fn text(words: &str) -> Vec<Inline> {
    vec![Inline::text(words)]
}

fn para(words: &str) -> DocBlock {
    DocBlock::Paragraph { text: text(words) }
}

fn cell(words: &str) -> TableCell {
    TableCell {
        header: false,
        text: text(words),
    }
}

/// The body alone, as a page titled `T` writes it.
fn body(blocks: &[DocBlock]) -> String {
    page("T", blocks)
        .strip_prefix("# T\n\n")
        .expect("a body")
        .to_owned()
}

#[test]
fn a_heading_is_hashes_by_level_and_clamped() {
    let md = body(&[
        DocBlock::Heading {
            level: 2,
            text: text("Retry policy"),
        },
        DocBlock::Heading {
            level: 9,
            text: text("Deep"),
        },
        DocBlock::Heading {
            level: 0,
            text: text("Top\nline"),
        },
    ]);
    assert_eq!(md, "## Retry policy\n\n###### Deep\n\n# Top line\n");
}

#[test]
fn a_paragraph_escapes_markup_and_a_block_start() {
    assert_eq!(
        body(&[para("use *only* [this] <b> a_b")]),
        "use \\*only\\* \\[this\\] \\<b> a\\_b\n"
    );
    assert_eq!(body(&[para("# not a heading")]), "\\# not a heading\n");
    assert_eq!(body(&[para("- not a list")]), "\\- not a list\n");
    assert_eq!(body(&[para("12. not numbered")]), "12\\. not numbered\n");
    assert_eq!(body(&[para("12 apples")]), "12 apples\n");
}

#[test]
fn inline_runs_keep_their_marks() {
    let marked = |words: &str, strong, emphasis, code| Inline::Text {
        text: words.to_owned(),
        strong,
        emphasis,
        code,
    };
    let md = body(&[DocBlock::Paragraph {
        text: vec![
            marked("bold", true, false, false),
            Inline::text(" and "),
            marked("soft", false, true, false),
            Inline::text(" and "),
            marked("a`b", true, true, true),
            Inline::text(" "),
            Inline::Link {
                text: "Jira".to_owned(),
                href: "https://jira.example/browse/PAY-231".to_owned(),
            },
        ],
    }]);
    assert_eq!(
        md,
        "**bold** and *soft* and ``a`b`` [Jira](https://jira.example/browse/PAY-231)\n"
    );
}

#[test]
fn a_chip_is_its_key_and_a_mention_a_first_name() {
    let md = body(&[DocBlock::Paragraph {
        text: vec![
            Inline::Issue {
                key: "PAY-231".to_owned(),
                entity_id: None,
                title: None,
                status: None,
            },
            Inline::Break,
            Inline::Mention {
                user: Some("priya".to_owned()),
                name: Some("Priya Nair".to_owned()),
                me: false,
            },
        ],
    }]);
    assert_eq!(md, "PAY-231 @Priya\n");
}

#[test]
fn a_list_is_bulleted_or_numbered_and_nests_under_its_item() {
    let md = body(&[
        DocBlock::List {
            ordered: false,
            items: vec![
                vec![para("one")],
                vec![
                    para("two"),
                    DocBlock::List {
                        ordered: true,
                        items: vec![vec![para("a")], vec![para("b")]],
                    },
                ],
            ],
        },
        DocBlock::List {
            ordered: true,
            items: vec![vec![para("first")], vec![para("second")]],
        },
    ]);
    assert_eq!(
        md,
        "- one\n- two\n\n  1. a\n  2. b\n\n1. first\n2. second\n"
    );
}

#[test]
fn code_is_fenced_longer_than_any_backtick_run_inside() {
    assert_eq!(
        body(&[DocBlock::Code {
            language: Some("rust".to_owned()),
            code: "fn main() {}\n".to_owned(),
        }]),
        "```rust\nfn main() {}\n```\n"
    );
    assert_eq!(
        body(&[DocBlock::Code {
            language: None,
            code: "```\nnested\n```".to_owned(),
        }]),
        "````\n```\nnested\n```\n````\n"
    );
}

#[test]
fn a_table_is_a_pipe_table_headed_by_its_first_row() {
    let md = body(&[DocBlock::Table {
        rows: vec![
            vec![cell("Service"), cell("Owner")],
            vec![cell("payouts|api"), cell("Mara")],
            vec![cell("ledger")],
        ],
    }]);
    assert_eq!(
        md,
        "| Service | Owner |\n| --- | --- |\n| payouts\\|api | Mara |\n| ledger |  |\n"
    );
    assert_eq!(
        body(&[DocBlock::Table {
            rows: vec![vec![cell("a"), cell("b"), cell("c")], vec![cell("1")]],
        }]),
        "| a | b | c |\n| --- | --- | --- |\n| 1 |  |  |\n",
        "a header wider than every row sets the width"
    );
    assert_eq!(page("T", &[DocBlock::Table { rows: vec![] }]), "# T\n");
}

#[test]
fn a_panel_is_a_quote_that_names_its_kind() {
    let md = body(&[DocBlock::Panel {
        kind: PanelKind::Warning,
        title: Some("Before you deploy".to_owned()),
        blocks: vec![para("Drain the queue."), para("Then deploy.")],
    }]);
    assert_eq!(
        md,
        "> **Warning** Before you deploy\n>\n> Drain the queue.\n>\n> Then deploy.\n"
    );
}

#[test]
fn a_page_link_is_a_relative_md_link() {
    let md = body(&[DocBlock::Paragraph {
        text: vec![
            Inline::text("See "),
            Inline::PageLink {
                text: "the runbook".to_owned(),
                title: "SEPA retry: runbook".to_owned(),
                space: None,
                entity_id: None,
            },
        ],
    }]);
    assert_eq!(md, "See [SEPA retry: runbook](sepa-retry-runbook.md)\n");
}

#[test]
fn a_placeholder_is_an_html_comment_naming_it() {
    let md = body(&[
        DocBlock::Placeholder {
            name: "jira--chart".to_owned(),
        },
        DocBlock::Placeholder {
            name: "toc".to_owned(),
        },
    ]);
    assert_eq!(md, "<!-- macro: jira- -chart -->\n\n<!-- macro: toc -->\n");
}

#[test]
fn markdown_goes_through_as_it_is_refs_included() {
    assert_eq!(
        page_from_markdown("Q", "Ask about [[mock:PAY-231]] *today*\n\n"),
        "# Q\n\nAsk about [[mock:PAY-231]] *today*\n"
    );
    assert_eq!(page_from_markdown("Q", " \n"), "# Q\n");
}

#[test]
fn a_file_name_keeps_letters_and_digits() {
    assert_eq!(
        page_file_name("SEPA retry: runbook"),
        "sepa-retry-runbook.md"
    );
    assert_eq!(page_file_name("  Größe / 2026  "), "größe-2026.md");
    assert_eq!(page_file_name("???"), "untitled.md");
}

#[test]
fn a_file_name_stays_in_its_folder_and_writable() {
    assert_eq!(page_file_name("../../etc/passwd"), "etc-passwd.md");
    assert_eq!(page_file_name("a\\b/c"), "a-b-c.md");
    assert_eq!(page_file_name(".."), "untitled.md");
    assert_eq!(page_file_name("CON"), "con-page.md");
    assert_eq!(page_file_name("com1"), "com1-page.md");
    assert_eq!(page_file_name("com12"), "com12.md");
    assert_eq!(page_file_name("console"), "console.md");
    let long = page_file_name(&"ä ".repeat(200));
    assert!(long.len() < 255, "{} bytes", long.len());
    assert!(
        !long.contains("-.md"),
        "no dash before the extension: {long}"
    );
}
