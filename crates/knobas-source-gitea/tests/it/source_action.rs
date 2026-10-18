//! Gitea's one source action, *Add label…* on a pull request (#1194,
//! ADR-0029), against the docker-free fake: what the far end is sent, what is
//! refused before a request leaves, and the dialog the view declares.
//!
//! `tests/live_gitea.rs` certifies the same against the real server
//! (`a_label_is_added_to_a_pull_request_through_its_source_action`); on
//! disagreement this fake is what is wrong.

use knobas_source::view::{ActionField, ChoiceOption, FieldKind, ViewAction};
use knobas_source::{Source, SourceError, WriteOp};
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::support::{TOKEN, instance};

const REPO: &str = "/api/v1/repos/tidewater/payout-service";
const PR: &str = "gitea:tidewater/payout-service#142";

fn source(server: &MockServer) -> Box<dyn Source> {
    knobas_source_gitea::build(instance(server.uri(), TOKEN, json!({}))).expect("builds")
}

fn add_label(entity: &str, fields: &[(&str, &str)]) -> WriteOp {
    WriteOp::SourceAction {
        entity: entity.to_owned(),
        id: "add_label".to_owned(),
        fields: fields
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect(),
    }
}

/// A Gitea that takes a pull request's labels and answers them, as 1.27 does.
async fn labelling() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(format!("{REPO}/issues/142/labels")))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!([{ "id": 7, "name": "needs-qa" }])),
        )
        .mount(&server)
        .await;
    server
}

async fn requests(server: &MockServer) -> Vec<(String, String, Value)> {
    server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .map(|r| {
            (
                r.method.to_string(),
                r.url.path().to_owned(),
                serde_json::from_slice(&r.body).unwrap_or(Value::Null),
            )
        })
        .collect()
}

/// The descriptor declares the action, and `source_action` beside it.
#[test]
fn the_descriptor_declares_add_label() {
    let d = knobas_source_gitea::descriptor_template();
    assert_eq!(d.source_actions, ["add_label"]);
    assert!(
        d.write_ops.iter().any(|w| w == "source_action"),
        "{:?}",
        d.write_ops
    );
}

/// *Add label…* adds the label the reader picked, by its id, on the issue the
/// pull request shares an index with: Gitea keeps a pull request's labels
/// there.
#[tokio::test]
async fn add_label_puts_the_picked_label_on_the_pull_request() {
    let server = labelling().await;
    source(&server)
        .write(add_label(PR, &[("label", "7")]))
        .await
        .expect("a declared source action is performed");
    assert_eq!(
        requests(&server).await,
        [(
            "POST".to_owned(),
            format!("{REPO}/issues/142/labels"),
            json!({ "labels": [7] })
        )]
    );
}

/// An id the descriptor does not declare is refused before a request leaves,
/// and the refusal names it (battery clause 11).
#[tokio::test]
async fn an_undeclared_source_action_is_refused_before_a_request() {
    let server = labelling().await;
    let error = source(&server)
        .write(WriteOp::SourceAction {
            entity: PR.to_owned(),
            id: "convert_to_draft".to_owned(),
            fields: [("label".to_owned(), "7".to_owned())].into(),
        })
        .await
        .expect_err("not declared");
    assert!(
        matches!(&error, SourceError::Protocol { status: None, message } if message.contains("convert_to_draft")),
        "{error:?}"
    );
    assert!(requests(&server).await.is_empty());
}

/// A label that is no label id, a missing one, and a target that is no pull
/// request are refused before a request leaves.
#[tokio::test]
async fn add_label_without_a_label_or_aimed_wrong_is_refused_before_a_request() {
    for op in [
        add_label(PR, &[]),
        add_label(PR, &[("label", "needs-qa")]),
        add_label("gitea:tidewater/payout-service", &[("label", "7")]),
    ] {
        let server = labelling().await;
        let error = source(&server)
            .write(op.clone())
            .await
            .expect_err("refused");
        assert!(
            matches!(error, SourceError::Protocol { status: None, .. }),
            "{op:?}: {error:?}"
        );
        assert!(requests(&server).await.is_empty(), "{op:?}");
    }
}

/// Gitea's own refusal (a label of another repository is a 422) reaches the
/// queue as a refusal with its sentence.
#[tokio::test]
async fn a_label_gitea_refuses_is_a_refusal_with_its_sentence() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(422).set_body_json(json!({
            "message": "label does not exist [id: 99]",
            "url": "https://gitea.example/api/swagger"
        })))
        .mount(&server)
        .await;
    let error = source(&server)
        .write(add_label(PR, &[("label", "99")]))
        .await
        .expect_err("refused");
    assert_eq!(error.status(), Some(422), "{error:?}");
    assert!(
        matches!(&error, SourceError::Protocol { message, .. } if message.contains("label does not exist")),
        "{error:?}"
    );
}

/// The dialog the view declares for it: the repository's labels the pull
/// request does not carry yet, as a required choice, the action's id fixed.
pub(crate) fn add_label_action(options: &[(&str, &str)], disabled: Option<&str>) -> ViewAction {
    ViewAction {
        fields: if options.is_empty() {
            Vec::new()
        } else {
            vec![ActionField {
                required: true,
                ..ActionField::new(
                    "label",
                    "Label",
                    FieldKind::Choice {
                        options: options
                            .iter()
                            .map(|(v, l)| ChoiceOption::new(*v, *l))
                            .collect(),
                    },
                )
            }]
        },
        values: [("id".to_owned(), "add_label".to_owned())].into(),
        disabled: disabled.map(str::to_owned),
        ..ViewAction::new("source_action", "Add label…")
    }
}
