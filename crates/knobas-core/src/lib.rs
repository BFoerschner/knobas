pub mod activity;
pub mod asset;
pub mod builds;
pub mod checkout;
pub mod code_facts;
pub mod context;
pub mod detail;
pub mod entity_list;
mod error;
pub mod evidence;
pub mod inbox;
mod item_field;
pub mod link;
pub mod link_set;
mod live;
pub mod mini_board;
pub mod note;
pub mod project;
pub mod reach;
pub mod signals;
pub mod start_work;
pub mod suggest;
pub mod viewed;
pub mod web_url;
pub mod write_queue;

pub use error::CoreError;

// The types the source SPI shares with the core moved to `knobas-spi-types`
// (#1234), which builds for `wasm32-wasip2`; these keep their old paths. The
// payload SQL macros moved with the module (`#[macro_export]` puts them at
// that crate's root), so they are re-exported here by name.
pub use knobas_spi_types::{
    body_text_shown, declared_array, declared_candidates, declared_flag, declared_integer,
    declared_list, declared_list_candidates, declared_names_as_parent, declared_parent_names,
    declared_parent_path, declared_parents, declared_project_path, declared_segments,
    declared_state, declared_state_candidates, declared_string, declared_ticket_paths,
    declared_tickets, declared_trigger, declares_field, first_string_of, list_values_of,
    path_line_read, project_path_part, state_of,
};
pub use knobas_spi_types::{delivery, entity, payload, tone};

/// Declare an enum whose variants are a **closed vocabulary shared with the
/// database**: each one has a stored or computed spelling, and for every user
/// but one (below) a CHECK constraint in the migrations lists exactly those
/// spellings.
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
/// One user has no constraint to be pinned against: `inbox::Category`, whose
/// spelling a statement computes rather than a column stores. It takes the
/// macro for the one list and the codec, and has no migration test.
///
/// It lives here, in the crate every other one depends on, because both sides
/// of the bridge need it: `link::Origin` here and `knobas_sync`'s `AuthState`,
/// `SyncTrigger` and `SyncOutcome` are the same kind of list against the same
/// kind of constraint. Being exported makes it callable from crates that have
/// no `serde` in scope, so the expansion names `::serde` absolutely; a caller
/// still needs the dependency, but gets a missing-crate error rather than a
/// baffling one inside a macro it did not write.
///
/// It also emits the codec every such enum needs and once wrote by hand
/// (#1706): `Display` as the stored spelling, `FromStr` off `ALL` refusing any
/// other spelling with [`UnknownValue`], and the `sqlx` `Type`/`Decode` pair
/// that reads the plain `text` column through that `FromStr`. A vocabulary
/// that must read an unknown spelling as a fallback rather than refuse it
/// (a row written by a newer knobas) parses and then picks its default; it
/// does not walk `ALL` itself. The expansion names `::sqlx` the way it names
/// `::serde`, so a caller needs `sqlx` as a dependency too.
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

        impl ::std::fmt::Display for $name {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                f.write_str(self.as_str())
            }
        }

        impl ::std::str::FromStr for $name {
            type Err = $crate::UnknownValue;

            fn from_str(s: &str) -> ::std::result::Result<Self, Self::Err> {
                // Read off `ALL` rather than a second match: one list.
                $name::ALL
                    .iter()
                    .copied()
                    .find(|value| value.as_str() == s)
                    .ok_or_else(|| $crate::UnknownValue {
                        vocabulary: stringify!($name),
                        value: s.to_owned(),
                    })
            }
        }

        // The column is plain `text`, so the codec borrows `str`'s rather
        // than declaring a PostgreSQL enum type that does not exist.
        impl ::sqlx::Type<::sqlx::Postgres> for $name {
            fn type_info() -> ::sqlx::postgres::PgTypeInfo {
                <str as ::sqlx::Type<::sqlx::Postgres>>::type_info()
            }

            fn compatible(ty: &::sqlx::postgres::PgTypeInfo) -> bool {
                <&str as ::sqlx::Type<::sqlx::Postgres>>::compatible(ty)
            }
        }

        impl<'r> ::sqlx::Decode<'r, ::sqlx::Postgres> for $name {
            fn decode(
                value: ::sqlx::postgres::PgValueRef<'r>,
            ) -> ::std::result::Result<Self, ::sqlx::error::BoxDynError> {
                let text = <&str as ::sqlx::Decode<'r, ::sqlx::Postgres>>::decode(value)?;
                Ok(text.parse::<$name>()?)
            }
        }
    };
}

/// A stored or sent spelling that is not one of a closed vocabulary's.
///
/// One type for every vocabulary [`closed_vocabulary!`] declares, naming the
/// vocabulary it was read as, so a decode failure says which column's list it
/// missed without each enum carrying an error type of its own.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("unknown {vocabulary} value {value:?}")]
pub struct UnknownValue {
    /// The enum the value was read as.
    pub vocabulary: &'static str,
    /// The spelling as it arrived.
    pub value: String,
}

// `string_at!` lived here until #277: SQL for "the string this *literal* JSON
// path leads to, or nothing", the shape ADR-0007's interim payload reads took.
// Every one of its call sites -- the mini board's status and priority, the
// project census's key and name, the four room statements -- now resolves a
// path the source itself declares, so the macro expanded nowhere and is gone
// rather than left as an exported shape with no expander and a doc comment
// describing call sites that no longer exist. Its three refusals are not:
// `payload::declared_string!` makes the same ones, once, and
// `payload::resolve_string` is the Rust half of them. A future payload read
// over a literal path -- the reads ADR-0007 still governs, which §10.8 lists
// -- takes them from there.

#[cfg(test)]
mod tests {
    crate::closed_vocabulary! {
        /// A vocabulary of the macro's own, so the generated codec and its error
        /// are tested at the macro; each declared enum keeps its own pins.
        pub enum Shade {
            Light => "light",
            DarkGrey => "dark_grey",
        }
    }

    #[test]
    fn every_spelling_parses_back_to_its_variant() {
        assert_eq!("light".parse::<Shade>(), Ok(Shade::Light));
        assert_eq!("dark_grey".parse::<Shade>(), Ok(Shade::DarkGrey));
    }

    #[test]
    fn a_spelling_outside_the_vocabulary_is_refused_by_name() {
        let refused = "dark-grey".parse::<Shade>().unwrap_err();
        assert_eq!(
            refused,
            crate::UnknownValue {
                vocabulary: "Shade",
                value: "dark-grey".to_owned(),
            }
        );
        assert_eq!(refused.to_string(), r#"unknown Shade value "dark-grey""#);
    }

    #[test]
    fn a_variant_displays_as_its_stored_spelling() {
        assert_eq!(Shade::DarkGrey.to_string(), "dark_grey");
    }
}
