//! The shared battery every adapter must pass (`knobas_source::contract`).
//!
//! Run here against the docker-free fake so `just check` and CI exercise it on
//! every commit; `tests/live_gitea.rs` runs the *same* battery against the real
//! seeded container, which is what exit criterion B is measured against.

mod support;

use knobas_source::contract::{Fault, battery};
use support::{Fake, State, TOKEN, dead_url, instance};

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
