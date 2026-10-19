//! The generated half of the IPC mirror (contract §2.6 as amended by #1325).
//!
//! The TypeScript shape of every type that crosses the bridge is generated
//! from its Rust type by `ts-rs`, one file per barrel, into
//! `app/src/lib/ipc/generated/`. The barrels beside that folder re-export the
//! names and keep their `invoke` functions by hand.
//!
//! The test below **writes** those files: run `cargo test -p knobas-app --lib
//! ipc_mirror` after changing a crossing type and commit what it changed.
//! `just ipc-mirror` (a step of `just check`) runs it and fails when the
//! committed files differ from what the Rust types produce, so a struct change
//! cannot ship without its TypeScript.
//!
//! A type joins a barrel's file with `#[ts(export_to = "<barrel>.ts")]` beside
//! its `TS` derive, and is listed in [`export_roots`] unless a listed type
//! already reaches it. The configuration is spelled here and not read from the
//! environment (`TS_RS_*`), so the output cannot depend on who runs it.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use ts_rs::{Config, ExportError, TS};

/// The barrel directory; the generated files go in its `generated/` folder.
fn ipc_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../app/src/lib/ipc")
}

/// §2.6's mapping where it differs from `ts-rs`'s default: `i64`/`u64` are a
/// JSON number on the wire and a `number` in the shell, not a `bigint`.
fn config(out_dir: &Path) -> Config {
    Config::new().with_large_int("number").with_out_dir(out_dir)
}

/// Every barrel's root types. Exporting a type exports the types it reaches,
/// each into the file its own `export_to` names.
fn export_roots(cfg: &Config) -> Result<(), ExportError> {
    // app.ts
    crate::commands::app::AppStatus::export_all(cfg)?;
    crate::commands::app::DbState::export_all(cfg)?;
    Ok(())
}

/// The `.ts` files directly in `dir`, by name.
fn ts_files(dir: &Path) -> BTreeSet<String> {
    let Ok(entries) = fs::read_dir(dir) else {
        return BTreeSet::new();
    };
    entries
        .map(|entry| entry.expect("a readable directory entry").file_name())
        .filter_map(|name| name.into_string().ok())
        .filter(|name| name.ends_with(".ts"))
        .collect()
}

/// Make `dest` hold exactly the files in `fresh`.
///
/// A file whose text is unchanged is not touched, and a changed one is
/// replaced by a rename: `just check` runs svelte-check beside this test, and
/// a reader must never see a half-written file.
fn sync(fresh: &Path, dest: &Path) {
    fs::create_dir_all(dest).expect("the generated folder can be created");
    let wanted = ts_files(fresh);
    for name in &wanted {
        let text = fs::read_to_string(fresh.join(name)).expect("ts-rs wrote the file");
        let target = dest.join(name);
        if fs::read_to_string(&target).ok().as_deref() == Some(text.as_str()) {
            continue;
        }
        let staging = dest.join(format!(".{name}.tmp"));
        fs::write(&staging, &text).expect("the staging file can be written");
        fs::rename(&staging, &target).expect("the staging file can be moved into place");
    }
    for name in ts_files(dest).difference(&wanted) {
        fs::remove_file(dest.join(name)).expect("a file no type produces can be removed");
    }
}

/// Writes `app/src/lib/ipc/generated/*.ts` from the Rust types.
///
/// Every generated file must belong to a barrel: a type that reaches one with
/// no `export_to` lands in a file of its own name, and that is a missing
/// annotation, not a new barrel (§10.8 freezes the module layout).
#[test]
fn the_generated_ipc_mirror_is_written() {
    let fresh = tempfile::tempdir().expect("a scratch directory");
    export_roots(&config(fresh.path())).expect("every root type exports");

    let barrels = ts_files(&ipc_dir());
    let produced = ts_files(fresh.path());
    let strays: Vec<&String> = produced.difference(&barrels).collect();
    assert!(
        strays.is_empty(),
        "these generated files match no barrel in app/src/lib/ipc/; give the type in each \
         `#[ts(export_to = \"<barrel>.ts\")]`: {strays:?}"
    );
    assert!(
        produced.contains("app.ts"),
        "the app barrel's types produced nothing, so this test proves nothing: {produced:?}"
    );

    sync(fresh.path(), &ipc_dir().join("generated"));
}
