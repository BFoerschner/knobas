//! Turning a stored configuration into a live adapter.
//!
//! The scheduler and *Test connection* both need "config + secret ⇒
//! `Box<dyn Source>`", so all three M1 adapters expose the identical pair
//! (interfaces §4.2):
//!
//! ```ignore
//! pub fn descriptor_template() -> knobas_source::SourceDescriptor;  // id == adapter_kind
//! pub fn build(instance: SourceInstance) -> Result<Box<dyn Source>, SourceError>;
//! ```
//!
//! [`SourceInstance`] stays plain serde data like the rest of the SPI (§3a),
//! so an adapter can later be built out of process from the same JSON.

use std::fmt;

use crate::AuthMethod;

/// Longest instance id knobas accepts.
pub const INSTANCE_ID_MAX: usize = 32;

/// One configured source instance: everything an adapter needs to be built.
///
/// [`id`](Self::id) is the instance id **and** the [`EntityRef`] namespace of
/// every item this instance emits, so it is immutable once chosen; two Jiras
/// are `jira` and `jira-eu`. [`display_name`](Self::display_name) is the
/// renameable half (interfaces §8 P10).
///
/// [`EntityRef`]: knobas_core::entity::EntityRef
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct SourceInstance {
    pub id: String,
    /// Which adapter builds this instance (`"jira"`, `"gitea"`, `"teamcity"`,
    /// `"mock"`) -- `knobas.source_config.kind`.
    ///
    /// A field rather than a reserved key inside [`config`](Self::config):
    /// stream F's registry routes on it, and a config blob that carries
    /// routing information is one an adapter's own `config_schema` would have
    /// to declare and then ignore. Distinct from [`id`](Self::id), which is
    /// this *instance* -- `jira` and `jira-eu` are two instances of one kind.
    pub kind: String,
    pub display_name: String,
    pub base_url: String,
    /// How to authenticate, or `None` for a source that needs no credential
    /// at all (the mock; a read-only internal service).
    ///
    /// `None` rather than a placeholder method with an empty secret: an
    /// adapter that must distinguish "no auth" from "auth configured, secret
    /// missing" would otherwise have to infer it from
    /// [`secret`](Self::secret), and `missing_secret` is a real state the
    /// sources view offers *Re-enter* for (§3).
    pub auth: Option<AuthMethod>,
    /// The secret from the OS keychain, or `None` when there is none stored --
    /// which is a source that will 401, not a source that authenticates
    /// anonymously (§3: `missing_secret`).
    ///
    /// Never logged: [`Debug`] redacts it, and nothing else may print it.
    pub secret: Option<String>,
    /// A **second** credential the same keychain item may carry: the account
    /// whose name and password open a channel the [`secret`](Self::secret)
    /// cannot (M4.1, issue #452).
    ///
    /// `None` for every source that has one credential, which is all of them
    /// but one. Uptime Kuma is the source that made this necessary and states
    /// the shape of the problem: its API key opens `/metrics` and *only*
    /// `/metrics`, and pausing a monitor is a socket.io session that an API
    /// key cannot log in to at all. So a Kuma with a key reads, and a Kuma
    /// with a key **and** an account reads and writes -- two credentials for
    /// one source, both optional-by-position rather than alternatives.
    ///
    /// **Not a second [`AuthMethod`].** The auth method is how the source
    /// authenticates its *ordinary* traffic, one per source, and the sources
    /// view, the credential-health strip and the re-enter form are all built
    /// on that being a single answer. An account offered as a rival method
    /// would make "which method is this source on" ambiguous and would let a
    /// reader pick the account *instead of* the key, which is a Kuma that
    /// cannot read.
    ///
    /// Never logged: [`Debug`] redacts the password, exactly as it redacts
    /// [`secret`](Self::secret).
    pub account: Option<Account>,
    /// The non-secret configuration, shaped by the adapter's `config_schema`.
    pub config: serde_json::Value,
}

