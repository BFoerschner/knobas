//! `translate_query` (#1208; *Make an editable copy*, #880, R-3.25): a Jira
//! view's JQL read into the conditions knobas can test, and the parts it
//! leaves out, each with the reason in prototype-6's words ("`component =
//! on-call` (knobas does not sync components)"). Moved here from the shell's
//! `jql.ts` unchanged: the shell holds no JQL parser.
//!
//! A part is one clause the query joins with a top-level `AND`; `ORDER BY` is
//! not one. A clause is kept when it is one field, `=`, `!=`, `in` or `not in`
//! (or `updated >=` a recent window) over values the search grammar reads.
//! Anything else stays out whole, never half-translated: an `OR`, a `NOT`,
//! parentheses, a Jira function other than `currentUser()` and
//! `openSprints()`, a field knobas does not sync, a negation over a field a
//! ticket may leave empty.
//!
//! Positions count `char`s, where the shell counted UTF-16 units; a clause's
//! text is the same slice of the query either way.

use knobas_source::query::{Condition, Key, LeftOut, Translation};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Word,
    Str,
    Op,
    Open,
    Close,
    Comma,
}

#[derive(Debug, Clone)]
struct Token {
    kind: Kind,
    text: String,
    start: usize,
    end: usize,
}

struct Clause {
    text: String,
    tokens: Vec<Token>,
}

/// JavaScript's `\s`: Unicode white space and the byte-order mark.
fn is_space(c: char) -> bool {
    c.is_whitespace() || c == '\u{feff}'
}

fn slice(chars: &[char], from: usize, to: usize) -> String {
    let to = to.min(chars.len());
    if to <= from {
        return String::new();
    }
    chars[from..to].iter().collect()
}

fn tokenize(chars: &[char]) -> Vec<Token> {
    let mut tokens = Vec::new();
    let len = chars.len();
    let mut at = 0;
    while at < len {
        let ch = chars[at];
        if is_space(ch) {
            at += 1;
            continue;
        }
        let start = at;
        if ch == '"' || ch == '\'' {
            at += 1;
            while at < len && chars[at] != ch {
                at += if chars[at] == '\\' { 2 } else { 1 };
            }
            at = (at + 1).min(len);
            let text = slice(chars, start + 1, at.saturating_sub(1));
            tokens.push(Token {
                kind: Kind::Str,
                text,
                start,
                end: at,
            });
        } else if ch == '(' || ch == ')' || ch == ',' {
            at += 1;
            let kind = match ch {
                '(' => Kind::Open,
                ')' => Kind::Close,
                _ => Kind::Comma,
            };
            tokens.push(Token {
                kind,
                text: ch.to_string(),
                start,
                end: at,
            });
        } else if "=!~<>".contains(ch) {
            at += if matches!(chars.get(at + 1), Some('=' | '~')) {
                2
            } else {
                1
            };
            let text = slice(chars, start, at);
            tokens.push(Token {
                kind: Kind::Op,
                text,
                start,
                end: at.min(len),
            });
        } else {
            while at < len && !is_space(chars[at]) && !"(),=!~<>\"'".contains(chars[at]) {
                at += 1;
            }
            let text = slice(chars, start, at);
            tokens.push(Token {
                kind: Kind::Word,
                text,
                start,
                end: at,
            });
        }
    }
    tokens
}

fn is_word(token: Option<&Token>, word: &str) -> bool {
    token.is_some_and(|t| t.kind == Kind::Word && t.text.to_lowercase() == word)
}

