//! What the adapter does against a live-shaped server, short of a sync run.

use crate::support::{Fake, State, TOKEN, dead_url, instance, source};
use knobas_source::instance::SourceInstance;
use knobas_source::{SourceError, WriteOp};

#[tokio::test]
async fn test_connection_reports_the_account_and_the_server_version() {
    let fake = Fake::start(&State::tidewater()).await;
    let source = source(fake.base_url(), serde_json::json!({}));
    let info = source.test_connection().await.unwrap();
    assert_eq!(info.account.as_deref(), Some("mara"));
    assert_eq!(info.server_version.as_deref(), Some("1.24.3"));
    // No connection note: `Gitea 1.24.3` was the version the report already
    // carries as `server_version`, and a note says only what nothing else on
    // the report says (#326).
    assert_eq!(info.detail, None, "{info:?}");
    // Gitea's personal access tokens do not expire, so there is nothing to
    // count down (interfaces §2.2 `CredentialHealth::secret_expires_at`).
    assert!(info.secret_expires_at.is_none());
}

/// The descriptor an instance reports is the instance's, not the template's:
/// the id is the `EntityRef` namespace of everything it emits.
#[tokio::test]
async fn an_instance_describes_itself() {
    let fake = Fake::start(&State::tidewater()).await;
    let mut input = instance(fake.base_url(), TOKEN, serde_json::json!({}));
    input.id = "gitea-eu".to_owned();
    let d = knobas_source_gitea::build(input).unwrap().descriptor();
    assert_eq!(d.id, "gitea-eu");
    assert_eq!(d.adapter_kind, "gitea");
    assert_eq!(d.name, "Tidewater Git");
}

#[tokio::test]
async fn a_bad_token_is_unauthorized() {
    let fake = Fake::start(&State::tidewater()).await;
    let source =
        knobas_source_gitea::build(instance(fake.base_url(), "wrong", serde_json::json!({})))
            .unwrap();
    assert!(matches!(
        source.test_connection().await,
        Err(SourceError::Unauthorized { .. })
    ));
}

#[tokio::test]
async fn a_closed_port_is_unreachable() {
    let source = source(dead_url(), serde_json::json!({}));
    let error = source.test_connection().await.unwrap_err();
    assert!(matches!(error, SourceError::Unreachable(_)), "{error:?}");
}

/// A Gitea served under a path prefix is the deployment that only breaks in
/// someone's real network: the API root has to be appended to the base URL,
/// not substituted for its path.
#[tokio::test]
async fn a_base_url_with_a_path_prefix_is_preserved() {
    let fake = Fake::start(&State::tidewater()).await;
    // The fake serves `/api/v1/...`; a source configured with a trailing slash
    // must reach exactly the same paths rather than `//api/v1/...`.
    let source = source(format!("{}/", fake.base_url()), serde_json::json!({}));
    assert_eq!(
        source.test_connection().await.unwrap().account.as_deref(),
        Some("mara")
    );
}

/// Battery clause 5 against a server that would have answered: an op this
/// adapter does not declare is refused **without a request being made**.
///
/// The ops it *does* declare are certified in `tests/it/write.rs` against a fake
/// that records bodies, and live in `tests/live_gitea.rs`. What only this can
/// see is the negative: the fake here answers, and the request count not moving
/// is the proof that nothing was sent.
#[tokio::test]
async fn an_undeclared_op_never_reaches_the_network() {
    let fake = Fake::start(&State::tidewater()).await;
    let source = source(fake.base_url(), serde_json::json!({}));
    let before = fake.requests().await;
    let refused = source
        .write(WriteOp::Transition {
            entity: "gitea:tidewater/payout-service#142".into(),
            status: "Done".into(),
        })
        .await;
    assert!(
        matches!(refused, Err(SourceError::Protocol { message: ref m, .. }) if m.contains("transition")),
        "{refused:?}"
    );
    assert_eq!(
        fake.requests().await,
        before,
        "an undeclared op must not call out"
    );
}

#[tokio::test]
async fn a_missing_token_cannot_authenticate() {
    let mut input = instance(
        "http://127.0.0.1:1".to_owned(),
        TOKEN,
        serde_json::json!({}),
    );
    input.secret = None;
    assert!(matches!(
        knobas_source_gitea::build(input),
        Err(SourceError::Unauthorized { .. })
    ));
}

#[tokio::test]
async fn an_unsupported_auth_method_is_refused_at_build_time() {
    let mut input = instance(
        "http://127.0.0.1:1".to_owned(),
        TOKEN,
        serde_json::json!({}),
    );
    input.auth = Some(knobas_source::AuthMethod::UserPassword);
    let err = knobas_source_gitea::build(input)
        .err()
        .expect("must be refused");
    assert!(
        matches!(err, SourceError::Protocol { message: ref m, .. } if m.contains("token")),
        "{err:?}"
    );
}

/// An instance id that cannot be an `EntityRef` namespace fails when the source
/// is added, not on every run afterwards.
#[tokio::test]
async fn an_unusable_instance_id_is_refused_at_build_time() {
    for bad in ["", "gitea:eu", "monitor"] {
        let input = SourceInstance {
            id: bad.to_owned(),
            ..instance(
                "http://127.0.0.1:1".to_owned(),
                TOKEN,
                serde_json::json!({}),
            )
        };
        assert!(
            matches!(
                knobas_source_gitea::build(input),
                Err(SourceError::Protocol { .. })
            ),
            "{bad:?}"
        );
    }
}
