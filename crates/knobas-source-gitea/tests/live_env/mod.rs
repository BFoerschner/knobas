//! What both live suites need: where the seeded container is, and an adapter
//! pointed at it.
//!
//! # Two suites, two compose configurations, two properties
//!
//! Everything else in this crate runs against a wiremock stand-in, because
//! `just check` and CI must stay docker-free (roadmap §3). The live suites are
//! where that fake is measured against the server that decides the answers --
//! **if the two disagree, the fake is wrong.** There are two of them because
//! one compose configuration cannot express both properties:
//!
//! * [`live_gitea`] -- `just gitea-live`, `testenv/docker-compose.yml` as it
//!   stands. Certifies the *shapes* interfaces §4.2 fixes: the key forms, the
//!   `{ok,data}` envelope, `state=all`, the discussion living on the issue of
//!   the same index, `sort=recentupdate` really ordering newest-first, `since=`
//!   being server-side and inclusive, and a revoked token's 401.
//! * [`live_gitea_capped`] -- `just gitea-live-capped`, that file plus
//!   `testenv/docker-compose.capped.yml`. Certifies exactly one property, and
//!   one the default configuration structurally cannot: that a listing survives
//!   a server answering **fewer** records than the `limit` the adapter asked
//!   for. Every corpus the seed creates fits in one page of 50, so against the
//!   default file a short first page and an exhausted listing are the same
//!   answer.
//!
//! [`live_gitea`]: ../live_gitea.rs
//! [`live_gitea_capped`]: ../live_gitea_capped.rs

// Compiled separately into each live test binary, and each uses a different
// part of it -- so without this, `clippy --all-targets -- -D warnings` fails on
// whatever one of them happens not to call.
#![allow(dead_code)]

use knobas_source::contract::VecSink;
use knobas_source::instance::SourceInstance;
use knobas_source::{AuthMethod, Source, SyncItem};

/// Where the seeded container is and what to read in it.
///
/// Read from the environment rather than hardcoded so whatever names testenv
/// settles on work without a code change here; `testenv/seed --env` prints
/// exactly these.
pub struct Env {
    pub url: String,
    pub token: String,
    pub owner: String,
    pub repo: String,
}

pub fn env() -> Env {
    let need = |key: &str| {
        std::env::var(key).unwrap_or_else(|_| {
            panic!(
                "{key} is not set -- start testenv's Gitea and seed it first, \
                 then `eval \"$(cd testenv && ./seed --env)\"` (or run `just gitea-live`)"
            )
        })
    };
    Env {
        url: need("KNOBAS_GITEA_URL").trim_end_matches('/').to_owned(),
        token: need("KNOBAS_GITEA_TOKEN"),
        owner: std::env::var("KNOBAS_GITEA_OWNER").unwrap_or_else(|_| "tidewater".to_owned()),
        repo: std::env::var("KNOBAS_GITEA_REPO").unwrap_or_else(|_| "payout-service".to_owned()),
    }
}

impl Env {
    pub fn full_name(&self) -> String {
        format!("{}/{}", self.owner, self.repo)
    }

    /// An adapter over this container, configured as `config` says.
    pub fn source(&self, config: serde_json::Value) -> Box<dyn Source> {
        self.source_with(&self.token, config)
    }

    pub fn source_with(&self, token: &str, config: serde_json::Value) -> Box<dyn Source> {
        knobas_source_gitea::build(SourceInstance {
            id: "gitea".to_owned(),
            kind: "gitea".to_owned(),
            display_name: "Tidewater Git".to_owned(),
            base_url: self.url.clone(),
            auth: Some(AuthMethod::Pat),
            secret: Some(token.to_owned()),
            config,
        })
        .expect("the adapter builds")
    }

    /// Scoped to the one seeded repository these assertions are written for.
    pub fn one_repo(&self) -> Box<dyn Source> {
        self.source(serde_json::json!({ "repos": [self.full_name()] }))
    }

    /// Scoped to every repository of the seeded organisation, which is what
    /// makes the run walk the repository *listing* rather than name its
    /// members.
    pub fn whole_owner(&self) -> Box<dyn Source> {
        self.source(serde_json::json!({ "owners": [self.owner.clone()] }))
    }
}

pub async fn full(source: &dyn Source) -> (Vec<SyncItem>, String) {
    let mut sink = VecSink(Vec::new());
    let cursor = source
        .sync(None, &mut sink)
        .await
        .expect("full sync against the container");
    (sink.0, cursor)
}

pub fn of_kind<'a>(items: &'a [SyncItem], kind: &str) -> Vec<&'a SyncItem> {
    items.iter().filter(|i| i.kind == kind).collect()
}