/// The query's parts: split on each `AND` outside parentheses, up to `ORDER
/// BY`. An `OR` outside parentheses makes the whole query one part: JQL binds
/// `AND` tighter, so `a AND b OR c` is `(a AND b) OR c`, and keeping `a` alone
/// would drop tickets `c` holds.
fn clauses_of(jql: &str) -> Vec<Clause> {
    let chars: Vec<char> = jql.chars().collect();
    let tokens = tokenize(&chars);
    let mut depth = 0usize;
    let mut end = tokens.len();
    let mut or = false;
    for (i, token) in tokens.iter().enumerate() {
        if depth == 0 && is_word(Some(token), "order") && is_word(tokens.get(i + 1), "by") {
            end = i;
            break;
        }
        if depth == 0 && is_word(Some(token), "or") {
            or = true;
        }
        match token.kind {
            Kind::Open => depth += 1,
            Kind::Close => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    let mut clauses = Vec::new();
    let mut current: Vec<Token> = Vec::new();
    let close = |current: &mut Vec<Token>, clauses: &mut Vec<Clause>| {
        if let (Some(first), Some(last)) = (current.first(), current.last()) {
            let text = slice(&chars, first.start, last.end);
            clauses.push(Clause {
                text,
                tokens: std::mem::take(current),
            });
        }
    };
    depth = 0;
    for token in &tokens[..end] {
        if !or && depth == 0 && is_word(Some(token), "and") {
            close(&mut current, &mut clauses);
            continue;
        }
        match token.kind {
            Kind::Open => depth += 1,
            Kind::Close => depth = depth.saturating_sub(1),
            _ => {}
        }
        current.push(token.clone());
    }
    close(&mut current, &mut clauses);
    clauses
}

/// Fields knobas does not sync, by Jira's name in lower case, and why.
fn unsynced(name: &str) -> Option<&'static str> {
    Some(match name {
        "component" | "components" => "knobas does not sync components",
        "fixversion" => "knobas does not sync fix versions",
        "affectedversion" => "knobas does not sync affected versions",
        "resolution" => "knobas does not sync resolutions",
        "statuscategory" => "knobas does not sync status categories",
        "issuefunction" => "a ScriptRunner function, only Jira can answer it",
        _ => return None,
    })
}

/// The Jira fields a condition tests, by Jira's name in lower case.
fn field(name: &str) -> Option<Key> {
    Some(match name {
        "project" => Key::Project,
        "status" => Key::Status,
        "issuetype" | "type" => Key::Issuetype,
        "priority" => Key::Priority,
        "assignee" => Key::Assignee,
        "labels" | "label" => Key::Label,
        "sprint" => Key::Sprint,
        "parent" | "epic link" => Key::Under,
        "updated" => Key::Updated,
        _ => return None,
    })
}

/// The fields a ticket may have no value for. Jira's `!=` and `not in` never
/// match a ticket without one, while knobas's `-` does, so a negated clause on
/// one of these would widen the copy and stays out.
fn optional(key: Key) -> Option<&'static str> {
    match key {
        Key::Assignee => Some("assignee"),
        Key::Label => Some("label"),
        Key::Sprint => Some("sprint"),
        Key::Priority => Some("priority"),
        _ => None,
    }
}

const CANNOT_READ: &str = "knobas cannot read this part of the query";

/// The first field name in a clause knobas does not sync, as its reason.
fn unsynced_in(tokens: &[Token]) -> Option<&'static str> {
    tokens
        .iter()
        .filter(|t| matches!(t.kind, Kind::Word | Kind::Str))
        .find_map(|t| unsynced(&t.text.to_lowercase()))
}

/// One value of a clause: a literal, or a function's name.
struct Value {
    text: String,
    func: bool,
}

/// The values after the operator: one, or a parenthesised list; `None` if
/// they are something else.
fn values_of(tokens: &[Token]) -> Option<Vec<Value>> {
    let one = |at: usize| -> Option<(Value, usize)> {
        let token = tokens.get(at)?;
        if !matches!(token.kind, Kind::Word | Kind::Str) {
            return None;
        }
        if token.kind == Kind::Word && tokens.get(at + 1).is_some_and(|t| t.kind == Kind::Open) {
            let mut depth = 0usize;
            for (i, t) in tokens.iter().enumerate().skip(at + 1) {
                match t.kind {
                    Kind::Open => depth += 1,
                    Kind::Close => {
                        depth -= 1;
                        if depth == 0 {
                            let text = token.text.to_lowercase();
                            return Some((Value { text, func: true }, i + 1));
                        }
                    }
                    _ => {}
                }
            }
            return None;
        }
        Some((
            Value {
                text: token.text.clone(),
                func: false,
            },
            at + 1,
        ))
    };
    if tokens.first().map(|t| t.kind) != Some(Kind::Open) {
        let (value, next) = one(0)?;
        return (next == tokens.len()).then(|| vec![value]);
    }
    let mut values = Vec::new();
    let mut at = 1;
    loop {
        let (value, next) = one(at)?;
        values.push(value);
        at = next;
        match tokens.get(at).map(|t| t.kind) {
            Some(Kind::Comma) => at += 1,
            Some(Kind::Close) if at == tokens.len() - 1 => return Some(values),
            _ => return None,
        }
    }
}

