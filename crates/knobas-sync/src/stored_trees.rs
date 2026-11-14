//! Every branch's file list, kept as git keeps it (the stored-trees pass of
//! PR #2479, 2026-11-14; migration `0085`, ADR-0041 and ADR-0042 as amended
//! that day): what lets *Files* and *Go to file* draw a branch's tree with no
//! request to the forge.
//!
//! **Trees, not lists.** Each directory level is stored once by its tree sha
//! (`sync.git_tree`), and each branch points at its root (`sync.branch_tree`).
//! A branch made from another, or pushed with one file changed, adds only the
//! levels on the way to what changed.
//!
//! **After a run, behind a person.** The pass runs after a run of its source
//! ended, on the run's task, so in the background lane: every request yields
//! to one a person waits for on the same server (the priority lane,
//! 2026-11-13), and all of them pass the per-server quota.
//!
//! **Only what moved.** A branch whose head the mirror holds at the commit
//! its row was read at costs nothing and writes nothing. One that moved (a
//! push, a force-push, a branch new to the mirror) is read again through
//! [`Source::read_tree`], which fetches only the levels the store does not
//! hold.
//!
//! **In order of use, a batch a run.** Default branches first, then branches
//! an open pull request is opened from and the person's own (their last
//! commit is the source's configured username), then the rest, newest first.
//! At most [`REQUESTS_PER_RUN`] requests inside [`BUDGET`] a run, so a
//! repository with thousands of branches fills over the runs after it.
//!
//! **Bounded.** A tree of more than [`TREE_CAP`] entries is not stored (its
//! row says so, with no root). A repository whose branches added
//! [`REPOSITORY_CAP`] entries stores no further branch; those stay read on
//! demand, as before.
//!
//! **Cleaned.** A withdrawn branch or repository drops its row in the sync's
//! own transaction (`0085`'s trigger), and the pass then deletes the levels
//! no branch reaches.

use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, OnceLock, PoisonError};
use std::time::Duration;

use knobas_source::Source;
pub use knobas_source::tree::TREE_CAP;
use knobas_source::tree::{HeldTrees, MODE_TREE, TreeLevel, TreeRead};
use sqlx::PgPool;

/// The most requests one run's pass sends: at Gitea's ten a second, half a
/// minute of the quota, which the priority lane gives up to a person's reads
/// whenever they come.
pub const REQUESTS_PER_RUN: u32 = 300;

/// How long one run's pass spends reading trees, at most.
pub const BUDGET: Duration = Duration::from_secs(60);

/// The most entries the branches of one repository may add to the store.
/// Dedup keeps most estates far under it; past it, the repository's
/// remaining branches stay read on demand.
pub const REPOSITORY_CAP: u64 = 1_000_000;

/// What one pass did.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Filled {
    /// Branches read and stored (a row written or moved).
    pub branches: usize,
    /// Branches past [`TREE_CAP`], their row stored with no root.
    pub too_large: usize,
    /// Levels newly stored.
    pub levels: usize,
    /// Requests sent.
    pub requests: u32,
    /// Branches that moved and were left for a later run (the budget).
    pub left: usize,
    /// Levels deleted because no branch reaches them.
    pub removed: u64,
}

/// A branch whose stored row stands at another commit than the mirror's.
#[derive(Debug, Clone, sqlx::FromRow)]
struct Owed {
    entity_id: String,
    repository_id: String,
    branch: String,
    commit: String,
    stored_root: Option<Vec<u8>>,
}

/// The tree levels of one source the store holds, by sha, with each tree's
/// entries in all; and the repository the read is for, with what its trees
/// held last.
struct Held<'a> {
    totals: &'a HashMap<String, u64>,
    estimate: Option<u64>,
}

impl HeldTrees for Held<'_> {
    fn held(&self, sha: &str) -> Option<u64> {
        self.totals.get(sha).copied()
    }

    fn estimate(&self) -> Option<u64> {
        self.estimate
    }
}

/// One pass at a time a source: two passes of one source (a run that ended
/// while the last one's pass still read) would each trust levels the other's
/// clean-up may remove.
fn passing() -> &'static Mutex<HashSet<String>> {
    static PASSING: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    PASSING.get_or_init(Default::default)
}

struct Passing(String);

impl Drop for Passing {
    fn drop(&mut self) {
        passing()
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&self.0);
    }
}

