//! The other forms of an item's web address an adapter declares (#1475).
//!
//! A source serves one page under more than one address. Confluence Data
//! Center 9 reports a page as `/spaces/ENG/pages/360458/SEPA+payout+retry+design`
//! and also serves it as `/pages/viewpage.action?pageId=360458`, as
//! `/display/ENG/SEPA+payout+retry+design` and as a tiny link `/x/CoAF`. The
//! mirror stores the one form the adapter reported (`sync.item.web_url`), and
//! a paste of any other form used to miss.
//!
//! Spec #491 decided the remedy in advance: no per-adapter URL parser, but a
//! **descriptor slot** -- a declaration, as [`SourceDescriptor::payload_paths`]
//! is one, that the paste resolver reads without knowing which adapter wrote
//! it. A [`UrlForm`] says: *an address of this shape, after the source's base
//! URL, names the item of this kind whose payload holds these values*.
//!
//! **Read on the Rust side, compared in the database.** [`UrlForm::read`] only
//! splits a pasted address into a base and the values its placeholders hold.
//! Whether that base *is* a source's base URL is decided in SQL, through the
//! same `web_url_normalized!` rule every stored address goes through
//! (`knobas_core::web_url` says why the rule has one implementation), and
//! whether an item holds those values is a payload comparison in the same
//! statement. So the resolver stays one read on the local mirror, never a
//! fetch (spec #491 story 16).
//!
//! [`SourceDescriptor::payload_paths`]: crate::SourceDescriptor::payload_paths

use knobas_spi_types::payload::PayloadPath;

/// One more form an item's page is served under, beside the `web_url` the
/// adapter reports.
///
/// `template` is the address **after the source's base URL**, starting with
/// `/`: literal text, with `{}` for one value and at most one `{*}`, last, for
/// the rest of the address. `{*}` alone is the whole address after the base,
/// its `/` included: a payload that keeps a relative address (Confluence's
/// tiny link, `/x/CoAF`) is matched by it. `fields[i]` is where the item's payload holds the
/// value of the template's `i`-th placeholder.
///
/// * `{}` ends at the next `/`, `?`, `&` or `#`, and its text is
///   form-decoded before it is compared (`%2B` is `+`, `+` is a space): a
///   Confluence title in a `/display/` link is spelled that way.
/// * `{*}` is the rest of the address, fragment aside, compared as written:
///   it stands for a stored *address* (Confluence's `_links.tinyui`), and an
///   address compared to an address is not decoded.
///
/// After the template the pasted address may go on only with a query
/// (`?…`, `&…`) or a trailing `/`: `…?pageId=360458&focusedCommentId=7` is
/// still the page.
///
/// The contract battery holds a declaration to [`check`].
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export_to = "sources.ts")]
pub struct UrlForm {
    /// The entity kind of the items this form names; one of the descriptor's
    /// `entity_kinds`.
    pub kind: String,
    /// The address after the base URL, e.g. `/pages/viewpage.action?pageId={}`.
    pub template: String,
    /// Where the payload holds each placeholder's value, in template order.
    pub fields: Vec<PayloadPath>,
}

/// One way a pasted address reads as a [`UrlForm`]: the base URL it was
/// pasted under, and the value each declared field must hold.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Reading {
    /// Everything before the template: compared with a source's base URL
    /// under the stored-address rule, never here.
    pub base: String,
    /// The form's [`UrlForm::kind`].
    pub kind: String,
    /// Each declared field and the value it must hold.
    pub fields: Vec<FieldValue>,
}

/// A declared field and the value a pasted address gives it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct FieldValue {
    pub path: PayloadPath,
    pub value: String,
}

/// A piece of a parsed template.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Piece<'a> {
    Literal(&'a str),
    One,
    Rest,
}

const ONE: &str = "{}";
const REST: &str = "{*}";

fn pieces(template: &str) -> Vec<Piece<'_>> {
    let mut out = Vec::new();
    let mut rest = template;
    while !rest.is_empty() {
        let next = [rest.find(ONE), rest.find(REST)]
            .into_iter()
            .flatten()
            .min();
        match next {
            Some(0) if rest.starts_with(ONE) => {
                out.push(Piece::One);
                rest = &rest[ONE.len()..];
            }
            Some(0) => {
                out.push(Piece::Rest);
                rest = &rest[REST.len()..];
            }
            Some(at) => {
                out.push(Piece::Literal(&rest[..at]));
                rest = &rest[at..];
            }
            None => {
                out.push(Piece::Literal(rest));
                rest = "";
            }
        }
    }
    out
}

impl UrlForm {
    /// A form from its parts, for an adapter's descriptor.
    pub fn of<I>(kind: &str, template: &str, fields: I) -> Self
    where
        I: IntoIterator<Item = PayloadPath>,
    {
        UrlForm {
            kind: kind.to_owned(),
            template: template.to_owned(),
            fields: fields.into_iter().collect(),
        }
    }

