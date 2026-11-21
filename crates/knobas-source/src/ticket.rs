//! A ticket's fields (ADR-0054): what a person may change on one ticket
//! besides its status, as its source says **now**, and a field's choices.
//!
//! [`Source::ticket_fields`](crate::Source::ticket_fields) answers a
//! [`TicketFields`] per ticket, read when its page opens and when a surface
//! is about to offer an edit, and never stored.
//! [`Source::field_options`](crate::Source::field_options) answers a searched
//! field's choices as [`FieldOptions`]. The change itself is one write,
//! [`WriteOp::EditTicket`](crate::WriteOp::EditTicket), carrying only what
//! changed; the write is not bound by these answers, and an adapter refuses
//! by name, at write time, a field it cannot set.
//!
//! Seven kinds of control ([`TicketFieldKind`]), all drawn by the shell: an
//! adapter ships no UI (ADR-0020, ADR-0029). Four field ids are knobas' own
//! (`title`, `assignee`, `priority`, `labels`), the ones
//! [`EditTicket`](crate::WriteOp::EditTicket) names; any other id is the
//! adapter's, and its value goes back to the adapter untouched.
//!
//! Plain serde data, like everything crossing the SPI (spec §3a); an external
//! adapter answers it as this JSON (`knobas:adapter/tickets`, 1.6.0).
//! **Frozen once landed** (§10.8).
//!
//! Every string is **source text**: the shell interpolates it and never
//! renders it as markup.

use serde::{Deserialize, Serialize};

/// The id of the summary, knobas' own: [`EditTicket::title`](crate::WriteOp::EditTicket).
pub const TITLE: &str = "title";
/// The id of the assignee, knobas' own.
pub const ASSIGNEE: &str = "assignee";
/// The id of the priority, knobas' own.
pub const PRIORITY: &str = "priority";
/// The id of the labels, knobas' own.
pub const LABELS: &str = "labels";
/// The four ids an [`EditTicket`](crate::WriteOp::EditTicket) names as typed
/// fields. Any other id is a source's own field.
pub const NAMED: [&str; 4] = [TITLE, ASSIGNEE, PRIORITY, LABELS];

/// What a person may change on one ticket besides its status, in the order
/// the source would show them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct TicketFields {
    pub fields: Vec<TicketField>,
}

impl TicketFields {
    /// The field named `id`, if the answer lists it.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<&TicketField> {
        self.fields.iter().find(|f| f.id == id)
    }
}

/// One thing of a ticket a person can change, with the value the source
/// holds now.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct TicketField {
    /// `title`, `assignee`, `priority` or `labels` for the four the op names.
    /// Any other id is the adapter's own, and its value goes in
    /// `EditTicket::fields` under it.
    pub id: String,
    /// What the field is called, in the reader's language (*Assignee*, *Due date*).
    pub label: String,
    pub kind: TicketFieldKind,
    /// What the source holds now, in the form the kind takes; `null` for none.
    #[ts(type = "unknown")]
    pub value: serde_json::Value,
    /// What the value reads as where it is not the value itself: a person's
    /// name for an account, a choice's label for its value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub shown: Option<String>,
    /// It may not be cleared.
    #[serde(default)]
    pub required: bool,
    /// Why it cannot be changed now, in words a person can act on; absent is
    /// editable. (*Not on this ticket's edit screen in Jira.*)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub readonly: Option<String>,
}

/// The control a [`TicketField`] takes, and so the form of its value.
///
/// - `text`: one line of text. Value: a string.
/// - `choice`: one of `options`, all of them here. Value: an option's `value`.
/// - `search`: one of a list too long to send with every ticket, read through
///   `field_options`. Value: an option's `value`. `scope` names the list in
///   the adapter's own words, so one answer serves every ticket whose field
///   names the same scope. `person`: the options are accounts.
/// - `labels`: several short names. Value: an array of strings. `create`: a
///   name the source has not seen may be added. With `scope`, the names on
///   offer are read through `field_options`; without, the shell offers the
///   ones the mirror holds for the source.
/// - `date`: a calendar day. Value: `YYYY-MM-DD`.
/// - `number`: a number. Value: a JSON number.
/// - `flag`: yes or no. Value: a boolean.
///
/// The kinds are documented here and not on each variant because the
/// generated TypeScript carries a type's doc but not a variant's (#1325).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(tag = "type", rename_all = "snake_case")]
#[ts(export_to = "entity.ts")]
pub enum TicketFieldKind {
    Text,
    Choice {
        options: Vec<FieldOption>,
    },
    Search {
        scope: String,
        #[serde(default)]
        person: bool,
    },
    Labels {
        #[serde(default)]
        scope: Option<String>,
        #[serde(default)]
        create: bool,
    },
    Date,
    Number,
    Flag,
}

