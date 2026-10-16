//! Link sets (#866, spec #808 *Link sets*, R-7.1, R-7.2, R-7.4, R-7.6,
//! R-7.7, R-7.11, ADR-0025): the knobas-owned rows of migration `0038` behind
//! six commands on `commands::entity` -- `link_sets`, `link_set`,
//! `save_link_set`, `delete_link_set`, `preview_link_set_rule` and
//! `hide_link_set_member`.
//!
//! A set is **read beside membership, never through it** (ADR-0025). Nothing
//! here writes a link, and `knobas_core::context` reads none of these tables;
//! what a job shows of a set is `knobas_core::reach`, which matches the rules
//! when it runs. Which tickets a rule matches is `knobas_core::link_set`'s SQL,
//! read the other way round here for the counts.
//!
//! **Members are fixed items in this ticket.** The migration has room for a
//! pattern (a stored query, #868); no command here writes or answers one.
//!
//! **"Yours"** (R-7.6's "39 (10 yours)") is a ticket whose declared assignee
//! is one of the usernames the sources were configured with: the identity
//! `assignee:@me` resolves to (`knobas_search::vocab`), read through the
//! declared `assignee` path (ADR-0007), so a ticket whose assignee the path
//! cannot read is not yours.

use sqlx::{PgPool, Row};

use crate::IpcError;

/// What a member can be (R-7.7's five panels), in panel order.
pub const MEMBER_KINDS: &[&str] = &["repo", "pipeline", "page", "asset", "monitor"];
/// What a rule can match on (R-7.1).
pub const RULE_KINDS: &[&str] = &["epic", "parent", "project", "manual"];

/// How long an id may grow before the slug stops, so an address stays short.
const MAX_ID_CHARS: usize = 48;

/// The entity kinds a `pipeline` member may not be: every other member kind,
/// and what is never a pipeline. A pipeline's kind is its source's own word
/// (`build_config` for TeamCity), so the rest is open.
const NOT_PIPELINES: &[&str] = &[
    "repo", "page", "asset", "monitor", "ticket", "pr", "branch", "commit", "build", "ctx", "note",
    "route",
];

/// One fixed member: an entity of one of [`MEMBER_KINDS`].
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LinkSetMember {
    pub kind: String,
    pub entity_id: String,
}

/// One rule (R-7.1): a ticket entity id for `epic`, `parent` and `manual`;
/// `<source id>:<project key>` for `project`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LinkSetRule {
    pub kind: String,
    pub value: String,
}

/// What `save_link_set` takes: the whole set, members and rules in order.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LinkSetDraft {
    /// `None` makes a new set with an id from its name; an id saves over that
    /// set.
    pub id: Option<String>,
    pub name: String,
    pub description: String,
    pub members: Vec<LinkSetMember>,
    pub rules: Vec<LinkSetRule>,
}

/// A member as the views draw it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct MemberRow {
    pub kind: String,
    pub entity_id: String,
    /// The item's title, or `None` when knobas has no entity of that id.
    pub title: Option<String>,
}

/// A rule as the views draw it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RuleRow {
    pub kind: String,
    pub value: String,
    /// The rule's ticket's title, or its project's name; `None` when knobas
    /// has none.
    pub title: Option<String>,
}

/// One set, as the Link sets view lists it (R-7.6).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LinkSetRow {
    /// A slug: `#/sets/<id>` opens the set.
    pub id: String,
    pub name: String,
    pub description: String,
    pub members: Vec<MemberRow>,
    pub rules: Vec<RuleRow>,
    /// How many live tickets its rules match now.
    pub tickets: i64,
    /// How many of those are yours.
    pub mine: i64,
}

/// A ticket a rule or a set applies to.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RuleTicket {
    pub entity_id: String,
    pub title: String,
    pub mine: bool,
}

/// One set, as its editor draws it (R-7.7): the row, and the tickets it shows
/// on now (*Shows on N tickets now*).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LinkSetDetail {
    #[serde(flatten)]
    pub set: LinkSetRow,
    pub shows_on: Vec<RuleTicket>,
}

