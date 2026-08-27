//! The TeamCity fidelity gate. Both halves run.
//!
//! * **Goldens** pin the bodies mockd serves.
//! * **Schemas** validate those bodies against `testenv/specs/teamcity.json`.
//!
//! ## What is vendored, and how far to trust it
//!
//! `testenv/specs/teamcity.json` is **OpenAPI 3.0.0** (`info.version`
//! `"2026.1 (current)"`), not the Swagger 2.0 this gate was first written
//! against: schemas live under `components.schemas` with **lowercase** names
//! (`server`, `user`, `build`), not under `definitions` with capitalised ones.
//!
//! It has **mixed authority**, and the two halves are not equally trustworthy:
//!
//! * The bulk was downloaded from **JetBrains' own guest TeamCity instance**
//!   (2026.1), i.e. emitted by a real server. Authoritative.
//! * Björn then **hand-augmented it** with information read out of the official
//!   HTML documentation. Those parts are one human's reading of prose, so they
//!   carry a human's error bar.
//!
//! The practical consequence, and the reason this is written down here rather
//! than only in `testenv/specs/README.md`: when this gate fails against some
//! constraint, **the constraint is a suspect too**. Check whether the clause
//! being violated is server-derived or hand-added before "fixing" mockd to
//! satisfy it — bending a fixture to match a mis-transcribed constraint makes
//! mockd *less* like the real TeamCity while turning the gate green.
//!
//! ## Dialect
//!
//! OpenAPI 3.0's Schema Object is a modified subset of JSON Schema draft-04,
//! and its divergences (`nullable: true` in place of a `["string","null"]` type
//! array, boolean `exclusiveMinimum`) are exactly the things a modern validator
//! misreads. This document happens to use **none** of them —
//! [`the_vendored_document_avoids_the_openapi_30_only_constructs`] pins that,
//! so the day a re-fetch introduces one, the failure names the dialect instead
//! of surfacing as a baffling type error. `format` is the one deliberate
//! exception; see [`schema_for`].
//!
//! ## Self-arming
//!
//! The schema half looks for the document and skips if it is absent, so the
//! file could in principle disappear again without wedging the suite. And
//! [`the_teamcity_swagger_blocker_is_recorded`] fails if the README's row ever
//! disagrees with what is on disk, in either direction.

mod common;

use common::{golden, normalise_base_url};
use knobas_mockd::spawn_mock_teamcity;

/// `(request path, golden name, `components.schemas` name)`. One table for both
/// halves, so an endpoint cannot be added to one gate and missed in the other.
///
/// The third column is lowercase because OpenAPI 3 TeamCity names its schemas
/// that way (`build`, not `Build`); the pointers are case-sensitive, and
/// [`every_served_schema_name_exists`] fails loudly rather than letting a
/// mis-cased name silently validate against nothing.
const SERVED: &[(&str, &str, &str)] = &[
    ("/app/rest/server", "server", "server"),
    (
        "/app/rest/users/current?fields=$long",
        "users_current",
        "user",
    ),
    (
        "/app/rest/buildTypes?fields=$long",
        "build_types",
        "buildTypes",
    ),
    (
        "/app/rest/builds?locator=state:any,count:100&fields=$long",
        "builds_all",
        "builds",
    ),
    (
        "/app/rest/builds/id:1187?fields=$long",
        "build_1187",
        "build",
    ),
];

fn spec_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testenv/specs/teamcity.json")
}

/// `Some(doc)` when the spec is vendored, `None` (having said so) when it is
/// not. Shared by every test that needs the document, so the skip path stays in
/// one place.
fn vendored_spec(test: &str) -> Option<serde_json::Value> {
    let path = spec_path();
    let Ok(raw) = std::fs::read_to_string(&path) else {
        // Not a silent pass: `the_teamcity_swagger_blocker_is_recorded` fails if
        // the README stops recording why this document is missing.
        eprintln!(
            "SKIP {test}: {} is not vendored yet (see testenv/specs/README.md)",
            path.display()
        );
        return None;
    };
    Some(serde_json::from_str(&raw).expect("teamcity.json must be JSON"))
}

