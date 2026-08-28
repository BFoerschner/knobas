//! The registry is P6's landing site: "config + secret ⇒ `Box<dyn Source>`",
//! in one place, so the scheduler and `test_source` build adapters the same way
//! and `knobas-sync` never links an adapter crate.

use knobas_app::sources::Registry;
use knobas_source::AuthMethod;
use knobas_source::instance::SourceInstance;
use knobas_sync::scheduler::AdapterRegistry;

/// A configured instance with a credential.
///
/// The secret is present because a real adapter refuses to build without one --
/// Jira's `build` returns `Unauthorized` for `secret: None`, deliberately, so a
/// scheduled run never discovers a missing credential mid-sync. Nothing here
/// reaches a network: `build` only constructs.
fn instance(instance_id: &str, kind: &str) -> SourceInstance {
    SourceInstance {
        id: instance_id.to_owned(),
        kind: kind.to_owned(),
        display_name: "X".to_owned(),
        base_url: "https://example.invalid".to_owned(),
        auth: Some(AuthMethod::Pat),
        secret: Some("not-a-real-token".to_owned()),
        config: serde_json::json!({}),
    }
}

/// The one adapter exempt from the read-only rule below.
///
/// Not a network source at all: the mock is the SPI's reference implementation
/// and its `write_ops` are what the contract battery's write clauses run
/// against. Named as a single literal rather than kept in a growing list, so
/// the first *real* adapter that declares a write fails the test.
const REFERENCE_ADAPTER: &str = "mock";

/// `list_adapters` returns one **template** per compiled-in kind, with
/// `id == adapter_kind` -- the Add-source form is generated from
/// `config_schema` + `auth_methods`, and the launcher reads kind metadata, with
/// nothing instantiated and no keychain touched (interfaces §2.2).
#[test]
fn every_template_names_its_own_kind_and_declares_a_config_schema() {
    let templates = Registry::builtin().descriptors();
    assert!(!templates.is_empty());
    for t in &templates {
        assert_eq!(t.id, t.adapter_kind, "a template's id is its kind");
        assert!(!t.name.is_empty(), "{} has no product name", t.adapter_kind);
        assert_eq!(
            t.config_schema["type"], "object",
            "{} has no object schema",
            t.adapter_kind
        );
        assert!(
            !t.entity_kinds.is_empty(),
            "{} declares no kinds",
            t.adapter_kind
        );
        for kind in &t.entity_kinds {
            assert_eq!(
                kind.monogram.chars().count(),
                2,
                "{:?} monogram must be two characters",
                kind.id
            );
        }
    }
    let mut kinds: Vec<_> = templates.iter().map(|t| t.adapter_kind.as_str()).collect();
    kinds.sort_unstable();
    let before = kinds.len();
    kinds.dedup();
    assert_eq!(
        kinds.len(),
        before,
        "two adapters claim the same kind: {kinds:?}"
    );
}

/// The adapter crates `knobas-app`'s own manifest links into the binary.
///
/// Read from `Cargo.toml` rather than listed here, because a listed one is a
/// list somebody has to remember to extend -- and the test below is named for
/// a property only the manifest can witness. Scoped to `[dependencies]`: a
/// dev-dependency is not linked into the app, and `knobas-source` is the SPI
/// rather than an adapter, which the trailing `-` excludes.
fn linked_adapter_crates() -> Vec<&'static str> {
    const MANIFEST: &str = include_str!("../Cargo.toml");
    MANIFEST
        .lines()
        .skip_while(|line| line.trim() != "[dependencies]")
        .skip(1)
        .take_while(|line| !line.trim_start().starts_with('['))
        .filter_map(|line| line.split_once('=').map(|(name, _)| name.trim()))
        .filter(|name| name.starts_with("knobas-source-"))
        .collect()
}

