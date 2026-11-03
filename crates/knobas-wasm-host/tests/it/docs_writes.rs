//! The docs writes cross the boundary (#2443): a component built against
//! `knobas:adapter@1.5.0` makes a section edit and a new page's body in its
//! own markup, refuses a section for each reason a built-in adapter does,
//! and makes them under the projection's terms: no way out, its deadline.

use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use knobas_source::docs::{EditRefused, SectionEdit, SectionRefusal};
use knobas_wasm_host::limits::Limits;
use knobas_wasm_host::load_fixture;
use serde_json::{Value, json};

use crate::docs::{counting_server, instance};

/// A page payload whose body is `body`, at version 3, as the fixture emits
/// one.
fn payload(body: &str) -> Value {
    json!({ "key": "page-x", "space": { "key": "HARB" }, "version": 3, "body": body })
}

/// One section the editor edits, then one for each reason a section is
/// refused, in [`SectionRefusal`]'s order.
const SECTIONS: &str = "<h2>Pilots</h2>\n<p>Board at buoy 3.</p>\n<ul><li>Ladder</li><li>Lamp</li></ul>\n\
<h2>Berths</h2><table><tr><td>7</td></tr></table>\
<h2>Radio</h2><pre>VHF 12</pre>\
<h2>Rules</h2><p>See <hb:page title=\"Berthing rules\"></hb:page>.</p>\
<h2>Tides</h2><p>Spring tides run <strong>fast</strong>.</p>";

/// The section edit is the whole body: the edited section's body in the
/// component's markup, every byte around it as it was, against the version
/// the payload holds.
#[test]
fn a_section_edit_is_the_whole_body_the_component_made() {
    let adapter = load_fixture();
    let edit = adapter.edit_section(
        "page",
        &payload(SECTIONS),
        1,
        "Board at buoy 4.\n\n- Ladder\n- Line",
    );
    let rest = &SECTIONS[SECTIONS.find("<h2>Berths").unwrap()..];
    assert_eq!(
        edit,
        Some(Ok(SectionEdit {
            body: format!(
                "<h2>Pilots</h2><p>Board at buoy 4.</p><ul><li>Ladder</li><li>Line</li></ul>{rest}"
            ),
            base_version: 3,
        }))
    );
}

/// The projection names each section with the text the editor opens with or
/// its refusal, and the edit refuses the same section for the same reason.
/// No such section, a payload with no version and a kind with no pages are
/// missing.
#[test]
fn each_refused_section_is_refused_by_the_edit_for_its_reason() {
    let adapter = load_fixture();
    let page = payload(SECTIONS);
    let sections = adapter.project("page", &page).unwrap().sections;
    let texts: Vec<_> = sections
        .iter()
        .map(|s| (s.number, s.text.as_deref(), s.refusal))
        .collect();
    assert_eq!(
        texts,
        [
            (1, Some("Board at buoy 3.\n\n- Ladder\n- Lamp"), None),
            (2, None, Some(SectionRefusal::Table)),
            (3, None, Some(SectionRefusal::Code)),
            (4, None, Some(SectionRefusal::Macro)),
            (5, None, Some(SectionRefusal::Formatting)),
        ]
    );
    for (number, refusal) in [
        (2, SectionRefusal::Table),
        (3, SectionRefusal::Code),
        (4, SectionRefusal::Macro),
        (5, SectionRefusal::Formatting),
    ] {
        assert_eq!(
            adapter.edit_section("page", &page, number, "x"),
            Some(Err(EditRefused::Section(refusal))),
            "section {number}"
        );
    }
    let mut unversioned = page.clone();
    unversioned.as_object_mut().unwrap().remove("version");
    for (kind, payload, number) in [
        ("page", &page, 6),
        ("page", &page, 0),
        ("page", &unversioned, 1),
        ("survey", &page, 1),
    ] {
        assert_eq!(
            adapter.edit_section(kind, payload, number, "x"),
            Some(Err(EditRefused::Missing)),
            "{kind} section {number}"
        );
    }
}

/// A new page's body in the component's markup: `## ` a section, `- ` a
/// list item, a blank line a paragraph.
#[test]
fn a_new_page_s_body_is_the_component_s_markup_of_the_text() {
    assert_eq!(
        load_fixture().body_from_text("Intro\nthat runs on\n\n## Steps\n- restart\n- a < b"),
        Some(
            "<p>Intro that runs on</p><h2>Steps</h2><ul><li>restart</li><li>a &lt; b</li></ul>"
                .to_owned()
        )
    );
}

/// A section edit that traps or runs past the projection's deadline makes
/// nothing, within its limit, and the next edit is made as before.
#[test]
fn a_section_edit_that_traps_or_spins_makes_nothing() {
    let adapter = load_fixture().with_limits(Limits {
        deadline: Duration::from_secs(1),
        memory_mib: 32,
    });
    for key in ["trap", "spin"] {
        let mut asks = payload(SECTIONS);
        asks[key] = true.into();
        let started = Instant::now();
        assert_eq!(adapter.edit_section("page", &asks, 1, "x"), None, "{key}");
        assert!(started.elapsed() < Duration::from_secs(10), "{key}");
    }
    assert!(matches!(
        adapter.edit_section("page", &payload(SECTIONS), 1, "x"),
        Some(Ok(_))
    ));
}

/// The section edit has no HTTP capability, as the projection has none: an
/// edit that sends a request to a host the adapter's manifest declares, and
/// which the adapter's own calls reach, is refused before it leaves, so it
/// traps and makes nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_section_edit_reaches_no_host_even_one_its_manifest_declares() {
    let (address, accepted) = counting_server().await;
    let adapter = load_fixture().with_hosts(vec![
        knobas_wasm_host::allow::HostPattern::parse(&address).unwrap(),
    ]);
    let url = format!("http://{address}/page");
    // The adapter's own call reaches the host: the direction a refusal
    // could otherwise hide.
    let mut fetching = instance();
    fetching.config = json!({ "fetch": url });
    adapter
        .build(fetching)
        .unwrap()
        .test_connection()
        .await
        .expect("the declared host answers the adapter's own call");
    assert_eq!(accepted.load(Ordering::SeqCst), 1);

    let mut asks = payload(SECTIONS);
    asks["fetch"] = url.into();
    let editing = adapter.clone();
    let edit = tokio::task::spawn_blocking(move || editing.edit_section("page", &asks, 1, "x"))
        .await
        .unwrap();
    assert_eq!(edit, None, "the refused request trapped the edit");
    assert_eq!(
        accepted.load(Ordering::SeqCst),
        1,
        "nothing reached the host"
    );
}