/// Read and store the trees of `source_id`'s branches that moved, in order of
/// use, inside the run's budget, then remove the levels no branch reaches.
///
/// # Errors
///
/// A database failure. A tree the source fails to answer is left for the
/// next run, not an error.
pub async fn refresh(
    pool: &PgPool,
    source_id: &str,
    source: &dyn Source,
) -> Result<Filled, sqlx::Error> {
    refresh_within(pool, source_id, source, REQUESTS_PER_RUN, BUDGET).await
}

/// [`refresh`] with its budget given: at most `requests` requests inside
/// `budget`.
///
/// # Errors
///
/// As [`refresh`].
pub async fn refresh_within(
    pool: &PgPool,
    source_id: &str,
    source: &dyn Source,
    requests: u32,
    budget: Duration,
) -> Result<Filled, sqlx::Error> {
    let mut filled = Filled::default();
    let Some(shape) = source.tree_shape() else {
        return Ok(filled);
    };
    {
        let mut busy = passing().lock().unwrap_or_else(PoisonError::into_inner);
        if !busy.insert(source_id.to_owned()) {
            return Ok(filled);
        }
    }
    let _passing = Passing(source_id.to_owned());
    let mut sweep = sqlx::query("delete from sync.tree_gc where source_id = $1")
        .bind(source_id)
        .execute(pool)
        .await?
        .rows_affected()
        > 0;
    let owed = owed(pool, source_id, &shape).await?;
    if !owed.is_empty() {
        let mut totals = held(pool, source_id).await?;
        let mut repositories = repositories(pool, source_id).await?;
        let deadline = tokio::time::Instant::now() + budget;
        let mut owed = owed.into_iter();
        while let Some(branch) = owed.next() {
            if filled.requests >= requests || tokio::time::Instant::now() >= deadline {
                filled.left = 1 + owed.len();
                break;
            }
            let (added, estimate) = repositories
                .get(&branch.repository_id)
                .copied()
                .unwrap_or((0, None));
            if added >= REPOSITORY_CAP {
                continue;
            }
            let read = {
                let held = Held {
                    totals: &totals,
                    estimate,
                };
                tokio::time::timeout_at(
                    deadline,
                    source.read_tree(&branch.repository_id, &branch.commit, &held, TREE_CAP),
                )
                .await
            };
            let read = match read {
                Err(_) => {
                    filled.left = 1 + owed.len();
                    break;
                }
                // This adapter reads no tree.
                Ok(Ok(None)) => break,
                Ok(Err(error)) => {
                    tracing::debug!(source_id, branch = branch.entity_id, %error, "a branch's tree was not read");
                    continue;
                }
                Ok(Ok(Some(read))) => read,
            };
            match read {
                TreeRead::TooLarge { entries, requests } => {
                    filled.requests += requests;
                    store(pool, source_id, &branch, None, &[], &[], entries, 0).await?;
                    sweep |= branch.stored_root.is_some();
                    filled.too_large += 1;
                }
                TreeRead::Read {
                    root,
                    levels,
                    requests,
                } => {
                    filled.requests += requests;
                    let Some((new_totals, entries)) = totals_of(&root, &levels, &totals) else {
                        tracing::debug!(
                            source_id,
                            branch = branch.entity_id,
                            "a branch's tree read short of its levels"
                        );
                        continue;
                    };
                    if entries > TREE_CAP {
                        store(pool, source_id, &branch, None, &[], &[], entries, 0).await?;
                        sweep |= branch.stored_root.is_some();
                        filled.too_large += 1;
                        continue;
                    }
                    let fresh: Vec<&TreeLevel> = levels
                        .iter()
                        .filter(|level| !totals.contains_key(&level.sha))
                        .collect();
                    let added: u64 = fresh.iter().map(|level| level.entries.len() as u64).sum();
                    let level_totals: Vec<i64> = fresh
                        .iter()
                        .map(|level| i64::try_from(new_totals[&level.sha]).unwrap_or(i64::MAX))
                        .collect();
                    let root_bytes = hex(&root);
                    store(
                        pool,
                        source_id,
                        &branch,
                        Some(&root_bytes),
                        &fresh,
                        &level_totals,
                        entries,
                        added,
                    )
                    .await?;
                    sweep |= branch
                        .stored_root
                        .as_ref()
                        .is_some_and(|old| *old != root_bytes);
                    filled.levels += fresh.len();
                    filled.branches += 1;
                    for (sha, total) in new_totals {
                        totals.insert(sha, total);
                    }
                    let repository = repositories
                        .entry(branch.repository_id.clone())
                        .or_insert((0, None));
                    repository.0 += added;
                    repository.1 = Some(repository.1.unwrap_or(0).max(entries));
                }
            }
        }
    }
    if sweep {
        filled.removed = remove_unreached(pool, source_id).await?;
    }
    if filled != Filled::default() {
        tracing::debug!(source_id, ?filled, "stored trees");
    }
    Ok(filled)
}

