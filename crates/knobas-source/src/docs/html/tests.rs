use super::*;
use crate::docs::{DocBlock, Inline, ListItem, TableCell};

fn plain(html: &str) -> Vec<DocBlock> {
    to_blocks(html, &Plain)
}

fn para(text: &str) -> DocBlock {
    DocBlock::Paragraph {
        text: vec![Inline::text(text)],
    }
}

fn strong(text: &str) -> Inline {
    Inline::Text {
        text: text.into(),
        strong: true,
        emphasis: false,
        code: false,
    }
}

#[test]
fn headings_paragraphs_marks_and_links() {
    assert_eq!(
        plain(
            "<h3>Rollout</h3><p>Ship <b>twice</b> a week,<br/>see <a href=\"https://x.test/a?b=1&amp;c=2\">the  plan</a>.</p>"
        ),
        vec![
            DocBlock::Heading {
                level: 3,
                text: vec![Inline::text("Rollout")]
            },
            DocBlock::Paragraph {
                text: vec![
                    Inline::text("Ship "),
                    strong("twice"),
                    Inline::text(" a week,"),
                    Inline::Break,
                    Inline::text("see "),
                    Inline::Link {
                        text: "the plan".into(),
                        href: "https://x.test/a?b=1&c=2".into()
                    },
                    Inline::text("."),
                ]
            },
        ]
    );
}

#[test]
fn nested_lists_and_wrappers_keep_their_nesting() {
    assert_eq!(
        plain(
            "<div><section><ul><li>Build<ol><li><span>unit</span></li><li>live</li></ol></li><li><blockquote><p>Ship</p></blockquote></li></ul></section></div>"
        ),
        vec![DocBlock::List {
            ordered: false,
            items: vec![
                ListItem::new(vec![
                    para("Build"),
                    DocBlock::List {
                        ordered: true,
                        items: vec![
                            ListItem::new(vec![para("unit")]),
                            ListItem::new(vec![para("live")])
                        ]
                    }
                ]),
                ListItem::new(vec![para("Ship")]),
            ]
        }]
    );
}

#[test]
fn a_table_reads_its_rows_through_thead_and_tbody() {
    assert_eq!(
        plain(
            "<table><thead><tr><th>Host</th><th>Port</th></tr></thead><tbody><tr><td>db</td><td><p>5432</p><p>5433</p></td></tr></tbody></table>"
        ),
        vec![DocBlock::Table {
            rows: vec![
                vec![
                    TableCell::new(true, vec![para("Host")]),
                    TableCell::new(true, vec![para("Port")]),
                ],
                vec![
                    TableCell::new(false, vec![para("db")]),
                    TableCell::new(false, vec![para("5432"), para("5433")]),
                ],
            ]
        }]
    );
}

#[test]
fn a_cell_keeps_its_spans_and_its_blocks() {
    let cell = |blocks| TableCell::new(false, blocks);
    assert_eq!(
        plain(concat!(
            "<table>",
            r#"<tr><th colspan="2">Owner</th><th rowspan=" 3 ">Notes</th></tr>"#,
            "<tr><th>Team</th><td><ul><li>Mara</li><li>Tomasz</li></ul><p>On call</p></td></tr>",
            r#"<tr><th rowspan="0" colspan="x">Ops</th><td><table><tr><td>inner</td></tr></table><pre>run</pre><blink>!</blink></td></tr>"#,
            "</table>"
        )),
        vec![DocBlock::Table {
            rows: vec![
                vec![
                    TableCell {
                        colspan: 2,
                        ..TableCell::new(true, vec![para("Owner")])
                    },
                    TableCell {
                        rowspan: 3,
                        ..TableCell::new(true, vec![para("Notes")])
                    },
                ],
                vec![
                    TableCell::new(true, vec![para("Team")]),
                    cell(vec![
                        DocBlock::List {
                            ordered: false,
                            items: vec![
                                ListItem::new(vec![para("Mara")]),
                                ListItem::new(vec![para("Tomasz")]),
                            ],
                        },
                        para("On call"),
                    ]),
                ],
                vec![
                    // A span that is no positive number spans one.
                    TableCell::new(true, vec![para("Ops")]),
                    cell(vec![
                        DocBlock::Table {
                            rows: vec![vec![cell(vec![para("inner")])]],
                        },
                        DocBlock::Code {
                            code: "run".into(),
                            language: None,
                        },
                        DocBlock::Placeholder {
                            name: "blink".into(),
                        },
                    ]),
                ],
            ]
        }]
    );
}