    /// Every way `url` reads as this form: one [`Reading`] per base URL the
    /// template fits after, shortest base first. Empty for an address that
    /// is not this form, and for a declaration [`check`] refuses.
    #[must_use]
    pub fn read(&self, url: &str) -> Vec<Reading> {
        if check_form(self).is_err() {
            return Vec::new();
        }
        let url = url.split('#').next().unwrap_or_default();
        let Some(authority) = url.find("://").map(|at| at + 3) else {
            return Vec::new();
        };
        // The base is a scheme, a host and perhaps a path: never a query.
        let end = url.find('?').unwrap_or(url.len());
        let pieces = pieces(&self.template);
        url[authority..end]
            .match_indices('/')
            .map(|(at, _)| authority + at)
            .filter_map(|at| {
                let values = fit(&pieces, &url[at..])?;
                Some(Reading {
                    base: url[..at].to_owned(),
                    kind: self.kind.clone(),
                    fields: self
                        .fields
                        .iter()
                        .cloned()
                        .zip(values)
                        .map(|(path, value)| FieldValue { path, value })
                        .collect(),
                })
            })
            .collect()
    }
}

/// The placeholders' values when `rest` is the template, or `None`.
fn fit(pieces: &[Piece<'_>], mut rest: &str) -> Option<Vec<String>> {
    let mut values = Vec::new();
    for piece in pieces {
        match piece {
            Piece::Literal(text) => rest = rest.strip_prefix(text)?,
            Piece::One => {
                let end = rest.find(['/', '?', '&', '#']).unwrap_or(rest.len());
                if end == 0 {
                    return None;
                }
                values.push(form_decoded(&rest[..end])?);
                rest = &rest[end..];
            }
            Piece::Rest => {
                if rest.is_empty() {
                    return None;
                }
                values.push(rest.to_owned());
                rest = "";
            }
        }
    }
    // What may follow the form: nothing, a trailing slash, or more query.
    (rest.is_empty() || rest == "/" || rest.starts_with(['?', '&'])).then_some(values)
}

/// `application/x-www-form-urlencoded` decoding: `+` is a space, `%XX` a
/// byte. `None` for a malformed escape or bytes that are not UTF-8, which
/// is a miss rather than a guess.
fn form_decoded(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        match bytes[at] {
            b'+' => out.push(b' '),
            b'%' => {
                let hex = text.get(at + 1..at + 3)?;
                out.push(u8::from_str_radix(hex, 16).ok()?);
                at += 2;
            }
            byte => out.push(byte),
        }
        at += 1;
    }
    String::from_utf8(out).ok()
}

/// One form's own rules: see [`check`].
fn check_form(form: &UrlForm) -> Result<(), String> {
    let template = &form.template;
    if !template.starts_with('/') && template != REST {
        return Err(format!(
            "{template:?} must start with `/`, or be `{{*}}` alone: it is the address after the base URL"
        ));
    }
    let pieces = pieces(template);
    let placeholders = pieces
        .iter()
        .filter(|p| !matches!(p, Piece::Literal(_)))
        .count();
    if placeholders == 0 {
        return Err(format!(
            "{template:?} has no placeholder, so it would name every item of its kind"
        ));
    }
    if placeholders != form.fields.len() {
        return Err(format!(
            "{template:?} has {placeholders} placeholder(s) and declares {} field(s)",
            form.fields.len()
        ));
    }
    if let Some(at) = pieces.iter().position(|p| *p == Piece::Rest)
        && at != pieces.len() - 1
    {
        return Err(format!(
            "{template:?}: `{{*}}` is the rest of the address, so it comes last"
        ));
    }
    if pieces
        .windows(2)
        .any(|pair| !matches!(pair[0], Piece::Literal(_)) && !matches!(pair[1], Piece::Literal(_)))
    {
        return Err(format!(
            "{template:?}: two placeholders in a row cannot be told apart"
        ));
    }
    if form.fields.iter().any(|path| path.segments().is_empty()) {
        return Err(format!(
            "{template:?}: an empty path is the whole record, never a value"
        ));
    }
    Ok(())
}

