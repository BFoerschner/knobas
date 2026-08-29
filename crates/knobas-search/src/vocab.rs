//! What the grammar parses *against*.
//!
//! [`parse`] on its own cannot tell `/ji` from `/nope`, `type:build` from
//! `type:hypervisor`, or `@me` from a stranger: all three depend on what this
//! installation is actually configured with. That knowledge is collected here,
//! once per query, so the parser stays pure and testable.
//!
//! Three things live in a [`Vocabulary`]:
//!
//! * the configured **sources** (id, adapter kind, display name), which is what
//!   `/alias` and `source:` resolve through;
//! * the **identity** behind `@me` -- the usernames the sources were configured
//!   with, so "who am I" is never a second thing to fill in (interfaces §4.2);
//! * the [`KindCatalog`], the display metadata a result group is labelled from.
//!
//! [`parse`]: crate::query::parse

use std::collections::BTreeMap;

use knobas_source::{KindInfo, SourceDescriptor};
use sqlx::PgPool;

use crate::SearchError;

/// One configured source instance, as the grammar sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceVocab {
    /// The instance id, which is also the entity namespace (`"jira-eu"`).
    pub id: String,
    /// Which adapter this is an instance of (`"jira"`). Several instances can
    /// share one -- that is the whole point of `/ji` meaning *the Jiras*.
    pub adapter_kind: String,
    /// The name shown in the sources list, matched as a typing prefix.
    pub display_name: String,
    /// The account this source was configured with, if it needed one. The
    /// union of these is [`Vocabulary::identity`].
    pub username: Option<String>,
}

/// Everything one query's grammar is resolved against.
#[derive(Debug, Clone, Default)]
pub struct Vocabulary {
    /// Configured sources, **ordered by id**. The order is load-bearing: it is
    /// the order `/ji` returns its instances in, and therefore the order that
    /// ends up in the generated SQL and in every test's expectation.
    pub sources: Vec<SourceVocab>,
    /// The distinct usernames the sources were configured with, sorted. This
    /// is what `@me` filters by.
    pub identity: Vec<String>,
    /// Display metadata for result groups.
    pub kinds: KindCatalog,
}

/// Built-in source aliases (spec §4), mapping an alias to an **adapter kind**
/// rather than to an instance.
///
/// That indirection is the reason `/ji` finds a second Jira the day it is
/// added, with no table to update. An alias whose adapter is not configured
/// resolves to nothing, which is the honest answer: `/cf` in M1 means "a
/// Confluence you have not added yet".
const SOURCE_ALIASES: &[(&str, &str)] = &[
    ("ji", "jira"),
    ("jira", "jira"),
    ("gt", "gitea"),
    ("git", "gitea"),
    ("gitea", "gitea"),
    ("tc", "teamcity"),
    ("ci", "teamcity"),
    ("teamcity", "teamcity"),
    ("cf", "confluence"),
    ("conf", "confluence"),
    ("wiki", "confluence"),
    ("confluence", "confluence"),
    ("nt", "note"),
    ("note", "note"),
    ("notes", "note"),
    ("local", "note"),
    ("as", "asset"),
    ("asset", "asset"),
    ("assets", "asset"),
    ("infra", "asset"),
    ("kuma", "asset"),
    ("flowrun", "asset"),
];

/// The one query a vocabulary is loaded with.
///
/// It stays **one** query on purpose: this runs on the hot path of every
/// keystroke, and a second round trip per search costs more than the search.
///
/// `kind` is `0001`'s column name for the *adapter* kind; interfaces §2.2 spells
/// the DTO field `adapter_kind`. Renaming the column would be a migration, and
/// `0002` was the only one M1 gets -- so the alias lives here.
///
/// The columns are named rather than `select *`ed. That is a rule for every
/// query in this crate (interfaces §1): the mirror's view carries a `tsvector`,
/// and reading one into a `FromRow` struct panics at runtime.
const SOURCE_VOCAB_SQL: &str = r"
select id,
       kind                  as adapter_kind,
       display_name,
       -- 'Who am I' is not a setting anyone should have to fill in twice: it
       -- is the username each source was configured with (interfaces §4.2).
       config ->> 'username' as username
  from knobas.source_config
 where enabled
 order by id