/// Each new level's entries in all, bottom up (its own and every level's
/// below, held or new), and the root's; `None` where a directory names a
/// level neither read nor held.
fn totals_of(
    root: &str,
    levels: &[TreeLevel],
    held: &HashMap<String, u64>,
) -> Option<(HashMap<String, u64>, u64)> {
    let by_sha: HashMap<&str, &TreeLevel> = levels.iter().map(|l| (l.sha.as_str(), l)).collect();
    let mut done: HashMap<String, u64> = HashMap::new();
    fn total(
        sha: &str,
        by_sha: &HashMap<&str, &TreeLevel>,
        held: &HashMap<String, u64>,
        done: &mut HashMap<String, u64>,
        depth: usize,
    ) -> Option<u64> {
        if let Some(t) = done.get(sha).or_else(|| held.get(sha)) {
            return Some(*t);
        }
        // A tree cannot hold itself; a loop is a broken answer.
        if depth > 4096 {
            return None;
        }
        let level = by_sha.get(sha)?;
        let mut sum = level.entries.len() as u64;
        for item in level.entries.iter().filter(|i| i.mode == MODE_TREE) {
            sum += total(&item.sha, by_sha, held, done, depth + 1)?;
        }
        done.insert(sha.to_owned(), sum);
        Some(sum)
    }
    let entries = total(root, &by_sha, held, &mut done, 0)?;
    // Levels read but held already are not new.
    done.retain(|sha, _| !held.contains_key(sha));
    Some((done, entries))
}

/// The branches of `source_id` whose stored row is missing or at another
/// commit than the mirror's, live ones of live repositories, in order of use.
async fn owed(
    pool: &PgPool,
    source_id: &str,
    shape: &knobas_source::tree::TreeShape,
) -> Result<Vec<Owed>, sqlx::Error> {
    let path = |p: &[&str]| p.iter().map(|s| (*s).to_owned()).collect::<Vec<String>>();
    let (pull_kind, head, state, open) = match &shape.pull {
        Some(pull) => (pull.kind, path(pull.head), path(pull.state), pull.open),
        None => ("", Vec::new(), Vec::new(), ""),
    };
    sqlx::query_as::<_, Owed>(OWED)
        .bind(source_id)
        .bind(shape.repository)
        .bind(path(shape.default_branch))
        .bind(shape.branch)
        .bind(path(shape.branch_name))
        .bind(path(shape.branch_commit))
        .bind(pull_kind)
        .bind(head)
        .bind(state)
        .bind(open)
        .fetch_all(pool)
        .await
        .map(|rows| {
            // A branch matched by two names of its repository is one branch.
            let mut seen = HashSet::new();
            rows.into_iter()
                .filter(|row| seen.insert(row.entity_id.clone()))
                .collect()
        })
}

