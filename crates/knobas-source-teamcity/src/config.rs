//! What the generated Add-source form fills in (spec §3a), and nothing else:
//! secrets live in the OS keychain (spec §14) and never reach this struct.

use knobas_source::SourceError;

/// The widest window one configuration may keep. A full sync fetches the
/// newest `builds_per_config` builds per configuration, so this number *is*
/// the retained history, and an unbounded one would mean pulling a build
/// server's entire lifetime on every re-sync.
const MAX_BUILDS_PER_CONFIG: u32 = 10_000;

/// Interfaces §4.1's TeamCity default: 5 requests per second.
const DEFAULT_RATE_LIMIT: u32 = 5;
/// Interfaces §4.1's TeamCity default burst: 10.
const DEFAULT_BURST: u32 = 10;
/// Interfaces §4.1's TeamCity page size.
pub(crate) const DEFAULT_BUILDS_PER_CONFIG: u32 = 100;

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TeamCityConfig {
    /// TeamCity project ids to sync; empty means every project the token can
    /// see. Applied client-side against `buildType(projectId)`: the locator
    /// grammar mockd serves has no project dimension (interfaces §5).
    pub project_ids: Vec<String>,
    /// Build configuration ids to sync; empty means every one in scope.
    pub build_type_ids: Vec<String>,
    /// How many finished builds a full sync fetches per configuration.
    pub builds_per_config: u32,
    /// Only for [`AuthMethod::UserPassword`](knobas_source::AuthMethod); the
    /// password is the keychain secret. Bearer tokens need no username.
    pub username: Option<String>,
    /// Per-instance request budget (interfaces §4.1 default: 5 req/s).
    pub rate_limit_per_sec: u32,
}

impl Default for TeamCityConfig {
    fn default() -> Self {
        Self {
            project_ids: Vec::new(),
            build_type_ids: Vec::new(),
            builds_per_config: DEFAULT_BUILDS_PER_CONFIG,
            username: None,
            rate_limit_per_sec: DEFAULT_RATE_LIMIT,
        }
    }
}

impl TeamCityConfig {
    /// Read the `source_config.config` blob. A JSON `null` (the column's
    /// default before anything was written) and `{}` both mean "defaults".
    ///
    /// # Errors
    ///
    /// [`SourceError::Protocol`] for a key this adapter's schema does not
    /// describe -- the Add-source form is generated from that schema, so an
    /// unknown key is a field that would silently do nothing -- and for a
    /// value outside the range the schema declares.
    pub fn from_json(value: &serde_json::Value) -> Result<Self, SourceError> {
        if value.is_null() {
            return Ok(Self::default());
        }
        let cfg: Self = serde_json::from_value(value.clone())
            .map_err(|e| SourceError::protocol(format!("teamcity config: {e}")))?;
        cfg.validate()?;
        Ok(cfg)
    }

    /// The burst allowance for [`Self::rate_limit_per_sec`].
    ///
    /// Twice the sustained rate, which is interfaces §4.1's pairing (5 / 10)
    /// at the default and keeps the same shape when a user raises the rate.
    pub(crate) fn burst(&self) -> u32 {
        // Saturating rather than wrapping: nothing above refuses a `u32::MAX`
        // rate, and a wrapped burst of 0 would silently become "one request"
        // inside the limiter.
        self.rate_limit_per_sec.saturating_mul(2).max(DEFAULT_BURST)
    }

    fn validate(&self) -> Result<(), SourceError> {
        if self.builds_per_config == 0 || self.builds_per_config > MAX_BUILDS_PER_CONFIG {
            return Err(SourceError::protocol(format!(
                "teamcity config: builds_per_config must be 1..={MAX_BUILDS_PER_CONFIG}, got {}",
                self.builds_per_config
            )));
        }
        if self.rate_limit_per_sec == 0 {
            return Err(SourceError::protocol(
                "teamcity config: rate_limit_per_sec must be at least 1".to_owned(),
            ));
        }
        Ok(())
    }
}