/// One clause as a condition, or the reason it is left out.
fn read_clause(tokens: &[Token]) -> Result<Condition, String> {
    let mut depth = 0i64;
    let mut or = false;
    for token in tokens {
        match token.kind {
            Kind::Open => depth += 1,
            Kind::Close => depth -= 1,
            _ if depth == 0 && is_word(Some(token), "or") => or = true,
            _ => {}
        }
    }
    if or {
        let reason =
            unsynced_in(tokens).unwrap_or("an OR between conditions, which knobas cannot say");
        return Err(reason.into());
    }
    let head = match tokens.first() {
        Some(head)
            if matches!(head.kind, Kind::Word | Kind::Str) && !is_word(Some(head), "not") =>
        {
            head
        }
        _ => return Err(unsynced_in(tokens).unwrap_or(CANNOT_READ).into()),
    };
    let name = head.text.to_lowercase();
    if let Some(reason) = unsynced(&name) {
        return Err(reason.into());
    }

    // The operator: a symbol, or Jira's words for one.
    let next = tokens.get(1);
    let (op, rest) = match next {
        Some(t) if t.kind == Kind::Op => (t.text.clone(), &tokens[2..]),
        _ if is_word(next, "not") && is_word(tokens.get(2), "in") => {
            ("not in".into(), &tokens[3..])
        }
        _ if is_word(next, "is") && is_word(tokens.get(2), "not") => {
            ("is not".into(), &tokens[3..])
        }
        Some(t) if t.kind == Kind::Word => (t.text.to_lowercase(), &tokens[2..]),
        _ => return Err(CANNOT_READ.into()),
    };

    if op == "was" || op == "changed" {
        return Err("knobas does not keep a ticket's history".into());
    }
    if op == "~" || op == "!~" {
        return Err("a Jira text search, only Jira can answer it".into());
    }
    let Some(key) = field(&name) else {
        return Err(format!("knobas has no condition for {}", head.text));
    };
    let Some(values) = values_of(rest) else {
        return Err(CANNOT_READ.into());
    };
    if op == "is"
        || op == "is not"
        || values
            .iter()
            .any(|v| !v.func && ["empty", "null"].contains(&v.text.to_lowercase().as_str()))
    {
        return Err("knobas has no condition for an empty field".into());
    }

    if key == Key::Updated {
        let window = match values.as_slice() {
            [one] if !one.func => recent_window(&one.text),
            _ => None,
        };
        return match window {
            Some(window) if op == ">=" || op == ">" => Ok(Condition {
                key,
                negated: false,
                values: vec![window],
            }),
            _ => Err("knobas tests only how recently a ticket changed".into()),
        };
    }
    if !["=", "!=", "in", "not in"].contains(&op.as_str()) {
        return Err(CANNOT_READ.into());
    }
    let negated = op == "!=" || op == "not in";
    // Jira's `!=` drops a ticket with no value; knobas's `-` keeps it.
    if let Some(optional) = optional(key).filter(|_| negated) {
        return Err(format!(
            "knobas would also keep the tickets with no {optional}"
        ));
    }

    if key == Key::Sprint {
        if !values.iter().all(|v| v.func && v.text == "opensprints") {
            return Err("knobas knows only the active sprint".into());
        }
        return Ok(Condition {
            key,
            negated,
            values: vec!["current".into()],
        });
    }
    if key == Key::Under && negated {
        return Err("knobas can only keep the tickets under an epic".into());
    }
    let mut read = Vec::new();
    for value in values {
        if value.func && key == Key::Assignee && value.text == "currentuser" {
            read.push("me".into());
        } else if value.func {
            return Err("a Jira function, only Jira can answer it".into());
        } else {
            read.push(value.text);
        }
    }
    Ok(Condition {
        key,
        negated,
        values: read,
    })
}

