//! The JQL subset mockd accepts.
//!
//! Deliberately exactly the interfaces doc §5 subset, so an adapter that needs
//! more has to request it rather than discover that the mock happened to be
//! lenient:
//!
//! ```text
//! jql      := [ clause { "AND" clause } ] [ order ]
//! order    := "ORDER" "BY" ( "updated" | "created" ) [ "ASC" | "DESC" ] [ "," "id" "ASC" ]
//! clause   := "updated" ( ">=" | ">" ) literal
//!           | "project" ( "=" ident | "IN" "(" ident { "," ident } ")" )
//!           | "key" "=" ( ident | '"' ident '"' )
//! literal  := '"' ( "yyyy-MM-dd HH:mm" | "yyyy/MM/dd HH:mm" | "yyyy-MM-dd" | "yyyy/MM/dd" ) '"'
//! ```
//!
//! Keywords are case-insensitive. An empty or absent `jql` matches every issue.
//! The clause list is optional, so a bare `ORDER BY created ASC` with no
//! `WHERE`-equivalent is valid — that is the full-sync query stream A sends
//! (`created` since #1623; `updated` stays for the ceiling probe's `DESC` and
//! for the queries written before it). The `, id ASC` the walk appends since
//! #1720 is accepted and changes nothing: [`crate::jira`] has always broken a
//! tie on the first key by id ascending, which is the order it asks for.
//! `key = "…"` since #2097 is the Jira adapter's refresh, one issue by its
//! key, as #1623 grew this subset for the walk's order: the adapter's tests
//! against mockd send the query the adapter sends.
//!
//! **Date literals are interpreted in the server's zone**
//! ([`MockState::server_offset`](crate::state::MockState::server_offset),
//! default `+02:00`), never in UTC. That is real JQL behaviour, and it is why
//! `serverInfo.serverTime` carries an offset an adapter must read: a watermark
//! formatted in UTC and sent to a `+02:00` server asks about a two-hour-earlier
//! instant on every incremental run.

use chrono::{DateTime, Duration, FixedOffset, NaiveDate, NaiveDateTime, TimeZone, Utc};

/// A parsed query from the accepted subset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Jql {
    /// Lower bound on `updated`, already resolved into UTC from the server's
    /// zone. Inclusive: `>` is normalised to `>= literal + 1 minute`, because
    /// JQL's time resolution is one minute and that is what a real instance
    /// compares against.
    pub updated_gte: Option<DateTime<Utc>>,
    /// Project keys to keep; empty means every project.
    pub projects: Vec<String>,
    /// The one issue key to keep (#2097); `None` keeps every issue.
    pub key: Option<String>,
    /// The field the query sorts by: `updated` unless it said `ORDER BY
    /// created`.
    pub order_by: SortKey,
    /// `true` unless the query said `ASC`. Newest-first is Jira's own default
    /// for an unordered query.
    pub desc: bool,
}

/// What `ORDER BY` names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortKey {
    Updated,
    /// The one key an edit cannot move (#1623): what the Jira adapter's walk
    /// pages over.
    Created,
}

impl Default for Jql {
    fn default() -> Self {
        Self {
            updated_gte: None,
            projects: Vec::new(),
            key: None,
            order_by: SortKey::Updated,
            desc: true,
        }
    }
}

/// Why a query was refused. Both variants name the offending text, because
/// that string ends up in the 400 body *and* in the violation's detail, which
/// is what makes a failing adapter test legible.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JqlError {
    Unsupported(String),
    BadDate(String),
}

impl std::fmt::Display for JqlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unsupported(t) => write!(
                f,
                "Unsupported JQL near {t:?}: mockd accepts only \
                 [updated >=|> \"date\"] [AND project =|IN (...)] [AND key = KEY] [ORDER BY updated|created ASC|DESC[, id ASC]]"
            ),
            Self::BadDate(t) => write!(
                f,
                "Unparseable date literal {t:?}: expected yyyy-MM-dd HH:mm, \
                 yyyy/MM/dd HH:mm, yyyy-MM-dd or yyyy/MM/dd"
            ),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Token {
    text: String,
    /// Came out of a `"…"` literal, so it is a value and never a keyword.
    quoted: bool,
}

