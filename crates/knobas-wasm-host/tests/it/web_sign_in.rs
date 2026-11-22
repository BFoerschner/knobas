//! The descriptor's `web_sign_in` across the WIT boundary (ADR-0050, ux29): a
//! 1.6.0 component's sign-in page reaches the host, and an older one's is
//! dropped, so its site offers *Fill sign-in* alone.

use knobas_source::web_sign_in::WebSignInPages;
use knobas_wasm_host::WasmAdapter;

#[test]
fn a_1_6_component_declares_its_sign_in_page() {
    let adapter = WasmAdapter::load(&knobas_wasm_host::fixture_component()).expect("1.6.0 loads");
    assert!(adapter.declares_web_sign_in());
    assert_eq!(
        adapter.template().web_sign_in,
        Some(WebSignInPages::at(&["/login"]))
    );
}

#[test]
fn an_older_component_has_its_sign_in_page_dropped() {
    for (path, version) in [
        (knobas_wasm_host::fixture_component_v1_5(), "1.5.0"),
        (knobas_wasm_host::fixture_component_v1_3(), "1.3.0"),
    ] {
        let adapter = WasmAdapter::load(&path).unwrap_or_else(|e| panic!("{version} loads: {e}"));
        assert!(!adapter.declares_web_sign_in(), "{version}");
        assert_eq!(
            adapter.template().web_sign_in,
            None,
            "{version}: its JSON declares one"
        );
    }
}
