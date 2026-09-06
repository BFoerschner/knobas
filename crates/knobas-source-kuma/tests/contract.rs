//! The shared battery every adapter must pass (`knobas_source::contract`), plus
//! the sync behaviours only a *changing* server can show.
//!
//! Run here against the docker-free recording so `just check` exercises it on
//! every commit; `tests/live_kuma.rs` runs against the real seeded container,
//! which is what the acceptance criteria are measured against.

mod support;

use knobas_source::contract::{Fault, VecSink, battery};
use support::{Fake, KEY, METRICS, adapter, dead_url};

#[tokio::test]
async fn passes_the_contract_battery() {
    let fake = Fake::start().await;
    let base_url = fake.base_url();
    battery(move |fault| match fault {
        Fault::None => adapter(base_url.clone(), KEY),
        // A live instance answering a key it does not know, which is the shape
        // a rotated API key actually arrives in.
        Fault::Unauthorized => adapter(base_url.clone(), "uk1_rotated-away"),
        Fault::Unreachable => adapter(dead_url(), KEY),
    })
    .await;
}

/// The eight monitors `testenv/monitors.json` names, as the mirror holds them.
///
/// The battery checks that a full sync yields *something* well-formed; this
/// checks that it yields the estate -- one item per monitor, keyed on Kuma's
/// own id, with the fields spec #427 asks a monitor to carry.
#[tokio::test]
async fn a_full_sync_mirrors_every_monitor_with_its_state() {
    let fake = Fake::start().await;
    let source = adapter(fake.base_url(), KEY);
    let mut sink = VecSink(Vec::new());
    source.sync(None, &mut sink).await.unwrap();

    let names: Vec<&str> = sink.0.iter().map(|i| i.title.as_str()).collect();
    assert_eq!(
        names.len(),
        8,
        "the recording holds the eight seeded monitors: {names:?}"
    );
    for expected in [
        "knobas-teamcity",
        "knobas-jira",
        "knobas-confluence",
        "teamcity (tunnel)",
        "jira (tunnel)",
        "confluence (tunnel)",
        "gitea",
        "canary",
    ] {
        assert!(names.contains(&expected), "{expected:?} is missing: {names:?}");
    }

    let canary = sink
        .0
        .iter()
        .find(|i| i.title == "canary")
        .expect("the canary is in the recording");
    assert_eq!(canary.entity.to_string(), "kuma:8");
    assert_eq!(canary.kind, "monitor");
    assert_eq!(canary.payload["state"], "up");
    assert_eq!(canary.payload["type"], "http");
    assert_eq!(canary.payload["url"], "http://host.docker.internal:8299/");
    assert!(canary.payload["response_time_ms"].is_number());
    assert!(canary.payload["uptime"]["1d"].is_number());
    assert!(canary.web_url.as_deref().unwrap().ends_with("/dashboard/8"));

    // A ping monitor: addressed by hostname, with no URL at all. The two are
    // one kind and the payload has both keys either way.
    let ping = sink
        .0
        .iter()
        .find(|i| i.title == "knobas-teamcity")
        .expect("the servers are pinged");
    assert_eq!(ping.payload["type"], "ping");
    assert_eq!(ping.payload["url"], serde_json::Value::Null);
    assert_eq!(ping.payload["hostname"], "46.224.117.158");

    // A monitor whose last check did not answer: `-1` is a sentinel and the
    // mirror carries it as no response time, never as minus one millisecond.
    let down = sink
        .0
        .iter()
        .find(|i| i.title == "jira (tunnel)")
        .expect("the tunnel was down when this was recorded");
    assert_eq!(down.payload["state"], "down");
    assert_eq!(down.payload["response_time_ms"], serde_json::Value::Null);
}

/// **The deleted monitor, at the seam a scheduled poll actually uses.**
///
/// Spec #427's story 54 -- *a monitor deleted in Kuma disappears from the
/// mirror on the next run, so the roster never shows ghosts* -- cannot rest on
/// the engine's sweep, which only fires on a cursor-less run. So the adapter
/// reports the deletion itself, and this is where that is pinned: a second run
/// over a corpus one monitor shorter emits a tombstone carrying the name the
/// first run recorded.
///
/// `just kuma-live` witnesses the same thing against the real Kuma, with a
/// monitor really deleted through the seed's helper.
#[tokio::test]
async fn a_monitor_that_stops_being_published_is_tombstoned() {
    let fake = Fake::start().await;
    let source = adapter(fake.base_url(), KEY);

    let mut first = VecSink(Vec::new());
    let cursor = source.sync(None, &mut first).await.unwrap();
    assert!(first.0.iter().any(|i| i.title == "canary"));

    // The same document with every one of the canary's series gone -- which is
    // exactly what Kuma answers once a monitor is deleted, and (measured) also
    // what it answers while one is paused.
    let without: String = METRICS
        .lines()
        .filter(|line| !line.contains("monitor_id=\"8\""))
        .collect::<Vec<_>>()
        .join("\n");
    let shorter = Fake::serving(&without).await;
    let source = adapter(shorter.base_url(), KEY);

    let mut second = VecSink(Vec::new());
    source.sync(Some(cursor), &mut second).await.unwrap();

    let tombstones: Vec<_> = second.0.iter().filter(|i| i.deleted).collect();
    assert_eq!(tombstones.len(), 1, "{:?}", second.0.len());
    assert_eq!(tombstones[0].entity.to_string(), "kuma:8");
    assert_eq!(
        tombstones[0].title, "canary",
        "a tombstone renders with the name the last run recorded"
    );
    // And the seven that are left are re-emitted: every run that emits at all
    // emits the whole roster.
    assert_eq!(second.0.iter().filter(|i| !i.deleted).count(), 7);
}

