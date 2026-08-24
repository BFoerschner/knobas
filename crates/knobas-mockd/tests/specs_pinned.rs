use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

fn specs_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testenv/specs")
}

/// `SHA256SUMS` is in `shasum -a 256` format: `<64 hex>  <filename>`.
#[test]
fn vendored_specs_match_sha256sums() {
    let dir = specs_dir();
    let sums = std::fs::read_to_string(dir.join("SHA256SUMS")).expect("SHA256SUMS must exist");
    let mut checked = 0;
    for line in sums.lines().filter(|l| !l.trim().is_empty()) {
        let (want, name) = line
            .split_once("  ")
            .unwrap_or_else(|| panic!("malformed SHA256SUMS line: {line:?}"));
        let bytes = std::fs::read(dir.join(name.trim()))
            .unwrap_or_else(|e| panic!("{name}: {e} -- run testenv/specs/fetch.sh"));
        let got = format!("{:x}", Sha256::digest(&bytes));
        assert_eq!(
            got,
            want.trim(),
            "{name} does not match its pin. A vendored contract changed: review the diff, \
             then regenerate with `shasum -a 256 *.json *.wadl > SHA256SUMS` -- never the \
             other way round."
        );
        checked += 1;
    }
    assert!(checked >= 4, "SHA256SUMS pins only {checked} files");
}

/// The WADL is the request allowlist's source (Task 2) and the response-schema
/// source (Task 7). Both break invisibly if the document stops being the DC v2
/// one, so pin the two facts the generator relies on.
#[test]
fn wadl_is_the_datacenter_v2_contract() {
    let wadl = std::fs::read_to_string(specs_dir().join("jira-dc-rest.wadl")).unwrap();
    assert!(
        wadl.contains(r#"title="Jira 9.17.0""#),
        "WADL version changed"
    );
    assert!(
        wadl.contains(r#"path="api/2/search""#),
        "no /api/2/search resource"
    );
    assert!(
        !wadl.contains("search/jql"),
        "this is a Cloud document -- DC keeps /rest/api/2/search (roadmap §4 gotcha 4)"
    );
}