/// The plan: every live branch of the source under a live repository, with
/// the commit the mirror holds for it, where the stored row is missing or at
/// another commit; default branches first, then those an open pull request
/// is opened from or whose last commit is the source's configured username,
/// then the rest, newest first.
const OWED: &str = r"
with src as (
  select kind, nullif(btrim(config->>'username'), '') as me
    from knobas.source_config where id = $1
),
repos as (
  select i.entity_id, lower(i.title) as name, i.payload #>> $3::text[] as default_branch
    from sync.item i
    join knobas.entity e on e.id = i.entity_id and e.deleted_at is null
   where i.source_id = $1 and i.kind = $2
),
branches as (
  select i.entity_id, i.payload #>> $5::text[] as name, i.payload #>> $6::text[] as commit,
         i.item_updated_at, i.author, lower(ip.name) as parent
    from sync.item i
    join knobas.entity e on e.id = i.entity_id and e.deleted_at is null
    join sync.item_parent ip on ip.entity_id = i.entity_id and ip.kind = $2
                            and ip.adapter_kind = (select kind from src)
   where i.source_id = $1 and i.kind = $4
),
heads as materialized (
  select distinct lower(ip.name) as parent, i.payload #>> $8::text[] as head
    from sync.item i
    join knobas.entity e on e.id = i.entity_id and e.deleted_at is null
    join sync.item_parent ip on ip.entity_id = i.entity_id and ip.kind = $2
                            and ip.adapter_kind = (select kind from src)
   where i.source_id = $1 and i.kind = $7 and i.payload #>> $9::text[] = $10
)
select b.entity_id, r.entity_id as repository_id, b.name as branch, b.commit,
       bt.root as stored_root
  from branches b
  join repos r on r.name = b.parent
  left join sync.branch_tree bt on bt.entity_id = b.entity_id
  left join heads h on h.parent = b.parent and h.head = b.name
 where b.name is not null
   and b.commit ~ '^([0-9a-f]{40}|[0-9a-f]{64})$'
   and bt.commit_sha is distinct from b.commit
 order by case when b.name = r.default_branch then 0
               when b.author = (select me from src) or h.head is not null then 1
               else 2 end,
          b.item_updated_at desc nulls last,
          b.entity_id
";

/// Every level the store holds for the source, by sha, with its entries in
/// all.
async fn held(pool: &PgPool, source_id: &str) -> Result<HashMap<String, u64>, sqlx::Error> {
    let rows: Vec<(String, i64)> =
        sqlx::query_as("select encode(sha, 'hex'), total from sync.git_tree where source_id = $1")
            .bind(source_id)
            .fetch_all(pool)
            .await?;
    Ok(rows
        .into_iter()
        .map(|(sha, total)| (sha, u64::try_from(total).unwrap_or(0)))
        .collect())
}

/// Each repository's entries its branches added, and the most entries one of
/// its trees holds.
async fn repositories(
    pool: &PgPool,
    source_id: &str,
) -> Result<HashMap<String, (u64, Option<u64>)>, sqlx::Error> {
    let rows: Vec<(String, i64, Option<i64>)> = sqlx::query_as(
        "select repository_id, sum(added)::bigint, max(entries) filter (where root is not null)
           from sync.branch_tree where source_id = $1 group by repository_id",
    )
    .bind(source_id)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(repository, added, most)| {
            (
                repository,
                (
                    u64::try_from(added).unwrap_or(0),
                    most.and_then(|m| u64::try_from(m).ok()),
                ),
            )
        })
        .collect())
}

/// Store `branch`'s tree: the new levels, and its row at its commit, in one
/// transaction. A row whose branch the mirror dropped meanwhile is not
/// written.
#[allow(clippy::too_many_arguments)]
async fn store(
    pool: &PgPool,
    source_id: &str,
    branch: &Owed,
    root: Option<&[u8]>,
    levels: &[&TreeLevel],
    totals: &[i64],
    entries: u64,
    added: u64,
) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    if !levels.is_empty() {
        let mut level_shas = Vec::with_capacity(levels.len());
        let mut at = Vec::new();
        let mut names = Vec::new();
        let mut modes = Vec::new();
        let mut sizes = Vec::new();
        let mut shas = Vec::new();
        for (n, level) in levels.iter().enumerate() {
            level_shas.push(hex(&level.sha));
            for item in &level.entries {
                at.push(i32::try_from(n + 1).unwrap_or(i32::MAX));
                names.push(item.name.as_str());
                modes.push(i32::try_from(item.mode).unwrap_or(0));
                sizes.push(i64::try_from(item.size).unwrap_or(i64::MAX));
                shas.push(hex(&item.sha));
            }
        }
        sqlx::query(INSERT_LEVELS)
            .bind(source_id)
            .bind(&level_shas)
            .bind(totals)
            .bind(&at)
            .bind(&names)
            .bind(&modes)
            .bind(&sizes)
            .bind(&shas)
            .execute(&mut *tx)
            .await?;
    }
    sqlx::query(
        "insert into sync.branch_tree
                (entity_id, source_id, repository_id, branch, commit_sha, root, entries, added)
         select $1, $2, $3, $4, $5, $6, $7, $8
          where exists (select 1 from knobas.entity e
                         where e.id = $1 and e.deleted_at is null)
         on conflict (entity_id) do update set
           repository_id = excluded.repository_id, branch = excluded.branch,
           commit_sha = excluded.commit_sha, root = excluded.root,
           entries = excluded.entries, added = excluded.added, read_at = now()",
    )
    .bind(&branch.entity_id)
    .bind(source_id)
    .bind(&branch.repository_id)
    .bind(&branch.branch)
    .bind(&branch.commit)
    .bind(root)
    .bind(i64::try_from(entries).unwrap_or(i64::MAX))
    .bind(i64::try_from(added).unwrap_or(i64::MAX))
    .execute(&mut *tx)
    .await?;
    tx.commit().await
}

