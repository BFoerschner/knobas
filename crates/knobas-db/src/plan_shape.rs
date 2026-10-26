//! Plan shapes a statement must not have, whatever the planner's statistics
//! say: the checker #770 wrote for the suggestion pass (`knobas-core`'s
//! `tests/it/suggest_plans.rs`), shared since #778 with the reads the shell
//! waits on (`knobas-app`'s `tests/it/read_plans.rs`).
//!
//! A wall clock in the gate would measure the machine, so a plan test asserts
//! the shapes the slow plans had instead, on `explain (format json)` output:
//!
//! * nothing executed once per row -- the inner side of a nested loop, or a
//!   correlated subplan that is not hashed -- scans a table or a CTE. An index
//!   probe per row is what a lookup should cost; a scan per row is the
//!   quadratic plan. Tables and CTEs small by construction are the caller's
//!   `small` list;
//! * no index scan on the inner side of a nested loop is linked to the outer
//!   row only by a range (#771);
//! * no nested loop crosses its outer side with a relation its inner side
//!   does not parameterize by the outer row;
//! * no anti-join is hashed on part of its key: on `relation` without both of
//!   the pair's ids, or with any of the key left to a join filter (#771).

use sqlx::PgPool;

/// Autovacuum off on every table of the `knobas` and `sync` schemas, so
/// nothing analyzes them behind a test's back: the method #767 and #768 used.
///
/// # Panics
///
/// If the catalog read or an `alter table` fails.
pub async fn never_analyzed(pool: &PgPool) {
    let tables: Vec<String> = sqlx::query_scalar(
        "select format('%I.%I', n.nspname, c.relname) from pg_class c
           join pg_namespace n on n.oid = c.relnamespace
          where c.relkind = 'r' and n.nspname in ('knobas', 'sync')",
    )
    .fetch_all(pool)
    .await
    .expect("list the tables");
    for table in tables {
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "alter table {table} set (autovacuum_enabled = false)"
        )))
        .execute(pool)
        .await
        .expect("switch autovacuum off");
    }
}

/// Every shape above that `explain` -- one statement's `explain (format json)`
/// answer -- has, described, with `small` the tables and CTEs it may scan per
/// row. Empty for a plan that holds.
#[must_use]
pub fn findings(explain: &serde_json::Value, small: &[&str]) -> Vec<String> {
    let mut texts = Vec::new();
    strings(explain, &mut texts);
    let hashed: Vec<String> = texts
        .iter()
        .flat_map(|t| {
            t.match_indices("hashed SubPlan ")
                .map(|(i, _)| &t[i + "hashed ".len()..])
        })
        .map(|rest| {
            rest.chars()
                .take_while(|c| c.is_alphanumeric() || *c == ' ')
                .collect::<String>()
        })
        .collect();
    let mut found = Vec::new();
    walk(&explain[0]["Plan"], &hashed, small, &[], &mut found);
    found
}

/// Every string anywhere in `value`.
fn strings<'a>(value: &'a serde_json::Value, out: &mut Vec<&'a str>) {
    match value {
        serde_json::Value::String(s) => out.push(s),
        serde_json::Value::Array(items) => items.iter().for_each(|v| strings(v, out)),
        serde_json::Value::Object(map) => map.values().for_each(|v| strings(v, out)),
        _ => {}
    }
}

fn children(node: &serde_json::Value) -> impl Iterator<Item = &serde_json::Value> {
    node["Plans"].as_array().into_iter().flatten()
}

fn describe(node: &serde_json::Value) -> String {
    format!(
        "{} on {}",
        node["Node Type"].as_str().unwrap_or("?"),
        node["Relation Name"]
            .as_str()
            .or(node["CTE Name"].as_str())
            .unwrap_or("-")
    )
}

/// What `node` and everything under it would scan once per row of an outer
/// side: every table or CTE scan but the `small` ones, not descending into
/// init plans or hashed subplans, which run once.
fn scans_per_row(
    node: &serde_json::Value,
    hashed: &[String],
    small: &[&str],
    out: &mut Vec<String>,
) {
    let scanned = match node["Node Type"].as_str() {
        Some("Seq Scan") => node["Relation Name"].as_str(),
        Some("CTE Scan") => node["CTE Name"].as_str(),
        _ => None,
    };
    if scanned.is_some_and(|name| !small.contains(&name)) {
        out.push(describe(node));
    }
    for child in children(node) {
        if child["Parent Relationship"] != "InitPlan" && !is_hashed(child, hashed) {
            scans_per_row(child, hashed, small, out);
        }
    }
}

