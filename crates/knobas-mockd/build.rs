//! Generates the Jira DC request allowlist and the response-schema table from
//! the vendored WADL. See `src/allowlist.rs` for how the tables are consumed
//! and `docs/superpowers/plans/2026-08-24-m1-plan-03-testenv.md` Task 2 for the
//! four properties of the document this parser relies on:
//!
//! 1. `<resources base=".../rest/">` — every resource path is relative to
//!    `/rest/`, which mockd strips before lookup.
//! 2. Resources nest and a child's `path` carries no leading separator, so the
//!    join is `parent/child` with one separator inserted.
//! 3. Every query parameter lives under `<method><request>`; the document has
//!    no resource-level query params, no matrix params and no Jersey regex
//!    templates, so the matcher needs none of that.
//! 4. Four `(verb, path)` pairs are overloaded (the avatar upload's
//!    multipart twin), so duplicate keys **union** their query sets.

use std::collections::{BTreeMap, BTreeSet};
use std::{env, fs, path::PathBuf};

use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};

const WADL: &str = "../../testenv/specs/jira-dc-rest.wadl";

/// The endpoints mockd actually serves. Only their schemas are embedded — all
/// 259 would be ~1 MB of `&'static str` for no benefit.
const SERVED: &[(&str, &str)] = &[
    ("GET", "api/2/search"),
    ("GET", "api/2/serverInfo"),
    ("GET", "api/2/myself"),
    ("GET", "api/2/issue/{issueIdOrKey}"),
    ("GET", "api/2/issue/{issueIdOrKey}/comment"),
    ("GET", "api/2/issue/{issueIdOrKey}/worklog"),
];

/// Marks the WADL's embedded JSON *Schema* apart from its embedded *example*:
/// both live in an xhtml `<code>` block, only the schema carries this id.
const SCHEMA_MARKER: &str = r#""id":"https://docs.atlassian.com/jira/REST/schema/"#;

type Key = (String, String);

fn join(parent: &str, child: &str) -> String {
    if parent.is_empty() {
        child.trim_start_matches('/').to_owned()
    } else {
        format!(
            "{}/{}",
            parent.trim_end_matches('/'),
            child.trim_start_matches('/')
        )
    }
}

/// One spelling per path: the WADL has a handful of trailing-slash resources
/// (`api/2/auditing/`, `api/2/user/properties/`).
fn normalize(path: &str) -> String {
    path.trim_end_matches('/').to_owned()
}

fn local_name(e: &BytesStart<'_>) -> String {
    String::from_utf8_lossy(e.local_name().as_ref()).into_owned()
}

fn attr(e: &BytesStart<'_>, k: &str) -> Option<String> {
    e.attributes().flatten().find_map(|a| {
        (a.key.local_name().as_ref() == k.as_bytes())
            .then(|| String::from_utf8_lossy(&a.value).into_owned())
    })
}

#[derive(Default)]
struct Parse {
    /// (VERB, path) -> allowed query params; overloads union (fact 4).
    routes: BTreeMap<Key, BTreeSet<String>>,
    /// (VERB, path) -> the 200 response's embedded JSON Schema.
    schemas: BTreeMap<Key, String>,
    /// Resource paths, innermost last.
    stack: Vec<String>,
    /// The method currently being read.
    cur: Option<Key>,
    in_request: bool,
    resp_200: bool,
    /// An `<xhtml:code>` block being collected.
    code_text: Option<String>,
}

impl Parse {
    /// `is_empty` distinguishes `<x/>` from `<x>`: a self-closing element gets
    /// no matching `End`, so whatever it opened has to be closed here. A
    /// self-closing `<resource/>` pushed and never popped would corrupt every
    /// path after it.
    fn start(&mut self, e: &BytesStart<'_>, is_empty: bool) {
        match local_name(e).as_str() {
            "resource" => {
                let parent = self.stack.last().cloned().unwrap_or_default();
                self.stack
                    .push(join(&parent, &attr(e, "path").unwrap_or_default()));
                if is_empty {
                    self.stack.pop();
                }
            }
            "method" => {
                let verb = attr(e, "name").unwrap_or_default().to_ascii_uppercase();
                let path = self.stack.last().cloned().unwrap_or_default();
                let key = (verb, normalize(&path));
                self.routes.entry(key.clone()).or_default();
                self.cur = if is_empty { None } else { Some(key) };
            }
            "request" => self.in_request = !is_empty,
            "response" => {
                self.resp_200 = !is_empty && attr(e, "status").as_deref() == Some("200");
            }
            "param" => {
                if self.in_request
                    && attr(e, "style").as_deref() == Some("query")
                    && let (Some(k), Some(n)) = (self.cur.as_ref(), attr(e, "name"))
                {
                    self.routes.entry(k.clone()).or_default().insert(n);
                }
            }
            "code" if self.resp_200 && !is_empty => self.code_text = Some(String::new()),
            _ => {}
        }
    }

