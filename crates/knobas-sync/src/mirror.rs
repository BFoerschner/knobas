//! Asserting that a Rust shape and its TypeScript mirror are the same shape.
//!
//! **One implementation, because a duplicated detector is two detectors that
//! drift.** Four types on this bridge need it -- `SourceSyncStatus`,
//! `SyncReport`, `CredentialHealth` here, and `knobas-app`'s sources DTOs --
//! and they were checked four times by hand before this module existed. Three
//! of those copies did it wrong in the same way.
//!
//! # What the wrong way was
//!
//! `mirror.contains("run_id:")` over the whole file. It passes as soon as
//! *any* interface in `sources.ts` declares a field of that name -- so deleting
//! `run_id` from `interface SourceSyncStatus` left the test green, masked by
//! `SyncProgress.run_id`. The check has to be scoped to the interface that
//! claims to mirror the struct, which is what [`interface_body`] does.
//!
//! # Both directions
//!
//! A Rust field with no TypeScript counterpart is a value the frontend cannot
//! read; a TypeScript field with no Rust counterpart is a form input the
//! backend silently discards. [`assert_shape`] fails on either.

/// The body of `export interface <name> { ... }`, brace-matched.
///
/// # Panics
/// If the mirror declares no such interface, or never closes it.
#[must_use]
pub fn interface_body<'m>(mirror: &'m str, name: &str) -> &'m str {
    let header = format!("export interface {name} {{");
    let start = mirror
        .find(&header)
        .unwrap_or_else(|| panic!("the mirror declares no `interface {name}`"))
        + header.len();
    let rest = &mirror[start..];
    let mut depth = 1_usize;
    for (at, ch) in rest.char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return &rest[..at];
                }
            }
            _ => {}
        }
    }
    panic!("`interface {name}` is never closed");
}

/// The field names an interface body declares at its **top level**, in
/// declaration order.
///
/// Depth-aware: a field of an inline object type (`nested: { deep: string }`)
/// belongs to that object, not to this interface, and reporting it would make
/// the "no counterpart in the Rust struct" half fire on a shape that is
/// perfectly correct. Found by the fixture below, which has one.
#[must_use]
pub fn declared_fields(body: &str) -> Vec<&str> {
    let mut fields = Vec::new();
    let mut depth = 0_i32;
    for line in body.lines() {
        let trimmed = line.trim();
        let top_level = depth == 0;
        depth += i32::try_from(line.matches('{').count()).unwrap_or(0);
        depth -= i32::try_from(line.matches('}').count()).unwrap_or(0);

        if !top_level
            || trimmed.starts_with("//")
            || trimmed.starts_with('*')
            || trimmed.starts_with("/*")
        {
            continue;
        }
        let Some((field, _)) = trimmed.split_once(':') else {
            continue;
        };
        let field = field.trim().trim_end_matches('?');
        if !field.is_empty() && field.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            fields.push(field);
        }
    }
    fields
}

/// Assert that `value`'s keys are exactly `expected`, and exactly what
/// `interface <name>` in `mirror` declares.
///
/// `expected` is spelled out by the caller on purpose: serializing an instance
/// gives the *current* key set, so comparing it only against the mirror would
/// let a field be dropped from both sides at once and stay green. The literal
/// list is the third witness.
///
/// # Panics
/// With the field and the direction, on any of: a key the list does not name, a
/// key the mirror does not declare, or a field the mirror declares that the
/// struct does not have.
pub fn assert_shape(mirror: &str, name: &str, value: &serde_json::Value, expected: &[&str]) {
    let object = value
        .as_object()
        .unwrap_or_else(|| panic!("{name} is not a JSON object: {value}"));
    let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
    keys.sort_unstable();
    let mut want: Vec<&str> = expected.to_vec();
    want.sort_unstable();
    assert_eq!(
        keys, want,
        "{name} grew or lost a field; the TypeScript mirror has to grow or lose it too"
    );

    let body = interface_body(mirror, name);
    let declared = declared_fields(body);
    for key in &keys {
        assert!(
            declared.contains(key),
            "{name}.{key} is missing from `interface {name}` in the mirror -- \
             a value the frontend cannot read"
        );
    }
    for field in &declared {
        assert!(
            keys.contains(field),
            "the mirror's {name}.{field} has no counterpart in the Rust struct \
             -- a field the backend silently discards"
        );
    }
}