fn is_hashed(node: &serde_json::Value, hashed: &[String]) -> bool {
    node["Parent Relationship"] == "SubPlan"
        && hashed.iter().any(|h| node["Subplan Name"] == h.as_str())
}

/// The aliases the scans under `node` give their rows.
fn aliases(node: &serde_json::Value, out: &mut Vec<String>) {
    if let Some(alias) = node["Alias"].as_str() {
        out.push(alias.to_owned());
    }
    for child in children(node) {
        aliases(child, out);
    }
}

/// Whether anything under `node` (init plans aside) names a column of one of
/// `aliases`: what makes the inner side of a nested loop a probe
/// parameterized by the outer row rather than the same rows every time.
fn mentions(node: &serde_json::Value, aliases: &[String]) -> bool {
    let mut texts = Vec::new();
    strings(node, &mut texts);
    texts.iter().any(|text| {
        aliases.iter().any(|alias| {
            text.contains(&format!("\"{alias}\"."))
                || text.match_indices(&format!("{alias}.")).any(|(i, _)| {
                    !text[..i]
                        .chars()
                        .next_back()
                        .is_some_and(|c| c.is_alphanumeric() || c == '_')
                })
        })
    })
}

/// The tables and CTEs under `node` but the `small` ones, init plans aside.
fn relations(node: &serde_json::Value, small: &[&str], out: &mut Vec<String>) {
    let name = node["Relation Name"].as_str().or(node["CTE Name"].as_str());
    if name.is_some_and(|name| !small.contains(&name)) {
        out.push(describe(node));
    }
    for child in children(node) {
        if child["Parent Relationship"] != "InitPlan" {
            relations(child, small, out);
        }
    }
}

/// Whether an index condition's clause compares by a range: `<`, `>`, `<=`,
/// `>=`, and nothing that narrows to a key (`=`, `@@`, `&&`).
fn is_range(clause: &str) -> bool {
    [" < ", " > ", " <= ", " >= "]
        .iter()
        .any(|op| clause.contains(op))
}

/// The index scans under `node` (init plans and hashed subplans aside) whose
/// index condition reaches the rows around it (`around`: the loop's outer
/// side and the inner side's other scans) only through a range, or not at all:
/// each outer row then walks a slice of the index the range leaves -- about
/// half the table for `entity_id < o.entity_id` -- and filters it, which is the
/// scan per row in an index's clothing (#771).
fn linked_only_by_a_range(
    node: &serde_json::Value,
    hashed: &[String],
    small: &[&str],
    around: &[String],
    out: &mut Vec<String>,
) {
    let is_index = matches!(
        node["Node Type"].as_str(),
        Some("Index Scan" | "Index Only Scan" | "Bitmap Index Scan")
    );
    let is_small = node["Relation Name"]
        .as_str()
        .is_some_and(|name| small.contains(&name));
    if is_index && !is_small {
        let cond = node["Index Cond"].as_str().unwrap_or("");
        let own = node["Alias"].as_str().unwrap_or("");
        let others: Vec<String> = around.iter().filter(|a| *a != own).cloned().collect();
        // A clause on a subplan's answer computed inside the inner side --
        // the ticket lookup's `id = ANY ((InitPlan n).col1)`, built from the
        // key the outer row found -- is a link too.
        let linked = cond.split(" AND ").any(|clause| {
            let reaches = mentions(&serde_json::Value::String(clause.to_owned()), &others)
                || clause.contains("(InitPlan ")
                || clause.contains("(SubPlan ");
            reaches && !is_range(clause)
        });
        if !linked {
            out.push(format!(
                "{} using {} ({})",
                describe(node),
                node["Index Name"].as_str().unwrap_or("?"),
                if cond.is_empty() {
                    "no condition"
                } else {
                    cond
                }
            ));
        }
    }
    for child in children(node) {
        if child["Parent Relationship"] != "InitPlan" && !is_hashed(child, hashed) {
            linked_only_by_a_range(child, hashed, small, around, out);
        }
    }
}

