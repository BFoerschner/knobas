//! The generated Jira DC contract tables and the matcher over them.
//!
//! `ROUTES`, `RESPONSE_SCHEMAS` and [`ROUTE_COUNT`] come from `build.rs`, which
//! reads them out of the pinned `testenv/specs/jira-dc-rest.wadl`. Nothing in
//! this file is hand-maintained: widening the allowlist means changing the
//! contract document, which the spec pin then refuses until it is reviewed.

include!(concat!(env!("OUT_DIR"), "/jira_contract.rs"));

/// What the contract says about one `(verb, path)`.
#[derive(Debug)]
pub enum Lookup {
    /// The contract defines this route; `query` is every parameter it declares.
    Allowed { query: &'static [&'static str] },
    /// The path exists, this verb on it does not; `allowed` is what does.
    MethodNotAllowed { allowed: Vec<&'static str> },
    /// The contract has no such path at all.
    NotFound,
}

fn segments(p: &str) -> impl Iterator<Item = &str> {
    p.trim_matches('/').split('/')
}

/// A pattern segment matches a request segment when it is a `{template}` (any
/// non-empty value) or is byte-equal.
fn matches(pattern: &str, path: &str) -> bool {
    let pat: Vec<&str> = segments(pattern).collect();
    let req: Vec<&str> = segments(path).collect();
    pat.len() == req.len()
        && pat.iter().zip(&req).all(|(pat, seg)| {
            (pat.starts_with('{') && pat.ends_with('}') && !seg.is_empty()) || pat == seg
        })
}

fn is_template(pattern: &str) -> bool {
    pattern.contains('{')
}

/// The contract's verdict on `verb` + `path`.
///
/// `verb` is upper-case; `path` is the URL path with the `/rest/` prefix
/// already stripped, e.g. `"api/2/issue/PAY-231"`.
pub fn lookup(verb: &str, path: &str) -> Lookup {
    let path = path.trim_end_matches('/');

    // A literal route wins over a templated one, so `api/2/issue/picker` never
    // resolves as `api/2/issue/{issueIdOrKey}` and pick up its query set.
    let literal = ROUTES
        .iter()
        .find(|(v, p, _)| *v == verb && !is_template(p) && matches(p, path));
    let hit = literal.or_else(|| {
        ROUTES
            .iter()
            .find(|(v, p, _)| *v == verb && is_template(p) && matches(p, path))
    });
    if let Some((_, _, q)) = hit {
        return Lookup::Allowed { query: q };
    }

    let allowed: Vec<&'static str> = ROUTES
        .iter()
        .filter(|(_, p, _)| matches(p, path))
        .map(|(v, _, _)| *v)
        .collect();
    if allowed.is_empty() {
        Lookup::NotFound
    } else {
        Lookup::MethodNotAllowed { allowed }
    }
}

/// The WADL's embedded JSON Schema for the 200 response of an endpoint mockd
/// implements, keyed by the WADL's own template path (`"api/2/search"`).
pub fn response_schema(verb: &str, template_path: &str) -> Option<&'static str> {
    RESPONSE_SCHEMAS
        .iter()
        .find(|(v, p, _)| *v == verb && *p == template_path)
        .map(|(_, _, s)| *s)
}