";

#[derive(sqlx::FromRow)]
struct SourceConfigRow {
    id: String,
    adapter_kind: String,
    display_name: String,
    username: Option<String>,
}

impl Vocabulary {
    /// Read the configured sources and the identity behind `@me`.
    ///
    /// The [`KindCatalog`] is passed in rather than loaded: it comes from the
    /// compiled-in adapters, not from the database, and the caller is the one
    /// that has the registry (open question **E-Q2**).
    ///
    /// A **disabled** source is not in the vocabulary at all, so `/ji` stops
    /// resolving to it and the token is reported as unknown. That is the
    /// truthful answer -- its rows are still in the mirror, but the user turned
    /// the source off.
    ///
    /// # Errors
    ///
    /// [`SearchError::Db`] if the configuration cannot be read.
    pub async fn load(pool: &PgPool, kinds: KindCatalog) -> Result<Self, SearchError> {
        let rows = sqlx::query_as::<_, SourceConfigRow>(SOURCE_VOCAB_SQL)
            .fetch_all(pool)
            .await?;
        let sources: Vec<SourceVocab> = rows
            .into_iter()
            .map(|row| SourceVocab {
                id: row.id,
                adapter_kind: row.adapter_kind,
                display_name: row.display_name,
                // A blank username is the JSON field being present and empty,
                // which is nobody -- and an empty string in `identity` would
                // make `@me` match every item with no author.
                username: row.username.filter(|name| !name.trim().is_empty()),
            })
            .collect();

        let mut identity: Vec<String> = sources.iter().filter_map(|s| s.username.clone()).collect();
        identity.sort_unstable();
        identity.dedup();

        Ok(Self {
            sources,
            identity,
            kinds,
        })
    }

    /// Which configured source instances a `/token` or `source:token` names.
    ///
    /// Resolution order, most specific first:
    ///
    /// 1. an exact configured **id** -- so `/jira-eu` is that instance and
    ///    nothing else, even though `jira` is also an alias;
    /// 2. a built-in **alias** ([`SOURCE_ALIASES`]) resolved to an adapter
    ///    kind, then every configured instance of it;
    /// 3. the **adapter kind** typed out in full, for an adapter with no alias;
    /// 4. a **unique** case-insensitive prefix of a configured id or display
    ///    name, which is what makes the box answer while the name is still
    ///    being typed. Ambiguous prefixes resolve to nothing rather than to a
    ///    guess.
    ///
    /// An empty result is reported to the user as an unknown token by the
    /// parser; it is never silently treated as "no filter".
    #[must_use]
    pub fn resolve_source(&self, token: &str) -> Vec<String> {
        let needle = token.trim().to_lowercase();
        if needle.is_empty() {
            return Vec::new();
        }

        if let Some(exact) = self.sources.iter().find(|s| s.id.to_lowercase() == needle) {
            return vec![exact.id.clone()];
        }

        let adapter = SOURCE_ALIASES
            .iter()
            .find(|(alias, _)| *alias == needle)
            .map_or(needle.as_str(), |(_, kind)| *kind);
        let by_kind: Vec<String> = self
            .sources
            .iter()
            .filter(|s| s.adapter_kind.to_lowercase() == adapter)
            .map(|s| s.id.clone())
            .collect();
        if !by_kind.is_empty() {
            return by_kind;
        }

        let by_prefix: Vec<String> = self
            .sources
            .iter()
            .filter(|s| {
                s.id.to_lowercase().starts_with(&needle)
                    || s.display_name.to_lowercase().starts_with(&needle)
            })
            .map(|s| s.id.clone())
            .collect();
        if by_prefix.len() == 1 {
            by_prefix
        } else {
            Vec::new()
        }
    }

