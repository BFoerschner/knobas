pub mod activity;
pub mod context;
pub mod entity;
mod error;
pub mod inbox;
pub mod link;
pub mod mini_board;
pub mod note;
pub mod project;
pub mod start_work;
pub mod suggest;
pub mod write_queue;

pub use error::CoreError;

/// Declare an enum whose variants are a **closed vocabulary shared with the
/// database**: each one has a stored spelling, and a CHECK constraint in the
/// migrations lists exactly those spellings.
///
/// The point is that `ALL` and `as_str` are generated from the *same* variant
/// list as the enum itself, so the three cannot drift. A hand-written `ALL`
/// beside a hand-written enum is a list that a new variant silently misses --
/// and for these enums that is not a cosmetic bug: the value reaches a `text`
/// column with a CHECK constraint on it, so an unlisted spelling is a failed
/// `INSERT` at runtime, on a path that may be nowhere near the code that added
/// the variant.
///
/// With this, adding a variant necessarily adds it to `ALL`, and the tests
/// that walk `ALL` against the migration then fail until the constraint knows
/// about it too -- which is a red test instead of a broken write.
///
/// It lives here, in the crate every other one depends on, because both sides
/// of the bridge need it: `link::Origin` here and `knobas_sync`'s `AuthState`,
/// `SyncTrigger` and `SyncOutcome` are the same kind of list against the same
/// kind of constraint. Being exported makes it callable from crates that have
/// no `serde` in scope, so the expansion names `::serde` absolutely; a caller
/// still needs the dependency, but gets a missing-crate error rather than a
/// baffling one inside a macro it did not write.
///
/// The derive set is every trait a fieldless label wants and none that costs
/// anything: `Hash` is there because `Origin` had it before it was declared
/// this way, and taking it away would be a change to a public type that
/// nothing asked for.
#[macro_export]
macro_rules! closed_vocabulary {
    (
        $(#[$enum_meta:meta])*
        pub enum $name:ident {
            $( $(#[$variant_meta:meta])* $variant:ident => $wire:literal ),+ $(,)?
        }
    ) => {
        $(#[$enum_meta])*
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, Hash, ::serde::Serialize, ::serde::Deserialize,
        )]
        #[serde(rename_all = "snake_case")]
        pub enum $name {
            $( $(#[$variant_meta])* $variant, )+
        }

        impl $name {
            /// Every variant, generated from the same list as the variants --
            /// so one cannot be added without appearing here.
            pub const ALL: &'static [$name] = &[ $( $name::$variant ),+ ];

            /// The spelling stored in the database and put on the wire.
            #[must_use]
            pub fn as_str(self) -> &'static str {
                match self { $( $name::$variant => $wire ),+ }
            }
        }
    };
}

/// SQL for "the string this JSON path leads to, or nothing" -- the shape every
/// **payload read outside an adapter** (ADR-0007) takes here.
///
/// `->>` yields an object's or an array's *text form* rather than nothing, so
/// without the type check a source that spells a field some other way would
/// arrive as a value like `{"id":3}` -- a guess dressed as an observation. The
/// `nullif(btrim(...))` is the same refusal for a value that is blank or only
/// whitespace: unreadable, not a value.
///
/// It lives here rather than in the one module that first needed it, for the
/// reason [`closed_vocabulary!`] does: both sides of the bridge read payloads
/// now. The mini board's status and priority reads are in this crate; the
/// room's own list statements are in `knobas_app::commands::entity`, and a
/// second copy of this guard is how one of them quietly starts stringifying
/// objects while the other does not. Exported, so a caller outside this crate
/// gets the same three refusals rather than its own two.
///
/// `$path` is a literal because that is what `concat!` can fold: every
/// statement built with this is a `&'static str`, so nothing here can
/// concatenate a value into SQL.
#[macro_export]
macro_rules! string_at {
    ($path:literal) => {
        concat!(
            "(case when jsonb_typeof(",
            $path,
            ") = 'string' then nullif(btrim(",
            $path,
            " #>> '{}'), '') end)"
        )
    };
}