/// `-7d` as `7d`, `-2W` as `2w`: `/^-(\d+)([dw])$/i`.
fn recent_window(text: &str) -> Option<String> {
    let rest = text.strip_prefix('-')?;
    let unit = rest.chars().last()?;
    let digits = &rest[..rest.len() - unit.len_utf8()];
    let unit = unit.to_ascii_lowercase();
    (!digits.is_empty()
        && digits.bytes().all(|b| b.is_ascii_digit())
        && (unit == 'd' || unit == 'w'))
        .then(|| format!("{digits}{unit}"))
}

fn translate(clauses: Vec<Clause>) -> Translation {
    let mut conditions: Vec<Condition> = Vec::new();
    let mut left_out = Vec::new();
    let mut kept = 0;
    for clause in clauses {
        let mut read = read_clause(&clause.tokens);
        if let Ok(condition) = &read {
            let held = conditions
                .iter_mut()
                .find(|c| c.key == condition.key && c.negated == condition.negated);
            match held {
                Some(_) if !condition.negated => {
                    read = Err("knobas reads two values of one field as either".into());
                }
                Some(held) => {
                    for value in &condition.values {
                        if !held.values.contains(value) {
                            held.values.push(value.clone());
                        }
                    }
                }
                None => conditions.push(condition.clone()),
            }
        }
        match read {
            Err(reason) => left_out.push(LeftOut {
                clause: clause.text,
                reason,
            }),
            Ok(_) => kept += 1,
        }
    }
    Translation {
        conditions,
        kept,
        left_out,
    }
}

/// One JQL query read into conditions.
#[cfg(test)]
fn translate_jql(jql: &str) -> Translation {
    translate(clauses_of(jql))
}

/// A Jira view's query read into conditions (the `jira_view` payload
/// `crate::views` writes): its JQL, the active sprint Jira adds to a Scrum
/// board's, and a Kanban board's sub-filter. A payload field that is missing
/// or not a string is empty, as the projection reads it.
pub(crate) fn translate_view(view: &serde_json::Value) -> Translation {
    let text = |key: &str| view.get(key).and_then(serde_json::Value::as_str);
    let scrum = text("type").is_some_and(|t| t.to_lowercase() == "scrum");
    let mut clauses = clauses_of(text("jql").unwrap_or(""));
    if scrum {
        clauses.extend(clauses_of("sprint in openSprints()"));
    }
    clauses.extend(clauses_of(text("sub_query").unwrap_or("")));
    translate(clauses)
}

