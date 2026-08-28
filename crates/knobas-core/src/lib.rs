pub mod activity;
pub mod entity;
mod error;
pub mod link;

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
/// kind of constraint.
#[macro_export]
macro_rules! closed_vocabulary {
    (
        $(#[$enum_meta:meta])*
        pub enum $name:ident {
            $( $(#[$variant_meta:meta])* $variant:ident => $wire:literal ),+ $(,)?
        }
    ) => {
        $(#[$enum_meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
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
