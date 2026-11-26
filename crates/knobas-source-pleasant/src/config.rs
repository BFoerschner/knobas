//! A Pleasant source's configuration: the account it signs in as, and how it
//! walks the tree.

use knobas_source::SourceError;

/// How a sync reads the tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Walk {
    /// One request for the whole tree (`GET folders`). The default.
    #[default]
    Tree,
    /// A folder at a time (`GET folders/{id}?recurseLevel=1`), eight in
    /// flight: for a server that does not send a large tree inside 30 s.
    Folders,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct PleasantConfig {
    /// The account knobas signs in as (the Add-source form's identity field).
    pub username: Option<String>,
    pub walk: Walk,
}

impl PleasantConfig {
    /// # Errors
    /// [`SourceError::Protocol`] for a configuration that does not decode.
    pub fn from_json(value: &serde_json::Value) -> Result<Self, SourceError> {
        if value.is_null() {
            return Ok(Self::default());
        }
        serde_json::from_value(value.clone())
            .map_err(|e| SourceError::protocol(format!("pleasant configuration is not valid: {e}")))
    }
}