    /// A vocabulary for tests: two Jiras (so `/ji` has something to be plural
    /// about), a Gitea, a TeamCity, one identity and five declared kinds.
    #[cfg(any(test, feature = "test-util"))]
    #[must_use]
    pub fn fixture() -> Self {
        let source = |id: &str, adapter_kind: &str, display_name: &str, username: Option<&str>| {
            SourceVocab {
                id: id.to_owned(),
                adapter_kind: adapter_kind.to_owned(),
                display_name: display_name.to_owned(),
                username: username.map(str::to_owned),
            }
        };
        Self {
            sources: vec![
                source("gitea", "gitea", "Gitea", None),
                source("jira", "jira", "Jira", Some("mara.lindqvist")),
                source("jira-eu", "jira", "Jira EU", None),
                // A display name that is *not* a prefix of its id, so
                // that step 4 of `resolve_source` has something only it can
                // answer.
                source("teamcity", "teamcity", "Buildserver", None),
            ],
            identity: vec!["mara.lindqvist".to_owned()],
            kinds: KindCatalog::from_kinds(["ticket", "pr", "build", "page", "commit"]),
        }
    }
}

/// Irregular plurals, keyed on the kind id.
///
/// Only the ones a compiled-in adapter can actually emit; anything else falls
/// back to `label + "s"`, which is right far more often than it is wrong and
/// visibly wrong when it is not.
const IRREGULAR_PLURALS: &[(&str, &str)] = &[
    ("pr", "Pull requests"),
    ("commit", "Commits"),
    ("branch", "Branches"),
    ("repo", "Repositories"),
    ("page", "Pages"),
    ("note", "Notes"),
    ("asset", "Assets"),
];

/// The character a one-letter kind's monogram is padded with.
const MONOGRAM_PAD: char = '·';

/// Display metadata for entity kinds: what a result group is labelled and
/// chipped with.
///
/// Spec §3a is explicit that this comes from the adapter's
/// `SourceDescriptor::entity_kinds`, so that adding a source needs no change in
/// search or in the UI. A kind no compiled-in adapter declares -- an older
/// mirror, a disabled adapter, a source added by a plugin -- still has to be
/// grouped, labelled and chipped, so everything is *derivable* and declared
/// metadata merely wins where it exists.
#[derive(Debug, Clone)]
pub struct KindCatalog {
    /// `BTreeMap` rather than `HashMap`: [`Self::declared_kinds`] feeds chips
    /// and generated SQL, and both want a stable order.
    declared: BTreeMap<String, KindInfo>,
    /// The kinds knobas **owns**, which no descriptor declares.
    ///
    /// Separate from `declared` rather than merged into it, and that is the
    /// whole shape of the seam. Two reasons, and both are load-bearing:
    ///
    /// * [`Self::is_empty`] means *"nothing has told me what kinds exist"* --
    ///   the E-Q2 state the product ships in. Owned kinds are compiled in and
    ///   are always here, so folding them into `declared` would make the
    ///   catalog claim to have been told, and `type:hypervisor` would start
    ///   being reported as unknown while nothing had wired the registry
    ///   through.
    /// * a descriptor must not be able to claim one. Merged, "first
    ///   declaration wins" would hand `note` to whichever adapter declared it
    ///   first; kept apart, [`Self::info`] can prefer the owner.
    owned: BTreeMap<String, KindInfo>,
}

impl Default for KindCatalog {
    /// The honest default while nothing wires the adapter registry through
    /// (**E-Q2**): no descriptor has spoken, and knobas' own kinds are still
    /// knobas'.
    fn default() -> Self {
        Self {
            declared: BTreeMap::new(),
            owned: owned_kinds(),
        }
    }
}