/// A username and password, stored beside a source's ordinary secret.
///
/// One struct rather than two `Option<String>` fields on
/// [`SourceInstance`], because half an account is not a weaker account: a
/// username with no password logs in to nothing, and the adapter that reads
/// it would have to invent a rule for the half-filled case. Present or
/// absent, and the type says which.
#[derive(Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Account {
    pub username: String,
    pub password: String,
}

/// Hand-written for [`SourceInstance`]'s reason: a derived `Debug` puts the
/// password into every `tracing` line that ever formats a struct holding one.
///
/// The **username survives**, and that is deliberate rather than an oversight:
/// "which account did this source try to log in as" is the first question a
/// refused login raises, and a line that redacts both halves cannot answer it.
impl fmt::Debug for Account {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Account")
            .field("username", &self.username)
            .field("password", &"<redacted>")
            .finish()
    }
}

/// Hand-written so the secret cannot reach a log line through `{:?}`.
impl fmt::Debug for SourceInstance {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SourceInstance")
            .field("id", &self.id)
            .field("kind", &self.kind)
            .field("display_name", &self.display_name)
            .field("base_url", &self.base_url)
            .field("auth", &self.auth)
            .field("secret", &self.secret.as_ref().map(|_| "<redacted>"))
            .field("account", &self.account)
            .field("config", &self.config)
            .finish()
    }
}

/// Why an instance id cannot be used.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum InstanceIdError {
    #[error("an instance id must not be empty")]
    Empty,
    #[error("an instance id must be at most {INSTANCE_ID_MAX} characters, this one is {0}")]
    TooLong(usize),
    #[error("an instance id must start with a lowercase letter, {0:?} does not")]
    BadFirst(char),
    #[error("an instance id may hold only lowercase letters, digits and '-', {0:?} may not appear")]
    BadChar(char),
    #[error("{0:?} is a namespace knobas keeps for its own entities")]
    Reserved(String),
}