async fn tc(base: &str, path: &str) -> serde_json::Value {
    reqwest::Client::new()
        .get(format!("{base}{path}"))
        .header("Accept", "application/json")
        .header(
            "Authorization",
            format!("Bearer {}", knobas_mockd::TEAMCITY_TOKEN),
        )
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap()
}

#[tokio::test]
async fn every_served_response_matches_its_golden() {
    let s = spawn_mock_teamcity().await;
    let b = s.base_url();
    for (path, name, _) in SERVED {
        let mut body = tc(&b, path).await;
        normalise_base_url(&mut body, &b);
        golden("teamcity", name, &body);
    }
    s.assert_no_violations();
}

/// A validator for one `components.schemas` entry.
///
/// The validator needs a document carrying both the reference and the schemas
/// it resolves against, so the wrapper republishes `components.schemas`
/// unaltered at the same pointer prefix the document's own internal `$ref`s
/// use (`#/components/schemas/...`) — every nested reference then resolves
/// without rewriting a single one.
///
/// **`format` is an annotation here, not an assertion.** Stated explicitly
/// rather than left to the validator's default, because the default depends on
/// which draft the crate infers for a document that carries no `$schema` — and
/// a gate should not have its strictness decided by inference. TeamCity
/// overloads `format` to carry its own locator grammars: of the distinct values
/// in this document, all but four are strings like `BuildLocator`,
/// `"String value"` and `"Should be in the form \"profileId:<profileId>...\""`,
/// which no validator can assert
/// ([`teamcity_overloads_format_with_its_own_locator_grammars`] pins that).
///
/// This costs nothing today: the four standard values reachable from the five
/// served bodies are `int32`/`int64`/`double`, which are OpenAPI numeric hints
/// that JSON Schema ignores anyway. The document's three `date-time` fields
/// (`token.creationTime`, `token.expirationTime`,
/// `DeploymentStateEntry.changeDate`) are not reachable from any served body,
/// so no timestamp assertion is being given up here — TeamCity's compact
/// `20260822T101000+0000` build dates are plain `type: string` in the spec and
/// are still type-checked.
fn schema_for(doc: &serde_json::Value, schema: &str) -> jsonschema::Validator {
    let wrapper = serde_json::json!({
        "$ref": format!("#/components/schemas/{schema}"),
        "components": { "schemas": doc["components"]["schemas"] },
    });
    jsonschema::options()
        .should_validate_formats(false)
        .build(&wrapper)
        .unwrap_or_else(|e| panic!("#/components/schemas/{schema} does not compile: {e}"))
}

/// A mis-cased or renamed schema name resolves to nothing, and the dangerous
/// failure mode would be a validator that treats an unresolvable `$ref` as
/// "nothing to check" and passes everything.
///
/// Measured, not assumed: with `SERVED`'s `server` changed to `Server`,
/// `jsonschema` 0.51 fails at build time with `Pointer
/// '/components/schemas/Server' does not exist`, so the main gate does *not*
/// pass vacuously. This test is kept anyway, for the error message — it names
/// the lowercase-schema-names trap that OpenAPI 3 TeamCity sets, where the
/// validator's pointer error only says the pointer is missing.
#[test]
fn every_served_schema_name_exists() {
    let Some(doc) = vendored_spec("every_served_schema_name_exists") else {
        return;
    };
    let schemas = doc["components"]["schemas"]
        .as_object()
        .expect("components.schemas must be an object");
    let missing: Vec<&str> = SERVED
        .iter()
        .map(|(_, _, s)| *s)
        .filter(|s| !schemas.contains_key(*s))
        .collect();
    assert!(
        missing.is_empty(),
        "SERVED names absent from components.schemas: {missing:?}. TeamCity's OpenAPI 3 \
         document uses lowercase schema names; check the case before renaming anything."
    );
}

