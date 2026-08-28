//! The TeamCity endpoints, behind a small seam.
//!
//! [`Rest`] exists so the run in `sync.rs` can be tested against a fake that
//! answers locators the way TeamCity does, without a socket -- and so the wire
//! contract is verified in one place, against `knobas-mockd`.

use knobas_http::HttpClient;
use knobas_source::instance::SourceInstance;
use knobas_source::{ConnectionInfo, SourceError};

use crate::TeamCityConfig;
use crate::http;
use crate::rest::{
    BUILD_FIELDS, BUILD_TYPE_FIELDS, Build, BuildType, CurrentUser, ListEnvelope, Locator,
    SERVER_FIELDS, Server, USER_FIELDS,
};

/// One record, kept both as parsed fields and as the JSON it arrived as --
/// `SyncItem::payload` is the raw record verbatim (spec §3a).
#[derive(Debug, Clone)]
pub(crate) struct Rec<T> {
    pub raw: serde_json::Value,
    pub rec: T,
}

/// What one sync run and one connection test need from TeamCity.
#[async_trait::async_trait]
pub(crate) trait Rest: Send + Sync {
    async fn server(&self) -> Result<Server, SourceError>;
    /// Whom this credential authenticates as. `None` when the server serves
    /// the endpoint but names nobody.
    async fn current_user(&self) -> Result<CurrentUser, SourceError>;
    async fn build_types(&self) -> Result<Vec<Rec<BuildType>>, SourceError>;
    async fn builds(&self, locator: &Locator) -> Result<Vec<Rec<Build>>, SourceError>;
}

/// The real transport.
#[derive(Debug)]
pub(crate) struct HttpRest {
    client: HttpClient,
}

impl HttpRest {
    /// # Errors
    ///
    /// Whatever [`http::credential`] and [`http::client`] report: an
    /// unauthenticatable configuration, a missing secret, or a base URL that
    /// is not one.
    pub(crate) fn new(
        instance: &SourceInstance,
        cfg: &TeamCityConfig,
    ) -> Result<Self, SourceError> {
        let credential = http::credential(
            instance.auth,
            cfg.username.as_deref(),
            instance.secret.as_deref(),
        )?;
        Ok(Self {
            client: http::client(&instance.base_url, cfg, credential)?,
        })
    }

    /// One record, parsed **and** kept verbatim.
    ///
    /// Every response is decoded to `serde_json::Value` first and then into
    /// the typed shape, because `SyncItem::payload` is the raw record and
    /// re-serialising the typed one would silently drop every field this
    /// adapter does not model (spec §3a's re-mapping guarantee).
    async fn get_raw(
        &self,
        path: &str,
        query: &[(&str, &str)],
    ) -> Result<serde_json::Value, SourceError> {
        self.client.get_json::<serde_json::Value>(path, query).await
    }

    async fn list<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        query: &[(&str, &str)],
    ) -> Result<Vec<Rec<T>>, SourceError> {
        let body = self.get_raw(path, query).await?;
        let envelope: ListEnvelope = serde_json::from_value(body).map_err(|e| {
            SourceError::protocol(format!(
                "{path} did not answer with the collection envelope the TeamCity REST contract \
                 documents: {e}"
            ))
        })?;
        envelope
            .items
            .into_iter()
            .map(|raw| {
                serde_json::from_value(raw.clone())
                    .map(|rec| Rec { raw, rec })
                    .map_err(|e| SourceError::protocol(format!("teamcity: {path}: {e}")))
            })
            .collect()
    }

    async fn one<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        query: &[(&str, &str)],
    ) -> Result<T, SourceError> {
        let body = self.get_raw(path, query).await?;
        serde_json::from_value(body)
            .map_err(|e| SourceError::protocol(format!("teamcity: {path}: {e}")))
    }
}

#[async_trait::async_trait]
impl Rest for HttpRest {
    async fn server(&self) -> Result<Server, SourceError> {
        self.one("app/rest/server", &[("fields", SERVER_FIELDS)])
            .await
    }

    async fn current_user(&self) -> Result<CurrentUser, SourceError> {
        self.one("app/rest/users/current", &[("fields", USER_FIELDS)])
            .await
    }

    async fn build_types(&self) -> Result<Vec<Rec<BuildType>>, SourceError> {
        self.list("app/rest/buildTypes", &[("fields", BUILD_TYPE_FIELDS)])
            .await
    }

    async fn builds(&self, locator: &Locator) -> Result<Vec<Rec<Build>>, SourceError> {
        let rendered = locator.render();
        self.list(
            "app/rest/builds",
            &[("locator", rendered.as_str()), ("fields", BUILD_FIELDS)],
        )
        .await
    }
}

/// What the Add-source flow shows after *Test connection* (P4).
///
/// `user` is optional because `/app/rest/users/current` is one request more
/// than the version needs: a server that refuses it should still report the
/// version it reached rather than failing the whole test.
pub(crate) fn connection_info(server: &Server, user: Option<&CurrentUser>) -> ConnectionInfo {
    ConnectionInfo {
        // "Connected as …" is what tells a user a wrong-account token from a
        // working one, so the username wins over the display name: it is the
        // spelling TeamCity itself uses everywhere else.
        account: user.and_then(|u| u.username.clone().or_else(|| u.name.clone())),
        server_version: server.version.clone(),
        // TeamCity publishes no token expiry over REST, so the credential
        // health strip shows no countdown for this source.
        secret_expires_at: None,
        detail: server.build_number.as_ref().map(|b| format!("build {b}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server() -> Server {
        Server {
            version: Some("2025.07.2".to_owned()),
            build_number: Some("189456".to_owned()),
        }
    }

    #[test]
    fn the_server_and_user_records_become_a_connection_report() {
        let info = connection_info(
            &server(),
            Some(&CurrentUser {
                username: Some("mara".to_owned()),
                name: Some("Mara Lindqvist".to_owned()),
            }),
        );
        assert_eq!(info.server_version.as_deref(), Some("2025.07.2"));
        assert_eq!(
            info.account.as_deref(),
            Some("mara"),
            "the username, not the display name: it is what a wrong-account token is spotted by"
        );
        // TeamCity exposes no token expiry over REST.
        assert_eq!(info.secret_expires_at, None);
        assert_eq!(info.detail.as_deref(), Some("build 189456"));
    }

    /// Every field of `ConnectionInfo` is optional (P4), and a server that
    /// says less must not make *Test connection* fail.
    #[test]
    fn a_server_that_names_nobody_still_reports_a_connection() {
        let info = connection_info(&server(), None);
        assert_eq!(info.account, None);
        assert_eq!(info.server_version.as_deref(), Some("2025.07.2"));

        let bare = connection_info(
            &Server {
                version: None,
                build_number: None,
            },
            Some(&CurrentUser {
                username: None,
                name: Some("Mara Lindqvist".to_owned()),
            }),
        );
        assert_eq!(
            bare.account.as_deref(),
            Some("Mara Lindqvist"),
            "the display name is the fallback, not nothing"
        );
        assert_eq!(bare.server_version, None);
        assert_eq!(bare.detail, None);
    }
}
