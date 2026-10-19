//! `translate_query` (#1208, ADR-0029): a ticket system's own query read into
//! knobas' search grammar, for *Make an editable copy* of a saved view.
//!
//! A source's query language stays the source's. The adapter that owns it
//! reads a saved view's query into [`Condition`]s the search grammar can test,
//! and names every part it left out with the reason, in the reader's words.
//! JQL is Jira's answer (`knobas-source-jira`'s `translate`). The shell renders
//! the conditions as search text and never parses the source's language.
//!
//! The ticket-system role's optional read
//! [`TRANSLATE_QUERY`](crate::role::TRANSLATE_QUERY): a source that does not
//! claim it, or answers `None`, offers no editable copy, and its saved views
//! still show.
//!
//! Plain serde data, like everything crossing the SPI (spec §3a); an external
//! adapter answers it as this JSON (`knobas:adapter/query`).

/// A key of the search grammar a condition tests, as the grammar spells it
/// (`under:PAY-12`, `label:on-call`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Key {
    /// `project:PAY`.
    Project,
    /// `status:Done`.
    Status,
    /// `issuetype:Bug`.
    Issuetype,
    /// `priority:High`.
    Priority,
    /// `assignee:me` or an account.
    Assignee,
    /// The tickets under an epic or parent.
    Under,
    /// `label:on-call`.
    Label,
    /// `current` is the active sprint.
    Sprint,
    /// A knobas signal (`has:review`).
    Has,
    /// A window, `7d` or `2w`.
    Updated,
    /// Assigned to me within a window.
    Assigned,
}

/// One condition: a key, *is* or *is not*, and values any of which it means.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Condition {
    /// The key it tests.
    pub key: Key,
    /// *Is not*: `-key:value` in the grammar.
    pub negated: bool,
    /// The values, any of which it means, as the grammar reads them.
    pub values: Vec<String>,
}

/// One part of the source's query knobas left out, as the source spells it,
/// and why.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LeftOut {
    /// The part, as the source's query spells it.
    pub clause: String,
    /// Why knobas cannot test it, in the reader's words.
    pub reason: String,
}

/// A saved view's query as knobas can test it: the conditions, in the query's
/// order, how many of the query's parts they keep, and the parts left out.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Translation {
    /// What a copy tests, in the query's order.
    pub conditions: Vec<Condition>,
    /// How many of the query's parts the conditions keep: a part, not a
    /// condition, since two parts may read into one condition.
    pub kept: u32,
    /// The parts it leaves out, in the query's order.
    pub left_out: Vec<LeftOut>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_translation_crosses_as_plain_json_with_the_grammar_s_keys() {
        let translation = Translation {
            conditions: vec![Condition {
                key: Key::Under,
                negated: true,
                values: vec!["PAY-12".into()],
            }],
            kept: 1,
            left_out: vec![LeftOut {
                clause: "fixVersion is EMPTY".into(),
                reason: "knobas does not sync fix versions".into(),
            }],
        };
        let wire = serde_json::to_value(&translation).unwrap();
        assert_eq!(
            wire,
            serde_json::json!({
                "conditions": [{"key": "under", "negated": true, "values": ["PAY-12"]}],
                "kept": 1,
                "left_out": [{"clause": "fixVersion is EMPTY",
                              "reason": "knobas does not sync fix versions"}],
            })
        );
        assert_eq!(
            serde_json::from_value::<Translation>(wire).unwrap(),
            translation
        );
    }
}