impl TicketFieldKind {
    /// The kind's word on the wire (`"search"`), for messages.
    #[must_use]
    pub fn word(&self) -> &'static str {
        match self {
            TicketFieldKind::Text => "text",
            TicketFieldKind::Choice { .. } => "choice",
            TicketFieldKind::Search { .. } => "search",
            TicketFieldKind::Labels { .. } => "labels",
            TicketFieldKind::Date => "date",
            TicketFieldKind::Number => "number",
            TicketFieldKind::Flag => "flag",
        }
    }

    /// Whether a field of this kind has a list `field_options` answers:
    /// `search`, and `labels` with a scope.
    #[must_use]
    pub fn has_options(&self) -> bool {
        matches!(
            self,
            TicketFieldKind::Search { .. } | TicketFieldKind::Labels { scope: Some(_), .. }
        )
    }

    /// Whether `value` is in the form this kind takes. `null` fits every
    /// kind (none held); a `choice`'s value must be one of its options.
    #[must_use]
    pub fn fits(&self, value: &serde_json::Value) -> bool {
        use serde_json::Value;
        if value.is_null() {
            return true;
        }
        match self {
            TicketFieldKind::Text | TicketFieldKind::Search { .. } => value.is_string(),
            TicketFieldKind::Choice { options } => value
                .as_str()
                .is_some_and(|v| options.iter().any(|o| o.value == v)),
            TicketFieldKind::Labels { .. } => value
                .as_array()
                .is_some_and(|a| a.iter().all(Value::is_string)),
            TicketFieldKind::Date => value.as_str().is_some_and(is_day),
            TicketFieldKind::Number => value.is_number(),
            TicketFieldKind::Flag => value.is_boolean(),
        }
    }
}

/// `YYYY-MM-DD`, a real day.
fn is_day(s: &str) -> bool {
    s.len() == 10 && chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").is_ok()
}

/// One choice of a `choice` or `search` field, or a label on offer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct FieldOption {
    /// What is written: the account, the option's id or name.
    pub value: String,
    /// What a person reads.
    pub label: String,
    /// A second, muted word: the account under a person's name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub hint: Option<String>,
}

/// A searched field's choices for what was typed
/// ([`Source::field_options`](crate::Source::field_options)).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct FieldOptions {
    pub options: Vec<FieldOption>,
    /// Every choice there is for `typed` is here. `false`: the source has
    /// more than it sent, and a longer `typed` asks again.
    pub complete: bool,
}

/// What is wrong with a [`TicketFields`] answer: the rules battery clause 15
/// holds every answer to (section 17 of the 2026-11-21 ruling). `None` is an
/// answer that keeps them.
///
/// Ids are unique; `title` is `text`; `assignee` is `search` with `person`,
/// or `choice`; `priority` is `choice` or `search`; `labels` is `labels`;
/// every `value` fits its kind.
#[must_use]
pub fn fault(answer: &TicketFields) -> Option<String> {
    let mut seen = std::collections::HashSet::new();
    for f in &answer.fields {
        if f.id.trim().is_empty() {
            return Some("a field has a blank id".to_owned());
        }
        if !seen.insert(f.id.as_str()) {
            return Some(format!("field {:?} is listed twice", f.id));
        }
        let kind_ok = match f.id.as_str() {
            TITLE => matches!(f.kind, TicketFieldKind::Text),
            ASSIGNEE => matches!(
                f.kind,
                TicketFieldKind::Search { person: true, .. } | TicketFieldKind::Choice { .. }
            ),
            PRIORITY => matches!(
                f.kind,
                TicketFieldKind::Choice { .. } | TicketFieldKind::Search { .. }
            ),
            LABELS => matches!(f.kind, TicketFieldKind::Labels { .. }),
            _ => true,
        };
        if !kind_ok {
            return Some(format!(
                "field {:?} is of kind {:?}, which knobas' own {:?} cannot take",
                f.id,
                f.kind.word(),
                f.id
            ));
        }
        if !f.kind.fits(&f.value) {
            return Some(format!(
                "field {:?} holds {}, which is not a value of kind {:?}",
                f.id,
                f.value,
                f.kind.word()
            ));
        }
    }
    None
}