#[tokio::test]
async fn teamcity_responses_conform_to_the_vendored_spec() {
    let Some(doc) = vendored_spec("teamcity_responses_conform_to_the_vendored_spec") else {
        return;
    };

    let s = spawn_mock_teamcity().await;
    let b = s.base_url();
    // Accumulated across every endpoint, not asserted per-iteration: the first
    // failing endpoint would otherwise hide the other four, and a gate being
    // armed for the first time wants the whole picture in one run.
    let mut report = Vec::new();
    for (request, _, schema_name) in SERVED {
        let body = tc(&b, request).await;
        let validator = schema_for(&doc, schema_name);
        let errs: Vec<String> = validator
            .iter_errors(&body)
            .map(|e| format!("    {} at {}", e, e.instance_path()))
            .collect();
        if !errs.is_empty() {
            report.push(format!(
                "  {request} violates #/components/schemas/{schema_name}:\n{}",
                errs.join("\n")
            ));
        }
    }
    assert!(
        report.is_empty(),
        "mockd deviates from the vendored TeamCity spec:\n{}\n\
         Before changing mockd: teamcity.json is partly hand-augmented from the HTML docs \
         (see testenv/specs/README.md). A violated constraint may be the transcription, \
         not the fixture.",
        report.join("\n")
    );
    s.assert_no_violations();
}

