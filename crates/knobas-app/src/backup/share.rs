//! What a **share export** carries: the parts, and the tables each one is.
//!
//! Spec #427 (M4.2), `CONTEXT.md`'s *Share export*. A backup is everything
//! knobas owns and is meant for the person who took it; a share export is the
//! same archive format restricted to the parts a colleague should get, so that
//! handing somebody a link map does not hand them a diary.
//!
//! Pure, and separated from [`super`] for the reason [`super::policy`] is: the
//! only decision here -- which tables a part is -- is the one worth reading
//! without a database in front of you.
//!
//! # Why a table list and not a schema
//!
//! Because "these parts and no others" is a promise about a list somebody
//! chose. The backup is deliberately schema-scoped so that a table a later
//! migration adds rides in it without anyone remembering
//! ([`knobas_db::backup::dump`]); a share export must do the opposite, because
//! a scope that silently grew when a migration landed is precisely the leak
//! this feature exists to prevent. The cost is that a new table is *not*
//! shareable until someone puts it in a part here, which is the direction that
//! fails safely.
//!
//! # `knobas.setting` is in no part
//!
//! `CONTEXT.md` and the spec both name "the time settings" as part of *time*,
//! and they are not here. `knobas.setting` is one key/value table holding
//! every feature's bookkeeping at once -- the backup schedule, the last
//! export, the first-run flag, the standup publish target, the inbox
//! notification kinds, the smart-list seen stamps, the observation-sweep
//! horizon -- and `pg_dump` restricts an archive by *table*, never by row. So
//! the only two options were the whole table or none of it, and the whole
//! table would restore this machine's backup schedule and first-run state onto
//! the recipient's: a share export whose declared purpose is that nothing
//! personal is in it, carrying the sharer's settings. None of it, then, and
//! the one genuinely time-shaped row (`time.passive_attribution`) is a
//! preference the recipient sets for themselves.
//!
//! # `knobas.entity` rides with four parts
//!
//! An entity row is the *address* of a thing -- id, kind, title -- and a link,
//! an asset, a route, a context anchor and a note all name one. `knobas.asset`
//! and `knobas.route` go further and take their primary key from it
//! (`asset_entity_fk`, `route_entity_fk`), so an asset restored without its
//! entity is a row nothing can open.
//!
//! **The whole table travels**, and this is the one consequence worth reading
//! twice: `pg_dump` restricts an archive by table and never by row, so "the
//! entity rows the links reference" is not an argument list anyone can write.
//! Every entity's id, kind and title crosses -- a note's title included, with
//! notes switched off, because a note *is* an entity. What a notes-off export
//! withholds is the note's **body**; what it cannot withhold is that a note by
//! that title exists. Everything else hanging off an entity that no chosen
//! part names stays behind: the body, the blocks, the worklogs, the activity
//! stream, the mirror.

/// Which parts of the owned schema a share export carries.
///
/// The defaults are the spec's: links, assets, contexts and sources on; notes
/// and time off. `#[serde(default)]` on the struct means a caller may send
/// only the toggles it wants to change and get the ratified answer for the
/// rest -- and a knobas built before a future part still decodes a payload
/// that names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct ShareParts {
    /// `knobas.link` and the entity rows its ends name.
    pub links: bool,
    /// `knobas.asset` and `knobas.route` -- the estate.
    pub assets: bool,
    /// `knobas.context`.
    pub contexts: bool,
    /// `knobas.note`. **Off by default**: a note is the private half of what
    /// knobas holds.
    pub notes: bool,
    /// The timer, its blocks, the worklogs they became and the observations
    /// behind them. **Off by default**, for the reason notes are: a colleague
    /// reading a link map has no use for somebody's hours.
    pub time: bool,
    /// `knobas.source_config` -- which systems knobas talks to, and how. No
    /// secret is in it: spec §14 puts every credential in the OS keychain and
    /// nothing secret ever reaches Postgres.
    pub sources: bool,
}

impl Default for ShareParts {
    fn default() -> Self {
        Self {
            links: true,
            assets: true,
            contexts: true,
            notes: false,
            time: false,
            sources: true,
        }
    }
}

/// The address book every referencing part needs with it.
const ENTITY: &str = "entity";

impl ShareParts {
    /// Whether any part is on at all.
    #[must_use]
    pub const fn any(self) -> bool {
        self.links || self.assets || self.contexts || self.notes || self.time || self.sources
    }