/// A change of any kind re-emits the whole roster, and the cursor moves with
/// it. The battery pins the *idle* direction; this pins the other one, which is
/// the direction a stuck cursor would break -- a mirror that goes on showing
/// `up` for a monitor that went down.
#[tokio::test]
async fn a_state_change_re_emits_the_roster_and_moves_the_cursor() {
    let fake = Fake::start().await;
    let source = adapter(fake.base_url(), KEY);
    let mut first = VecSink(Vec::new());
    let cursor = source.sync(None, &mut first).await.unwrap();

    let flipped = METRICS.replace(
        "monitor_status{monitor_id=\"8\",monitor_name=\"canary\",monitor_type=\"http\",\
         monitor_url=\"http://host.docker.internal:8299/\",monitor_hostname=\"null\",\
         monitor_port=\"null\"} 1",
        "monitor_status{monitor_id=\"8\",monitor_name=\"canary\",monitor_type=\"http\",\
         monitor_url=\"http://host.docker.internal:8299/\",monitor_hostname=\"null\",\
         monitor_port=\"null\"} 0",
    );
    assert_ne!(flipped, METRICS, "the recording's canary line moved");

    let changed = Fake::serving(&flipped).await;
    let source = adapter(changed.base_url(), KEY);
    let mut second = VecSink(Vec::new());
    let moved = source.sync(Some(cursor.clone()), &mut second).await.unwrap();

    assert_ne!(moved, cursor, "a change must move the cursor");
    assert_eq!(second.0.len(), 8, "every run that emits at all is full");
    let canary = second.0.iter().find(|i| i.title == "canary").unwrap();
    assert_eq!(canary.payload["state"], "down");
}

/// The credential-health path, end to end at this seam: a key Kuma refuses is
/// `Unauthorized` **carrying the 401**, from both `test_connection` and
/// mid-sync -- which is where a rotated key actually surfaces, since the
/// connection was tested once when the source was added.
#[tokio::test]
async fn a_refused_key_is_unauthorized_and_says_which_status() {
    let fake = Fake::start().await;
    let source = adapter(fake.base_url(), "uk1_rotated-away");

    let tested = source.test_connection().await;
    assert!(
        matches!(
            tested,
            Err(knobas_source::SourceError::Unauthorized { status: Some(401) })
        ),
        "{tested:?}"
    );
    let synced = source.sync(None, &mut VecSink(Vec::new())).await;
    assert!(
        matches!(
            synced,
            Err(knobas_source::SourceError::Unauthorized { status: Some(401) })
        ),
        "{synced:?}"
    );
}

/// What *Test connection* puts on screen: the Kuma that answered, and how much
/// it is watching. No account -- an API key belongs to the instance, not to a
/// person, and `/metrics` never says whose it is.
#[tokio::test]
async fn test_connection_reports_the_version_and_the_roster_size() {
    let fake = Fake::start().await;
    let info = adapter(fake.base_url(), KEY).test_connection().await.unwrap();
    assert_eq!(info.server_version.as_deref(), Some("2.5.3"));
    assert_eq!(info.detail.as_deref(), Some("8 monitors"));
    assert_eq!(info.account, None);
    assert!(info.discovered.is_empty());
}

/// Every write op is refused, because this adapter declares none. The battery
/// checks the refusal; this checks that the refusal *names the op*, so a
/// descriptor that ever declares one has a diagnosable failure rather than a
/// silent 404.
#[tokio::test]
async fn every_write_is_refused_by_name() {
    let fake = Fake::start().await;
    let source = adapter(fake.base_url(), KEY);
    let refused = source
        .write(knobas_source::WriteOp::Comment {
            entity: "kuma:8".to_owned(),
            body: "nothing to say to a monitor".to_owned(),
        })
        .await;
    let message = match refused {
        Err(knobas_source::SourceError::Protocol { message, .. }) => message,
        other => panic!("expected Protocol, got {other:?}"),
    };
    assert!(message.contains("comment"), "{message}");
    assert!(message.contains("read-only"), "{message}");
}
