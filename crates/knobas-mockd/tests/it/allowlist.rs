use knobas_mockd::allowlist::{Lookup, ROUTE_COUNT, lookup, response_schema};

#[test]
fn the_wadl_yielded_the_whole_surface() {
    // 392 (verb, path) pairs in the pinned document; the floor guards against a
    // parser that silently walks only the first nesting level.
    // Bound through a local so this stays a runtime assertion: clippy refuses
    // `assert!` on a constant, and a `const {}` block would turn a shallow-walk
    // regression into a compile error instead of a named failing test.
    let generated = ROUTE_COUNT;
    assert!(generated >= 380, "only {generated} routes generated");
}

#[test]
fn search_allows_exactly_the_documented_query_parameters() {
    let Lookup::Allowed { query } = lookup("GET", "api/2/search") else {
        panic!("GET api/2/search must be allowed")
    };
    let mut got: Vec<&str> = query.to_vec();
    got.sort_unstable();
    assert_eq!(
        got,
        [
            "expand",
            "fields",
            "jql",
            "maxResults",
            "startAt",
            "validateQuery"
        ]
    );
}

#[test]
fn template_segments_match_a_concrete_value() {
    assert!(matches!(
        lookup("GET", "api/2/issue/PAY-231"),
        Lookup::Allowed { .. }
    ));
    assert!(matches!(
        lookup("GET", "api/2/issue/PAY-231/comment"),
        Lookup::Allowed { .. }
    ));
    assert!(matches!(
        lookup("GET", "api/2/issue/PAY-231/worklog"),
        Lookup::Allowed { .. }
    ));
    // Fact 2: `project` and `{projectIdOrKey}` are separate nested resources.
    assert!(matches!(
        lookup("GET", "api/2/project/PAY"),
        Lookup::Allowed { .. }
    ));
}

#[test]
fn a_literal_segment_wins_over_a_template_segment() {
    // Both `api/2/issue/{issueIdOrKey}` and `api/2/issue/picker` exist. The
    // literal route's query set is the one a request to /issue/picker gets.
    let Lookup::Allowed { query: picker } = lookup("GET", "api/2/issue/picker") else {
        panic!("issue/picker must be allowed")
    };
    let Lookup::Allowed { query: issue } = lookup("GET", "api/2/issue/PAY-231") else {
        panic!("issue/{{key}} must be allowed")
    };
    assert_ne!(picker, issue, "the template route shadowed the literal one");
}

#[test]
fn the_cloud_dialect_is_not_in_this_contract() {
    // roadmap §4 gotcha 4: an adapter that speaks Cloud must fail here.
    assert!(matches!(
        lookup("GET", "api/3/search/jql"),
        Lookup::NotFound
    ));
    assert!(matches!(
        lookup("GET", "api/2/search/jql"),
        Lookup::NotFound
    ));
}

#[test]
fn a_known_path_with_the_wrong_verb_reports_what_is_allowed() {
    let Lookup::MethodNotAllowed { allowed } = lookup("DELETE", "api/2/search") else {
        panic!("DELETE api/2/search must be method-not-allowed, not not-found")
    };
    let mut allowed = allowed;
    allowed.sort_unstable();
    assert_eq!(allowed, ["GET", "POST"]);
}

#[test]
fn response_schemas_are_present_for_every_endpoint_mockd_serves() {
    for (verb, path, title) in [
        ("GET", "api/2/search", "Search Results"),
        ("GET", "api/2/serverInfo", "Server Info"),
        ("GET", "api/2/myself", "User"),
        ("GET", "api/2/issue/{issueIdOrKey}", "Issue"),
        (
            "GET",
            "api/2/issue/{issueIdOrKey}/comment",
            "Comments With Pagination",
        ),
        (
            "GET",
            "api/2/issue/{issueIdOrKey}/worklog",
            "Worklog With Pagination",
        ),
    ] {
        let raw = response_schema(verb, path)
            .unwrap_or_else(|| panic!("no embedded schema for {verb} {path}"));
        let v: serde_json::Value = serde_json::from_str(raw).unwrap();
        assert_eq!(v["title"], title, "{verb} {path}");
    }
}
