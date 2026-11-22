//! *Open in…* for an API spec (ux31, section 23, ADR-0057): an application's
//! API spec command run on files knobas wrote into a scratch folder of its own
//! ([`super::scratch`]): `{file}` the spec as shown, `{collection}` a folder of
//! Bruno requests written by the page's worker, `{name}` the API's title. No
//! checkout is needed, no shell is used, and the only arguments from knobas
//! are paths it made (ADR-0016).
//!
//! For the shipped Bruno entry ([`knobas_core::checkout::open::BRUNO`]) the
//! collection is always written; on macOS, where Bruno drops a folder handed
//! to it before it has a window, knobas starts it first, waits for it to
//! answer, hands the folder, and once more 3 s later.

use std::path::{Path, PathBuf};
use std::time::Duration;

use knobas_core::checkout::open::{self as rules, BRUNO};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

use super::scratch;
use crate::IpcError;

/// What the page hands over: the spec as shown and, where asked, its collection.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export_to = "api.ts")]
pub struct SpecToOpen {
    /// The API's title.
    pub name: String,
    pub spec_text: String,
    /// `yaml` or `json`.
    pub spec_ext: String,
    /// The collection's files, by path from its folder; `None` where the command does not name `{collection}`.
    pub collection: Option<Vec<(String, String)>>,
    /// `repository\npath`: which folder is this spec's.
    pub key: String,
}

/// What an open did: the argument vector, the folder, and what was left out.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, ts_rs::TS)]
#[ts(export_to = "api.ts")]
pub struct OpenedSpec {
    pub argv: Vec<String>,
    pub folder: String,
    /// Whether the folder was written now; `false` where it was opened as it was.
    pub wrote: bool,
    pub left_out: Vec<(String, String)>,
}

/// The files knobas writes for `spec` with `template`: the spec where `{file}` asks or the collection names it, the
/// collection where `{collection}` asks or the application is Bruno.
#[must_use]
pub fn files_for(template: &str, is_bruno: bool, spec: &SpecToOpen) -> Vec<(String, Vec<u8>)> {
    let ext = if spec.spec_ext == "json" {
        "json"
    } else {
        "yaml"
    };
    let wants_collection = is_bruno || template.contains("{collection}");
    let mut out = Vec::new();
    if template.contains("{file}") || wants_collection {
        out.push((format!("openapi.{ext}"), spec.spec_text.as_bytes().to_vec()));
    }
    if wants_collection {
        for (path, text) in spec.collection.as_deref().unwrap_or_default() {
            out.push((path.clone(), text.as_bytes().to_vec()));
        }
    }
    out
}

/// Opens `spec` in `application`.
///
/// # Errors
/// An unknown application, one with no API spec command, a command that cannot be run, a folder that cannot be
/// written, or a program that would not start or failed at once.
pub async fn open_in(
    pool: &PgPool,
    profile_dir: &Path,
    application: &str,
    spec: SpecToOpen,
) -> Result<OpenedSpec, IpcError> {
    let stored = crate::checkout::open::load(pool).await?;
    let app = stored
        .by_id(application)
        .ok_or_else(|| {
            IpcError::invalid(format!(
                "'{application}' is not one of your applications: see Settings › Checkouts"
            ))
        })?
        .clone();
    let template = app
        .api_command
        .clone()
        .ok_or_else(|| IpcError::invalid(format!("{} has no API spec command", app.name)))?;
    let is_bruno = app.id == BRUNO;
    let folder = scratch::folder_for(&scratch::root(profile_dir), &spec.key, &spec.name);
    let files = files_for(&template, is_bruno, &spec);
    let ext = if spec.spec_ext == "json" {
        "json"
    } else {
        "yaml"
    };
    let target = folder.clone();
    let written = tokio::task::spawn_blocking(move || scratch::write(&target, &files))
        .await
        .map_err(IpcError::internal)?
        .map_err(IpcError::invalid)?;
    let file = folder.join(format!("openapi.{ext}"));
    let argv = rules::fill_api(&template, &file, &folder, &safe_title(&spec.name))
        .map_err(|error| IpcError::invalid(format!("'{template}' cannot be run: {error}")))?;
    let to_run = argv.clone();
    let warm = is_bruno && cfg!(target_os = "macos") && argv.first().is_some_and(|p| p == "open");
    tokio::task::spawn_blocking(move || {
        if warm {
            open_bruno_on_mac(&to_run)
        } else {
            crate::checkout::open::spawn(&to_run)
        }
    })
    .await
    .map_err(IpcError::internal)?
    .map_err(|error| IpcError::invalid(format!("Could not open {}: {error}", app.name)))?;
    Ok(OpenedSpec {
        argv,
        folder: folder.display().to_string(),
        wrote: written.wrote,
        left_out: written.left_out,
    })
}

/// The API's title as a word an argument may hold.
fn safe_title(name: &str) -> String {
    name.chars()
        .filter(|c| !c.is_control())
        .collect::<String>()
        .trim()
        .to_owned()
}

/// Bruno on macOS opens a folder only while it runs with a window (`app.on('open-file')`); a folder handed at its
/// start is dropped. So: start it where it is not running, wait until it is and has had a moment for its window, hand
/// the folder, and once more 3 s later (handing an open folder only brings it forward).
fn open_bruno_on_mac(argv: &[String]) -> std::io::Result<()> {
    if !bruno_running() {
        crate::checkout::open::spawn(&["open".to_owned(), "-a".to_owned(), "Bruno".to_owned()])?;
        for _ in 0..60 {
            if bruno_running() {
                break;
            }
            std::thread::sleep(Duration::from_millis(250));
        }
        std::thread::sleep(Duration::from_millis(2500));
    }
    crate::checkout::open::spawn(argv)?;
    let again = argv.to_vec();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(3));
        let _ = crate::checkout::open::spawn(&again);
    });
    Ok(())
}

/// Whether Bruno runs, by Launch Services' list of running applications.
fn bruno_running() -> bool {
    std::process::Command::new("pgrep")
        .args(["-x", "Bruno"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// The scratch folders' size, for Settings.
#[must_use]
pub fn scratch_size(profile_dir: &Path) -> u64 {
    scratch::size(&scratch::root(profile_dir))
}

/// The scratch root, for a command that clears it.
#[must_use]
pub fn scratch_root(profile_dir: &Path) -> PathBuf {
    scratch::root(profile_dir)
}

#[cfg(test)]
mod tests {
    use super::{SpecToOpen, files_for};

    fn spec() -> SpecToOpen {
        SpecToOpen {
            name: "Pets".into(),
            spec_text: "openapi: 3.0.3".into(),
            spec_ext: "yaml".into(),
            collection: Some(vec![("bruno.json".into(), "{}".into())]),
            key: "r\np".into(),
        }
    }

    #[test]
    fn only_what_the_command_names_is_written_but_bruno_always_gets_its_collection() {
        let names = |t: &str, b: bool| {
            files_for(t, b, &spec())
                .into_iter()
                .map(|(p, _)| p)
                .collect::<Vec<_>>()
        };
        assert_eq!(names("code {file}", false), ["openapi.yaml"]);
        assert_eq!(
            names("bru {collection}", false),
            ["openapi.yaml", "bruno.json"]
        );
        assert_eq!(
            names("explorer bruno://app", true),
            ["openapi.yaml", "bruno.json"]
        );
        assert!(names("echo {name}", false).is_empty());
    }
}
