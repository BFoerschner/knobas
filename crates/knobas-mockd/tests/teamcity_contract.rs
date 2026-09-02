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
//! [`the_teamcity_swagger_blocker_is_recorded`] fails if the README's row and
//! the disk ever disagree — genuinely in both directions: a row saying
//! "BLOCKED" beside a present file, *and* a row advertising a vendored document
//! that is not there. The second half is the one that matters for a skip: it is
//! what stops the schema gate quietly switching itself off again.
//!
//! ## What this gate does *not* prove
//!
//! These schemas carry **no `required` arrays and no `additionalProperties:
//! false`** anywhere. So a green run means "nothing mockd serves has the wrong
//! type", not "mockd serves everything it should" — a body could drop half its
//! fields and still conform. The goldens are what pin presence; the schemas pin
//! shape. Neither half is redundant, and neither should be read as the other.

mod common;

use common::{golden, normalise_base_url};
use knobas_mockd::spawn_mock_teamcity;

/// One endpoint, in one table used by both halves of the gate, so an endpoint
/// cannot be added to one and missed in the other.
struct Served {
    /// What mockd is asked for.
    request: &'static str,
    /// The golden snapshot pinning the body.
    golden: &'static str,
    /// The `components.schemas` entry the body is validated against. Lowercase,
    /// because OpenAPI 3 TeamCity names its schemas that way (`build`, not
    /// `Build`) and the pointers are case-sensitive.
    schema: &'static str,
    /// The OpenAPI path template whose 200 response *declares* `schema`.
    ///
    /// This field is what makes `schema` checkable rather than merely
    /// plausible. `schema` alone is a hand-written name, and validating a body
    /// against the *wrong but existing* schema is silent: these schemas carry
    /// no `required` and no `additionalProperties: false`, so an unrelated
    /// schema constrains almost nothing and the gate stays green. Mapping
    /// `/app/rest/builds` to `buildTypes` — a one-word slip between adjacent
    /// names in a document that also has `build` and `buildType` — passed all
    /// six tests before this field existed.
    ///
    /// Two tests close that: [`every_served_schema_is_the_one_the_document_declares`]
    /// checks `schema` against this template's declared 200 response, and
    /// [`every_served_request_matches_its_path_template`] checks this template
    /// against `request`, so a wrong `declared_by` cannot be used to launder a
    /// wrong `schema`.
    declared_by: &'static str,
}

