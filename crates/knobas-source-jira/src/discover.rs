//! Reading the instance's own field table for the ids no user can be expected
//! to type (#297).
//!
//! One id so far: the **Epic Link** custom field. On a classic Data Center
//! project that field is the *only* place epic membership lives -- `#276`
//! measured `fields.parent` absent from every issue of the seeded `PAY` and
//! `OPS`, epic children included -- and its id is minted per instance. Three
//! seeds of one script produced `customfield_10101`, `customfield_10109` and
//! `customfield_10101`, and on one of them `customfield_10102` was *Epic
//! Status*. So a copied id does not merely fail: it silently reads the wrong
//! field.
//!
//! Which is why the picking below is by **plugin key** and not by id and not,
//! first, by name.

use crate::model::FieldMeta;

/// The Greenhopper plugin's key for the Epic Link field, as
/// `field.schema.custom`.
///
/// The one property of the field an administrator cannot rename, and the one
/// that tells it from its three siblings -- `…:gh-epic-label` (Epic Name),
/// `…:gh-epic-status` (Epic Status) and `…:gh-epic-color` -- which sit beside
/// it in the same id range and would each be accepted by an id-shaped guess.
pub(crate) const EPIC_LINK_SCHEMA: &str = "com.pyxis.greenhopper.jira:gh-epic-link";

/// The display name Jira ships the field under, for the fallback below.
const EPIC_LINK_NAME: &str = "Epic Link";

/// This instance's Epic Link field id, or `None` where the instance has no
/// such field.
///
/// **The plugin key decides.** An instance whose administrator renamed the
/// field -- or runs Jira in a language that ships it renamed -- still matches,
/// and a custom field somebody called "Epic Link" by hand does not win over
/// the real one.
///
/// **The name is only a fallback**, for an instance that answers the field
/// table without a `schema` (an older Jira Software, a plugin that declines to
/// describe itself). It is deliberately narrow: exactly `Epic Link`,
/// case-insensitively, and a **custom** field only, so no system field can be
/// reached by it.
///
/// `None` is a first-class answer, not a failure: an instance with no Jira
/// Software at all has no Epic Link field, and the source keeps the behaviour
/// it has today -- no epic membership mirrored, said out loud in
/// [`crate::source`]'s connection detail rather than left to be discovered by
/// an empty Contexts view.
pub(crate) fn epic_link_field(fields: &[FieldMeta]) -> Option<String> {
    let by_key = fields.iter().find(|f| {
        f.schema
            .as_ref()
            .and_then(|s| s.custom.as_deref())
            .is_some_and(|key| key == EPIC_LINK_SCHEMA)
    });
    by_key
        .or_else(|| {
            fields.iter().find(|f| {
                f.custom
                    && f.name
                        .as_deref()
                        .is_some_and(|n| n.trim().eq_ignore_ascii_case(EPIC_LINK_NAME))
            })
        })
        // A field id is pasted straight into the `fields=` query parameter, so
        // an instance answering with something that is not a bare id is not
        // one this can use -- and `JiraConfig` would refuse it at the next
        // save anyway. Refusing here is what keeps the two agreeing.
        .filter(|f| crate::config::is_field_id(&f.id))
        .map(|f| f.id.clone())
}

/// The Greenhopper plugin's key for the Sprint field (#811).
pub(crate) const SPRINT_SCHEMA: &str = "com.pyxis.greenhopper.jira:gh-sprint";

/// The display name Jira ships the Sprint field under.
const SPRINT_NAME: &str = "Sprint";