#[test]
fn pre_is_code_and_script_and_style_say_nothing() {
    assert_eq!(
        plain(
            "<style>p { color: red }</style><pre>let a = 1;\n  a &lt; 2</pre><script>alert('x')</script><p>after</p>"
        ),
        vec![
            DocBlock::Code {
                code: "let a = 1;\n  a < 2".into(),
                language: None
            },
            para("after"),
        ]
    );
}

#[test]
fn a_script_holding_markup_opens_and_closes_nothing() {
    assert_eq!(
        plain(
            "<div><SCRIPT>if (a<b) { s = \"</div><p>x</p>\"; }</Script>kept</div><style>a>b{}</style><p>after</p>"
        ),
        vec![para("kept"), para("after")]
    );
    // An unclosed script runs to the end, as an unclosed element does.
    assert_eq!(plain("<p>before</p><script>if (a<b)"), vec![para("before")]);
}

#[test]
fn an_empty_attribute_name_finds_nothing() {
    assert_eq!(attr("a href=\"x\"", ""), None);
}

#[test]
fn broken_markup_reads_as_the_text_it_holds() {
    assert_eq!(
        plain("<p>One</em> and <b>two<ul><li>three"),
        vec![
            DocBlock::Paragraph {
                text: vec![Inline::text("One and "), strong("two")]
            },
            DocBlock::List {
                ordered: false,
                items: vec![ListItem::new(vec![para("three")])]
            },
        ]
    );
    assert_eq!(
        plain("a < b and <p class=\"x"),
        vec![para("a < b and <p class=\"x")]
    );
    assert_eq!(plain(""), vec![]);
}

#[test]
fn an_element_the_hook_declines_is_a_placeholder_named_after_it() {
    assert_eq!(
        plain("<p>Before <x-widget id=\"7\">inside</x-widget> after</p><x-chart/>"),
        vec![
            para("Before"),
            DocBlock::Placeholder {
                name: "x-widget".into()
            },
            para("after"),
            DocBlock::Placeholder {
                name: "x-chart".into()
            },
        ]
    );
}

/// A hook that knows two elements: `<x-note>` as a panel over its children,
/// `<x-user name=…>` as a mention. `<x-wrap>` it unwraps, the rest it
/// declines.
struct Wiki;

impl Hook for Wiki {
    fn element(&self, element: &Node, reading: &Reading<'_>) -> Hooked {
        match element.name() {
            Some("x-note") => Hooked::Blocks(vec![DocBlock::Panel {
                kind: crate::docs::PanelKind::Note,
                title: element.attr("title").map(str::to_owned),
                blocks: reading.blocks(element.children()),
            }]),
            Some("x-user") => Hooked::Inlines(vec![Inline::Mention {
                user: element.attr("name").map(str::to_owned),
                name: None,
                me: false,
            }]),
            Some("x-wrap") => Hooked::Unwrap,
            _ => Hooked::Decline,
        }
    }
}

#[test]
fn the_hook_answers_blocks_inlines_or_unwraps() {
    assert_eq!(
        to_blocks(
            "<p>Ask <x-user name=\"ana\"/> <x-wrap><em>now</em></x-wrap></p><x-note title=\"Heads up\"><ul><li>one</li></ul></x-note>",
            &Wiki
        ),
        vec![
            DocBlock::Paragraph {
                text: vec![
                    Inline::text("Ask "),
                    Inline::Mention {
                        user: Some("ana".into()),
                        name: None,
                        me: false
                    },
                    Inline::text(" "),
                    Inline::Text {
                        text: "now".into(),
                        strong: false,
                        emphasis: true,
                        code: false
                    },
                ]
            },
            DocBlock::Panel {
                kind: crate::docs::PanelKind::Note,
                title: Some("Heads up".into()),
                blocks: vec![DocBlock::List {
                    ordered: false,
                    items: vec![ListItem::new(vec![para("one")])]
                }]
            },
        ]
    );
}

#[test]
fn the_hook_sees_the_marks_it_sits_under() {
    struct Echo;
    impl Hook for Echo {
        fn element(&self, element: &Node, reading: &Reading<'_>) -> Hooked {
            Hooked::Inlines(vec![reading.text(element.text())])
        }
    }
    assert_eq!(
        to_blocks("<p><strong>a <x-name>b</x-name></strong></p>", &Echo),
        vec![DocBlock::Paragraph {
            text: vec![strong("a b")]
        }]
    );
}

#[test]
fn the_tree_keeps_where_each_element_sits() {
    let html = "<h1>A</h1><p>b<br/>";
    let nodes = parse(html);
    let spans: Vec<(usize, usize)> = nodes
        .iter()
        .filter_map(|n| match n {
            Node::Element { span, .. } => Some(*span),
            _ => None,
        })
        .collect();
    assert_eq!(spans, vec![(0, 10), (10, html.len())]);
}

