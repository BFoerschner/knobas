//! TeamCity's `fields=` grammar, parsed and **applied**.
//!
//! `fields=count,build(id,status,running-info(percentageComplete))` is a
//! recursive projection: names at one level, optional parenthesised children
//! under a name. mockd parses it and actually projects, because an adapter that
//! asks for a field and never notices it is missing is the bug this mock exists
//! to catch.
//!
//! Two deliberate strictnesses (crate deviation 6):
//!
//! * an unknown name is **400 + `UnknownField`**, where real TeamCity drops it
//!   silently;
//! * of TeamCity's presets only `$long` is accepted (as "the whole subtree");
//!   `$short` and `$locator` are refused rather than approximated, because a
//!   mock that guesses at a preset's contents teaches the adapter a shape the
//!   real server does not serve.
//!
//! Serialisers in [`crate::teamcity`] emit **every** key an object can have,
//! using `null` for the ones this instance does not: that is what gives a name
//! a stable "known" set to be validated against, independent of the particular
//! build. [`project`] then drops the nulls, which is TeamCity's own
//! omit-when-absent wire shape.

use serde_json::Value;

/// One name in a `fields=` expression, with its children (empty when the name
/// was not parenthesised).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldSel {
    pub name: String,
    pub children: Vec<FieldSel>,
}

/// The preset that means "everything below here".
const LONG: &str = "$long";

/// Parses a `fields=` expression, or returns the message naming what is wrong.
pub fn parse(raw: &str) -> Result<Vec<FieldSel>, String> {
    let mut chars = raw.chars().peekable();
    let sels = parse_list(&mut chars)?;
    match chars.next() {
        None => Ok(sels),
        Some(c) => Err(format!("unbalanced {c:?} in fields={raw:?}")),
    }
}

type Chars<'a> = std::iter::Peekable<std::str::Chars<'a>>;

fn parse_list(chars: &mut Chars<'_>) -> Result<Vec<FieldSel>, String> {
    let mut out = Vec::new();
    loop {
        let mut name = String::new();
        while let Some(&c) = chars.peek() {
            if c == ',' || c == '(' || c == ')' {
                break;
            }
            name.push(c);
            chars.next();
        }
        let name = name.trim().to_owned();
        let children = if chars.peek() == Some(&'(') {
            chars.next();
            let kids = parse_list(chars)?;
            if chars.next() != Some(')') {
                return Err("unclosed '(' in fields=".to_owned());
            }
            kids
        } else {
            Vec::new()
        };
        if name.is_empty() {
            if children.is_empty() && chars.peek() != Some(&',') {
                break;
            }
            return Err("empty field name in fields=".to_owned());
        }
        out.push(FieldSel { name, children });
        if chars.peek() == Some(&',') {
            chars.next();
        } else {
            break;
        }
    }
    Ok(out)
}

/// Projects `full` down to `sel`, or returns the first name `full` does not
/// have.
///
/// `full` is the complete object (nulls for absent values); the result carries
/// only the selected names, and only those whose value is not null.
pub fn project(full: &Value, sel: &[FieldSel]) -> Result<Value, String> {
    if sel.iter().any(|s| s.name == LONG) {
        return Ok(strip_nulls(full));
    }
    if let Some(bad) = sel.iter().find(|s| s.name.starts_with('$')) {
        return Err(bad.name.clone());
    }
    match full {
        // An array inherits its parent's selection: `build(id,state)` means
        // "each build, projected to id and state".
        Value::Array(items) => items
            .iter()
            .map(|i| project(i, sel))
            .collect::<Result<Vec<_>, _>>()
            .map(Value::Array),
        Value::Object(obj) => {
            let mut out = serde_json::Map::new();
            for s in sel {
                let Some(v) = obj.get(&s.name) else {
                    return Err(s.name.clone());
                };
                if v.is_null() {
                    // Known name, absent on this instance: TeamCity omits it.
                    //
                    // The children are *offered* for checking but a null
                    // carries no key set, so `check_names` can only accept
                    // them: a typo inside an absent object IS excused by its
                    // absence. (An earlier comment here claimed the opposite.
                    // It was wrong -- see `check_names`' own doc, and
                    // `teamcity.rs::the_new_names_are_still_a_closed_set`,
                    // which pins the real behaviour so the gap stays visible.)
                    check_names(v, &s.children)?;
                    continue;
                }
                let projected = if s.children.is_empty() {
                    strip_nulls(v)
                } else {
                    project(v, &s.children)?
                };
                out.insert(s.name.clone(), projected);
            }
            Ok(Value::Object(out))
        }
        // A scalar with children asked of it: the name exists, the shape does
        // not support a sub-selection. TeamCity ignores it; so do we.
        other => Ok(other.clone()),
    }
}

/// Validates `sel`'s names against `full` without producing anything, for the
/// null case above. A null carries no key set, so nothing can be checked and
/// the only honest answer is to accept.
fn check_names(full: &Value, sel: &[FieldSel]) -> Result<(), String> {
    if full.is_null() {
        return Ok(());
    }
    project(full, sel).map(|_| ())
}

/// The complete object minus the keys this instance does not have.
fn strip_nulls(v: &Value) -> Value {
    match v {
        Value::Object(o) => Value::Object(
            o.iter()
                .filter(|(_, x)| !x.is_null())
                .map(|(k, x)| (k.clone(), strip_nulls(x)))
                .collect(),
        ),
        Value::Array(a) => Value::Array(a.iter().map(strip_nulls).collect()),
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sel(name: &str, children: &[FieldSel]) -> FieldSel {
        FieldSel {
            name: name.to_owned(),
            children: children.to_vec(),
        }
    }

    #[test]
    fn the_grammar_nests() {
        assert_eq!(
            parse("count,build(id,running-info(percentageComplete))").unwrap(),
            vec![
                sel("count", &[]),
                sel(
                    "build",
                    &[
                        sel("id", &[]),
                        sel("running-info", &[sel("percentageComplete", &[])]),
                    ]
                ),
            ]
        );
    }

    #[test]
    fn an_unbalanced_expression_is_refused() {
        assert!(parse("count,build(id").is_err());
        assert!(parse("count,build(id))").is_err());
        assert!(parse("count,,id").is_err());
    }

    #[test]
    fn projection_selects_recurses_and_drops_nulls() {
        let full = json!({
            "count": 1,
            "href": "/app/rest/builds",
            "build": [{ "id": 7, "state": "finished", "running-info": null }],
        });
        let got = project(&full, &parse("count,build(id,running-info)").unwrap()).unwrap();
        assert_eq!(got, json!({ "count": 1, "build": [{ "id": 7 }] }));
    }

    #[test]
    fn an_unknown_name_is_reported_by_name() {
        let full = json!({ "count": 1, "build": [{ "id": 7 }] });
        assert_eq!(
            project(&full, &parse("count,build(idd)").unwrap()),
            Err("idd".to_owned())
        );
        assert_eq!(
            project(&full, &parse("counnt").unwrap()),
            Err("counnt".to_owned())
        );
    }

    #[test]
    fn long_is_everything_and_the_other_presets_are_refused() {
        let full = json!({ "id": 7, "gone": null });
        assert_eq!(
            project(&full, &parse("$long").unwrap()).unwrap(),
            json!({ "id": 7 })
        );
        assert_eq!(
            project(&full, &parse("$short").unwrap()),
            Err("$short".to_owned())
        );
    }
}