fn is_slug(id: &str) -> bool {
    !id.is_empty()
        && !id.starts_with('-')
        && !id.ends_with('-')
        && !id.contains("--")
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// A name as an address: lowercase letters and digits joined by single
/// hyphens, `set` for a name with neither.
fn slug(name: &str) -> String {
    let mut out = String::new();
    for character in name.chars() {
        if character.is_ascii_alphanumeric() {
            out.push(character.to_ascii_lowercase());
        } else if !out.ends_with('-') {
            out.push('-');
        }
        if out.trim_end_matches('-').len() >= MAX_ID_CHARS {
            break;
        }
    }
    let slug = out.trim_matches('-').to_owned();
    if slug.is_empty() {
        "set".to_owned()
    } else {
        slug
    }
}

/// Whether `value` is `<namespace>:<rest>` with neither half blank.
fn qualified(value: &str) -> bool {
    value
        .split_once(':')
        .is_some_and(|(left, right)| !left.trim().is_empty() && !right.trim().is_empty())
}

/// Why a rule cannot be saved or previewed, or `None`.
fn rule_refusal(rule: &LinkSetRule) -> Option<String> {
    if !RULE_KINDS.contains(&rule.kind.as_str()) {
        return Some(format!(
            "rule kind `{}` is not one of {}",
            rule.kind,
            RULE_KINDS.join(", ")
        ));
    }
    if !qualified(&rule.value) {
        return Some(if rule.kind == "project" {
            format!("`{}` is not a project (`<source>:<key>`)", rule.value)
        } else {
            format!("`{}` is not a ticket id", rule.value)
        });
    }
    None
}

/// Why a draft cannot be saved, or `None`. Entities are checked against
/// `knobas.entity` in [`save`].
fn refusal(draft: &LinkSetDraft) -> Option<String> {
    if draft.name.trim().is_empty() {
        return Some("a link set needs a name".to_owned());
    }
    if let Some(id) = &draft.id
        && !is_slug(id)
    {
        return Some(format!("`{id}` is not a link set id"));
    }
    for (at, member) in draft.members.iter().enumerate() {
        if !MEMBER_KINDS.contains(&member.kind.as_str()) {
            return Some(format!(
                "member kind `{}` is not one of {}",
                member.kind,
                MEMBER_KINDS.join(", ")
            ));
        }
        if draft.members[..at]
            .iter()
            .any(|earlier| earlier.entity_id == member.entity_id)
        {
            return Some(format!("`{}` is in the set twice", member.entity_id));
        }
    }
    for (at, rule) in draft.rules.iter().enumerate() {
        if let Some(why) = rule_refusal(rule) {
            return Some(why);
        }
        if draft.rules[..at].contains(rule) {
            return Some(format!("the rule `{} {}` is there twice", rule.kind, rule.value));
        }
    }
    None
}

/// Whether an entity of `entity_kind` can be a member of kind `member_kind`.
fn fits(member_kind: &str, entity_kind: &str) -> bool {
    if member_kind == "pipeline" {
        !NOT_PIPELINES.contains(&entity_kind)
    } else {
        member_kind == entity_kind
    }
}

/// The live tickets each rule of `$2`/`$3`/`$4` (the group, kind and value
/// arrays) applies to, and whether each is yours; `$1` is the declarations.
/// One row per group and ticket, in key order.
const RULE_TICKETS: &str = concat!(
    "with recursive rules(n, rule, kind, value) as (
        select n, rule::int, kind, value
          from unnest($2::int[], $3::text[], $4::text[]) with ordinality as u(n, kind, value, rule)
    ),
    ",
    knobas_core::link_set_rule_tickets!(),
    "
    select n, entity_id, title, mine from (
        select distinct on (t.n, i.entity_id) t.n, i.entity_id, i.title,
               coalesce(lower(",
    knobas_core::declared_string!("$1", "assignee"),
    ") in (select lower(btrim(sc.config->>'username')) from knobas.source_config sc
                where nullif(btrim(sc.config->>'username'), '') is not null), false) as mine
          from rule_ticket t
          join sync.live_item i on i.entity_id = t.id and i.kind = 'ticket'
         order by t.n, i.entity_id
    ) x
    order by n, length(entity_id), entity_id"
);

