//! The descriptor's `web_sign_in` and `web_links` across the WIT boundary
//! (ADR-0050, ux30): a 1.7.0 component's sign-in page and links reach the
//! host, and an older one's are dropped, so its site offers *Fill sign-in*
//! alone and opens at its base URL.

use knobas_source::web_links::{self, WebLinks};
use knobas_source::web_sign_in::WebSignInPages;
use knobas_wasm_host::WasmAdapter;

#[test]
fn a_1_7_component_declares_its_sign_in_page() {
    let adapter = WasmAdapter::load(&knobas_wasm_host::fixture_component()).expect("1.7.0 loads");
    assert!(adapter.declares_web_sign_in());
    assert_eq!(
        adapter.template().web_sign_in,
        Some(WebSignInPages::at(&["/login"]).with_credential())
    );
}

/// The links an external adapter declares are read and build the addresses
/// a built-in's do, from the typed base URL.
#[test]
fn a_1_7_component_declares_its_web_links() {
    let adapter = WasmAdapter::load(&knobas_wasm_host::fixture_component()).expect("1.7.0 loads");
    assert!(adapter.declares_web_links());
    let links: WebLinks = adapter.template().web_links.clone().expect("declared");
    assert_eq!(links.home.as_deref(), Some("/#/desk"));
    let survey = links.page("survey").expect("a survey's page");
    assert_eq!(
        web_links::page("https://desk.example/", survey, "SV-1").as_deref(),
        Some("https://desk.example/#/desk?survey=SV-1")
    );
    assert_eq!(
        web_links::home("https://desk.example/", Some(&links)),
        "https://desk.example/#/desk"
    );
}

#[test]
fn an_older_component_has_its_sign_in_page_dropped() {
    for (path, version) in [
        (knobas_wasm_host::fixture_component_v1_6(), "1.6.0"),
        (knobas_wasm_host::fixture_component_v1_5(), "1.5.0"),
        (knobas_wasm_host::fixture_component_v1_3(), "1.3.0"),
    ] {
        let adapter = WasmAdapter::load(&path).unwrap_or_else(|e| panic!("{version} loads: {e}"));
        assert!(!adapter.declares_web_sign_in(), "{version}");
        assert!(!adapter.declares_web_links(), "{version}");
        assert_eq!(
            adapter.template().web_links,
            None,
            "{version}: its JSON declares them"
        );
        assert_eq!(
            adapter.template().web_sign_in,
            None,
            "{version}: its JSON declares one"
        );
    }
}
