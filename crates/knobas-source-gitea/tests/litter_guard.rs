//! Docker-free coverage for [`live_env::Litter`], the guard the live suites
//! clean up after themselves with.
//!
//! # Why this file exists next to a suite that already exercises the guard
//!
//! `tests/live_gitea.rs` drives `Litter` against the real container, but it is
//! `#[ignore]`d and needs Docker, a seeded Gitea and a token -- so nothing it
//! proves is proved by `just check`. Two of the guard's properties are load
//! bearing enough that they must not wait for someone to run `just gitea-live`:
//!
//! * **[`LITTER`] decides what gets deleted.** `Litter::new` deletes every
//!   branch of the mutated repository whose name starts with it. That is safe
//!   only while nothing `testenv/seed-gitea.sh` creates starts with it, and
//!   that was a fact somebody checked by hand once (PR #153) rather than a
//!   property anything re-checks. A fixture branch named `knobas-anything`
//!   would be deleted by the next live run and nothing would say so.
//! * **The removal of an earlier run's leftovers has no other witness.**
//!   Deleting it from `Litter::new` breaks no live test: residue simply
//!   accumulates until `live_gitea_capped`'s `HEADROOM` refuses to start at 19
//!   pull requests, runs later and in another suite. A `Litter::new` that
//!   *asserted* on residue would be red on the run that inherits it and green
//!   on the retry, which is the shape Fable's #143 ruling refuses in advance --
//!   so the witness belongs here, against a stand-in whose state the test
//!   dictates, where there is no retry to be green on.
//!
//! Both run in `just check`: no container, no token, no `#[ignore]`.

mod live_env;

use live_env::LITTER;

/// The repository root, from this crate's manifest directory.
fn root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("the workspace root is two levels above this crate")
}

fn fixture() -> serde_json::Value {
    let path = root().join("fixtures/tidewater/work.json");
    serde_json::from_str(
        &std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display())),
    )
    .expect("fixtures/tidewater/work.json is JSON")
}

/// The head branches `seed-gitea.sh` invents for the two pull requests the
/// fixture gives no `from` for.
///
/// Read out of the script rather than copied here, so a third synthesized name
/// is covered the day it is added. The parse is deliberately narrow -- the
/// `echo`ed word of each arm of one `case` -- and asserts it found something,
/// so a rewrite that moves the names elsewhere fails loudly instead of
/// silently checking nothing.
fn synthesized_branch_names(script: &str) -> Vec<String> {
    let body = script
        .split_once("synth_branch() {")
        .expect("testenv/seed-gitea.sh defines synth_branch()")
        .1
        .split_once("\n}")
        .expect("synth_branch()'s body ends at a closing brace")
        .0;
    let names: Vec<String> = body
        .lines()
        .filter_map(|line| line.split_once("echo \""))
        .filter_map(|(_, rest)| rest.split_once('"'))
        .map(|(name, _)| name.to_owned())
        .filter(|name| !name.is_empty())
        .collect();
    assert!(
        !names.is_empty(),
        "no synthesized branch name was found in seed-gitea.sh's synth_branch(); the parse below \
         is checking nothing"
    );
    names
}

/// Every branch name `testenv/seed-gitea.sh` puts in the container, from the
/// three places it takes them from: the fixture's own `branches[]`, the head
/// branch each pull request names, and the two it synthesizes for the pull
/// requests that name none. `main` comes from `auto_init`, not from a list.
fn seeded_branch_names() -> Vec<String> {
    let fixture = fixture();
    let named = |rows: &serde_json::Value, field: &str| -> Vec<String> {
        rows.as_array()
            .expect("the fixture's collections are arrays")
            .iter()
            .filter_map(|row| row[field].as_str().map(str::to_owned))
            .collect()
    };
    let script = root().join("testenv/seed-gitea.sh");
    let script = std::fs::read_to_string(&script)
        .unwrap_or_else(|e| panic!("read {}: {e}", script.display()));

    let mut names = vec!["main".to_owned()];
    names.extend(named(&fixture["branches"], "name"));
    names.extend(named(&fixture["prs"], "from"));
    names.extend(named(&fixture["repos"], "default_branch"));
    names.extend(synthesized_branch_names(&script));
    names
}

/// Every string anywhere in a JSON document, however deeply nested.
fn strings(value: &serde_json::Value, into: &mut Vec<String>) {
    match value {
        serde_json::Value::String(text) => into.push(text.clone()),
        serde_json::Value::Array(rows) => rows.iter().for_each(|row| strings(row, into)),
        serde_json::Value::Object(fields) => {
            fields.values().for_each(|field| strings(field, into))
        }
        _ => {}
    }
}

/// **The destructive rule, pinned.** `Litter::new` deletes every branch of the
/// mutated repository whose name starts with [`LITTER`], so a seeded branch
/// that started with it would be destroyed by the next `just gitea-live` and
/// re-created by the next seed, silently, forever.
///
/// Two assertions, because they fail for different reasons. The first knows
/// how `seed-gitea.sh` decides on branch names and checks exactly those. The
/// second knows nothing and checks every string in the fixture: `knobas-` is
/// this application's name, and the fixture is a fictional freight company, so
/// nothing in it should carry that prefix under any key at all. It is the one
/// that still holds when the fixture grows a field the first does not know
/// about -- which is precisely how a `knobas-`-prefixed branch would arrive.
#[test]
fn nothing_the_seed_puts_in_the_container_could_be_taken_for_this_suites_litter() {
    let seeded = seeded_branch_names();
    let collides: Vec<&String> = seeded.iter().filter(|n| n.starts_with(LITTER)).collect();
    assert!(
        collides.is_empty(),
        "testenv/seed-gitea.sh seeds {collides:?}, which start with {LITTER:?} -- the prefix \
         live_env::Litter::clear_leftovers deletes branches by. The next `just gitea-live` would \
         destroy them and the next seed would put them back, on every run, silently. Rename the \
         branch."
    );

    let mut everything = Vec::new();
    strings(&fixture(), &mut everything);
    let suspects: Vec<&String> = everything.iter().filter(|s| s.starts_with(LITTER)).collect();
    assert!(
        suspects.is_empty(),
        "fixtures/tidewater/work.json carries {suspects:?}, which start with {LITTER:?} -- the \
         prefix live_env::Litter::clear_leftovers deletes branches by. Tidewater is a fictional freight \
         company and knobas- is this application's own name, so nothing in the fixture should \
         begin with it; if one of these is a branch name, the live suite will delete it."
    );
}