fn tokenize(raw: &str) -> Result<Vec<Token>, JqlError> {
    let mut out = Vec::new();
    let mut chars = raw.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            c if c.is_whitespace() => {}
            '"' => {
                let mut s = String::new();
                loop {
                    match chars.next() {
                        Some('"') => break,
                        Some(c) => s.push(c),
                        None => return Err(JqlError::Unsupported(format!("\"{s}"))),
                    }
                }
                out.push(Token {
                    text: s,
                    quoted: true,
                });
            }
            '(' | ')' | ',' => out.push(Token {
                text: c.to_string(),
                quoted: false,
            }),
            '>' | '<' | '=' | '!' | '~' => {
                let mut s = c.to_string();
                if chars.peek() == Some(&'=') {
                    s.push(chars.next().expect("peeked"));
                }
                out.push(Token {
                    text: s,
                    quoted: false,
                });
            }
            _ => {
                let mut s = c.to_string();
                while let Some(&n) = chars.peek() {
                    if n.is_whitespace() || "(),><=!~\"".contains(n) {
                        break;
                    }
                    s.push(n);
                    chars.next();
                }
                out.push(Token {
                    text: s,
                    quoted: false,
                });
            }
        }
    }
    Ok(out)
}

/// The date literal formats a real Jira DC accepts, in the order they are tried.
const DATE_TIME_FMTS: &[&str] = &["%Y-%m-%d %H:%M", "%Y/%m/%d %H:%M"];
const DATE_FMTS: &[&str] = &["%Y-%m-%d", "%Y/%m/%d"];

fn parse_literal(raw: &str, off: FixedOffset) -> Result<DateTime<Utc>, JqlError> {
    let naive = DATE_TIME_FMTS
        .iter()
        .find_map(|f| NaiveDateTime::parse_from_str(raw, f).ok())
        .or_else(|| {
            DATE_FMTS
                .iter()
                .find_map(|f| NaiveDate::parse_from_str(raw, f).ok())
                .map(|d| d.and_hms_opt(0, 0, 0).expect("midnight is a valid time"))
        })
        .ok_or_else(|| JqlError::BadDate(raw.to_owned()))?;
    // Resolved in the *server's* zone, not UTC. A fixed offset has no DST gap
    // or fold, so `single()` is always `Some` — asserted rather than silently
    // unwrapped, because moving to a named timezone later would change that.
    let local = off.from_local_datetime(&naive).single().unwrap_or_else(|| {
        panic!("a fixed offset has no ambiguous local times, but {naive} was one")
    });
    Ok(local.with_timezone(&Utc))
}