/// The tickets each group of rules applies to, by group.
async fn tickets_of(
    pool: &PgPool,
    rules: &[(i32, &LinkSetRule)],
) -> Result<Vec<(i32, RuleTicket)>, IpcError> {
    if rules.is_empty() {
        return Ok(Vec::new());
    }
    let declarations =
        crate::sources::paths::declared_paths(pool, &crate::sources::Registry::builtin()).await?;
    let groups: Vec<i32> = rules.iter().map(|(n, _)| *n).collect();
    let kinds: Vec<&str> = rules.iter().map(|(_, rule)| rule.kind.as_str()).collect();
    let values: Vec<&str> = rules.iter().map(|(_, rule)| rule.value.as_str()).collect();
    let rows = sqlx::query(RULE_TICKETS)
        .bind(declarations.as_param())
        .bind(&groups)
        .bind(&kinds)
        .bind(&values)
        .fetch_all(pool)
        .await?;
    Ok(rows
        .iter()
        .map(|row| {
            (
                row.get("n"),
                RuleTicket {
                    entity_id: row.get("entity_id"),
                    title: row.get("title"),
                    mine: row.get("mine"),
                },
            )
        })
        .collect())
}

/// The sets `ids` names, or every set for `None`, each with its members, its
/// rules and how many tickets it shows on; and per set, those tickets.
async fn read(
    pool: &PgPool,
    ids: Option<&[String]>,
) -> Result<Vec<(LinkSetRow, Vec<RuleTicket>)>, IpcError> {
    // Make sure the stored parent resolutions are the binary's (#627), as
    // every search does before it reads them.
    crate::commands::search::searcher(pool).await?;
    let sets = sqlx::query(
        "select id, name, description from knobas.link_set
          where $1::text[] is null or id = any($1)
          order by lower(name), id",
    )
    .bind(ids)
    .fetch_all(pool)
    .await?;
    let ids: Vec<String> = sets.iter().map(|row| row.get("id")).collect();
    let members = sqlx::query(
        "select m.set_id, m.kind, m.entity_id, e.title
           from knobas.link_set_member m
           left join knobas.entity e on e.id = m.entity_id
          where m.set_id = any($1) and m.entity_id is not null
          order by m.set_id, m.position",
    )
    .bind(&ids)
    .fetch_all(pool)
    .await?;
    let rules = sqlx::query(
        "select r.set_id, r.kind, r.value,
                coalesce(e.title, (select p.name from sync.item_project p
                                    where r.kind = 'project' and p.source_id || ':' || p.key = r.value
                                      and p.name is not null limit 1)) as title
           from knobas.link_set_rule r
           left join knobas.entity e on r.kind <> 'project' and e.id = r.value
          where r.set_id = any($1)
          order by r.set_id, r.position",
    )
    .bind(&ids)
    .fetch_all(pool)
    .await?;
    let mut out: Vec<(LinkSetRow, Vec<RuleTicket>)> = sets
        .iter()
        .map(|row| {
            (
                LinkSetRow {
                    id: row.get("id"),
                    name: row.get("name"),
                    description: row.get("description"),
                    members: Vec::new(),
                    rules: Vec::new(),
                    tickets: 0,
                    mine: 0,
                },
                Vec::new(),
            )
        })
        .collect();
    let at = |id: &str| ids.iter().position(|known| known == id);
    for row in &members {
        if let Some(index) = at(row.get("set_id")) {
            out[index].0.members.push(MemberRow {
                kind: row.get("kind"),
                entity_id: row.get("entity_id"),
                title: row.get("title"),
            });
        }
    }
    let mut stored: Vec<(i32, LinkSetRule)> = Vec::new();
    for row in &rules {
        if let Some(index) = at(row.get("set_id")) {
            let rule = LinkSetRule {
                kind: row.get("kind"),
                value: row.get("value"),
            };
            out[index].0.rules.push(RuleRow {
                kind: rule.kind.clone(),
                value: rule.value.clone(),
                title: row.get("title"),
            });
            stored.push((i32::try_from(index).map_err(IpcError::internal)?, rule));
        }
    }
    let grouped: Vec<(i32, &LinkSetRule)> = stored.iter().map(|(n, rule)| (*n, rule)).collect();
    for (n, ticket) in tickets_of(pool, &grouped).await? {
        let entry = &mut out[usize::try_from(n).map_err(IpcError::internal)?];
        entry.0.tickets += 1;
        entry.0.mine += i64::from(ticket.mine);
        entry.1.push(ticket);
    }
    Ok(out)
}

/// Every link set, by name, with its contents, rules and ticket counts
/// (R-7.6).
///
/// # Errors
///
/// `internal` if a read fails.
pub async fn list(pool: &PgPool) -> Result<Vec<LinkSetRow>, IpcError> {
    Ok(read(pool, None).await?.into_iter().map(|(row, _)| row).collect())
}

