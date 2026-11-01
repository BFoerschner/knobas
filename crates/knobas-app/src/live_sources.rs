//! `knobas-app live-sources apply|remove --demo` (#2371): one source per live
//! product on the demo profile, without the Settings form.
//!
//! **The app's own path, not a side door.** Each source goes through
//! `sources::crud` -- `add` or `update`, then `set_secret`, which stores the
//! credential under the profile's keychain service and tests it -- so the row
//! and the credential land exactly where the app reads them. `testenv/
//! live-sources.sh` builds the list from `testenv/seed --env` and pipes it in.
//!
//! **Debug builds and the demo profile only.** A release binary refuses, so an
//! installed knobas carries no way to write credentials from a pipe, and the
//! real profile is never touched (P13). Run it with the binary that will read
//! the credentials (`target/debug/knobas-app` for `just demo`, the bundle's
//! own for a bundle): macOS trusts the binary that wrote a keychain item, and
//! any other one asks.

use std::io::Read as _;
use std::sync::Arc;

use knobas_secrets::SecretStore;
use knobas_sync::config::{self, CredentialHealth};
use knobas_sync::scheduler::AdapterRegistry;
use sqlx::PgPool;

use crate::sources::live::LiveSources;
use crate::sources::{NewSource, SecretInput, SourcePatch, SourcesError, crud};

/// The first argument that selects this subcommand.
pub const ARG: &str = "live-sources";

/// Whether [`apply`] created the source or updated it in place.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Applied {
    Added,
    Updated,
}

/// What [`apply`] did to one source, and what its credential test said.
#[derive(Debug)]
pub struct Outcome {
    pub applied: Applied,
    pub health: CredentialHealth,
}

/// Add `source`, or bring the one under its id up to it: address,
/// configuration, name, interval, and the credential. Either way the
/// credential is stored and tested by `crud::set_secret`, so a second run
/// with the same input changes nothing, and one with a rotated token stores
/// the new token.
///
/// # Errors
/// [`SourcesError::Invalid`] for a source of another adapter kind under the
/// id (a kind never changes, P10), and whatever `crud` refuses.
pub async fn apply(
    pool: &PgPool,
    secrets: &Arc<dyn SecretStore>,
    registry: &Arc<dyn AdapterRegistry>,
    live: &LiveSources,
    source: NewSource,
) -> Result<Outcome, SourcesError> {
    let id = source.id.clone();
    let secret = SecretInput {
        value: source.secret.value.clone(),
        account: source.secret.account.clone(),
    };
    let applied = match config::get(pool, &id).await? {
        Some(row) if row.adapter_kind != source.adapter_kind => {
            return Err(SourcesError::Invalid(format!(
                "source {id:?} is a {} source, not {}; remove it first",
                row.adapter_kind, source.adapter_kind
            )));
        }
        Some(_) => {
            let patch = SourcePatch {
                display_name: Some(source.display_name),
                base_url: Some(source.base_url),
                config: Some(source.config),
                sync_interval_secs: Some(source.sync_interval_secs),
                enabled: Some(source.enabled),
            };
            crud::update(pool, registry.as_ref(), &id, patch).await?;
            Applied::Updated
        }
        None => {
            crud::add(pool, secrets, registry.as_ref(), source).await?;
            Applied::Added
        }
    };
    let health = crud::set_secret(pool, secrets, registry, live, &id, secret).await?;
    Ok(Outcome { applied, health })
}

/// Remove the source `id` with what it synced, and its credential. `false`
/// when there was none.
///
/// # Errors
/// Whatever `crud::delete` refuses but absence.
pub async fn remove(
    pool: &PgPool,
    secrets: &Arc<dyn SecretStore>,
    live: &LiveSources,
    id: &str,
) -> Result<bool, SourcesError> {
    match crud::delete(pool, secrets, live, id, true).await {
        Ok(()) => Ok(true),
        Err(SourcesError::NotFound(_)) => Ok(false),
        Err(error) => Err(error),
    }
}

/// The subcommand, when the process was started as one: its exit code.
/// `None` for every other start, which goes on to be the app.
#[must_use]
pub fn main_if_asked() -> Option<i32> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) != Some(ARG) {
        return None;
    }
    Some(match run(&args[1..]) {
        Ok(()) => 0,
        Err(message) => {
            eprintln!("live-sources: {message}");
            1
        }
    })
}