#[test]
fn a_span_past_what_html_draws_is_cut_to_it() {
    let [DocBlock::Table { rows }] =
        &plain(r#"<table><tr><td colspan="4000000000" rowspan="70000">x</td></tr></table>"#)[..]
    else {
        panic!("one table")
    };
    assert_eq!((rows[0][0].colspan, rows[0][0].rowspan), (1_000, 65_534));
}

/// Plain HTML leaves end tags out where a browser implies them (#2449): an
/// unclosed `p` ends at the next paragraph, not around it.
#[test]
fn an_unclosed_paragraph_ends_at_the_next_one() {
    assert_eq!(
        plain("<p>a<p>b<div>c</div>d"),
        vec![para("a"), para("b"), para("c"), para("d")]
    );
}

/// Names of a tree's elements, children in brackets: the shape a browser
/// would build, without the spans.
fn shape(nodes: &[Node]) -> String {
    nodes
        .iter()
        .filter_map(|n| {
            n.name()
                .map(|name| format!("{name}[{}]", shape(n.children())))
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[test]
fn list_items_and_cells_left_open_end_at_their_next_sibling() {
    assert_eq!(
        plain("<ul><li>a<li>b<ul><li>c<li>d</ul><li>e</ul>"),
        vec![DocBlock::List {
            ordered: false,
            items: vec![
                ListItem::new(vec![para("a")]),
                ListItem::new(vec![
                    para("b"),
                    DocBlock::List {
                        ordered: false,
                        items: vec![
                            ListItem::new(vec![para("c")]),
                            ListItem::new(vec![para("d")]),
                        ]
                    }
                ]),
                ListItem::new(vec![para("e")]),
            ]
        }]
    );
    assert_eq!(
        shape(&parse(
            "<table><tr><th>h<td>a<tr><td>b<td><p>c<td>d</table>"
        )),
        "table[tr[th[] td[]] tr[td[] td[p[]] td[]]]"
    );
    assert_eq!(
        shape(&parse("<dl><dt>a<dd>b<dt>c<dd>d</dl>")),
        "dl[dt[] dd[] dt[] dd[]]"
    );
    assert_eq!(
        shape(&parse("<select><option>a<option>b</select>")),
        "select[option[] option[]]"
    );
}

#[test]
fn an_implied_end_stops_at_a_table_a_nested_list_and_a_namespaced_element() {
    // A list item in a table's cell leaves the item round the table open; a
    // nested table's cell leaves the outer cell open; an inner list's item
    // leaves the outer item open.
    assert_eq!(
        shape(&parse("<li>a<table><tr><td><li>b<table><tr><td>c<td>d")),
        "li[table[tr[td[li[table[tr[td[] td[]]]]]]]]"
    );
    // A `table` does end an open paragraph, as in a browser.
    assert_eq!(shape(&parse("<p>a<table><tr><td>b")), "p[] table[tr[td[]]]");
    // A Confluence macro body inside a paragraph keeps its own paragraphs.
    assert_eq!(
        shape(&parse(
            "<p>x<ac:structured-macro><ac:rich-text-body><p>a</p><p>b</p></ac:rich-text-body></ac:structured-macro>y</p>"
        )),
        "p[ac:structured-macro[ac:rich-text-body[p[] p[]]]]"
    );
}

#[test]
fn an_implied_end_ends_where_the_next_tag_starts() {
    let nodes = parse("<p>ab<p>cd</p>");
    let spans: Vec<_> = nodes
        .iter()
        .map(|n| match n {
            Node::Element { span, .. } => *span,
            _ => (0, 0),
        })
        .collect();
    assert_eq!(spans, vec![(0, 5), (5, 14)]);
}

/// Markup anyone can author nests as deep as it likes; the reader must not
/// overflow the stack on it (#2449).
#[test]
fn nesting_past_the_depth_reads_as_the_text_it_holds() {
    let deep = 100_000;
    let markup = format!(
        "{}words{}<p>after</p>",
        "<div><span><b>".repeat(deep / 3),
        "</b></span></div>".repeat(deep / 3)
    );
    assert_eq!(
        plain(&markup),
        vec![
            DocBlock::Paragraph {
                text: vec![strong("words")]
            },
            para("after")
        ]
    );
}

/// The deepest stack a source builds: a hook that reads its children back
/// through the reader inside a sentence, nested past the depth.
#[test]
fn a_hook_reading_back_through_the_reader_stays_bounded_too() {
    let deep = 100_000;
    let markup = format!(
        "{}words{}",
        "<x-note><p><b>".repeat(deep / 3),
        "</b></p></x-note>".repeat(deep / 3)
    );
    let blocks = to_blocks(&markup, &Wiki);
    assert!(format!("{blocks:?}").contains("\"words\""), "{blocks:?}");
}