/// Every per-row scan and every anti-join hashed without the pair's ids.
///
/// `enclosing` is the aliases of the outer sides of the nested loops `node`
/// sits on the inner side of: their rows are parameters inside it too, so
/// a probe keyed by one of them is linked though the loop right above it
/// does not name it -- the planner substitutes an equal column from further
/// out (`i.entity_id = (r.id)` for `i.entity_id = ip.entity_id`, #2185).
fn walk(
    node: &serde_json::Value,
    hashed: &[String],
    small: &[&str],
    enclosing: &[String],
    out: &mut Vec<String>,
) {
    let mut inside = enclosing.to_vec();
    if node["Node Type"] == "Nested Loop" {
        let mut outer_aliases = enclosing.to_vec();
        for outer in children(node).filter(|c| c["Parent Relationship"] == "Outer") {
            aliases(outer, &mut outer_aliases);
        }
        inside.clone_from(&outer_aliases);
        for inner in children(node).filter(|c| c["Parent Relationship"] == "Inner") {
            let mut scans = Vec::new();
            scans_per_row(inner, hashed, small, &mut scans);
            out.extend(
                scans
                    .into_iter()
                    .map(|s| format!("a nested loop rescans {s}")),
            );
            // Linked means to anything the loop's rows carry: the outer side,
            // or a sibling scan inside the inner side the index is probed for.
            let mut around = outer_aliases.clone();
            aliases(inner, &mut around);
            let mut ranged = Vec::new();
            linked_only_by_a_range(inner, hashed, small, &around, &mut ranged);
            out.extend(ranged.into_iter().map(|r| {
                format!("a nested loop walks {r} per outer row, linked to it only by a range")
            }));
            if !mentions(inner, &outer_aliases) {
                let mut read = Vec::new();
                relations(inner, small, &mut read);
                out.extend(
                    read.into_iter()
                        .map(|r| format!("a nested loop crosses its outer side with {r}")),
                );
            }
        }
    }
    if node["Parent Relationship"] == "SubPlan" && !is_hashed(node, hashed) {
        let mut scans = Vec::new();
        scans_per_row(node, hashed, small, &mut scans);
        out.extend(
            scans
                .into_iter()
                .map(|s| format!("a correlated subplan rescans {s}")),
        );
    }
    if node["Node Type"] == "Hash Join"
        && node["Join Type"]
            .as_str()
            .is_some_and(|t| t.contains("Anti"))
    {
        let cond = node["Hash Cond"].as_str().unwrap_or("");
        if cond.contains("relation") && !(cond.contains("from_id") && cond.contains("to_id")) {
            out.push(format!("an anti-join hashed on {cond}"));
        }
        // Part of the key left to a filter: every row of a hash bucket is
        // compared with it, which is the `relation`-only hash of #770 in any
        // other spelling.
        if let Some(filter) = node["Join Filter"].as_str() {
            out.push(format!(
                "an anti-join hashed on {cond} leaves part of its key to a filter: {filter}"
            ));
        }
    }
    for child in children(node) {
        let params = if child["Parent Relationship"] == "Inner" {
            inside.as_slice()
        } else {
            enclosing
        };
        walk(child, hashed, small, params, out);
    }
}

/// Every nested loop in `explain` whose outer side reads `relation` by a
/// sequential scan: a statement driven from that table, a probe into the
/// rest per row of it.
///
/// Linear rather than quadratic, so [`findings`] passes it, but it is the
/// shape `sync.live_item` took on tables never analyzed (#767, #768, #778):
/// `knobas.entity` estimated at 0.5 % of its rows, scanned whole, and
/// `sync.item` probed once per entity -- 430 ms for an inbox that reads a
/// few hundred rows. Init plans and subplans are not the outer side of
/// anything and are not looked into.
#[must_use]
pub fn driven_from(explain: &serde_json::Value, relation: &str) -> Vec<String> {
    let mut found = Vec::new();
    driven(&explain[0]["Plan"], relation, &mut found);
    found
}

fn driven(node: &serde_json::Value, relation: &str, out: &mut Vec<String>) {
    if node["Node Type"] == "Nested Loop" {
        for outer in children(node).filter(|c| c["Parent Relationship"] == "Outer") {
            if seq_scans(outer, relation) {
                out.push(format!(
                    "a nested loop is driven from a Seq Scan on {relation}"
                ));
            }
        }
    }
    for child in children(node) {
        driven(child, relation, out);
    }
}