/// Parses `raw` against the accepted subset. `off` is the server's zone: JQL
/// date literals are resolved in it, never in UTC.
pub fn parse_jql(raw: &str, off: FixedOffset) -> Result<Jql, JqlError> {
    let toks = tokenize(raw)?;
    let mut jql = Jql::default();
    let mut i = 0;

    let kw = |t: &Token| (!t.quoted).then(|| t.text.to_ascii_lowercase());
    let is_kw = |t: Option<&Token>, want: &str| t.and_then(kw).as_deref() == Some(want);

    // The clause list is optional and may be followed by ORDER BY.
    while i < toks.len() && !is_kw(toks.get(i), "order") {
        if !jql.projects.is_empty() || jql.updated_gte.is_some() || jql.key.is_some() {
            // Every clause after the first must be joined with AND.
            if !is_kw(toks.get(i), "and") {
                return Err(JqlError::Unsupported(toks[i].text.clone()));
            }
            i += 1;
        }
        let field = toks
            .get(i)
            .and_then(kw)
            .ok_or_else(|| JqlError::Unsupported(unexpected(&toks, i)))?;
        i += 1;
        match field.as_str() {
            "updated" => {
                let op = toks
                    .get(i)
                    .and_then(kw)
                    .ok_or_else(|| JqlError::Unsupported(unexpected(&toks, i)))?;
                i += 1;
                if op != ">=" && op != ">" {
                    return Err(JqlError::Unsupported(op));
                }
                let lit = toks
                    .get(i)
                    .filter(|t| t.quoted)
                    .ok_or_else(|| JqlError::Unsupported(unexpected(&toks, i)))?;
                i += 1;
                let mut at = parse_literal(&lit.text, off)?;
                if op == ">" {
                    // JQL compares at minute resolution, so strictly-greater
                    // than a minute means from the next minute on.
                    at += Duration::minutes(1);
                }
                jql.updated_gte = Some(at);
            }
            "key" => {
                if !is_kw(toks.get(i), "=") {
                    return Err(JqlError::Unsupported(unexpected(&toks, i)));
                }
                i += 1;
                let v = toks
                    .get(i)
                    .ok_or_else(|| JqlError::Unsupported(unexpected(&toks, i)))?;
                i += 1;
                jql.key = Some(v.text.to_ascii_uppercase());
            }
            "project" => {
                let op = toks
                    .get(i)
                    .and_then(kw)
                    .ok_or_else(|| JqlError::Unsupported(unexpected(&toks, i)))?;
                i += 1;
                match op.as_str() {
                    "=" => {
                        let v = toks
                            .get(i)
                            .ok_or_else(|| JqlError::Unsupported(unexpected(&toks, i)))?;
                        i += 1;
                        jql.projects.push(v.text.to_ascii_uppercase());
                    }
                    "in" => {
                        if !is_kw(toks.get(i), "(") {
                            return Err(JqlError::Unsupported(unexpected(&toks, i)));
                        }
                        i += 1;
                        loop {
                            let v = toks
                                .get(i)
                                .ok_or_else(|| JqlError::Unsupported(unexpected(&toks, i)))?;
                            if v.text == ")" {
                                return Err(JqlError::Unsupported("()".to_owned()));
                            }
                            jql.projects.push(v.text.to_ascii_uppercase());
                            i += 1;
                            match toks.get(i).map(|t| t.text.as_str()) {
                                Some(",") => i += 1,
                                Some(")") => {
                                    i += 1;
                                    break;
                                }
                                _ => return Err(JqlError::Unsupported(unexpected(&toks, i))),
                            }
                        }
                    }
                    other => return Err(JqlError::Unsupported(other.to_owned())),
                }
            }
            other => return Err(JqlError::Unsupported(other.to_owned())),
        }
    }

    if i < toks.len() {
        // ORDER BY (updated|created) [ASC|DESC] [, id ASC]
        i += 1; // "order"
        if !is_kw(toks.get(i), "by") {
            return Err(JqlError::Unsupported(unexpected(&toks, i)));
        }
        i += 1;
        jql.order_by = if is_kw(toks.get(i), "updated") {
            SortKey::Updated
        } else if is_kw(toks.get(i), "created") {
            SortKey::Created
        } else {
            return Err(JqlError::Unsupported(unexpected(&toks, i)));
        };
        i += 1;
        match toks.get(i).and_then(kw).as_deref() {
            None | Some(",") => {}
            Some("asc") => {
                jql.desc = false;
                i += 1;
            }
            Some("desc") => {
                jql.desc = true;
                i += 1;
            }
            Some(other) => return Err(JqlError::Unsupported(other.to_owned())),
        }
        // The tiebreak the search already applies, so it is parsed and kept
        // nowhere (#1720).
        if toks.get(i).is_some_and(|t| !t.quoted && t.text == ",") {
            i += 1;
            if !is_kw(toks.get(i), "id") || !is_kw(toks.get(i + 1), "asc") {
                return Err(JqlError::Unsupported(unexpected(&toks, i)));
            }
            i += 2;
        }
        if i < toks.len() {
            return Err(JqlError::Unsupported(toks[i].text.clone()));
        }
    }

    Ok(jql)
}

fn unexpected(toks: &[Token], i: usize) -> String {
    toks.get(i)
        .map_or_else(|| "end of query".to_owned(), |t| t.text.clone())
}
