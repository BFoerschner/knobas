//! What every configured source declares about its own payloads (#277).
//!
//! `knobas-core` holds the readers and knows no adapter; the adapters hold the
//! declarations and know no reader. This is the one place that sees both, for
//! the same reason `inbox::declared_ops` is: the registry is here.
//!
//! # Why this is resolved per read rather than stored beside the mirror
//!
//! The alternative was a column on `knobas.source_config` written at bring-up,
//! which would have left every core read's signature alone. It was declined,
//! and the reason is not effort:
//!
//! * **A declaration is a property of the running binary's adapter, not of the
//!   installation.** A stored copy is a second answer, and it is stale from the
//!   moment an adapter learns a spelling until something re-writes the row --
//!   so the mini board would be reading last week's Jira adapter's idea of
//!   where a status lives.
//! * **It would cost a migration on a frozen surface** (`0013`) and a writer
//!   on the bring-up path, to cache something a `const` table already answers
//!   in microseconds.
//!
//! What it costs instead is one listing of `knobas.source_config` per read,
//! which is the same round trip `declared_ops` already makes for the inbox.

use knobas_core::payload::Declarations;
use knobas_sync::scheduler::AdapterRegistry;
use sqlx::PgPool;

use crate::IpcError;

/// Every configured source's declared payload paths, by source id.
///
/// The **template** descriptor, which is what `list_adapters` serves: where a
/// source keeps its status is a property of the adapter kind, not of the
/// instance, so this needs no secret and no built adapter. The same reading
/// `inbox::declared_ops` makes of `write_ops`, for the same reason.
///
/// A configured source whose adapter kind is not compiled in declares nothing
/// and every path-driven read misses on its items -- which is the honest
/// answer: knobas cannot know where a source it cannot build keeps anything.
///
/// # Errors
///
/// `internal` if the source listing fails.
pub async fn declared_paths(
    pool: &PgPool,
    registry: &dyn AdapterRegistry,
) -> Result<Declarations, IpcError> {
    let by_kind: std::collections::HashMap<String, Vec<knobas_source::KindPaths>> = registry
        .descriptors()
        .into_iter()
        .map(|d| (d.adapter_kind, d.payload_paths))
        .collect();
    let sources = knobas_sync::config::list(pool)
        .await
        .map_err(IpcError::internal)?;
    Ok(sources
        .into_iter()
        .fold(
            Declarations::empty(),
            |declarations, source| match by_kind.get(&source.adapter_kind) {
                Some(paths) => declarations.with(source.id, paths.clone()),
                None => declarations,
            },
        ))
}

#[cfg(test)]
mod tests {
    use knobas_source::{KindPaths, PayloadPath, SourceDescriptor};

    /// One instance of an adapter kind declares that kind's paths under its own
    /// **instance** id, which is the entity namespace every reader joins on: a
    /// second Jira at `jira-eu` must get the same declaration under its own
    /// name, or every path-driven read misses on half the corpus.
    #[tokio::test]
    async fn every_configured_instance_gets_its_kinds_declaration() {
        let pool = knobas_db::test_util::test_pool().await;
        knobas_db::migrate::run(&pool).await.unwrap();
        for id in ["paths_jira", "paths_jira_eu"] {
            sqlx::query(
                "insert into knobas.source_config (id, kind, display_name, base_url, auth_kind)
                 values ($1, 'paths_kind', $1, 'http://localhost', 'pat')
                 on conflict (id) do nothing",
            )
            .bind(id)
            .execute(&pool)
            .await
            .unwrap();
        }

        struct One;
        impl knobas_sync::scheduler::AdapterRegistry for One {
            fn descriptors(&self) -> Vec<SourceDescriptor> {
                vec![SourceDescriptor {
                    id: "paths_kind".to_owned(),
                    adapter_kind: "paths_kind".to_owned(),
                    name: "Paths".to_owned(),
                    capabilities: Vec::new(),
                    adapter_version: "0".to_owned(),
                    auth_methods: Vec::new(),
                    write_ops: Vec::new(),
                    entity_kinds: Vec::new(),
                    config_schema: serde_json::json!({}),
                    payload_paths: vec![KindPaths {
                        kind: "ticket".to_owned(),
                        status_name: vec![PayloadPath::of(["fields", "status", "name"])],
                        ..KindPaths::default()
                    }],
                }]
            }
            fn build(
                &self,
                _: knobas_source::instance::SourceInstance,
            ) -> Result<Box<dyn knobas_source::Source>, knobas_source::SourceError> {
                unreachable!("declared_paths reads templates and builds nothing")
            }
        }

        let declared = super::declared_paths(&pool, &One).await.unwrap();
        for id in ["paths_jira", "paths_jira_eu"] {
            assert_eq!(
                declared
                    .get(id, "ticket")
                    .map(|paths| paths.status_name.clone()),
                Some(vec![PayloadPath::of(["fields", "status", "name"])]),
                "{id} must carry its adapter kind's declaration under its own instance id"
            );
        }
        assert_eq!(
            declared.get("paths_jira", "pr"),
            None,
            "a kind the adapter does not declare stays undeclared, and every reader misses on it"
        );
    }
}
