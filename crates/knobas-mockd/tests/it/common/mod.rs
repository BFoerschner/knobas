//! Shared helpers for the two contract suites (`jira_contract`,
//! `teamcity_contract`). One copy so the two gates cannot drift apart in how
//! they normalise or where they store a snapshot.

/// Compares `actual` against `tests/golden/<api>/<name>.json`, or rewrites it
/// when `UPDATE_GOLDEN` is set.
///
/// # Panics
///
/// If the snapshot is absent, or differs from `actual`.
pub fn golden(api: &str, name: &str, actual: &serde_json::Value) {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(api)
        .join(format!("{name}.json"));
    let pretty = serde_json::to_string_pretty(actual).unwrap() + "\n";
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, &pretty).unwrap();
        return;
    }
    let want = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{}: {e} -- regenerate with UPDATE_GOLDEN=1", path.display()));
    assert_eq!(
        pretty, want,
        "{api}/{name} drifted; review, then UPDATE_GOLDEN=1 if intended"
    );
}

/// Rewrites every occurrence of the server's ephemeral `http://127.0.0.1:<port>`
/// to a stable stand-in, so a snapshot is not invalidated by the port the OS
/// happened to hand out.
pub fn replace_base_url(v: &mut serde_json::Value, base: &str, with: &str) {
    match v {
        serde_json::Value::String(s) => {
            if s.contains(base) {
                *s = s.replace(base, with);
            }
        }
        serde_json::Value::Array(a) => {
            for x in a {
                replace_base_url(x, base, with);
            }
        }
        serde_json::Value::Object(o) => {
            for (_, x) in o.iter_mut() {
                replace_base_url(x, base, with);
            }
        }
        _ => {}
    }
}

/// [`replace_base_url`] with the stand-in every golden in this crate uses.
pub fn normalise_base_url(v: &mut serde_json::Value, base: &str) {
    replace_base_url(v, base, "http://mockd.test");
}
