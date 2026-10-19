//! From a source's configured method, username and keychain secret to the
//! [`Auth`] the transport sends (spec #1302, issue #1310).
//!
//! Every built-in adapter used to carry its own copy of this mapping, the same
//! text with its name swapped in. What differs between them is data, and an
//! adapter states it once as a [`CredentialRules`] value: which methods it
//! speaks, how its server spells a personal access token, and the two
//! sentences a refusal reads as. The order of the judgement, and the classes
//! it refuses with, are the same for all of them and live here.

use knobas_source::{AuthMethod, SourceError};

use crate::Auth;

/// How a source's server expects a personal access token to be sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenSpelling {
    /// `Authorization: Bearer <pat>` -- Jira and Confluence DC, TeamCity.
    Bearer,
    /// `Authorization: token <pat>` -- Gitea's own scheme (and Forgejo's).
    Token,
}

/// One adapter's rules for turning configuration into a credential.
///
/// A `const` in the adapter, handed to [`credential`] on every build.
#[derive(Debug, Clone, Copy)]
pub struct CredentialRules {
    /// How [`AuthMethod::Pat`] goes on the wire.
    pub pat: TokenSpelling,
    /// The methods this adapter speaks. Any other is refused with
    /// [`unsupported`](Self::unsupported)'s sentence.
    pub methods: &'static [AuthMethod],
    /// The refusal for a source with no method configured.
    pub unconfigured: &'static str,
    /// The refusal for a method outside [`methods`](Self::methods), given
    /// that method.
    pub unsupported: fn(AuthMethod) -> String,
}

/// The [`Auth`] a source's configuration means under `rules`.
///
/// How each method a rule set accepts is sent:
///
/// * [`AuthMethod::Pat`]: the secret in `rules.pat`'s spelling.
/// * [`AuthMethod::UserPassword`]: HTTP Basic, `username` and the secret.
/// * [`AuthMethod::ApiToken`]: HTTP Basic with an empty username and the
///   secret as the password (Uptime Kuma's `/metrics`).
/// * [`AuthMethod::OAuth`]: no wire form; refused as unsupported even by
///   rules that list it.
///
/// `username` is used as given, and ignored for every method but
/// user + password. An adapter whose form lets a blank one through trims it
/// before the call.
///
/// # Errors
///
/// [`SourceError::Unauthorized`] when the keychain holds no secret -- the
/// `missing_secret` state of contract §3, which the sources view offers
/// *Re-enter* for and the scheduler does not back off over.
/// [`SourceError::Protocol`] for a configuration that cannot authenticate at
/// all: no method chosen ([`unconfigured`](CredentialRules::unconfigured)), a
/// method the adapter does not speak
/// ([`unsupported`](CredentialRules::unsupported)), or user + password with no
/// username beside it.
pub fn credential(
    rules: &CredentialRules,
    auth: Option<AuthMethod>,
    username: Option<&str>,
    secret: Option<&str>,
) -> Result<Auth, SourceError> {
    // The configuration is judged **before** the secret, and the order is the
    // whole point. These are two states with two remedies (§3, §10.2):
    // `missing_secret` sends the user to *Re-enter*, a misconfigured source
    // sends them to the source's settings. A source that could not
    // authenticate even with a secret in hand has the second problem, and
    // reporting `Unauthorized` for it -- which is what checking the secret
    // first does -- sends the user to retype a token that was never the issue.
    let Some(method) = auth else {
        // `SourceInstance::auth` is optional because some sources need no
        // credential (P6). No built-in adapter is one of them, and no method
        // can be guessed at from a bare secret.
        return Err(SourceError::protocol(rules.unconfigured));
    };
    if !rules.methods.contains(&method) {
        return Err(SourceError::protocol((rules.unsupported)(method)));
    }
    let scheme = match method {
        AuthMethod::Pat => Scheme::Token(rules.pat),
        AuthMethod::UserPassword => Scheme::Basic(username.ok_or_else(|| {
            SourceError::protocol(
                "user + password authentication needs a username in the source configuration",
            )
        })?),
        AuthMethod::ApiToken => Scheme::Basic(""),
        AuthMethod::OAuth => return Err(SourceError::protocol((rules.unsupported)(method))),
    };
    let secret = secret.ok_or_else(SourceError::unauthorized)?.to_owned();
    Ok(match scheme {
        Scheme::Token(TokenSpelling::Bearer) => Auth::Bearer(secret),
        Scheme::Token(TokenSpelling::Token) => Auth::GiteaToken(secret),
        Scheme::Basic(username) => Auth::Basic {
            username: username.to_owned(),
            password: secret,
        },
    })
}

/// How the source authenticates, decided from configuration alone.
enum Scheme<'a> {
    Token(TokenSpelling),
    Basic(&'a str),
}