impl KindCatalog {
    /// Build a catalog from what the compiled-in adapters declare, plus what
    /// knobas owns.
    ///
    /// First declaration of a kind wins, so the result does not depend on how
    /// a registry happens to order two adapters that both emit `"ticket"`.
    ///
    /// A descriptor that declares a kind knobas owns is **ignored for that
    /// kind** -- see [`Self::owned`]. It is not an error here: refusing an
    /// adapter is the sync engine's job (`PgSink::check`), and a catalog that
    /// panicked or failed would take the launcher down over a labelling
    /// question.
    ///
    /// [`Self::owned`]: KindCatalog
    pub fn from_descriptors(descriptors: impl IntoIterator<Item = SourceDescriptor>) -> Self {
        let owned = owned_kinds();
        let mut declared: BTreeMap<String, KindInfo> = BTreeMap::new();
        for descriptor in descriptors {
            for kind in descriptor.entity_kinds {
                if owned.contains_key(&kind.id) {
                    continue;
                }
                declared.entry(kind.id.clone()).or_insert(kind);
            }
        }
        Self { declared, owned }
    }

    /// Display metadata for one kind: knobas' own if it owns the kind, the
    /// adapter's if one declared it, derived otherwise.
    ///
    /// Owned first, because a kind knobas owns is not an adapter's to rename.
    #[must_use]
    pub fn info(&self, kind: &str) -> KindInfo {
        self.owned
            .get(kind)
            .or_else(|| self.declared.get(kind))
            .cloned()
            .unwrap_or_else(|| derive_kind_info(kind))
    }

    /// The kinds knobas owns rather than mirrors, in a stable order.
    #[must_use]
    pub fn owned_kinds(&self) -> Vec<String> {
        self.owned.keys().cloned().collect()
    }

    /// Whether this catalog knows the kind at all -- declared by an adapter, or
    /// owned by knobas.
    ///
    /// This is what the grammar disambiguates on: `type:note` is a kind filter
    /// because knobas owns notes, even though no descriptor mentions them.
    #[must_use]
    pub fn is_known(&self, kind: &str) -> bool {
        self.owned.contains_key(kind) || self.declared.contains_key(kind)
    }

    /// Every kind a compiled-in adapter declares, in a stable order.
    #[must_use]
    pub fn declared_kinds(&self) -> Vec<String> {
        self.declared.keys().cloned().collect()
    }

    /// Whether any **adapter** has told this catalog what kinds it emits.
    ///
    /// An empty catalog is **not** the statement "there are no kinds"; it is
    /// knobas not yet knowing what kinds sources have, because nothing wires
    /// the adapter registry through until stream F's `list_adapters` (open
    /// question **E-Q2**). Callers that disambiguate on [`Self::is_known`] have
    /// to check this first, or they will read "I have not been told" as "no".
    ///
    /// Owned kinds are deliberately *not* counted. They are compiled in, so
    /// they are here in every catalog including the default one -- and a
    /// predicate that they made non-empty would answer "I have been told"
    /// before anybody had said anything.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.declared.is_empty()
    }

    /// Whether an **adapter** declared this kind.
    ///
    /// Deliberately narrower than [`Self::is_known`]: a kind knobas owns is
    /// declared by nobody, which is the whole of story 21, and a predicate that
    /// blurred the two would make "no descriptor declares it" untestable. The
    /// grammar wants `is_known`; this is for anything asking about descriptors.
    #[must_use]
    pub fn is_declared(&self, kind: &str) -> bool {
        self.declared.contains_key(kind)
    }

    /// A catalog that declares these kind ids with derived metadata.
    ///
    /// For tests and for the `KindCatalog::default()` path: it makes
    /// `type:build` resolve without inventing a descriptor around it.
    #[cfg(any(test, feature = "test-util"))]
    #[must_use]
    pub fn from_kinds<'a>(kinds: impl IntoIterator<Item = &'a str>) -> Self {
        Self {
            declared: kinds
                .into_iter()
                .map(|kind| (kind.to_owned(), derive_kind_info(kind)))
                .collect(),
            owned: owned_kinds(),
        }
    }
}