/// The fields of `answer` that keep the rules one by one, the others dropped
/// with a line naming each (risk 9: an installed guest is not held to the
/// battery at run time, and a field whose value does not fit is never drawn
/// half). Duplicate ids keep the first.
#[must_use]
pub fn keep_fitting(answer: TicketFields) -> (TicketFields, Vec<String>) {
    let mut dropped = Vec::new();
    let mut kept: Vec<TicketField> = Vec::new();
    for f in answer.fields {
        let one = TicketFields {
            fields: vec![f.clone()],
        };
        if kept.iter().any(|k| k.id == f.id) {
            dropped.push(format!("field {:?} is listed twice", f.id));
        } else if let Some(why) = fault(&one) {
            dropped.push(why);
        } else {
            kept.push(f);
        }
    }
    (TicketFields { fields: kept }, dropped)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn every_kind() -> Vec<TicketFieldKind> {
        vec![
            TicketFieldKind::Text,
            TicketFieldKind::Choice {
                options: vec![FieldOption {
                    value: "High".into(),
                    label: "High".into(),
                    hint: None,
                }],
            },
            TicketFieldKind::Search {
                scope: "PAY".into(),
                person: true,
            },
            TicketFieldKind::Labels {
                scope: Some("tidewater/payout-service".into()),
                create: false,
            },
            TicketFieldKind::Date,
            TicketFieldKind::Number,
            TicketFieldKind::Flag,
        ]
    }

    #[test]
    fn every_kind_round_trips_under_its_word() {
        for kind in every_kind() {
            let wire = serde_json::to_value(&kind).unwrap();
            assert_eq!(wire["type"], kind.word(), "{kind:?}");
            let back: TicketFieldKind = serde_json::from_value(wire).unwrap();
            assert_eq!(back, kind);
        }
        assert_eq!(
            serde_json::to_value(TicketFieldKind::Search {
                scope: "PAY".into(),
                person: true
            })
            .unwrap(),
            json!({ "type": "search", "scope": "PAY", "person": true })
        );
    }

    /// A peer that sends only what it must (no `shown`, `required`,
    /// `readonly`; a search with no `person`, labels with no scope) decodes.
    #[test]
    fn a_bare_answer_from_a_peer_decodes() {
        let answer: TicketFields = serde_json::from_value(json!({ "fields": [
            { "id": "title", "label": "Summary", "kind": { "type": "text" }, "value": "x" },
            { "id": "assignee", "label": "Assignee", "kind": { "type": "search", "scope": "PAY" }, "value": null },
            { "id": "labels", "label": "Labels", "kind": { "type": "labels" }, "value": [] }
        ] }))
        .unwrap();
        let t = answer.get("title").unwrap();
        assert_eq!(t.shown, None);
        assert!(!t.required);
        assert_eq!(t.readonly, None);
        assert_eq!(
            answer.get("assignee").unwrap().kind,
            TicketFieldKind::Search {
                scope: "PAY".into(),
                person: false
            }
        );
        assert_eq!(
            answer.get("labels").unwrap().kind,
            TicketFieldKind::Labels {
                scope: None,
                create: false
            }
        );
        let wire = serde_json::to_value(t).unwrap();
        assert!(wire.get("shown").is_none() && wire.get("readonly").is_none());
    }

    #[test]
    fn values_fit_their_kinds() {
        let [text, choice, search, labels, date, number, flag]: [TicketFieldKind; 7] =
            every_kind().try_into().unwrap();
        assert!(text.fits(&json!("x")) && !text.fits(&json!(1)));
        assert!(choice.fits(&json!("High")) && !choice.fits(&json!("Low")));
        assert!(search.fits(&json!("mara")) && !search.fits(&json!(["mara"])));
        assert!(labels.fits(&json!(["a", "b"])) && !labels.fits(&json!(["a", 1])));
        assert!(date.fits(&json!("2026-11-21")) && !date.fits(&json!("2026-13-01")));
        assert!(!date.fits(&json!("21.11.2026")));
        assert!(number.fits(&json!(3.5)) && !number.fits(&json!("3")));
        assert!(flag.fits(&json!(true)) && !flag.fits(&json!("yes")));
        for kind in every_kind() {
            assert!(kind.fits(&serde_json::Value::Null), "{kind:?}");
        }
        assert!(search.has_options() && labels.has_options() && !choice.has_options());
        assert!(
            !TicketFieldKind::Labels {
                scope: None,
                create: true
            }
            .has_options()
        );
    }

    fn field(id: &str, kind: TicketFieldKind, value: serde_json::Value) -> TicketField {
        TicketField {
            id: id.into(),
            label: id.into(),
            kind,
            value,
            shown: None,
            required: false,
            readonly: None,
        }
    }

    #[test]
    fn the_rules_name_what_breaks_them() {
        let ok = TicketFields {
            fields: vec![
                field("title", TicketFieldKind::Text, json!("x")),
                field(
                    "assignee",
                    TicketFieldKind::Search {
                        scope: "PAY".into(),
                        person: true,
                    },
                    json!("mara"),
                ),
                field("due", TicketFieldKind::Date, json!("2026-12-01")),
            ],
        };
        assert_eq!(fault(&ok), None);
        let twice = TicketFields {
            fields: vec![ok.fields[0].clone(), ok.fields[0].clone()],
        };
        assert!(fault(&twice).unwrap().contains("twice"));
        let wrong_kind = TicketFields {
            fields: vec![field("title", TicketFieldKind::Number, json!(1))],
        };
        assert!(fault(&wrong_kind).unwrap().contains("\"title\""));
        let not_a_person = TicketFields {
            fields: vec![field(
                "assignee",
                TicketFieldKind::Search {
                    scope: "PAY".into(),
                    person: false,
                },
                json!(null),
            )],
        };
        assert!(fault(&not_a_person).is_some());
        let unfit = TicketFields {
            fields: vec![field("due", TicketFieldKind::Date, json!("soon"))],
        };
        assert!(fault(&unfit).unwrap().contains("\"due\""));

        let (kept, dropped) = keep_fitting(TicketFields {
            fields: vec![
                ok.fields[0].clone(),
                unfit.fields[0].clone(),
                ok.fields[0].clone(),
            ],
        });
        assert_eq!(kept.fields.len(), 1);
        assert_eq!(dropped.len(), 2);
    }
}
