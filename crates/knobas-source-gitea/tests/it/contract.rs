//! The shared battery every adapter must pass (`knobas_source::contract`).
//!
//! Run here against the docker-free fake so `just check` and CI exercise it on
//! every commit; `tests/live_gitea.rs` runs the *same* battery against the real
//! seeded container, which is what exit criterion B is measured against.

use crate::support::{Fake, State, TOKEN, dead_url, instance};
use knobas_source::contract::{Fault, battery};

#[tokio::test]
async fn passes_the_contract_battery() {
    let fake = Fake::start(&State::tidewater()).await;
    let base_url = fake.base_url();
    battery(move |fault| {
        let (base_url, token) = match fault {
            Fault::None => (base_url.clone(), TOKEN.to_owned()),
            // A live instance answering an invalid token, which is the shape a
            // revoked PAT actually arrives in.
            Fault::Unauthorized => (base_url.clone(), "revoked".to_owned()),
            Fault::Unreachable => (dead_url(), TOKEN.to_owned()),
        };
        knobas_source_gitea::build(instance(base_url, &token, serde_json::json!({})))
            .expect("the adapter builds")
    })
    .await;
}

/// The role data check (#1199) on the fake's Tidewater corpus: the code host
/// the adapter claims is filled by the records a full sync emits.
/// `tests/live_gitea.rs` holds it to the real container.
#[tokio::test]
async fn fills_every_role_it_claims() {
    let fake = Fake::start(&State::tidewater()).await;
    let source =
        knobas_source_gitea::build(instance(fake.base_url(), TOKEN, serde_json::json!({})))
            .expect("the adapter builds");
    knobas_source::contract::fills_its_roles(&*source).await;
}