const USAGE: &str = "usage: knobas-app live-sources apply|remove --demo [--data-dir <dir>] \
                     [<id>...]  (apply reads a JSON array of sources on stdin)";

fn run(args: &[String]) -> Result<(), String> {
    if !cfg!(debug_assertions) {
        return Err("only a debug build adds live sources".to_owned());
    }
    let (action, rest) = args.split_first().ok_or(USAGE)?;
    let mut demo = false;
    let mut data_dir = None;
    let mut ids = Vec::new();
    let mut rest = rest.iter();
    while let Some(arg) = rest.next() {
        match arg.as_str() {
            crate::DEMO_FLAG => demo = true,
            // A scratch profile, for the recipe's own tests: the app data
            // directory the profile goes under, in place of the platform's.
            "--data-dir" => data_dir = Some(rest.next().ok_or(USAGE)?.into()),
            id => ids.push(id.to_owned()),
        }
    }
    if !demo {
        return Err(format!("only the demo profile: pass {}", crate::DEMO_FLAG));
    }
    if crate::sources::SecretStoreKind::from_env() == crate::sources::SecretStoreKind::Memory {
        return Err(format!(
            "{}=memory keeps credentials in this process only; unset it",
            crate::sources::SECRET_STORE_ENV
        ));
    }
    let app_data_dir = match data_dir {
        Some(dir) => dir,
        None => dirs::data_dir()
            .ok_or("no platform data directory")?
            .join(crate::APP_IDENTIFIER),
    };
    let profile = crate::Profile::new(true, &app_data_dir);
    let sources: Vec<NewSource> = match action.as_str() {
        "apply" => {
            let mut input = String::new();
            std::io::stdin()
                .read_to_string(&mut input)
                .map_err(|e| format!("stdin: {e}"))?;
            serde_json::from_str(&input).map_err(|e| format!("the sources on stdin: {e}"))?
        }
        "remove" => Vec::new(),
        _ => return Err(USAGE.to_owned()),
    };
    tauri::async_runtime::block_on(over_profile(&profile, sources, ids))
}

async fn over_profile(
    profile: &crate::Profile,
    sources: Vec<NewSource>,
    remove_ids: Vec<String>,
) -> Result<(), String> {
    // The demo profile's own server: refused while a demo window holds it,
    // which is the message to quit that window first.
    let db = knobas_db::EmbeddedDb::start(profile.db_config(None))
        .await
        .map_err(|e| format!("the demo profile's database ({e}); quit any demo knobas first"))?;
    let outcome = async {
        knobas_db::migrate::run(db.pool())
            .await
            .map_err(|e| format!("migrating: {e}"))?;
        let secrets =
            crate::sources::secret_store(profile, crate::sources::SecretStoreKind::Keychain);
        let registry: Arc<dyn AdapterRegistry> = Arc::new(crate::sources::Registry::default());
        let live = LiveSources::default();
        let mut failed = 0;
        for source in sources {
            let id = source.id.clone();
            match apply(db.pool(), &secrets, &registry, &live, source).await {
                Ok(o) => {
                    let verb = match o.applied {
                        Applied::Added => "added",
                        Applied::Updated => "updated",
                    };
                    let detail = o
                        .health
                        .detail
                        .map(|d| format!(" ({d})"))
                        .unwrap_or_default();
                    println!(
                        "{id}: {verb}, credential {}{detail}",
                        o.health.state.as_str()
                    );
                }
                Err(error) => {
                    failed += 1;
                    println!("{id}: not applied: {error}");
                }
            }
        }
        for id in remove_ids {
            match remove(db.pool(), &secrets, &live, &id).await {
                Ok(true) => println!("{id}: removed"),
                Ok(false) => println!("{id}: not there"),
                Err(error) => {
                    failed += 1;
                    println!("{id}: not removed: {error}");
                }
            }
        }
        if failed == 0 {
            Ok(())
        } else {
            Err(format!("{failed} source(s) failed"))
        }
    }
    .await;
    if let Err(error) = db.stop().await {
        eprintln!("live-sources: stopping the demo profile's database: {error}");
    }
    outcome
}