/// The kinds knobas owns, as the catalog holds them.
///
/// The list is `knobas_core::entity::OWNED_KINDS` and lives there rather than
/// here because it is tied to `RESERVED_NAMESPACES`: an owned kind's id is also
/// the namespace its entities are addressed in, which is what migration
/// `0006`'s sweep-safety constraint is written on.
///
/// `full_sync_exhaustive: false`, always. An owned kind is emitted by no sync,
/// so there is no full sync for it to be exhaustive over -- and the same
/// reasoning `derive_kind_info` records applies twice over here: an affirmative
/// would be a claim nothing vouches for, on the one kind that has no source to
/// re-fetch it from.
fn owned_kinds() -> BTreeMap<String, KindInfo> {
    knobas_core::entity::OWNED_KINDS
        .iter()
        .map(|owned| {
            (
                owned.id.to_owned(),
                KindInfo {
                    id: owned.id.to_owned(),
                    label: owned.label.to_owned(),
                    plural: owned.plural.to_owned(),
                    monogram: owned.monogram.to_owned(),
                    full_sync_exhaustive: false,
                },
            )
        })
        .collect()
}

/// Everything a kind's chip needs, worked out from its id alone.
fn derive_kind_info(kind: &str) -> KindInfo {
    let label = derive_label(kind);
    KindInfo {
        id: kind.to_owned(),
        plural: derive_plural(kind, &label),
        monogram: derive_monogram(kind),
        label,
        // Display metadata for a kind **no adapter declared**, so there is no
        // read path to make a claim about. `false` is the only safe fallback:
        // the sweep's precondition is a claim, and inventing an affirmative
        // one here would license tombstoning rows nothing vouched for. Nothing
        // in the sync engine reads this value anyway -- it reads the
        // descriptor the adapter returned.
        full_sync_exhaustive: false,
    }
}