    fn end(&mut self, name: &str) {
        match name {
            "resource" => {
                self.stack.pop();
            }
            "method" => self.cur = None,
            "request" => self.in_request = false,
            "response" => self.resp_200 = false,
            "code" => {
                if let (Some(text), Some(k)) = (self.code_text.take(), self.cur.as_ref())
                    && text.contains(SCHEMA_MARKER)
                    && SERVED.iter().any(|(v, p)| *v == k.0 && *p == k.1)
                {
                    self.schemas.insert(k.clone(), text);
                }
            }
            _ => {}
        }
    }
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed={WADL}");
    let xml = fs::read_to_string(WADL).unwrap_or_else(|e| panic!("{WADL}: {e}"));

    let mut p = Parse::default();
    let mut r = Reader::from_str(&xml);
    r.config_mut().trim_text(false);
    loop {
        match r.read_event().expect("WADL must parse") {
            Event::Eof => break,
            Event::Start(e) => p.start(&e, false),
            Event::Empty(e) => p.start(&e, true),
            Event::End(e) => {
                let name = String::from_utf8_lossy(e.local_name().as_ref()).into_owned();
                p.end(&name);
            }
            Event::Text(t) => {
                if let Some(s) = p.code_text.as_mut() {
                    s.push_str(&t.xml10_content().expect("WADL text must be UTF-8"));
                }
            }
            Event::CData(t) => {
                if let Some(s) = p.code_text.as_mut() {
                    s.push_str(&String::from_utf8_lossy(&t));
                }
            }
            // quick-xml splits text at entity references and reports them
            // separately, so a schema containing `&amp;` would otherwise lose
            // the character silently and produce JSON that no longer parses.
            Event::GeneralRef(t) => {
                if let Some(s) = p.code_text.as_mut() {
                    match t.resolve_char_ref().expect("valid character reference") {
                        Some(c) => s.push(c),
                        None => {
                            let name = t.decode().expect("entity name must be UTF-8");
                            s.push(match name.as_ref() {
                                "amp" => '&',
                                "lt" => '<',
                                "gt" => '>',
                                "quot" => '"',
                                "apos" => '\'',
                                other => panic!("unhandled XML entity &{other}; in a schema"),
                            });
                        }
                    }
                }
            }
            _ => {}
        }
    }

    assert!(
        p.routes.len() >= 380,
        "only {} routes parsed -- check the nesting walk",
        p.routes.len()
    );
    assert_eq!(
        p.schemas.len(),
        SERVED.len(),
        "missing embedded schemas: got {:?}",
        p.schemas.keys().collect::<Vec<_>>()
    );

    let out = PathBuf::from(env::var("OUT_DIR").unwrap()).join("jira_contract.rs");
    let mut src = String::new();
    src.push_str("pub const ROUTE_COUNT: usize = ");
    src.push_str(&p.routes.len().to_string());
    src.push_str(";\npub static ROUTES: &[(&str, &str, &[&str])] = &[\n");
    for ((verb, path), q) in &p.routes {
        let params = q
            .iter()
            .map(|x| format!("{x:?}"))
            .collect::<Vec<_>>()
            .join(", ");
        src.push_str(&format!("    ({verb:?}, {path:?}, &[{params}]),\n"));
    }
    src.push_str("];\npub static RESPONSE_SCHEMAS: &[(&str, &str, &str)] = &[\n");
    for ((verb, path), schema) in &p.schemas {
        assert!(
            !schema.contains("\"####"),
            "schema for {verb} {path} breaks the raw-string fence"
        );
        src.push_str(&format!(
            "    ({verb:?}, {path:?}, r####\"{schema}\"####),\n"
        ));
    }
    src.push_str("];\n");
    fs::write(&out, src).unwrap();
}