    /// The `knobas` tables this selection dumps, in a stable order and without
    /// repeats.
    ///
    /// Unqualified names, which is what [`knobas_db::backup::dump_tables`]
    /// takes. Empty exactly when [`any`](Self::any) is false, and that case is
    /// refused before it reaches `pg_dump` -- an argument list with no scope
    /// in it dumps the whole database.
    #[must_use]
    pub fn tables(self) -> Vec<&'static str> {
        let mut tables: Vec<&'static str> = Vec::new();
        let mut push = |name: &'static str| {
            if !tables.contains(&name) {
                tables.push(name);
            }
        };
        // Entity first, so a reader of the argument list sees the address book
        // in front of the things that have addresses.
        if self.links || self.assets || self.contexts || self.notes {
            push(ENTITY);
        }
        if self.links {
            push("link");
        }
        if self.assets {
            push("asset");
            push("route");
        }
        if self.contexts {
            push("context");
        }
        if self.notes {
            push("note");
        }
        if self.time {
            push("timer");
            push("block");
            push("worklog");
            push("heartbeat");
        }
        if self.sources {
            push("source_config");
        }
        tables
    }

    /// Every part off -- the shape a "select nothing" UI produces, and the one
    /// a dump refuses.
    #[must_use]
    pub const fn none() -> Self {
        Self {
            links: false,
            assets: false,
            contexts: false,
            notes: false,
            time: false,
            sources: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The ratified defaults, spelled out rather than derived: this is the
    /// sentence `CONTEXT.md` and spec #427 both write, and a `Default` that
    /// drifted from it would still look like a `Default`.
    #[test]
    fn the_defaults_are_the_ratified_ones() {
        let parts = ShareParts::default();
        assert!(parts.links && parts.assets && parts.contexts && parts.sources);
        assert!(
            !parts.notes && !parts.time,
            "notes and time are off by default -- a colleague gets the link map, not the hours"
        );
        assert_eq!(
            parts.tables(),
            vec![
                "entity",
                "link",
                "asset",
                "route",
                "context",
                "source_config"
            ],
        );
    }

    /// Each toggle alone is its own tables and the address book they need,
    /// and **nothing else** -- the property the whole feature is.
    #[test]
    fn each_part_alone_is_exactly_its_own_tables() {
        for (parts, expected) in [
            (
                ShareParts {
                    links: true,
                    ..ShareParts::none()
                },
                vec!["entity", "link"],
            ),
            (
                ShareParts {
                    assets: true,
                    ..ShareParts::none()
                },
                vec!["entity", "asset", "route"],
            ),
            (
                ShareParts {
                    contexts: true,
                    ..ShareParts::none()
                },
                vec!["entity", "context"],
            ),
            (
                ShareParts {
                    notes: true,
                    ..ShareParts::none()
                },
                vec!["entity", "note"],
            ),
            (
                ShareParts {
                    time: true,
                    ..ShareParts::none()
                },
                vec!["timer", "block", "worklog", "heartbeat"],
            ),
            (
                ShareParts {
                    sources: true,
                    ..ShareParts::none()
                },
                vec!["source_config"],
            ),
        ] {
            assert_eq!(parts.tables(), expected, "{parts:?}");
        }
    }

    /// *Time* carries no `setting` row, and neither does anything else.
    ///
    /// The module docs say why; this is the assertion that keeps it true. The
    /// failure it exists for is the well-meant one: somebody reads
    /// "the time settings" in `CONTEXT.md`, adds `"setting"` to the time part,
    /// and every share export from then on carries the sharer's backup
    /// schedule and first-run flag.
    #[test]
    fn no_part_carries_the_settings_table() {
        for parts in [
            ShareParts::default(),
            ShareParts {
                time: true,
                ..ShareParts::default()
            },
            ShareParts {
                links: true,
                assets: true,
                contexts: true,
                notes: true,
                time: true,
                sources: true,
            },
        ] {
            assert!(
                !parts.tables().contains(&"setting"),
                "`knobas.setting` is one table for every feature's bookkeeping: {parts:?}"
            );
        }
    }

    /// The tables the archive must never carry, whatever is on.
    ///
    /// The activity stream and the write queue are knobas's own history of
    /// what happened on this machine, and the mirror re-syncs. Named here
    /// because "everything else is excluded" is the kind of claim a table list
    /// makes silently and stops making silently.
    #[test]
    fn nothing_carries_the_activity_stream_the_queue_or_the_mirror() {
        let everything = ShareParts {
            links: true,
            assets: true,
            contexts: true,
            notes: true,
            time: true,
            sources: true,
        };
        for forbidden in [
            "activity",
            "write_queue",
            "sync_run",
            "start_work_step",
            "inbox_state",
            "item",
        ] {
            assert!(
                !everything.tables().contains(&forbidden),
                "`{forbidden}` is in a share export"
            );
        }
    }

    /// Nothing on is nothing to dump, and that has to be *askable* -- the
    /// caller refuses it rather than handing `pg_dump` an argument list with
    /// no scope in it, which dumps the whole database.
    #[test]
    fn no_part_at_all_is_no_tables_at_all() {
        assert!(!ShareParts::none().any());
        assert!(ShareParts::none().tables().is_empty());
        assert!(ShareParts::default().any());
    }

    /// A payload naming one toggle leaves the ratified answer on the rest --
    /// and every field still decodes from the spelling the mirror sends.
    #[test]
    fn a_partial_payload_decodes_onto_the_defaults() {
        let decoded: ShareParts = serde_json::from_value(serde_json::json!({ "notes": true }))
            .expect("a payload naming one part");
        assert_eq!(
            decoded,
            ShareParts {
                notes: true,
                ..ShareParts::default()
            }
        );

        let all: ShareParts = serde_json::from_value(serde_json::json!({
            "links": false, "assets": false, "contexts": false,
            "notes": false, "time": false, "sources": false
        }))
        .expect("a payload naming every part");
        assert_eq!(all, ShareParts::none());
    }
}