/// Whether `node` or a join under it seq-scans `relation`.
fn seq_scans(node: &serde_json::Value, relation: &str) -> bool {
    (node["Node Type"] == "Seq Scan" && node["Relation Name"] == relation)
        || children(node)
            .filter(|c| {
                !matches!(
                    c["Parent Relationship"].as_str(),
                    Some("InitPlan" | "SubPlan")
                )
            })
            .any(|c| seq_scans(c, relation))
}

/// Every read of `relation` that runs once per row of an outer side -- the
/// inner side of a nested loop, or a subplan that is not hashed -- and
/// filters what it reads on the item's `payload`, or through a subplan,
/// which is how a declared read is planned (#2185): a declared read tested
/// per row, so the index it walks narrows to the key it was probed by and
/// the payload decides the rest. A probe of the table's primary key reads
/// one row whatever its filter, and is not one. That is a scan per row the checks
/// in [`findings`] cannot see, since its index condition links it to the
/// outer row by equality: a link-set member walked its source's every run
/// through the declared `pipeline` path that way, about 100 ms a quiet
/// member at 30,000 runs.
#[must_use]
pub fn filtered_per_row(explain: &serde_json::Value, relation: &str) -> Vec<String> {
    let mut texts = Vec::new();
    strings(explain, &mut texts);
    let hashed: Vec<String> = texts
        .iter()
        .flat_map(|t| {
            t.match_indices("hashed SubPlan ")
                .map(|(i, _)| &t[i + "hashed ".len()..])
        })
        .map(|rest| {
            rest.chars()
                .take_while(|c| c.is_alphanumeric() || *c == ' ')
                .collect::<String>()
        })
        .collect();
    let mut found = Vec::new();
    per_row_filters(&explain[0]["Plan"], &hashed, relation, false, &mut found);
    found
}

fn per_row_filters(
    node: &serde_json::Value,
    hashed: &[String],
    relation: &str,
    per_row: bool,
    out: &mut Vec<String>,
) {
    if per_row
        && node["Relation Name"] == relation
        && !node["Index Name"]
            .as_str()
            .is_some_and(|index| index.ends_with("_pkey"))
        && node["Filter"]
            .as_str()
            .is_some_and(|filter| filter.contains("payload") || filter.contains("SubPlan"))
    {
        out.push(format!(
            "{} using {} filters the payload once per outer row",
            describe(node),
            node["Index Name"].as_str().unwrap_or("no index")
        ));
    }
    let loop_ = node["Node Type"] == "Nested Loop";
    for child in children(node) {
        let relationship = child["Parent Relationship"].as_str();
        let child_per_row = match relationship {
            Some("InitPlan") => false,
            Some("SubPlan") => !is_hashed(child, hashed) || per_row,
            Some("Inner") if loop_ => true,
            _ => per_row,
        };
        per_row_filters(child, hashed, relation, child_per_row, out);
    }
}

/// Every read of `relation` in `explain` that no `Limit` bounds (#2185): a
/// scan, or an index range, read to its end rather than stopped at the rows
/// a `Limit` above it asks for. A `Sort`, a hash, an aggregate or a bitmap
/// between the `Limit` and the read reads everything first, so it does not
/// bound it. What "one bounded lookup" means for a table that grows with the
/// source's history: the alert reconcile joined every kept sample of every
/// monitor and sorted them to keep the newest (148 ms at 600,000 samples),
/// where the head of each monitor's index range is one row.
#[must_use]
pub fn unbounded_reads(explain: &serde_json::Value, relation: &str) -> Vec<String> {
    let mut found = Vec::new();
    unbounded(&explain[0]["Plan"], relation, false, &mut found);
    found
}

fn unbounded(node: &serde_json::Value, relation: &str, limited: bool, out: &mut Vec<String>) {
    let kind = node["Node Type"].as_str().unwrap_or("");
    let limited = match kind {
        "Limit" => true,
        "Sort" | "Hash" | "Aggregate" | "WindowAgg" | "Materialize" | "Unique" | "SetOp"
        | "Bitmap Heap Scan" => false,
        _ => limited,
    };
    if node["Relation Name"] == relation && !limited {
        out.push(format!(
            "{} using {} is read to its end, no Limit above it",
            describe(node),
            node["Index Name"].as_str().unwrap_or("no index")
        ));
    }
    for child in children(node) {
        unbounded(child, relation, limited, out);
    }
}
