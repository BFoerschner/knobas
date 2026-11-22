//! Servers a person saved (ux31, section 15): a name, an address and, where
//! the server's certificate comes from a root of its own, a CA file to trust
//! for it. No secret: Björn ruled that a secret is typed on the explorer's page
//! and kept in memory only (2026-11-22). One `knobas.setting` row,
//! [`SERVERS_KEY`], as the *Open in…* list is one; no migration. A backup
//! carries the row; a share export carries no settings row at all.

use serde::{Deserialize, Serialize};
use sqlx::PgPool;

use crate::settings::{read_value, write_value};
use crate::{IpcError, IpcErrorCode};

/// The `knobas.setting` key holding the list.
pub const SERVERS_KEY: &str = "api.servers";

/// One saved server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export_to = "api.ts")]
pub struct SavedServer {
    /// Kept across saves; made from the name for a new one.
    pub id: String,
    pub name: String,
    /// `http` or `https`, a host, maybe a port and a path; no user, query or fragment.
    pub url: String,
    /// A PEM file of roots to trust for this server only.
    pub ca_file: Option<String>,
}

/// The saved list, and the server last used with each spec file.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export_to = "api.ts")]
pub struct SavedServers {
    pub servers: Vec<SavedServer>,
    /// `repository\npath` to a server's id.
    #[serde(default)]
    pub last: std::collections::BTreeMap<String, String>,
}

/// The saved servers; a row that does not parse reads as none.
///
/// # Errors
/// The read fails.
pub async fn load(pool: &PgPool) -> Result<SavedServers, IpcError> {
    Ok(read_value(pool, SERVERS_KEY)
        .await?
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default())
}

/// Checks a list and gives each server an id: kept where it has one, made from its name where not, never twice.
///
/// # Errors
/// A server with no name, or an address knobas does not take, by name.
pub fn checked(mut list: SavedServers) -> Result<SavedServers, IpcError> {
    let mut seen = std::collections::BTreeSet::new();
    for server in &mut list.servers {
        server.name = server.name.trim().to_owned();
        if server.name.is_empty() {
            return Err(IpcError::new(
                IpcErrorCode::Invalid,
                "A server needs a name.",
            ));
        }
        let url = url::Url::parse(server.url.trim()).map_err(|_| {
            IpcError::new(
                IpcErrorCode::Invalid,
                format!("{} is not an address.", server.url),
            )
        })?;
        if url.scheme() != "http" && url.scheme() != "https" {
            return Err(IpcError::new(
                IpcErrorCode::Invalid,
                format!("{}: knobas sends to http and https only.", server.name),
            ));
        }
        if url.host_str().is_none_or(str::is_empty) {
            return Err(IpcError::new(
                IpcErrorCode::Invalid,
                format!("{}: the address has no host.", server.name),
            ));
        }
        if !url.username().is_empty() || url.password().is_some() {
            return Err(IpcError::new(
                IpcErrorCode::Invalid,
                format!(
                    "{}: a user name or password in the address is not kept; type it on the explorer's page.",
                    server.name
                ),
            ));
        }
        if url.query().is_some() || url.fragment().is_some() {
            return Err(IpcError::new(
                IpcErrorCode::Invalid,
                format!("{}: the address has a query or a fragment.", server.name),
            ));
        }
        server.url = url.as_str().trim_end_matches('/').to_owned();
        if server.id.trim().is_empty() {
            server.id = crate::slug::slug(&server.name, "server");
        }
        let base = server.id.clone();
        let mut n = 2;
        while !seen.insert(server.id.clone()) {
            server.id = format!("{base}-{n}");
            n += 1;
        }
        server.ca_file = server
            .ca_file
            .take()
            .map(|f| f.trim().to_owned())
            .filter(|f| !f.is_empty());
    }
    let ids: std::collections::BTreeSet<_> = list.servers.iter().map(|s| s.id.clone()).collect();
    // A server removed is forgotten as any file's last.
    list.last.retain(|_, id| ids.contains(id));
    Ok(list)
}

/// Saves the whole list and answers it as stored.
///
/// # Errors
/// A refusal of [`checked`], or the write fails.
pub async fn save(pool: &PgPool, list: SavedServers) -> Result<SavedServers, IpcError> {
    let list = checked(list)?;
    write_value(
        pool,
        SERVERS_KEY,
        &serde_json::to_value(&list).map_err(IpcError::internal)?,
    )
    .await?;
    Ok(list)
}

#[cfg(test)]
mod tests {
    use super::{SavedServer, SavedServers, checked};

    fn server(id: &str, name: &str, url: &str) -> SavedServer {
        SavedServer {
            id: id.into(),
            name: name.into(),
            url: url.into(),
            ca_file: None,
        }
    }

    #[test]
    fn an_address_with_a_user_or_a_query_is_refused_by_name() {
        for url in [
            "https://mara:pw@teamcity.example.com",
            "https://teamcity.example.com/?a=1",
            "ftp://x.example.com",
            "https://",
            "nope",
        ] {
            let list = SavedServers {
                servers: vec![server("", "TC", url)],
                ..SavedServers::default()
            };
            assert!(checked(list).is_err(), "{url}");
        }
        let list = SavedServers {
            servers: vec![server("", " ", "https://a.example.com")],
            ..SavedServers::default()
        };
        assert!(checked(list).unwrap_err().message.contains("needs a name"));
    }

    #[test]
    fn ids_are_kept_and_unique_and_a_removed_server_is_forgotten() {
        let mut list = SavedServers {
            servers: vec![
                server("tc", "TeamCity", "https://tc.example.com/"),
                server("", "Team City", "https://tc2.example.com"),
                server("", "Team City", "http://localhost:8111"),
            ],
            ..SavedServers::default()
        };
        list.last.insert("repo\napi.yaml".into(), "gone".into());
        list.last.insert("repo\nb.yaml".into(), "tc".into());
        let out = checked(list).unwrap();
        assert_eq!(
            out.servers
                .iter()
                .map(|s| s.id.as_str())
                .collect::<Vec<_>>(),
            ["tc", "team-city", "team-city-2"]
        );
        assert_eq!(out.servers[0].url, "https://tc.example.com");
        assert_eq!(out.last.len(), 1);
    }
}