const SERVED: &[Served] = &[
    Served {
        request: "/app/rest/server",
        golden: "server",
        schema: "server",
        declared_by: "/app/rest/server",
    },
    Served {
        request: "/app/rest/users/current?fields=$long",
        golden: "users_current",
        schema: "user",
        // The only one whose request path is not itself a key in the document.
        // `current` is a *userLocator value*, not a distinct endpoint — TeamCity
        // documents the shape under the templated path — so the document still
        // supplies the ground truth here; only the substitution is ours.
        declared_by: "/app/rest/users/{userLocator}",
    },
    Served {
        request: "/app/rest/buildTypes?fields=$long",
        golden: "build_types",
        schema: "buildTypes",
        declared_by: "/app/rest/buildTypes",
    },
    Served {
        // `defaultFilter:false`: the default filter narrows a listing to the
        // default branch (issue #266), and two of the fixture's three builds
        // ran on a feature branch. This is the request that snapshots all of
        // them.
        request: "/app/rest/builds?locator=defaultFilter:false,count:100&fields=$long",
        golden: "builds_all",
        schema: "builds",
        declared_by: "/app/rest/builds",
    },
    Served {
        request: "/app/rest/builds/id:1187?fields=$long",
        golden: "build_1187",
        schema: "build",
        declared_by: "/app/rest/builds/{buildLocator}",
    },
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
    for e in SERVED {
        let mut body = tc(&b, e.request).await;
        normalise_base_url(&mut body, &b);
        golden("teamcity", e.golden, &body);
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
/// a gate should not have its strictness decided by inference.
///
/// What the exemption actually costs, measured over the **143-schema closure
/// reachable from the five `SERVED` roots** (not the whole document — the gate
/// never validates against the other 95):
///
/// | format | occurrences in the closure | effect of asserting it |
/// |---|---|---|
/// | `int32` | 101 | none — an OpenAPI numeric hint JSON Schema ignores either way |
/// | `int64` | 11 | none, same |
/// | `date-time` | 1 | the only real assertion given up |
///
/// So for 112 of the 113 this flag changes nothing at all. The one that matters
/// is `DeploymentStateEntry.changeDate`, reachable from the `buildTypes` root at
/// `$ref` depth 9 (`buildTypes → buildType → project → deploymentDashboards →
/// … → DeploymentHistory → DeploymentStateEntry`).
///
/// Asserting it would mean enforcing a label **TeamCity's own document
/// contradicts**. The document's other two `date-time` fields carry examples,
/// and those examples are ISO-8601 *basic* (`token.creationTime`:
/// `20250905T001122+0200`), which is not RFC 3339 — so a faithful mock of
/// TeamCity emitting a `changeDate` would fail an RFC 3339 assertion for being
/// faithful. [`teamcity_contradicts_its_own_date_time_label`] pins that
/// evidence, and
/// [`the_format_exemption_covers_only_hints_and_a_contradicted_date_time`] pins
/// the closure's format set, so a future `SERVED` entry that drags a new format
/// into scope forces this reasoning to be re-read instead of silently
/// inheriting the exemption.
///
/// **Correction, recorded because the wrong version was published first:** an
/// earlier draft of this comment claimed the locator-grammar overloading
/// (`BuildLocator`, `"String value"`) was the reason. That overloading is real —
/// 37 such values document-wide — but **zero of them occur in the reachable
/// closure**, so it is not a reason this gate needs the exemption. The same
/// draft claimed `double` was reachable (it occurs once, document-wide, outside
/// the closure) and that no `date-time` was reachable (one is).
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
        .map(|e| e.schema)
        .filter(|s| !schemas.contains_key(*s))
        .collect();
    assert!(
        missing.is_empty(),
        "SERVED names absent from components.schemas: {missing:?}. TeamCity's OpenAPI 3 \
         document uses lowercase schema names; check the case before renaming anything."
    );
}

/// The `$ref` the document declares for a path's `200 application/json`
/// response, i.e. what TeamCity itself says that endpoint returns.
fn declared_200_schema(doc: &serde_json::Value, template: &str) -> Option<String> {
    doc["paths"][template]["get"]["responses"]["200"]["content"]["application/json"]["schema"]
        ["$ref"]
        .as_str()
        .map(str::to_owned)
}

/// **The mapping is derived from the document, not asserted by hand.**
///
/// `every_served_schema_name_exists` only proves a name *exists*. That leaves
/// the more likely mistake uncaught: naming a schema that exists but belongs to
/// a different endpoint. With no `required` and no `additionalProperties:
/// false` anywhere in these schemas, an unrelated schema constrains almost
/// nothing, so the gate reports a green zero while checking nothing — measured,
/// not feared: mapping `/app/rest/builds` to `buildTypes` passed all six tests
/// before this existed.
///
/// TeamCity declares the answer for every one of the five, so it is checked
/// rather than trusted.
#[test]
fn every_served_schema_is_the_one_the_document_declares() {
    let Some(doc) = vendored_spec("every_served_schema_is_the_one_the_document_declares") else {
        return;
    };
    let mut wrong = Vec::new();
    for e in SERVED {
        let want = format!("#/components/schemas/{}", e.schema);
        match declared_200_schema(&doc, e.declared_by) {
            Some(got) if got == want => {}
            Some(got) => wrong.push(format!(
                "  {}: table says {want}, but {} declares {got}",
                e.request, e.declared_by
            )),
            None => wrong.push(format!(
                "  {}: {} declares no 200 application/json schema. Every SERVED entry must \
                 point at a path template that does — that is what makes the mapping checkable.",
                e.request, e.declared_by
            )),
        }
    }
    assert!(
        wrong.is_empty(),
        "SERVED disagrees with the vendored document about what these endpoints return:\n{}",
        wrong.join("\n")
    );
}

/// The other half of the lock. [`every_served_schema_is_the_one_the_document_declares`]
/// derives `schema` from `declared_by` — but `declared_by` is itself
/// hand-written, so on its own it could be edited to point at whichever path
/// declares the wrong schema someone wanted. This checks `declared_by` back
/// against `request`: same segment count, and each segment either identical or
/// a `{placeholder}` the request substitutes.
///
/// Together the two mean a wrong schema name cannot be laundered — it would
/// need a `declared_by` that both declares that schema *and* structurally
/// matches the request path.
#[test]
fn every_served_request_matches_its_path_template() {
    let mut wrong = Vec::new();
    for e in SERVED {
        let path = e.request.split('?').next().unwrap_or(e.request);
        let req: Vec<&str> = path.split('/').collect();
        let tmpl: Vec<&str> = e.declared_by.split('/').collect();
        let matches = req.len() == tmpl.len()
            && req.iter().zip(&tmpl).all(|(r, t)| {
                *r == *t || (t.starts_with('{') && t.ends_with('}') && !r.is_empty())
            });
        if !matches {
            wrong.push(format!(
                "  {path} is not an instance of {} — declared_by must be the template the \
                 request path fills in, or it is not evidence about this endpoint",
                e.declared_by
            ));
        }
    }
    assert!(
        wrong.is_empty(),
        "SERVED request paths do not match their declared_by templates:\n{}",
        wrong.join("\n")
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
    for e in SERVED {
        let body = tc(&b, e.request).await;
        let validator = schema_for(&doc, e.schema);
        let errs: Vec<String> = validator
            .iter_errors(&body)
            .map(|err| format!("    {} at {}", err, err.instance_path()))
            .collect();
        if !errs.is_empty() {
            report.push(format!(
                "  {} violates #/components/schemas/{}:\n{}",
                e.request,
                e.schema,
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

/// Every schema name reachable from `roots` by following
/// `#/components/schemas/...` references — i.e. exactly the part of the
/// document the gate can ever validate against.
///
/// The distinction is not pedantic. A property measured over all 238 schemas
/// says nothing about the ~143 that matter, and an earlier version of the
/// format pin below measured the document and claimed to be guarding the
/// closure. It could not have observed the thing it named.
fn reachable_closure(
    schemas: &serde_json::Value,
    roots: &[&str],
) -> std::collections::BTreeSet<String> {
    fn refs(v: &serde_json::Value, out: &mut Vec<String>) {
        match v {
            serde_json::Value::Object(map) => {
                if let Some(r) = map.get("$ref").and_then(serde_json::Value::as_str)
                    && let Some(name) = r.strip_prefix("#/components/schemas/")
                {
                    out.push(name.to_owned());
                }
                map.values().for_each(|c| refs(c, out));
            }
            serde_json::Value::Array(a) => a.iter().for_each(|c| refs(c, out)),
            _ => {}
        }
    }
    let mut seen: std::collections::BTreeSet<String> =
        roots.iter().map(|r| (*r).to_owned()).collect();
    let mut stack: Vec<String> = seen.iter().cloned().collect();
    while let Some(n) = stack.pop() {
        let mut found = Vec::new();
        refs(&schemas[&n], &mut found);
        for m in found {
            if seen.insert(m.clone()) {
                stack.push(m);
            }
        }
    }
    seen
}

/// Collect `format` values with their occurrence counts across a set of schemas.
fn formats_in(
    schemas: &serde_json::Value,
    names: &std::collections::BTreeSet<String>,
) -> std::collections::BTreeMap<String, usize> {
    fn walk(v: &serde_json::Value, out: &mut std::collections::BTreeMap<String, usize>) {
        match v {
            serde_json::Value::Object(map) => {
                if let Some(f) = map.get("format").and_then(serde_json::Value::as_str) {
                    *out.entry(f.to_owned()).or_default() += 1;
                }
                map.values().for_each(|c| walk(c, out));
            }
            serde_json::Value::Array(a) => a.iter().for_each(|c| walk(c, out)),
            _ => {}
        }
    }
    let mut out = std::collections::BTreeMap::new();
    for n in names {
        walk(&schemas[n], &mut out);
    }
    out
}

/// Pins the *scope* of `schema_for`'s format exemption, over the closure the
/// gate actually validates against rather than over the document.
///
/// Today that closure carries `int32` and `int64` — OpenAPI numeric hints JSON
/// Schema ignores whether or not assertion is on — plus exactly one
/// `date-time`. If a new `SERVED` entry or a re-fetch drags any other format
/// into scope, this fails and the exemption has to be re-argued rather than
/// silently extended to cover it.
#[test]
fn the_format_exemption_covers_only_hints_and_a_contradicted_date_time() {
    let Some(doc) =
        vendored_spec("the_format_exemption_covers_only_hints_and_a_contradicted_date_time")
    else {
        return;
    };
    let schemas = &doc["components"]["schemas"];
    let roots: Vec<&str> = SERVED.iter().map(|e| e.schema).collect();
    let closure = reachable_closure(schemas, &roots);
    let counts = formats_in(schemas, &closure);
    let found: std::collections::BTreeSet<&str> = counts.keys().map(String::as_str).collect();
    let expected: std::collections::BTreeSet<&str> =
        ["int32", "int64", "date-time"].into_iter().collect();
    assert_eq!(
        found,
        expected,
        "the formats reachable from the SERVED roots have changed ({} schemas in the closure). \
         `schema_for` disables format assertion on the strength of this exact set: two numeric \
         hints that assert nothing, and one `date-time` whose label TeamCity's own document \
         contradicts. A new format here is outside that argument — re-read `schema_for` before \
         letting the exemption cover it.",
        closure.len()
    );
}

/// The evidence for the exemption, kept executable so it cannot outlive its
/// reason: TeamCity labels fields `format: date-time` and then gives examples
/// that are ISO-8601 **basic**, not RFC 3339. Asserting the label would fail a
/// mock for faithfully reproducing the server.
///
/// Checked on `token`, which carries examples. The one `date-time` inside the
/// served closure (`DeploymentStateEntry.changeDate`) has no example of its
/// own, so the document's own contradiction elsewhere is the evidence for what
/// it would contain.
#[test]
fn teamcity_contradicts_its_own_date_time_label() {
    let Some(doc) = vendored_spec("teamcity_contradicts_its_own_date_time_label") else {
        return;
    };
    for field in ["creationTime", "expirationTime"] {
        let node = &doc["components"]["schemas"]["token"]["properties"][field];
        assert_eq!(
            node["format"], "date-time",
            "token.{field} is no longer labelled date-time"
        );
        let example = node["example"].as_str().unwrap_or_default();
        // RFC 3339 would be `2025-09-05T00:11:22+02:00`. TeamCity emits
        // `20250905T001122+0200`: eight leading digits, no separators.
        let basic = example.len() > 8
            && example[..8].chars().all(|c| c.is_ascii_digit())
            && example.as_bytes().get(8) == Some(&b'T');
        assert!(
            basic,
            "token.{field}'s example is {example:?}, which no longer looks like ISO-8601 basic. \
             That contradiction is why `schema_for` disables format assertion; if TeamCity now \
             emits RFC 3339 under its `date-time` labels, re-check whether the exemption is \
             still warranted."
        );
    }
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
    // Its *own* row, not just the word somewhere in the file: every other
    // vendored document already says "vendored", so a whole-file search would
    // pass without anyone touching the TeamCity row at all.
    let row = readme
        .lines()
        .find(|l| l.starts_with("| `teamcity.json` |"))
        .expect("README.md must have a `teamcity.json` row");
    // Read negatively, not positively: the blocked spelling is "**BLOCKED, not
    // vendored**", so a `contains("vendored")` check would pass on the very text
    // it is supposed to reject.
    let row_claims_vendored = !row.contains("BLOCKED") && !row.contains("not vendored");

    if spec_path().exists() {
        assert!(
            row_claims_vendored,
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

    // The absent direction. It used to assert that the README still explained
    // the *blocker*, which stopped discriminating the moment the blocker became
    // history: the phrase was gone, so the branch fired unconditionally and
    // accused the README of a fault that was really the missing file.
    //
    // What it has to check now is the mirror of the branch above — the row and
    // the disk must agree. A row still advertising a vendored document that is
    // not there sends the next reader looking for a file, and silently turns the
    // schema half back off.
    assert!(
        !row_claims_vendored,
        "teamcity.json is ABSENT, but its README row still advertises it as vendored: {row:?}. \
         The schema half of this gate is now skipping. Either restore the document (it is \
         pinned in SHA256SUMS) or flip the row back to `**BLOCKED, not vendored**` with the \
         reason — a skipped fidelity gate with nothing in the tree saying why is how a gate \
         stays off forever."
    );
}