/// This instance's Sprint field id, or `None` where it has none (#811).
///
/// Found the way [`epic_link_field`] finds Epic Link: by the plugin key, which
/// survives a rename, and only then by the name `Sprint` -- and the name only
/// for a **custom** field that describes **no** plugin key, so a field some
/// other plugin calls "Sprint" is never taken for Jira Software's. An id that
/// cannot go into `fields=` is not one.
pub(crate) fn sprint_field(fields: &[FieldMeta]) -> Option<String> {
    let key = |f: &FieldMeta| f.schema.as_ref().and_then(|s| s.custom.clone());
    fields
        .iter()
        .find(|f| key(f).is_some_and(|k| k == SPRINT_SCHEMA))
        .or_else(|| {
            fields.iter().find(|f| {
                f.custom
                    && key(f).is_none()
                    && f.name
                        .as_deref()
                        .is_some_and(|n| n.trim().eq_ignore_ascii_case(SPRINT_NAME))
            })
        })
        .filter(|f| crate::config::is_field_id(&f.id))
        .map(|f| f.id.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(id: &str, name: &str, custom: bool, schema: Option<&str>) -> FieldMeta {
        serde_json::from_value(serde_json::json!({
            "id": id,
            "name": name,
            "custom": custom,
            "schema": schema.map(|key| serde_json::json!({
                "type": "any", "custom": key, "customId": 10_101
            })),
        }))
        .expect("a field table entry")
    }

    /// The corpus a real Jira Software answers with, trimmed to the part that
    /// can be confused: the four Greenhopper epic fields, in the id order the
    /// product mints them, plus a system field.
    ///
    /// Epic Status sits **next to** Epic Link in that range, which is the
    /// whole hazard the id is per instance: `customfield_10102` here is a
    /// perfectly plausible thing for a reader to have copied off another
    /// server.
    /// The plugin key, spelled out rather than read from
    /// [`EPIC_LINK_SCHEMA`].
    ///
    /// Load-bearing: a fixture built from the constant agrees with whatever
    /// the constant says, so changing the constant to `gh-epic-status` would
    /// move the rule *and* the corpus together and every test here would pass.
    /// The literal is the second witness.
    const REAL_KEY: &str = "com.pyxis.greenhopper.jira:gh-epic-link";

    fn greenhopper() -> Vec<FieldMeta> {
        vec![
            field("summary", "Summary", false, None),
            field(
                "customfield_10100",
                "Epic Name",
                true,
                Some("com.pyxis.greenhopper.jira:gh-epic-label"),
            ),
            field("customfield_10101", "Epic Link", true, Some(REAL_KEY)),
            field(
                "customfield_10102",
                "Epic Status",
                true,
                Some("com.pyxis.greenhopper.jira:gh-epic-status"),
            ),
            field(
                "customfield_10103",
                "Epic Colour",
                true,
                Some("com.pyxis.greenhopper.jira:gh-epic-color"),
            ),
        ]
    }

    #[test]
    fn the_epic_link_field_is_the_one_with_the_greenhopper_key() {
        assert_eq!(
            epic_link_field(&greenhopper()).as_deref(),
            Some("customfield_10101")
        );
    }

    /// The sibling that costs the most if it is picked: Epic Status is a
    /// *status*, so an item whose payload carried it would look as though it
    /// belonged to an epic named "In Progress". Named as its own test because
    /// it is the mutation this module exists to survive.
    #[test]
    fn epic_status_is_never_the_answer() {
        let picked = epic_link_field(&greenhopper()).expect("the corpus has an Epic Link");
        assert_ne!(picked, "customfield_10102", "that is Epic Status");
        assert_ne!(picked, "customfield_10100", "that is Epic Name");
        assert_ne!(picked, "customfield_10103", "that is Epic Colour");
    }

    /// The plugin key beats the name, in both directions: a renamed real field
    /// is still found, and a look-alike somebody typed by hand does not
    /// displace it.
    #[test]
    fn the_plugin_key_beats_the_display_name() {
        let mut fields = greenhopper();
        fields[2] = field(
            "customfield_10109",
            "Übergeordnetes Epic",
            true,
            Some(REAL_KEY),
        );
        fields.insert(0, field("customfield_10500", "Epic Link", true, None));
        assert_eq!(
            epic_link_field(&fields).as_deref(),
            Some("customfield_10109"),
            "the renamed Greenhopper field wins over a hand-made one wearing its name"
        );
    }

    /// The fallback, for an instance that describes no schema at all.
    #[test]
    fn a_field_table_with_no_schemas_falls_back_to_the_name() {
        let fields = vec![
            field("summary", "Summary", false, None),
            field("customfield_10008", "Epic Link", true, None),
        ];
        assert_eq!(
            epic_link_field(&fields).as_deref(),
            Some("customfield_10008")
        );
    }

    /// …and the fallback reaches no system field, whatever it is called. A
    /// system field named "Epic Link" would be a Jira that renamed one of its
    /// own, and syncing it as the epic relation would mirror nonsense.
    #[test]
    fn the_name_fallback_never_reaches_a_system_field() {
        let fields = vec![field("parent", "Epic Link", false, None)];
        assert_eq!(epic_link_field(&fields), None);
    }

    /// An instance with no Jira Software has no Epic Link field, and that is
    /// an answer rather than a failure.
    #[test]
    fn an_instance_without_the_plugin_discovers_nothing() {
        let fields = vec![
            field("summary", "Summary", false, None),
            field(
                "customfield_10001",
                "Sprint",
                true,
                Some("com.example:other"),
            ),
        ];
        assert_eq!(epic_link_field(&fields), None);
        assert_eq!(epic_link_field(&[]), None);
    }

    /// An id that cannot go into `fields=` is not one this can hand back: the
    /// config's own validator would refuse it, and the two must not disagree
    /// about what a field id is.
    #[test]
    fn an_id_that_is_not_a_bare_field_id_is_not_discovered() {
        let fields = vec![field(
            "customfield_10101,*all",
            "Epic Link",
            true,
            Some(EPIC_LINK_SCHEMA),
        )];
        assert_eq!(epic_link_field(&fields), None);
        // The same rule the form applies, so a discovered id always saves.
        assert!(
            crate::JiraConfig::from_json(&serde_json::json!({
                "epic_link_field": "customfield_10101"
            }))
            .is_ok()
        );
    }

    /// The Sprint field's plugin key, spelled out for the reason [`REAL_KEY`]
    /// is.
    const SPRINT_KEY: &str = "com.pyxis.greenhopper.jira:gh-sprint";

    /// The Sprint field is found by its plugin key among the epic fields it
    /// sits beside, renamed or not (#811).
    #[test]
    fn the_sprint_field_is_the_one_with_the_greenhopper_key() {
        let mut fields = greenhopper();
        fields.push(field("customfield_10104", "Sprint", true, Some(SPRINT_KEY)));
        assert_eq!(sprint_field(&fields).as_deref(), Some("customfield_10104"));
        fields[5] = field("customfield_10107", "Iteration", true, Some(SPRINT_KEY));
        fields.insert(0, field("customfield_10500", "Sprint", true, None));
        assert_eq!(
            sprint_field(&fields).as_deref(),
            Some("customfield_10107"),
            "the renamed Greenhopper field wins over a hand-made one wearing its name"
        );
        // Epic Link is never the Sprint field, nor the other way round.
        assert_eq!(
            epic_link_field(&fields).as_deref(),
            Some("customfield_10101")
        );
    }

    /// The name is a fallback for a field table without schemas only: a
    /// field another plugin calls "Sprint" is not Jira Software's.
    #[test]
    fn the_sprint_name_fallback_takes_only_a_custom_field_with_no_plugin_key() {
        assert_eq!(
            sprint_field(&[field("customfield_10104", "Sprint", true, None)]).as_deref(),
            Some("customfield_10104")
        );
        assert_eq!(
            sprint_field(&[field(
                "customfield_10001",
                "Sprint",
                true,
                Some("com.example:other")
            )]),
            None
        );
        assert_eq!(
            sprint_field(&[field("sprint", "Sprint", false, None)]),
            None
        );
        assert_eq!(sprint_field(&greenhopper()), None);
        assert_eq!(
            sprint_field(&[field(
                "customfield_1,*all",
                "Sprint",
                true,
                Some(SPRINT_KEY)
            )]),
            None
        );
    }
}