/// The members of a TypeScript string union, `export type <name> = "a" | "b";`.
///
/// Read out of the mirror rather than listed in a test: a hand-written list is
/// the "remembered list" trap, and for the phase-emission check it is precisely
/// the list the test must not be allowed to define for itself.
///
/// # Panics
/// If the mirror declares no such union, or never terminates it.
#[must_use]
pub fn declared_union(mirror: &str, name: &str) -> Vec<String> {
    let head = format!("export type {name} =");
    let start = mirror
        .find(&head)
        .unwrap_or_else(|| panic!("the mirror declares no `type {name}`"))
        + head.len();
    let rest = &mirror[start..];
    let body = &rest[..rest
        .find(';')
        .unwrap_or_else(|| panic!("`type {name}` is never terminated"))];
    body.split('|')
        .map(|member| member.trim().trim_matches('"').to_owned())
        .filter(|member| !member.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The detector, driven over text it must accept and text it must reject.
    ///
    /// A net that cannot be shown to catch anything is not evidence there was
    /// nothing to catch -- and this net's first version caught nothing, because
    /// it searched the whole file.
    ///
    /// # What each part of this fixture is for
    ///
    /// Every feature here exists so that some assertion below *can fail*:
    /// `Other` and `After` for the scoping, the comment for the prose filter,
    /// `beta?` for the optional marker -- and `nested`'s inline object is
    /// deliberately spread over **three lines**, because [`declared_fields`]
    /// iterates by line. Written on one line (`nested: { inner: string };`) its
    /// field is never a candidate at all, so removing the depth guard left all
    /// nine of these tests green *including the one that names it*. A correct
    /// assertion whose fixture cannot produce the case it is about is not a
    /// test. There are no inline object types on the bridge today, which is
    /// precisely why the branch needs a witness here rather than a caller.
    const SAMPLE: &str = r#"
export interface Other {
  run_id: number;
  shared: string;
}

/** Doc. */
export interface Thing {
  // a comment mentioning ghost: here
  alpha: string;
  /** beta does something */
  beta?: number | null;
  nested: {
    inner: string;
  };
}

export interface After {
  gamma: boolean;
}
"#;

    fn thing(keys: &[&str]) -> serde_json::Value {
        serde_json::Value::Object(
            keys.iter()
                .map(|k| ((*k).to_owned(), serde_json::json!(1)))
                .collect(),
        )
    }

    #[test]
    fn the_body_is_brace_matched_and_stops_at_the_interface_it_names() {
        let body = interface_body(SAMPLE, "Thing");
        assert!(body.contains("alpha:"));
        assert!(
            body.contains("inner:"),
            "a nested object is inside the body"
        );
        assert!(
            !body.contains("gamma:"),
            "the match ran past the closing brace into the next interface"
        );
        assert!(
            !body.contains("shared:"),
            "the match started in the wrong interface"
        );
    }

    #[test]
    fn declared_fields_reads_the_optional_marker_and_skips_prose_and_nesting() {
        let fields = declared_fields(interface_body(SAMPLE, "Thing"));
        assert_eq!(fields, ["alpha", "beta", "nested"]);
        assert!(
            !fields.contains(&"ghost"),
            "a field name mentioned in a comment is not a declaration"
        );
        assert!(
            !fields.contains(&"inner"),
            "a field of an inline object type belongs to that object, not to \
             the interface"
        );
    }

    #[test]
    fn a_matching_shape_passes() {
        assert_shape(
            SAMPLE,
            "Thing",
            &thing(&["alpha", "beta", "nested"]),
            &["alpha", "beta", "nested"],
        );
    }

    /// **The whole point of the scoping.** `run_id` is declared by `Other`, so
    /// a whole-file `contains` would find it and pass.
    #[test]
    #[should_panic(expected = "Thing.run_id is missing from `interface Thing`")]
    fn a_field_only_another_interface_declares_is_not_found() {
        assert_shape(
            SAMPLE,
            "Thing",
            &thing(&["alpha", "beta", "nested", "run_id"]),
            &["alpha", "beta", "nested", "run_id"],
        );
    }

    #[test]
    #[should_panic(expected = "the mirror's Thing.alpha has no counterpart")]
    fn a_typescript_only_field_is_caught_too() {
        assert_shape(
            SAMPLE,
            "Thing",
            &thing(&["beta", "nested"]),
            &["beta", "nested"],
        );
    }

    #[test]
    #[should_panic(expected = "grew or lost a field")]
    fn the_expected_list_is_a_third_witness() {
        assert_shape(
            SAMPLE,
            "Thing",
            &thing(&["alpha", "beta", "nested"]),
            &["alpha", "beta"],
        );
    }

    #[test]
    fn a_union_is_read_across_however_many_lines_it_spans() {
        assert_eq!(
            declared_union(r#"export type One = "a" | "b" | "c";"#, "One"),
            ["a", "b", "c"]
        );
        assert_eq!(
            declared_union("export type Two =\n  | \"x\"\n  | \"y\";\n", "Two"),
            ["x", "y"]
        );
    }

    #[test]
    #[should_panic(expected = "declares no `type Absent`")]
    fn a_union_the_mirror_does_not_declare_is_a_failure_not_an_empty_list() {
        let _ = declared_union("export type Other = \"a\";", "Absent");
    }

    #[test]
    #[should_panic(expected = "declares no `interface Missing`")]
    fn an_interface_the_mirror_does_not_declare_is_a_failure_not_a_pass() {
        assert_shape(SAMPLE, "Missing", &thing(&["alpha"]), &["alpha"]);
    }
}