/// A descriptor's [`url_forms`](crate::SourceDescriptor::url_forms), held to
/// their rules: each names a declared kind, starts with `/`, has at least one
/// placeholder and exactly one field per placeholder, puts `{*}` last if at
/// all, never sets two placeholders side by side, and names no empty path.
///
/// # Errors
///
/// The first form that breaks a rule, and which rule.
pub fn check(descriptor: &crate::SourceDescriptor) -> Result<(), String> {
    for form in &descriptor.url_forms {
        if !descriptor.entity_kinds.iter().any(|k| k.id == form.kind) {
            return Err(format!(
                "{:?} names kind {:?}, which the descriptor does not emit",
                form.template, form.kind
            ));
        }
        check_form(form)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(segments: &[&str]) -> PayloadPath {
        PayloadPath::of(segments.iter().copied())
    }

    fn page_id() -> UrlForm {
        UrlForm::of("page", "/pages/viewpage.action?pageId={}", [path(&["id"])])
    }

    fn display() -> UrlForm {
        UrlForm::of(
            "page",
            "/display/{}/{}",
            [path(&["space", "key"]), path(&["title"])],
        )
    }

    fn tiny() -> UrlForm {
        UrlForm::of("page", "{*}", [path(&["_links", "tinyui"])])
    }

    fn values(readings: &[Reading]) -> Vec<(String, Vec<String>)> {
        readings
            .iter()
            .map(|r| {
                (
                    r.base.clone(),
                    r.fields.iter().map(|f| f.value.clone()).collect(),
                )
            })
            .collect()
    }

    #[test]
    fn a_query_form_reads_its_value_and_its_base() {
        assert_eq!(
            values(&page_id().read("http://wiki.example:8090/pages/viewpage.action?pageId=360458")),
            [(
                "http://wiki.example:8090".to_owned(),
                vec!["360458".to_owned()]
            )]
        );
    }

    #[test]
    fn a_base_with_a_context_path_is_kept_whole() {
        assert_eq!(
            values(&page_id().read("https://example.com/wiki/pages/viewpage.action?pageId=7")),
            [("https://example.com/wiki".to_owned(), vec!["7".to_owned()])]
        );
    }

    #[test]
    fn more_query_a_fragment_or_a_trailing_slash_still_read() {
        for url in [
            "http://w/pages/viewpage.action?pageId=360458&focusedCommentId=9",
            "http://w/pages/viewpage.action?pageId=360458#comment-9",
        ] {
            assert_eq!(values(&page_id().read(url))[0].1, ["360458"], "{url}");
        }
        assert_eq!(
            values(&display().read("http://w/display/ENG/Title/"))[0].1,
            ["ENG", "Title"]
        );
    }

    #[test]
    fn a_segment_is_form_decoded() {
        assert_eq!(
            values(&display().read("http://w/display/ENG/SEPA+payout+retry+design%3F%2B")),
            [(
                "http://w".to_owned(),
                vec!["ENG".to_owned(), "SEPA payout retry design?+".to_owned()]
            )]
        );
    }

    #[test]
    fn the_rest_is_compared_as_written() {
        // Every split is a reading: the database keeps the one whose base is a
        // source's.
        assert_eq!(
            values(&tiny().read("https://w.example/wiki/x/CoAF")),
            [
                (
                    "https://w.example".to_owned(),
                    vec!["/wiki/x/CoAF".to_owned()]
                ),
                (
                    "https://w.example/wiki".to_owned(),
                    vec!["/x/CoAF".to_owned()]
                ),
                (
                    "https://w.example/wiki/x".to_owned(),
                    vec!["/CoAF".to_owned()]
                ),
            ]
        );
    }

    #[test]
    fn another_shape_reads_as_nothing() {
        for url in [
            "http://w/pages/viewpage.action?pageIds=1",
            "http://w/pages/viewpage.action?pageId=",
            "http://w/display/ENG",
            "http://w/display/ENG/Title/history",
            "http://w/display/ENG/Bad%ZZ",
            "not a url",
        ] {
            assert!(
                page_id().read(url).is_empty() && display().read(url).is_empty(),
                "{url}"
            );
        }
    }

    #[test]
    fn a_malformed_declaration_reads_as_nothing_and_is_refused() {
        for (form, rule) in [
            (
                UrlForm::of("page", "pages/{}", [path(&["id"])]),
                "start with `/`",
            ),
            (UrlForm::of("page", "/pages/all", []), "no placeholder"),
            (UrlForm::of("page", "/pages/{}", []), "declares 0 field"),
            (
                UrlForm::of("page", "/x/{*}/y/{}", [path(&["a"]), path(&["b"])]),
                "comes last",
            ),
            (
                UrlForm::of("page", "/x/{}{}", [path(&["a"]), path(&["b"])]),
                "in a row",
            ),
            (UrlForm::of("page", "/x/{}", [path(&[])]), "empty path"),
            (
                UrlForm::of("page", "x{*}", [path(&["a"])]),
                "start with `/`",
            ),
        ] {
            let error = check_form(&form).expect_err(&form.template);
            assert!(error.contains(rule), "{error}");
            assert!(
                form.read("http://w/pages/1").is_empty(),
                "{}",
                form.template
            );
        }
    }
}
