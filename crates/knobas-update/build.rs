//! The keys this build trusts (ADR-0052, decisions D2).
//!
//! `keys/primary.pub` and `keys/spare.pub` are Björn's two public keys. Until both are
//! committed, the build trusts none of them and the updater says it is not set up. One without
//! the other is a mistake, and the build says so rather than shipping half a pair.

use std::fmt::Write as _;
use std::path::PathBuf;

fn main() {
    let dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets it"));
    let keys = dir.join("keys");
    println!("cargo:rerun-if-changed={}", keys.display());
    println!("cargo:rerun-if-env-changed=KNOBAS_UPDATE_EXTRA_PUBKEY");
    let primary = keys.join("primary.pub");
    let spare = keys.join("spare.pub");
    println!("cargo:rerun-if-changed={}", primary.display());
    println!("cargo:rerun-if-changed={}", spare.display());

    let mut out = String::from("/// The two committed public keys, by name, or none.\n");
    out.push_str("pub(crate) const SHIPPED: &[(&str, &str)] = &[\n");
    match (primary.is_file(), spare.is_file()) {
        (true, true) => {
            for (name, path) in [("primary", &primary), ("spare", &spare)] {
                let text = std::fs::read_to_string(path).expect("a key file reads");
                writeln!(out, "    ({name:?}, {:?}),", text.trim()).expect("a string takes it");
            }
        }
        (false, false) => {}
        _ => panic!(
            "crates/knobas-update/keys holds one of primary.pub and spare.pub: commit both \
             (docs/development.md, Update signing)"
        ),
    }
    out.push_str("];\n");
    let target =
        PathBuf::from(std::env::var("OUT_DIR").expect("cargo sets it")).join("shipped_keys.rs");
    std::fs::write(target, out).expect("OUT_DIR is writable");
}