/// JSON Schema for [`TeamCityConfig`]; the Add-source form is generated from
/// it (spec §3a), so every field carries the title and help text the form
/// shows.
#[must_use]
pub fn config_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "project_ids": {
                "type": "array",
                "items": { "type": "string" },
                "title": "Projects",
                "description": "TeamCity project ids to sync. Leave empty for every project this token can see."
            },
            "build_type_ids": {
                "type": "array",
                "items": { "type": "string" },
                "title": "Build configurations",
                "description": "Build configuration ids to sync. Leave empty for every configuration in the selected projects."
            },
            "builds_per_config": {
                "type": "integer",
                "minimum": 1,
                "maximum": MAX_BUILDS_PER_CONFIG,
                "default": DEFAULT_BUILDS_PER_CONFIG,
                "title": "Builds kept per configuration",
                "description": "How many finished builds a full sync fetches per configuration. Older builds are not mirrored."
            },
            "username": {
                "type": "string",
                "title": "Username",
                "description": "Only needed for user + password authentication; a token needs no username."
            },
            "rate_limit_per_sec": {
                "type": "integer",
                "minimum": 1,
                "default": DEFAULT_RATE_LIMIT,
                "title": "Requests per second",
                "description": "Per-instance request budget."
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_config_is_all_defaults() {
        let cfg = TeamCityConfig::from_json(&serde_json::json!({})).expect("empty config");
        assert!(cfg.project_ids.is_empty());
        assert!(cfg.build_type_ids.is_empty());
        assert_eq!(cfg.builds_per_config, 100);
        assert_eq!(cfg.rate_limit_per_sec, 5);
        assert_eq!(cfg.username, None);
        // A source stored before this field existed has `config: {}` -- and a
        // null column reads back as JSON null, not as an object.
        assert_eq!(
            TeamCityConfig::from_json(&serde_json::Value::Null)
                .expect("null config")
                .builds_per_config,
            100
        );
    }

    /// The Add-source form is generated from `config_schema`, so a key the
    /// struct does not know is a form field that silently does nothing.
    #[test]
    fn an_unknown_key_is_rejected() {
        let err = TeamCityConfig::from_json(&serde_json::json!({ "projects": ["Payout"] }))
            .expect_err("unknown key");
        assert!(
            matches!(&err, SourceError::Protocol { message: m, .. } if m.contains("projects")),
            "{err:?}"
        );
    }

    #[test]
    fn out_of_range_values_are_rejected() {
        for bad in [
            serde_json::json!({ "builds_per_config": 0 }),
            serde_json::json!({ "builds_per_config": 10_001 }),
            serde_json::json!({ "rate_limit_per_sec": 0 }),
        ] {
            let err = TeamCityConfig::from_json(&bad).expect_err("{bad} must be refused");
            assert!(matches!(err, SourceError::Protocol { .. }), "{err:?}");
        }
        // ...and the edges of the same ranges are accepted, so the test that
        // pins the refusal cannot be satisfied by refusing everything.
        for good in [
            serde_json::json!({ "builds_per_config": 1 }),
            serde_json::json!({ "builds_per_config": 10_000 }),
            serde_json::json!({ "rate_limit_per_sec": 1 }),
        ] {
            TeamCityConfig::from_json(&good).expect("the range's edge is inside it");
        }
    }

    /// The schema and the struct are two descriptions of one thing. Because
    /// the struct denies unknown fields, a property the struct dropped fails
    /// to parse here -- which is the drift this guards.
    #[test]
    fn every_schema_property_is_a_config_field() {
        let schema = config_schema();
        let props = schema["properties"].as_object().expect("properties");
        assert!(props.contains_key("project_ids"));
        assert!(props.contains_key("build_type_ids"));
        assert!(props.contains_key("builds_per_config"));
        let mut probe = serde_json::Map::new();
        for (key, spec) in props {
            let value = match spec["type"].as_str() {
                Some("array") => serde_json::json!([]),
                Some("integer") => serde_json::json!(1),
                Some("string") => serde_json::json!("x"),
                other => panic!("unhandled schema type {other:?} for {key:?}"),
            };
            probe.insert(key.clone(), value);
        }
        TeamCityConfig::from_json(&serde_json::Value::Object(probe))
            .expect("every field the generated form can fill must parse");
    }

    /// Interfaces §4.1 pairs TeamCity's 5 req/s with a burst of 10, and a
    /// raised rate keeps the same shape rather than staying pinned at 10.
    #[test]
    fn the_burst_is_twice_the_rate_and_never_below_the_default() {
        assert_eq!(TeamCityConfig::default().burst(), 10);
        let raised = TeamCityConfig::from_json(&serde_json::json!({ "rate_limit_per_sec": 20 }))
            .expect("config");
        assert_eq!(raised.burst(), 40);
        let low =
            TeamCityConfig::from_json(&serde_json::json!({ "rate_limit_per_sec": 1 })).expect("c");
        assert_eq!(low.burst(), 10, "the documented burst is a floor");
        let huge = TeamCityConfig {
            rate_limit_per_sec: u32::MAX,
            ..TeamCityConfig::default()
        };
        assert_eq!(huge.burst(), u32::MAX, "saturating, never wrapping to zero");
    }

    /// The schema's declared defaults are the ones the struct actually uses:
    /// the form shows the schema's, and a source saved without touching the
    /// field gets the struct's.
    #[test]
    fn the_schemas_defaults_are_the_structs_defaults() {
        let schema = config_schema();
        let cfg = TeamCityConfig::default();
        assert_eq!(
            schema["properties"]["builds_per_config"]["default"],
            serde_json::json!(cfg.builds_per_config)
        );
        assert_eq!(
            schema["properties"]["rate_limit_per_sec"]["default"],
            serde_json::json!(cfg.rate_limit_per_sec)
        );
    }
}