/// One link set with the tickets it shows on now (R-7.7).
///
/// # Errors
///
/// `not_found` if no set has that id; `internal` if a read fails.
pub async fn get(pool: &PgPool, id: &str) -> Result<LinkSetDetail, IpcError> {
    read(pool, Some(&[id.to_owned()]))
        .await?
        .into_iter()
        .next()
        .map(|(set, shows_on)| LinkSetDetail { set, shows_on })
        .ok_or_else(|| IpcError::not_found(format!("no link set is called {id}")))
}

/// The live tickets one rule applies to now, yours marked (R-7.7's live
/// counts), whether or not any set holds the rule.
///
/// # Errors
///
/// `invalid` for a rule [`rule_refusal`] names; `internal` if a read fails.
pub async fn preview(pool: &PgPool, rule: &LinkSetRule) -> Result<Vec<RuleTicket>, IpcError> {
    if let Some(why) = rule_refusal(rule) {
        return Err(IpcError::invalid(why));
    }
    crate::commands::search::searcher(pool).await?;
    Ok(tickets_of(pool, &[(0, rule)])
        .await?
        .into_iter()
        .map(|(_, ticket)| ticket)
        .collect())
}

/// Save a whole set -- a new one under an id from its name, or over the set
/// the draft names, its members and rules replaced by the draft's in order --
/// and answer it as stored. Every job it applies to reads the change on its
/// next read; nothing else is written.
///
/// # Errors
///
/// `invalid` for a draft [`refusal`] names, or a member knobas has no entity
/// for or whose entity is not of the member's kind; `internal` if the write
/// fails.
pub async fn save(pool: &PgPool, draft: LinkSetDraft) -> Result<LinkSetDetail, IpcError> {
    if let Some(why) = refusal(&draft) {
        return Err(IpcError::invalid(why));
    }
    let ids: Vec<&str> = draft.members.iter().map(|m| m.entity_id.as_str()).collect();
    let known: Vec<(String, String)> =
        sqlx::query_as("select id, kind from knobas.entity where id = any($1)")
            .bind(&ids)
            .fetch_all(pool)
            .await?;
    for member in &draft.members {
        match known.iter().find(|(id, _)| *id == member.entity_id) {
            None => {
                return Err(IpcError::invalid(format!(
                    "knobas has no item `{}`",
                    member.entity_id
                )));
            }
            Some((_, kind)) if !fits(&member.kind, kind) => {
                return Err(IpcError::invalid(format!(
                    "`{}` is a {kind}, not a {}",
                    member.entity_id, member.kind
                )));
            }
            Some(_) => {}
        }
    }
    let mut tx = pool.begin().await?;
    // One writer at a time picks ids, so two saves cannot take the same slug.
    sqlx::query("lock table knobas.link_set in share row exclusive mode")
        .execute(&mut *tx)
        .await?;
    let id = match &draft.id {
        Some(id) => id.clone(),
        None => {
            let base = slug(&draft.name);
            let taken: Vec<String> =
                sqlx::query_scalar("select id from knobas.link_set where id = $1 or id like $2")
                    .bind(&base)
                    .bind(format!("{base}-%"))
                    .fetch_all(&mut *tx)
                    .await?;
            let mut candidate = base.clone();
            let mut suffix = 1_u32;
            while taken.contains(&candidate) {
                suffix += 1;
                candidate = format!("{base}-{suffix}");
            }
            candidate
        }
    };
    sqlx::query(
        "insert into knobas.link_set as s (id, name, description) values ($1, $2, $3)
         on conflict (id) do update set name = excluded.name,
           description = excluded.description, updated_at = now()",
    )
    .bind(&id)
    .bind(draft.name.trim())
    .bind(draft.description.trim())
    .execute(&mut *tx)
    .await?;
    sqlx::query("delete from knobas.link_set_member where set_id = $1")
        .bind(&id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("delete from knobas.link_set_rule where set_id = $1")
        .bind(&id)
        .execute(&mut *tx)
        .await?;
    let kinds: Vec<&str> = draft.members.iter().map(|m| m.kind.as_str()).collect();
    sqlx::query(
        "insert into knobas.link_set_member (set_id, kind, entity_id, position)
         select $1, k, e, (p - 1)::int from unnest($2::text[], $3::text[]) with ordinality as u(k, e, p)",
    )
    .bind(&id)
    .bind(&kinds)
    .bind(&ids)
    .execute(&mut *tx)
    .await?;
    let rule_kinds: Vec<&str> = draft.rules.iter().map(|r| r.kind.as_str()).collect();
    let values: Vec<&str> = draft.rules.iter().map(|r| r.value.as_str()).collect();
    sqlx::query(
        "insert into knobas.link_set_rule (set_id, kind, value, position)
         select $1, k, v, (p - 1)::int from unnest($2::text[], $3::text[]) with ordinality as u(k, v, p)",
    )
    .bind(&id)
    .bind(&rule_kinds)
    .bind(&values)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    get(pool, &id).await
}

/// Forget a set: its members and rules go with it, and every job it applied
/// to stops showing it. Links drawn on tickets by hand are not touched.
///
/// # Errors
///
/// `not_found` if no set has that id; `internal` if the delete fails.
pub async fn delete(pool: &PgPool, id: &str) -> Result<(), IpcError> {
    let gone = sqlx::query("delete from knobas.link_set where id = $1")
        .bind(id)
        .execute(pool)
        .await?
        .rows_affected();
    if gone == 0 {
        return Err(IpcError::not_found(format!("no link set is called {id}")));
    }
    Ok(())
}

/// *Hide here* and *Show again* (R-7.5): hide one link-set member on one
/// ticket, or show it again, and answer whether it is now hidden. The set is
/// not touched. Hiding twice, or showing what is not hidden, changes nothing.
///
/// # Errors
///
/// `invalid` for an id that is not one; `internal` if the write fails.
pub async fn hide(
    pool: &PgPool,
    ticket_id: &str,
    member_id: &str,
    hidden: bool,
) -> Result<bool, IpcError> {
    for id in [ticket_id, member_id] {
        if !qualified(id) {
            return Err(IpcError::invalid(format!("`{id}` is not an entity id")));
        }
    }
    let statement = if hidden {
        "insert into knobas.link_set_hidden (ticket_id, member_id) values ($1, $2)
         on conflict do nothing"
    } else {
        "delete from knobas.link_set_hidden where ticket_id = $1 and member_id = $2"
    };
    sqlx::query(statement)
        .bind(ticket_id)
        .bind(member_id)
        .execute(pool)
        .await?;
    Ok(hidden)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft() -> LinkSetDraft {
        LinkSetDraft {
            id: None,
            name: "Payout stack".to_owned(),
            description: String::new(),
            members: vec![LinkSetMember {
                kind: "repo".to_owned(),
                entity_id: "gitea:tidewater/payout-service".to_owned(),
            }],
            rules: vec![LinkSetRule {
                kind: "epic".to_owned(),
                value: "jira:PAY-200".to_owned(),
            }],
        }
    }

    #[test]
    fn a_slug_is_an_address_whatever_the_name_was() {
        for (name, expected) in [
            ("Payout stack", "payout-stack"),
            ("  On-call   kit ", "on-call-kit"),
            ("...", "set"),
        ] {
            let made = slug(name);
            assert_eq!(made, expected, "{name:?}");
            assert!(is_slug(&made), "{made:?}");
        }
        assert!(slug(&"a".repeat(200)).len() <= MAX_ID_CHARS);
    }

    #[test]
    fn a_draft_is_refused_for_what_cannot_be_a_set() {
        assert_eq!(refusal(&draft()), None);
        type Change = fn(&mut LinkSetDraft);
        let cases: [(Change, &str); 6] = [
            (|d| d.name = "  ".to_owned(), "a link set needs a name"),
            (|d| d.id = Some("Not A Slug".to_owned()), "is not a link set id"),
            (|d| d.members[0].kind = "build".to_owned(), "member kind `build`"),
            (|d| d.members.push(d.members[0].clone()), "is in the set twice"),
            (|d| d.rules[0].kind = "label".to_owned(), "rule kind `label`"),
            (|d| d.rules[0].value = "PAY-200".to_owned(), "is not a ticket id"),
        ];
        for (change, why) in cases {
            let mut refused = draft();
            change(&mut refused);
            let said = refusal(&refused).unwrap_or_default();
            assert!(said.contains(why), "{said:?} should say {why:?}");
        }
    }

    #[test]
    fn a_member_fits_its_kind() {
        assert!(fits("repo", "repo"));
        assert!(!fits("repo", "page"));
        assert!(fits("pipeline", "build_config"));
        assert!(!fits("pipeline", "build"));
        assert!(!fits("pipeline", "ticket"));
    }
}