/// Today's cases from the shell's `jql.test.ts`, moved here verbatim (#1208):
/// each expected search text is the conditions as the shell writes them
/// (`conditionsText`), so every literal is byte for byte the old one.
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// The shell's `grammarToken` and `conditionsText`, for the assertions.
    fn query(copy: &Translation) -> String {
        let key = |k: Key| {
            serde_json::to_value(k)
                .unwrap()
                .as_str()
                .unwrap()
                .to_owned()
        };
        let mut parts = Vec::new();
        for c in &copy.conditions {
            for value in &c.values {
                let clean = value.replace('"', "");
                let sign = if c.negated { "-" } else { "" };
                let k = key(c.key);
                parts.push(if clean.chars().any(is_space) {
                    format!("{sign}{k}:\"{clean}\"")
                } else {
                    format!("{sign}{k}:{clean}")
                });
            }
        }
        parts.join(" ")
    }

    fn left(clause: &str, reason: &str) -> LeftOut {
        LeftOut {
            clause: clause.into(),
            reason: reason.into(),
        }
    }

    fn filter(jql: &str) -> serde_json::Value {
        json!({ "kind": "filter", "type": null, "jql": jql, "sub_query": null })
    }

    // ---- the seeded Jira views (testenv/seed-atlassian-content.sh) ----

    #[test]
    fn the_favourite_filter_keeps_its_project_and_leaves_out_the_fix_version_saying_why() {
        let copy = translate_view(&filter(
            "project = PAY AND fixVersion is EMPTY ORDER BY updated DESC",
        ));
        assert_eq!(query(&copy), "project:PAY");
        assert_eq!(
            copy.left_out,
            [left(
                "fixVersion is EMPTY",
                "knobas does not sync fix versions"
            )]
        );
        assert_eq!(copy.kept, 1);
    }

    #[test]
    fn the_kanban_board_keeps_both_projects_and_leaves_out_jira_s_default_sub_filter() {
        let copy = translate_view(&json!({
            "kind": "board",
            "type": "kanban",
            "jql": "project in (PAY, OPS) ORDER BY Rank ASC",
            "sub_query": "fixVersion in unreleasedVersions() OR fixVersion is EMPTY",
        }));
        assert_eq!(query(&copy), "project:PAY project:OPS");
        assert_eq!(
            copy.left_out,
            [left(
                "fixVersion in unreleasedVersions() OR fixVersion is EMPTY",
                "knobas does not sync fix versions",
            )]
        );
        assert_eq!(copy.kept, 1);
    }

    #[test]
    fn a_scrum_board_s_active_sprint_is_a_condition_as_jira_adds_it_to_the_board_s_query() {
        let copy = translate_view(&json!({
            "kind": "board", "type": "scrum", "jql": "project = PAY ORDER BY Rank ASC", "sub_query": null,
        }));
        assert_eq!(query(&copy), "project:PAY sprint:current");
        assert_eq!(copy.left_out, []);
        assert_eq!(copy.kept, 2);
    }

    // ---- clauses knobas can test ----

    #[test]
    fn each_clause_knobas_can_test_reads_as_its_condition() {
        for (jql, expected) in [
            ("status = Done", "status:Done"),
            ("status = \"In Review\"", "status:\"In Review\""),
            ("status != Done", "-status:Done"),
            (
                "status not in (Done, Closed)",
                "-status:Done -status:Closed",
            ),
            ("issuetype = Bug", "issuetype:Bug"),
            ("type = Story", "issuetype:Story"),
            (
                "priority in (High, Highest)",
                "priority:High priority:Highest",
            ),
            ("assignee = currentUser()", "assignee:me"),
            ("assignee = jbecker", "assignee:jbecker"),
            ("issuetype not in (Epic)", "-issuetype:Epic"),
            ("labels = on-call", "label:on-call"),
            ("sprint in openSprints()", "sprint:current"),
            ("parent = PAY-12", "under:PAY-12"),
            ("\"Epic Link\" = PAY-12", "under:PAY-12"),
            ("updated >= -7d", "updated:7d"),
            ("updated > -2w", "updated:2w"),
            ("PROJECT = pay", "project:pay"),
            ("project = 'PAY'", "project:PAY"),
        ] {
            let copy = translate_jql(jql);
            assert_eq!(copy.left_out, [], "{jql}");
            assert_eq!(query(&copy), expected, "{jql}");
        }
    }

    #[test]
    fn conditions_keep_the_query_s_order_and_order_by_is_not_a_part() {
        let copy = translate_jql("project in (PAY, OPS) AND status != Done ORDER BY Rank ASC");
        assert_eq!(query(&copy), "project:PAY project:OPS -status:Done");
        assert_eq!(copy.kept, 2);
        assert_eq!(copy.left_out, []);
    }

    #[test]
    fn an_empty_query_keeps_nothing_and_leaves_nothing_out() {
        let copy = translate_jql("ORDER BY Rank ASC");
        assert_eq!(
            (query(&copy), copy.kept, copy.left_out),
            (String::new(), 0, vec![])
        );
    }

    // ---- clauses knobas leaves out, with the reason ----

    #[test]
    fn each_clause_knobas_leaves_out_says_why() {
        for (jql, reason) in [
            ("component = on-call", "knobas does not sync components"),
            ("fixVersion = 2.4", "knobas does not sync fix versions"),
            (
                "affectedVersion = 2.3",
                "knobas does not sync affected versions",
            ),
            (
                "resolution = Unresolved",
                "knobas does not sync resolutions",
            ),
            (
                "statusCategory != Done",
                "knobas does not sync status categories",
            ),
            (
                "issueFunction in linkedIssuesOf(\"project = PAY\")",
                "a ScriptRunner function, only Jira can answer it",
            ),
            (
                "assignee in membersOf(payments)",
                "a Jira function, only Jira can answer it",
            ),
            (
                "summary ~ refund",
                "a Jira text search, only Jira can answer it",
            ),
            ("status was Done", "knobas does not keep a ticket's history"),
            (
                "assignee is EMPTY",
                "knobas has no condition for an empty field",
            ),
            ("sprint = 41", "knobas knows only the active sprint"),
            (
                "parent != PAY-12",
                "knobas can only keep the tickets under an epic",
            ),
            ("created >= -7d", "knobas has no condition for created"),
            (
                "updated <= -7d",
                "knobas tests only how recently a ticket changed",
            ),
            (
                "NOT status = Done",
                "knobas cannot read this part of the query",
            ),
            (
                "assignee != currentUser()",
                "knobas would also keep the tickets with no assignee",
            ),
            (
                "labels != tech-debt",
                "knobas would also keep the tickets with no label",
            ),
            (
                "sprint not in openSprints()",
                "knobas would also keep the tickets with no sprint",
            ),
            (
                "priority not in (Low)",
                "knobas would also keep the tickets with no priority",
            ),
        ] {
            let copy = translate_jql(&format!("project = PAY AND {jql}"));
            assert_eq!(query(&copy), "project:PAY", "{jql}");
            assert_eq!(copy.left_out, [left(jql, reason)], "{jql}");
        }
    }

    #[test]
    fn an_or_outside_parentheses_makes_the_whole_query_one_part_as_jql_binds_and_tighter() {
        let copy =
            translate_jql("project = PAY AND status = Done OR priority = High ORDER BY Rank ASC");
        assert_eq!(
            (query(&copy), copy.kept, copy.left_out),
            (
                String::new(),
                0,
                vec![left(
                    "project = PAY AND status = Done OR priority = High",
                    "an OR between conditions, which knobas cannot say",
                )]
            )
        );
    }

    #[test]
    fn an_or_inside_parentheses_stays_in_its_own_part() {
        let copy = translate_jql("project = PAY AND (status = Done OR priority = High)");
        assert_eq!(query(&copy), "project:PAY");
        let clauses: Vec<&str> = copy.left_out.iter().map(|p| p.clause.as_str()).collect();
        assert_eq!(clauses, ["(status = Done OR priority = High)"]);
    }

    #[test]
    fn an_or_over_a_field_knobas_does_not_sync_names_the_field() {
        let copy = translate_jql("component = a OR status = Done");
        assert_eq!(copy.left_out[0].reason, "knobas does not sync components");
    }

    #[test]
    fn a_second_clause_on_the_same_field_and_sign_is_left_out_knobas_would_read_the_two_as_either()
    {
        let copy = translate_jql("labels = a AND labels = b");
        assert_eq!(query(&copy), "label:a");
        assert_eq!(
            copy.left_out,
            [left(
                "labels = b",
                "knobas reads two values of one field as either"
            )]
        );
    }

    #[test]
    fn an_and_inside_parentheses_or_quotes_does_not_split_the_clause() {
        let copy = translate_jql("(status = Done AND priority = High) AND summary ~ \"a AND b\"");
        let clauses: Vec<&str> = copy.left_out.iter().map(|p| p.clause.as_str()).collect();
        assert_eq!(
            clauses,
            [
                "(status = Done AND priority = High)",
                "summary ~ \"a AND b\""
            ]
        );
    }
}