/// `"build_config"` -> `"Build config"`.
fn derive_label(kind: &str) -> String {
    let spaced: String = kind
        .chars()
        .map(|c| if c == '_' || c == '-' { ' ' } else { c })
        .collect();
    let mut chars = spaced.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

fn derive_plural(kind: &str, label: &str) -> String {
    IRREGULAR_PLURALS
        .iter()
        .find(|(id, _)| *id == kind)
        .map_or_else(|| format!("{label}s"), |(_, plural)| (*plural).to_owned())
}

/// Two characters, always.
///
/// The monogram is drawn into a fixed-width chip, so "two characters" is an
/// invariant and not a rough size. Two traps live in here:
///
/// * a `_`- or `-`-separated id reads far better as its initials
///   (`build_config` -> `BC`, not `BU`);
/// * uppercasing is **not length-preserving** in Unicode (`"ß"` uppercases to
///   `"SS"`), so taking two characters and *then* uppercasing can yield three.
///   Uppercase first, truncate second.
fn derive_monogram(kind: &str) -> String {
    let parts: Vec<&str> = kind.split(['_', '-']).filter(|p| !p.is_empty()).collect();
    let source: String = if parts.len() > 1 {
        parts.iter().filter_map(|p| p.chars().next()).collect()
    } else {
        kind.to_owned()
    };
    let mut monogram: String = source.to_uppercase().chars().take(2).collect();
    while monogram.chars().count() < 2 {
        monogram.push(MONOGRAM_PAD);
    }
    monogram
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_exact_id_beats_an_alias_that_would_widen_it() {
        let v = Vocabulary::fixture();
        // `jira` is both a configured instance and an alias for the adapter
        // kind two instances share. Naming the instance means the instance.
        assert_eq!(v.resolve_source("jira"), ["jira"]);
        assert_eq!(v.resolve_source("ji"), ["jira", "jira-eu"]);
    }

    #[test]
    fn an_adapter_kind_typed_out_resolves_like_its_alias() {
        let v = Vocabulary::fixture();
        assert_eq!(v.resolve_source("teamcity"), ["teamcity"]);
        assert_eq!(v.resolve_source("tc"), ["teamcity"]);
    }

    #[test]
    fn a_unique_prefix_resolves_and_an_ambiguous_one_does_not() {
        let v = Vocabulary::fixture();
        // "jira-e" is only `jira-eu`.
        assert_eq!(v.resolve_source("jira-e"), ["jira-eu"]);
        assert_eq!(v.resolve_source("team"), ["teamcity"]);
        // A display-name prefix works the same way, and this one shares no
        // prefix with any id -- the sources list is what the user reads, so
        // the name they see has to be typeable.
        assert_eq!(v.resolve_source("builds"), ["teamcity"]);
        assert_eq!(v.resolve_source("BUILDS"), ["teamcity"]);
        // "j" prefixes both Jiras and is not an alias: a guess would be worse
        // than nothing.
        assert!(v.resolve_source("j").is_empty());
        assert!(v.resolve_source("nope").is_empty());
        assert!(v.resolve_source("").is_empty());
    }

    #[test]
    fn an_alias_for_an_adapter_nobody_configured_resolves_to_nothing() {
        let v = Vocabulary::fixture();
        // `/cf` is a real alias with no Confluence behind it. Answering with
        // "no filter" would silently search everything.
        assert!(v.resolve_source("cf").is_empty());
        assert!(v.resolve_source("as").is_empty());
    }

    #[test]
    fn declared_metadata_wins_and_everything_else_is_derived() {
        let catalog = KindCatalog::from_descriptors([descriptor(&[KindInfo {
            id: "ticket".to_owned(),
            label: "Ticket".to_owned(),
            plural: "Tickets".to_owned(),
            monogram: "JI".to_owned(),
            full_sync_exhaustive: true,
        }])]);
        assert_eq!(catalog.info("ticket").monogram, "JI");
        assert_eq!(catalog.declared_kinds(), ["ticket"]);
        assert!(catalog.is_declared("ticket"));
        assert!(!catalog.is_declared("build_config"));

        // An adapter nobody compiled in still gets grouped, labelled and
        // chipped -- spec §3a: a new source needs zero changes in search.
        let derived = catalog.info("build_config");
        assert_eq!(
            (
                derived.label.as_str(),
                derived.plural.as_str(),
                derived.monogram.as_str()
            ),
            ("Build config", "Build configs", "BC")
        );
        assert_eq!(catalog.info("pr").plural, "Pull requests");
        assert_eq!(catalog.info("branch").plural, "Branches");
        assert_eq!(catalog.info("repo").plural, "Repositories");
    }

    /// **The catalog seam, in both directions** (issue #46, story 21).
    ///
    /// Forwards: a kind knobas owns is in the catalog, with knobas' own label,
    /// plural and monogram, and **no descriptor declares it**. That last clause
    /// is the point -- the alternative was a fake descriptor, which would put a
    /// source in the sources list that nobody configured and that syncs
    /// nothing.
    ///
    /// Backwards: everything the previous test asserts about descriptor-declared
    /// kinds still holds. A seam that made owned kinds work by making declared
    /// ones stop being declared would pass the forward half on its own.
    #[test]
    fn an_owned_kind_is_in_the_catalog_and_no_descriptor_declared_it() {
        let catalog = KindCatalog::from_descriptors([descriptor(&[KindInfo {
            id: "ticket".to_owned(),
            label: "Ticket".to_owned(),
            plural: "Tickets".to_owned(),
            monogram: "JI".to_owned(),
            full_sync_exhaustive: true,
        }])]);

        assert_eq!(catalog.owned_kinds(), ["note"]);
        for owned in knobas_core::entity::OWNED_KINDS {
            let info = catalog.info(owned.id);
            assert_eq!(
                (
                    info.label.as_str(),
                    info.plural.as_str(),
                    info.monogram.as_str()
                ),
                (owned.label, owned.plural, owned.monogram),
                "knobas declares its own kinds' display metadata"
            );
            // Emitted by no sync, so there is no full sync for it to be
            // exhaustive over. The sweep reads the *descriptor*, never this --
            // but an affirmative here would still be a claim nothing vouches
            // for, which is the same reasoning `derive_kind_info` records.
            assert!(!info.full_sync_exhaustive);
            assert!(catalog.is_known(owned.id), "the grammar can resolve it");
            assert!(
                !catalog.is_declared(owned.id),
                "and no descriptor pretended to be a source for it"
            );
            assert!(!catalog.declared_kinds().iter().any(|k| k == owned.id));
        }

        // Backwards: the descriptor half is untouched.
        assert_eq!(catalog.info("ticket").monogram, "JI");
        assert_eq!(catalog.declared_kinds(), ["ticket"]);
        assert!(catalog.is_declared("ticket") && catalog.is_known("ticket"));
        assert!(!catalog.is_declared("build_config") && !catalog.is_known("build_config"));
        // ...including while nothing has wired the registry through at all
        // (**E-Q2**), which is what the product ships with today.
        assert!(
            KindCatalog::default().is_empty(),
            "no descriptor has spoken"
        );
        assert!(
            KindCatalog::default().is_known("note"),
            "and yet notes exist"
        );
    }

    /// A descriptor cannot take a kind knobas owns away from it.
    ///
    /// Not defensiveness: `info` is what labels a result group and chips a row,
    /// so an adapter declaring `note` would rename the user's own notes in the
    /// launcher. The sink refuses to *mirror* such a kind
    /// (`knobas_sync::PgSink::check`); this is the display half of the same
    /// statement, and it is here because the catalog is the only place that
    /// merges the two lists.
    #[test]
    fn a_descriptor_cannot_claim_a_kind_knobas_owns() {
        let catalog = KindCatalog::from_descriptors([descriptor(&[KindInfo {
            id: "note".to_owned(),
            label: "Sticky".to_owned(),
            plural: "Stickies".to_owned(),
            monogram: "ST".to_owned(),
            full_sync_exhaustive: true,
        }])]);
        assert_eq!(catalog.info("note").plural, "Notes");
        assert!(!catalog.is_declared("note"));
        assert!(catalog.declared_kinds().is_empty());
    }

    #[test]
    fn the_first_declaration_of_a_kind_wins() {
        let catalog = KindCatalog::from_descriptors([
            descriptor(&[kind_info("ticket", "AA")]),
            descriptor(&[kind_info("ticket", "BB")]),
        ]);
        assert_eq!(catalog.info("ticket").monogram, "AA");
    }

    /// The monogram is drawn into a fixed-width chip, so "two characters" is an
    /// invariant and not a rough size.
    ///
    /// The trap is Unicode: uppercasing is not length-preserving, so taking two
    /// characters and *then* uppercasing can yield three -- `"ßx"` becomes
    /// `"SSX"`. Uppercasing first is what makes the invariant hold for every
    /// kind an adapter can name, and only a non-ASCII kind shows the
    /// difference.
    #[test]
    fn a_derived_monogram_is_always_two_characters() {
        for kind in [
            "ticket",
            "pr",
            "b",
            "",
            "ßx",
            "straße",
            "über",
            "日本語",
            "É",
            "build_config",
            "a_b_c",
        ] {
            let monogram = derive_monogram(kind);
            assert_eq!(
                monogram.chars().count(),
                2,
                "kind {kind:?} produced {monogram:?}"
            );
        }
    }

    #[test]
    fn a_short_kind_is_padded_and_a_compound_one_is_initialled() {
        assert_eq!(derive_monogram("pr"), "PR");
        assert_eq!(derive_monogram("b"), "B·");
        assert_eq!(derive_monogram(""), "··");
        assert_eq!(derive_monogram("build_config"), "BC");
        assert_eq!(derive_monogram("build-config"), "BC");
        assert_eq!(derive_monogram("a_b_c"), "AB");
    }

    fn kind_info(id: &str, monogram: &str) -> KindInfo {
        KindInfo {
            id: id.to_owned(),
            label: derive_label(id),
            plural: derive_plural(id, &derive_label(id)),
            monogram: monogram.to_owned(),
            full_sync_exhaustive: true,
        }
    }

    fn descriptor(kinds: &[KindInfo]) -> SourceDescriptor {
        SourceDescriptor {
            id: "x".to_owned(),
            adapter_kind: "x".to_owned(),
            name: "X".to_owned(),
            capabilities: Vec::new(),
            adapter_version: "0".to_owned(),
            auth_methods: Vec::new(),
            write_ops: Vec::new(),
            entity_kinds: kinds.to_vec(),
            config_schema: serde_json::Value::Object(serde_json::Map::new()),
        }
    }
}