/// Every adapter compiled into the binary is reachable through the table. A
/// crate that is a dependency but has no row is a source the Add-source form
/// never offers -- which is the failure mode of an append-only table that
/// somebody forgot to append to.
///
/// **Two oracles, because the named one cannot see the case that actually
/// happened.** The loop is the strong check for the three crates it names: it
/// calls each crate's own `descriptor_template`, so the kind it demands is the
/// adapter's spelling rather than a string repeated here. But it can only look
/// for names somebody wrote into it, and TeamCity went missing for a whole
/// milestone underneath a green run of this test -- the crate was not a
/// dependency, so the premise was false and the check was true and empty at
/// once. Adding the dependency fixes that instance and leaves the mechanism:
/// the *next* adapter can be linked with no row and no line here, and pass.
///
/// So the count is asserted against the manifest, which is the one document
/// that cannot be out of date about what is linked. Equal counts mean one row
/// per crate rather than merely the right total, because
/// `every_template_names_its_own_kind_and_declares_a_config_schema` already
/// rejects two rows claiming one kind.
#[test]
fn every_adapter_crate_linked_into_the_app_has_a_row() {
    let kinds: Vec<String> = Registry::builtin()
        .descriptors()
        .into_iter()
        .map(|t| t.adapter_kind)
        .collect();
    for expected in [
        knobas_source_mock::descriptor_template().adapter_kind,
        knobas_source_jira::descriptor_template().adapter_kind,
        knobas_source_teamcity::descriptor_template().adapter_kind,
        knobas_source_gitea::descriptor_template().adapter_kind,
    ] {
        assert!(
            kinds.contains(&expected),
            "{expected:?} is linked into knobas-app but has no registry row: {kinds:?}"
        );
    }

    let linked = linked_adapter_crates();
    let (crates, rows) = (linked.len(), kinds.len());
    assert_eq!(
        crates, rows,
        "knobas-app links {crates} adapter crates {linked:?} but the table has \
         {rows} rows {kinds:?}; a crate that is a dependency with no row is a \
         source the Add-source form never offers"
    );
}

/// M1 is read-only toward every source (interfaces §4.1). The battery already
/// enforces `Capability::Write` ⇔ non-empty `write_ops` per adapter; this
/// asserts the milestone-wide rule across the whole table at once.
#[test]
fn no_real_adapter_declares_a_write() {
    let mut checked = 0_usize;
    for t in Registry::builtin().descriptors() {
        if t.adapter_kind == REFERENCE_ADAPTER {
            // The exemption is asserted, not assumed: if the mock ever stopped
            // declaring a write, this test would silently become a check of
            // nothing while still naming an exception.
            assert!(
                !t.write_ops.is_empty(),
                "the reference adapter is exempt because it exercises the write \
                 battery; it no longer declares a write, so the exemption is stale"
            );
            continue;
        }
        checked += 1;
        assert!(
            t.write_ops.is_empty(),
            "{} declares write ops in a read-only milestone",
            t.adapter_kind
        );
        assert!(
            !t.capabilities.contains(&knobas_source::Capability::Write),
            "{} declares Capability::Write",
            t.adapter_kind
        );
    }
    assert!(
        checked >= 1,
        "no real adapter was checked, so this proves nothing"
    );
}

#[test]
fn building_an_instance_of_a_known_kind_yields_an_adapter_under_that_instances_id() {
    let registry = Registry::builtin();
    for template in registry.descriptors() {
        let built = registry
            .build(instance("my-instance", &template.adapter_kind))
            .unwrap_or_else(|e| panic!("{} should build: {e}", template.adapter_kind));
        assert_eq!(
            built.descriptor().id,
            "my-instance",
            "the instance id, not the adapter kind, is the entity namespace (P10)"
        );
        assert_eq!(built.descriptor().adapter_kind, template.adapter_kind);
    }
}

#[test]
fn an_unknown_kind_is_refused_by_name() {
    // `Box<dyn Source>` is not `Debug`, so the success arm is matched rather
    // than unwrapped.
    match Registry::builtin().build(instance("whatever", "nope")) {
        Err(knobas_source::SourceError::Protocol { message, .. }) => {
            assert!(message.contains("nope"), "{message}");
        }
        Err(other) => panic!("expected a Protocol error naming the kind, got {other:?}"),
        Ok(_) => panic!("an unknown kind must not build"),
    }
}

/// The routing key is `SourceInstance.kind`, not the instance id. Two Jiras are
/// `jira` and `jira-eu`, both of kind `jira` -- a registry keyed on the id
/// would build the first and fail on the second.
#[test]
fn the_registry_routes_on_the_kind_and_not_on_the_instance_id() {
    let registry = Registry::builtin();
    let built = registry
        .build(instance("jira-eu", "jira"))
        .expect("a second instance of a known kind builds");
    assert_eq!(built.descriptor().id, "jira-eu");
    assert_eq!(built.descriptor().adapter_kind, "jira");

    // ...and an instance id that happens to *be* a kind name is not what routes.
    assert!(
        registry
            .build(instance("jira", "not-a-compiled-in-kind"))
            .is_err(),
        "an unknown kind must be refused however the instance is named"
    );
}