/// Walk every object in `components.schemas` and collect the JSON pointers of
/// anything that only means what it looks like under OpenAPI 3.0's dialect.
fn openapi_30_only_constructs(schemas: &serde_json::Value) -> Vec<String> {
    fn walk(v: &serde_json::Value, at: &str, out: &mut Vec<String>) {
        match v {
            serde_json::Value::Object(map) => {
                if map.contains_key("nullable") {
                    out.push(format!(
                        "{at}: `nullable` (OAS 3.0 spelling of a null union)"
                    ));
                }
                // draft-04 spells the strict bounds as a boolean flag beside
                // `minimum`; every later draft makes them numbers. A validator
                // on the wrong side of that reads `exclusiveMinimum: true` as
                // "must be greater than true".
                for k in ["exclusiveMinimum", "exclusiveMaximum"] {
                    if map.get(k).is_some_and(serde_json::Value::is_boolean) {
                        out.push(format!("{at}/{k}: boolean (draft-04 spelling)"));
                    }
                }
                for (k, child) in map {
                    walk(child, &format!("{at}/{k}"), out);
                }
            }
            serde_json::Value::Array(items) => {
                for (i, child) in items.iter().enumerate() {
                    walk(child, &format!("{at}/{i}"), out);
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    walk(schemas, "#/components/schemas", &mut out);
    out
}

/// The gate validates this OpenAPI 3.0 document with a modern JSON Schema
/// validator, which is only sound because the document uses none of the
/// constructs where the two dialects disagree. That is a property of *this*
/// vendored file, not of OpenAPI 3.0, so it is pinned: a re-fetch that
/// introduces a `nullable` gets a failure that names the dialect, instead of an
/// unexplained type error somewhere in a 238-schema document.
#[test]
fn the_vendored_document_avoids_the_openapi_30_only_constructs() {
    let Some(doc) = vendored_spec("the_vendored_document_avoids_the_openapi_30_only_constructs")
    else {
        return;
    };
    let found = openapi_30_only_constructs(&doc["components"]["schemas"]);
    assert!(
        found.is_empty(),
        "teamcity.json now uses OpenAPI-3.0-only schema constructs that a 2020-12 validator \
         misreads:\n  {}\nHandle them in `schema_for` (a preprocessing pass that rewrites them \
         into the modern spelling) — do NOT relax the assertions to make them go away.",
        found.join("\n  ")
    );
}

/// The evidence for `schema_for` turning format assertion off, kept executable
/// so the exemption cannot outlive its reason: TeamCity overwhelmingly uses
/// `format` to name its own locator grammars rather than a JSON Schema format.
/// A validator is required to ignore formats it does not know, so leaving
/// assertion on would be *mostly* harmless — but "mostly" is the wrong footing
/// for a gate, and the day one of these strings collides with a real format
/// name the failure would be inscrutable. If this ever drops to zero, the
/// exemption should be revisited rather than kept out of habit.
#[test]
fn teamcity_overloads_format_with_its_own_locator_grammars() {
    let Some(doc) = vendored_spec("teamcity_overloads_format_with_its_own_locator_grammars") else {
        return;
    };
    // Every `format` this document actually uses in a JSON Schema sense.
    const JSON_SCHEMA_FORMATS: &[&str] = &["date-time", "date", "time", "uri", "uuid", "email"];
    // OpenAPI's own numeric hints. Not JSON Schema formats either, but they are
    // at least standardised somewhere and are not the point being made here.
    const OPENAPI_FORMATS: &[&str] = &["int32", "int64", "float", "double", "byte", "binary"];

    let mut foreign = std::collections::BTreeSet::new();
    fn walk(v: &serde_json::Value, out: &mut std::collections::BTreeSet<String>) {
        match v {
            serde_json::Value::Object(map) => {
                if let Some(f) = map.get("format").and_then(serde_json::Value::as_str) {
                    out.insert(f.to_owned());
                }
                map.values().for_each(|c| walk(c, out));
            }
            serde_json::Value::Array(a) => a.iter().for_each(|c| walk(c, out)),
            _ => {}
        }
    }
    walk(&doc["components"]["schemas"], &mut foreign);
    foreign.retain(|f| {
        !JSON_SCHEMA_FORMATS.contains(&f.as_str()) && !OPENAPI_FORMATS.contains(&f.as_str())
    });
    assert!(
        foreign.len() > 20,
        "teamcity.json no longer overloads `format` with TeamCity grammars (found only \
         {}: {foreign:?}). That was the reason `schema_for` disables format assertion — \
         re-check whether the exemption is still warranted.",
        foreign.len()
    );
}

#[test]
fn the_teamcity_swagger_blocker_is_recorded() {
    // The one thing that must never happen quietly: the schema half skipping
    // while nothing in the tree says why. Either the document is vendored, or
    // the README explains that it is not.
    let readme = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testenv/specs/README.md"),
    )
    .expect("testenv/specs/README.md");
    if spec_path().exists() {
        // Its *own* row, not just the word somewhere in the file: every other
        // vendored document already says "vendored", so a whole-file search
        // would pass without anyone touching the TeamCity row at all.
        let row = readme
            .lines()
            .find(|l| l.starts_with("| `teamcity.json` |"))
            .expect("README.md must have a `teamcity.json` row");
        // Negatively, not positively: the row's blocked spelling is "**BLOCKED,
        // not vendored**", so a `contains("vendored")` check passes on the very
        // text it is supposed to reject.
        assert!(
            !row.contains("BLOCKED") && !row.contains("not vendored"),
            "teamcity.json is present but its README row still says it is not: {row:?}. \
             Flip it to `vendored — <version>, <date>` and re-pin SHA256SUMS."
        );
        // `fetch.sh`'s postscript asks for the dialect to be recorded, because
        // the whole gate reads `components.schemas` on the strength of it. Row-
        // scoped for the same reason as above: `jira-cloud-v3.json` already
        // says "OpenAPI 3.0.1", so a whole-file search proves nothing.
        assert!(
            row.contains("OpenAPI 3"),
            "the teamcity.json row does not record that the document is OpenAPI 3: {row:?}. \
             The gate resolves `#/components/schemas/...` on that basis; a reader who assumes \
             Swagger 2.0 will look for `definitions` and find nothing."
        );
        // Not row-scoped: this is a paragraph-length caveat, and the phrase
        // appears nowhere else in the file, so there is nothing to borrow from.
        assert!(
            readme.contains("mixed authority"),
            "README.md no longer records that teamcity.json has mixed authority (server-derived \
             plus hand-augmented from the HTML docs). Without it, a future validation failure \
             against a hand-added constraint gets debugged with the wrong prior — as a mockd bug \
             rather than as a possibly mis-transcribed constraint."
        );
        return;
    }
    assert!(
        readme.contains("TeamCity blocker"),
        "teamcity.json is absent and README.md no longer records why. A skipped fidelity gate \
         with no written reason is how a gate stays off forever."
    );
}