/// The levels, each its entries side by side, grouped from one flat list of
/// entries by the level they are in (`$4`, from 1).
const INSERT_LEVELS: &str = r"
with entry as (
  select lvl,
         array_agg(name order by ord) as names,
         array_agg(mode order by ord) as modes,
         array_agg(size order by ord) as sizes,
         array_agg(sha order by ord) as shas
    from unnest($4::int[], $5::text[], $6::int[], $7::bigint[], $8::bytea[])
         with ordinality as x(lvl, name, mode, size, sha, ord)
   group by lvl
)
insert into sync.git_tree (source_id, sha, names, modes, sizes, shas, total)
select $1, l.sha,
       coalesce(e.names, '{}'), coalesce(e.modes, '{}'), coalesce(e.sizes, '{}'),
       coalesce(e.shas, '{}'), l.total
  from unnest($2::bytea[], $3::bigint[]) with ordinality as l(sha, total, n)
  left join entry e on e.lvl = l.n
on conflict do nothing
";

/// Delete the levels of `source_id` no branch's root reaches.
async fn remove_unreached(pool: &PgPool, source_id: &str) -> Result<u64, sqlx::Error> {
    Ok(sqlx::query(
        "with recursive reached(sha) as (
           select root from sync.branch_tree where source_id = $1 and root is not null
           union
           select u.sha
             from reached r
             join sync.git_tree t on t.source_id = $1 and t.sha = r.sha
             cross join lateral unnest(t.shas, t.modes) as u(sha, mode)
            where u.mode = 16384
         )
         delete from sync.git_tree t
          where t.source_id = $1
            and not exists (select 1 from reached r where r.sha = t.sha)",
    )
    .bind(source_id)
    .execute(pool)
    .await?
    .rows_affected())
}

/// A hex sha as bytes; what is not hex is kept as its text's bytes, which no
/// stored sha equals.
fn hex(sha: &str) -> Vec<u8> {
    let bytes = sha.as_bytes();
    if !bytes.len().is_multiple_of(2) || !bytes.iter().all(u8::is_ascii_hexdigit) {
        return bytes.to_vec();
    }
    bytes
        .chunks(2)
        .map(|pair| {
            let digit = |b: u8| (b as char).to_digit(16).unwrap_or(0) as u8;
            digit(pair[0]) << 4 | digit(pair[1])
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use knobas_source::tree::TreeItem;

    fn level(sha: &str, entries: &[(&str, u32, &str)]) -> TreeLevel {
        TreeLevel {
            sha: sha.to_owned(),
            entries: entries
                .iter()
                .map(|(name, mode, sha)| TreeItem {
                    name: (*name).to_owned(),
                    mode: *mode,
                    size: 0,
                    sha: (*sha).to_owned(),
                })
                .collect(),
        }
    }

    #[test]
    fn a_trees_entries_count_every_level_below_held_or_new() {
        let held = HashMap::from([("aa".to_owned(), 10)]);
        let levels = vec![
            level(
                "r0",
                &[
                    ("a", MODE_TREE, "aa"),
                    ("b", MODE_TREE, "bb"),
                    ("f", 0o100_644, "f1"),
                ],
            ),
            level("bb", &[("c", MODE_TREE, "aa"), ("g", 0o100_644, "g1")]),
        ];
        let (new, all) = totals_of("r0", &levels, &held).unwrap();
        assert_eq!(new["bb"], 2 + 10);
        assert_eq!(all, 3 + 10 + 12);
        assert!(!new.contains_key("aa"), "a held level is not new");
        assert!(totals_of("r0", &levels[..1], &held).is_none(), "bb missing");
    }

    #[test]
    fn a_sha_is_stored_as_its_bytes() {
        assert_eq!(hex("00ff10"), vec![0, 255, 16]);
        assert_eq!(hex("xyz"), b"xyz".to_vec());
    }
}
