//! What the Add-source form fills in, and what the adapter may believe about it.
//!
//! Nothing secret lives here (spec §14): the API key is in the OS keychain and
//! reaches the adapter as `SourceInstance::secret`. There is no username either
//! -- Kuma's `/metrics` takes a key and nothing else, which is spec #427's
//! story 50 ("add the Kuma source with only an API key, so that reading state
//! needs no account password") -- so the form asks for the base URL, the key,
//! and the transport tuning every adapter exposes.

use knobas_source::SourceError;

/// One Uptime Kuma source's configuration blob (`knobas.source_config.config`).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct KumaConfig {
    /// Per-instance rate limit (contract §4.1 shape).
    ///
    /// A poll is **one** request -- the whole of `/metrics` in one document --
    /// so this exists for the shape rather than for the traffic: it is what
    /// stops a source pointed at a shared Kuma from being the reason somebody
    /// else's dashboard is slow.
    pub rate_per_sec: u32,
    pub rate_burst: u32,
    pub connect_timeout_secs: u64,
    pub request_timeout_secs: u64,
}

impl Default for KumaConfig {
    fn default() -> Self {
        Self {
            rate_per_sec: 5,
            rate_burst: 10,
            connect_timeout_secs: 10,
            request_timeout_secs: 30,
        }
    }
}

impl KumaConfig {
    /// Parse and validate `knobas.source_config.config` for a Kuma instance.
    ///
    /// # Errors
    ///
    /// [`SourceError::Protocol`] naming the one thing to fix: an unreadable
    /// blob, or a value outside its range.
    pub fn from_json(value: &serde_json::Value) -> Result<Self, SourceError> {
        let cfg: Self = serde_json::from_value(value.clone()).map_err(|e| {
            SourceError::protocol(format!("uptime kuma configuration is not valid: {e}"))
        })?;
        cfg.validate()?;
        Ok(cfg)
    }

    fn validate(&self) -> Result<(), SourceError> {
        if self.rate_per_sec == 0 {
            return Err(SourceError::protocol("rate_per_sec must be at least 1"));
        }
        if self.rate_burst == 0 {
            return Err(SourceError::protocol("rate_burst must be at least 1"));
        }
        if self.connect_timeout_secs == 0 || self.request_timeout_secs == 0 {
            return Err(SourceError::protocol("timeouts must be at least 1 second"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An empty blob is a working source: every key has a default, so a form
    /// submitted untouched configures a Kuma that syncs.
    #[test]
    fn an_empty_blob_is_the_defaults() {
        assert_eq!(
            KumaConfig::from_json(&serde_json::json!({})).unwrap(),
            KumaConfig::default()
        );
    }

    /// `deny_unknown_fields`, so a key the schema does not describe is refused
    /// at *save* time rather than silently ignored on every run -- the form is
    /// generated from the schema, so a key it does not know is a mistake
    /// somebody made by hand.
    #[test]
    fn a_key_the_schema_does_not_describe_is_refused() {
        let refused = KumaConfig::from_json(&serde_json::json!({ "api_key": "uk1_secret" }));
        assert!(matches!(refused, Err(SourceError::Protocol { .. })));
    }

    /// A zero here would be a rate limiter that never lets a request through,
    /// or a timeout of nothing at all: refused when the source is saved rather
    /// than discovered as a source that can never sync.
    #[test]
    fn the_degenerate_values_are_refused_by_name() {
        for bad in [
            serde_json::json!({ "rate_per_sec": 0 }),
            serde_json::json!({ "rate_burst": 0 }),
            serde_json::json!({ "connect_timeout_secs": 0 }),
            serde_json::json!({ "request_timeout_secs": 0 }),
        ] {
            assert!(
                matches!(
                    KumaConfig::from_json(&bad),
                    Err(SourceError::Protocol { .. })
                ),
                "{bad} should be refused"
            );
        }
    }
}
