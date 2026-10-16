//! A parameter's spec, parsed (#852, R-9.7): what the Run… dialog asks a
//! parameter with.
//!
//! TeamCity keeps a parameter's spec as one string, its type's `rawValue`:
//! a word, then `key='value'` pairs --
//! `select data_1='info' label_1='Info' data_2='debug' display='normal'`,
//! `checkbox checkedValue='true' uncheckedValue='false'`,
//! `text label='Filter' description='Empty runs every test'`. A value
//! escapes with `|`, TeamCity's service-message escape: `|'` is a quote,
//! `||` a bar, `|n` and `|r` line breaks, `|[` and `|]` brackets.
//!
//! Read narrowly: a select is a select only with at least one `data_N` and
//! without `multiple='true'` (a multiple select's value is a joined list the
//! dialog has no field for, so it is typed as text); anything else, and
//! anything that does not parse, is text. A password is text here too: the
//! setting's `secret` flag is what keeps its value out of the view.

use knobas_source::pipeline::{ParameterKind, ParameterOption, ParameterSpec};

/// `name`'s spec from TeamCity's `raw`, or plain text without one.
pub(crate) fn parse(name: &str, raw: Option<&str>) -> ParameterSpec {
    let (word, pairs) = raw.map(split).unwrap_or_default();
    let get = |key: &str| {
        pairs
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.clone())
            .filter(|v| !v.trim().is_empty())
    };
    let kind = match word.as_str() {
        "select" if get("multiple").as_deref() != Some("true") => {
            let mut numbered: Vec<(u32, ParameterOption)> = pairs
                .iter()
                .filter_map(|(k, v)| {
                    let n: u32 = k.strip_prefix("data_")?.parse().ok()?;
                    Some((
                        n,
                        ParameterOption {
                            value: v.clone(),
                            label: get(&format!("label_{n}")).filter(|l| l != v),
                        },
                    ))
                })
                .collect();
            numbered.sort_by_key(|(n, _)| *n);
            if numbered.is_empty() {
                ParameterKind::Text
            } else {
                ParameterKind::Select {
                    options: numbered.into_iter().map(|(_, o)| o).collect(),
                }
            }
        }
        "checkbox" => ParameterKind::Checkbox {
            checked: get("checkedValue").unwrap_or_else(|| "true".to_owned()),
            unchecked: pairs
                .iter()
                .find(|(k, _)| k == "uncheckedValue")
                .map(|(_, v)| v.clone())
                .unwrap_or_default(),
        },
        _ => ParameterKind::Text,
    };
    ParameterSpec {
        name: name.to_owned(),
        label: get("label"),
        hint: get("description"),
        kind,
    }
}

/// The spec's leading word and its `key='value'` pairs, in order. A pair
/// that does not close ends the reading: what came before it stands.
fn split(raw: &str) -> (String, Vec<(String, String)>) {
    let raw = raw.trim_start();
    let word_end = raw.find(char::is_whitespace).unwrap_or(raw.len());
    let word = raw[..word_end].to_owned();
    let mut pairs = Vec::new();
    let mut rest = raw[word_end..].chars().peekable();
    loop {
        while rest.next_if(|c| c.is_whitespace()).is_some() {}
        let mut key = String::new();
        while let Some(c) = rest.next_if(|c| *c != '=' && !c.is_whitespace()) {
            key.push(c);
        }
        if key.is_empty() || rest.next() != Some('=') || rest.next() != Some('\'') {
            break;
        }
        let mut value = String::new();
        let mut closed = false;
        while let Some(c) = rest.next() {
            match c {
                '\'' => {
                    closed = true;
                    break;
                }
                '|' => match rest.next() {
                    Some('n') => value.push('\n'),
                    Some('r') => value.push('\r'),
                    Some(other) => value.push(other),
                    None => break,
                },
                other => value.push(other),
            }
        }
        if !closed {
            break;
        }
        pairs.push((key, value));
    }
    (word, pairs)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kind(raw: &str) -> ParameterKind {
        parse("p", Some(raw)).kind
    }

    fn option(value: &str, label: Option<&str>) -> ParameterOption {
        ParameterOption {
            value: value.to_owned(),
            label: label.map(str::to_owned),
        }
    }

    #[test]
    fn a_select_offers_its_values_in_their_numbered_order_with_their_labels() {
        assert_eq!(
            kind(
                "select data_2='debug' label_2='Debug' data_1='info' display='normal' data_10='trace'"
            ),
            ParameterKind::Select {
                options: vec![
                    option("info", None),
                    option("debug", Some("Debug")),
                    option("trace", None),
                ]
            }
        );
    }

    #[test]
    fn a_checkbox_sends_its_two_values_and_defaults_to_true_and_empty() {
        assert_eq!(
            kind("checkbox checkedValue='yes' uncheckedValue='no'"),
            ParameterKind::Checkbox {
                checked: "yes".into(),
                unchecked: "no".into()
            }
        );
        assert_eq!(
            kind("checkbox"),
            ParameterKind::Checkbox {
                checked: "true".into(),
                unchecked: String::new()
            }
        );
    }

    #[test]
    fn a_label_and_a_description_come_with_any_kind() {
        let spec = parse(
            "test.filter",
            Some("text label='Test filter' description='Empty runs every test' display='normal'"),
        );
        assert_eq!(spec.name, "test.filter");
        assert_eq!(spec.label.as_deref(), Some("Test filter"));
        assert_eq!(spec.hint.as_deref(), Some("Empty runs every test"));
        assert_eq!(spec.kind, ParameterKind::Text);
    }

    #[test]
    fn teamcitys_bar_escapes_are_read() {
        assert_eq!(
            kind("select data_1='it|'s' data_2='a||b' data_3='|[x|]'"),
            ParameterKind::Select {
                options: vec![
                    option("it's", None),
                    option("a|b", None),
                    option("[x]", None)
                ]
            }
        );
    }

    #[test]
    fn anything_the_dialog_has_no_field_for_is_text() {
        for raw in [
            "password display='hidden'",
            "select multiple='true' data_1='a' data_2='b' valueSeparator=','",
            "select display='normal'",
            "text regexp='[0-9]+' validationMode='regex'",
            "select data_1='unclosed",
            "",
        ] {
            assert_eq!(kind(raw), ParameterKind::Text, "{raw:?}");
        }
        assert_eq!(parse("p", None).kind, ParameterKind::Text);
    }

    #[test]
    fn a_pair_that_does_not_close_keeps_the_pairs_before_it() {
        assert_eq!(
            kind("select data_1='a' data_2='b"),
            ParameterKind::Select {
                options: vec![option("a", None)]
            }
        );
    }
}