/// Whether `id` may name a source instance: `[a-z][a-z0-9-]{0,31}`, and not
/// one of knobas' own namespaces.
///
/// Enforced at *add* time, which is the only moment it can be: the id is baked
/// into every entity id, link and activity row this source ever writes
/// (interfaces §8 P10), so there is no rename to fall back on. The sync
/// engine's `check_source_id` is the weaker last line of defence -- it refuses
/// what would corrupt the store -- and this is the rule a human is held to.
///
/// # Errors
///
/// One [`InstanceIdError`] per way an id can be unusable; the first violation
/// wins, so the message names one concrete thing to fix.
pub fn validate_instance_id(id: &str) -> Result<(), InstanceIdError> {
    let mut chars = id.chars();
    let Some(first) = chars.next() else {
        return Err(InstanceIdError::Empty);
    };
    if id.chars().count() > INSTANCE_ID_MAX {
        return Err(InstanceIdError::TooLong(id.chars().count()));
    }
    if !first.is_ascii_lowercase() {
        return Err(InstanceIdError::BadFirst(first));
    }
    for ch in chars {
        if !(ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-') {
            return Err(InstanceIdError::BadChar(ch));
        }
    }
    // The list lives beside `EntityRef`: a second copy is how an adapter comes
    // to pass validation here and be refused by the engine on every run.
    if knobas_core::entity::is_reserved_namespace(id) {
        return Err(InstanceIdError::Reserved(id.to_owned()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// §4.1: the instance id is the EntityRef namespace of every item the
    /// source emits and is immutable once chosen, so it is constrained to
    /// something the whole app can print, type, and put in a URL.
    #[test]
    fn accepts_the_ids_the_contract_names() {
        for good in [
            "jira",
            "jira-eu",
            "gitea",
            "teamcity",
            "uptime-kuma",
            "mock",
            "a",
        ] {
            validate_instance_id(good).unwrap_or_else(|e| panic!("{good:?} should be valid: {e}"));
        }
    }

    #[test]
    fn rejects_everything_else() {
        use InstanceIdError::*;
        assert_eq!(validate_instance_id(""), Err(Empty));
        assert_eq!(validate_instance_id("Jira"), Err(BadFirst('J')));
        assert_eq!(validate_instance_id("1jira"), Err(BadFirst('1')));
        assert_eq!(validate_instance_id("-jira"), Err(BadFirst('-')));
        assert_eq!(validate_instance_id("jira_eu"), Err(BadChar('_')));
        // Would split into a different namespace when parsed back out of an id.
        assert_eq!(validate_instance_id("jira:eu"), Err(BadChar(':')));
        assert_eq!(
            validate_instance_id("note"),
            Err(Reserved("note".to_owned()))
        );
        let long = "a".repeat(INSTANCE_ID_MAX + 1);
        assert_eq!(
            validate_instance_id(&long),
            Err(TooLong(INSTANCE_ID_MAX + 1))
        );
    }

    /// The secret is in this struct because `build` needs it; it must not be
    /// in any log line, and `Debug` is how it would get there.
    #[test]
    fn debug_redacts_the_secret() {
        let instance = SourceInstance {
            id: "jira".to_owned(),
            kind: "jira".to_owned(),
            display_name: "Jira".to_owned(),
            base_url: "https://jira.example".to_owned(),
            auth: Some(crate::AuthMethod::Pat),
            secret: Some("hunter2-the-real-token".to_owned()),
            account: Some(Account {
                username: "knobas".to_owned(),
                password: "hunter2-the-account-one".to_owned(),
            }),
            config: serde_json::json!({ "flavor": "datacenter" }),
        };
        let printed = format!("{instance:?}");
        assert!(
            !printed.contains("hunter2"),
            "Debug leaked the secret: {printed}"
        );
        assert!(printed.contains("<redacted>"), "{printed}");
        assert!(
            printed.contains("jira.example"),
            "everything else stays readable: {printed}"
        );
        // The second credential is redacted by the same rule, and its
        // username is not -- a refused login has to be able to say *as whom*.
        assert!(
            !printed.contains("hunter2-the-account-one"),
            "Debug leaked the account password: {printed}"
        );
        assert!(printed.contains("knobas"), "{printed}");
    }

    /// P6 keeps this plain serde data, which is what preserves the SPI's
    /// out-of-process property (§3a).
    #[test]
    fn round_trips_as_plain_json() {
        let instance = SourceInstance {
            id: "jira-eu".to_owned(),
            kind: "jira".to_owned(),
            display_name: "Jira EU".to_owned(),
            base_url: "https://jira.eu.example".to_owned(),
            auth: Some(crate::AuthMethod::UserPassword),
            secret: None,
            account: None,
            config: serde_json::json!({ "projects": ["PAY"] }),
        };
        let json = serde_json::to_value(&instance).unwrap();
        assert_eq!(json["auth"], "UserPassword");
        assert_eq!(json["secret"], serde_json::Value::Null);
        // The second credential crosses as plain data too, so an adapter run
        // out of process is handed the account the in-process one gets.
        assert_eq!(json["account"], serde_json::Value::Null);
        let with_account = SourceInstance {
            account: Some(Account {
                username: "knobas".to_owned(),
                password: "knobas-dev".to_owned(),
            }),
            ..instance.clone()
        };
        let with_json = serde_json::to_value(&with_account).unwrap();
        assert_eq!(with_json["account"]["username"], "knobas");
        let with_back: SourceInstance = serde_json::from_value(with_json).unwrap();
        assert_eq!(with_back.account.unwrap().password, "knobas-dev");

        let back: SourceInstance = serde_json::from_value(json).unwrap();
        assert_eq!(back.id, instance.id);
        // Two Jiras are `jira` and `jira-eu`, both of adapter kind `jira`:
        // the id is the namespace, the kind is what the registry routes on.
        assert_ne!(back.id, back.kind);
        assert_eq!(back.config, instance.config);

        // A source with no credential at all is `None`, not a placeholder
        // method with no secret behind it.
        let anonymous = SourceInstance {
            auth: None,
            ..instance
        };
        let json = serde_json::to_value(&anonymous).unwrap();
        assert_eq!(json["auth"], serde_json::Value::Null);
    }
}
