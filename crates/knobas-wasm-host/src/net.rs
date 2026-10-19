//! An external adapter's one way out: `wasi:http` outgoing requests, held
//! to its allowlist, with the credential attached by the host (ADR-0028,
//! issue #1203).
//!
//! **The allowlist.** A request leaves only for a host [`Allowlist`] allows,
//! read from its resolved scheme, host and port. Any other is refused with
//! `connection-refused` before a socket opens, and the refusal is logged on
//! the host's side with the host it named. The component follows redirects
//! itself (the transport follows none), so a redirect to an undeclared host
//! is refused the same way.
//!
//! **The credential.** It is attached only to a request whose origin
//! (scheme, host and port) is the source's address: `Authorization: Bearer
//! <secret>` for a token method (`Pat`, `ApiToken`), `Authorization: Basic`
//! with the config's `username` and the secret for `UserPassword`. Any
//! `Authorization` the component set on such a request is replaced. Every
//! response from the instance is redacted ([`crate::redact`]) before the
//! component reads it, and the component's `Accept-Encoding` is dropped from
//! those requests, so the body reaches the redactor decoded. A request to
//! any other declared host carries nothing of the host's.
//!
//! **The transport** is the workspace's reqwest, rustls with the platform
//! verifier and the system proxy, as the built-in adapters' is. It follows
//! no redirect and keeps no cookie. A transport error reaches the component
//! as a bare `wasi:http` error code, with no text that could quote a request.

use std::sync::{Arc, OnceLock};

use base64::Engine as _;
use bytes::Bytes;
use http_body_util::BodyExt;
use knobas_source::instance::SourceInstance;
use knobas_source::{AuthMethod, SourceError};
use wasmtime_wasi_http::{Error as HttpError, RequestOptions, WasiBody, WasiHttpHooks};

use crate::allow::{Allowlist, HostPattern, Target, declared_keys};
use crate::redact::{self, Needles, RedactedBody};

/// What a source's calls may reach and attach.
pub struct Policy {
    /// The source id, for the host's log lines.
    source_id: String,
    allowlist: Allowlist,
    /// The source's address, when it is an `http(s)` URL.
    instance: Option<Target>,
    credential: Option<http::HeaderValue>,
    needles: Needles,
}

impl std::fmt::Debug for Policy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Policy")
            .field("source_id", &self.source_id)
            .field("allowlist", &self.allowlist)
            .field("instance", &self.instance)
            .field(
                "credential",
                &self.credential.as_ref().map(|_| "<redacted>"),
            )
            .finish_non_exhaustive()
    }
}

impl Policy {
    /// The policy of a call that reaches nothing: the descriptor probes at
    /// load, before any source exists.
    #[must_use]
    pub fn none(source_id: &str) -> Self {
        Self {
            source_id: source_id.to_owned(),
            allowlist: Allowlist::default(),
            instance: None,
            credential: None,
            needles: Needles::default(),
        }
    }

    /// One source's policy: `patterns` resolved over its address and
    /// config (only the keys `config_schema` declares), and its credential.
    ///
    /// # Errors
    ///
    /// [`SourceError::Protocol`] for an auth method the host cannot attach
    /// (`OAuth`), `UserPassword` without a `username` in the config, or a
    /// secret that cannot be a header.
    pub fn for_source(
        instance: &SourceInstance,
        patterns: &[HostPattern],
        config_schema: &serde_json::Value,
    ) -> Result<Self, SourceError> {
        let allowlist = Allowlist::resolve(
            patterns,
            &instance.base_url,
            &instance.config,
            &declared_keys(config_schema),
        );
        let (credential, spellings) = credential(instance)?;
        Ok(Self {
            source_id: instance.id.clone(),
            allowlist,
            instance: Target::origin_of(&instance.base_url),
            credential,
            needles: Needles::new(spellings),
        })
    }
}

/// The `Authorization` value for `instance`, and every spelling of the
/// secret it carries. `None` without a method or a secret: such a source
/// sends no credential, and the server's 401 says so.
fn credential(
    instance: &SourceInstance,
) -> Result<(Option<http::HeaderValue>, Vec<String>), SourceError> {
    let (Some(method), Some(secret)) = (instance.auth, instance.secret.as_deref()) else {
        return Ok((None, Vec::new()));
    };
    let (value, mut spellings) = match method {
        AuthMethod::Pat | AuthMethod::ApiToken => {
            (format!("Bearer {secret}"), vec![secret.to_owned()])
        }
        AuthMethod::UserPassword => {
            let username = instance
                .config
                .get("username")
                .and_then(serde_json::Value::as_str)
                .filter(|u| !u.is_empty())
                .ok_or_else(|| {
                    SourceError::protocol(
                        "user + password authentication needs a username in the source \
                         configuration",
                    )
                })?;
            let pair =
                base64::engine::general_purpose::STANDARD.encode(format!("{username}:{secret}"));
            (format!("Basic {pair}"), vec![secret.to_owned(), pair])
        }
        AuthMethod::OAuth => {
            return Err(SourceError::protocol(
                "OAuth is not supported for an external adapter; use a token or user + password",
            ));
        }
    };
    let encoded: String = url::form_urlencoded::byte_serialize(secret.as_bytes()).collect();
    spellings.push(encoded);
    let mut header = http::HeaderValue::from_str(&value)
        .map_err(|_| SourceError::protocol("the secret cannot be sent in a header"))?;
    header.set_sensitive(true);
    Ok((Some(header), spellings))
}

/// The `wasi:http` hooks of one call's store.
pub struct Hooks {
    policy: Arc<Policy>,
}

impl Hooks {
    pub fn new(policy: Arc<Policy>) -> Self {
        Self { policy }
    }
}

/// Where a request may go, decided before it leaves.
enum Verdict {
    Refused(String),
    /// Allowed; `true` when it is for the instance.
    Allowed(bool),
}

impl Policy {
    fn judge(&self, uri: &http::Uri) -> Verdict {
        let scheme = uri.scheme_str().unwrap_or_default();
        let authority = uri.authority().map_or("", http::uri::Authority::as_str);
        match Target::parse(scheme, authority) {
            None => Verdict::Refused(format!("{scheme}://{authority} is not an http(s) host")),
            Some(target) if !self.allowlist.allows(&target) => Verdict::Refused(format!(
                "{scheme}://{authority} is not a host the adapter's manifest declares"
            )),
            Some(target) => Verdict::Allowed(self.instance.as_ref() == Some(&target)),
        }
    }

    /// Put the credential on a request for the instance, in place of any
    /// the component set.
    fn attach(&self, headers: &mut http::HeaderMap) {
        headers.remove(http::header::AUTHORIZATION);
        headers.remove(http::header::ACCEPT_ENCODING);
        if let Some(credential) = &self.credential {
            headers.insert(http::header::AUTHORIZATION, credential.clone());
        }
    }
}

type SendResult = Result<
    (
        http::Response<WasiBody>,
        Box<dyn Future<Output = Result<(), HttpError>> + Send>,
    ),
    HttpError,
>;

impl WasiHttpHooks for Hooks {
    // The transport sets `Host` from the URL it was given.
    fn set_host_header(&mut self) -> bool {
        false
    }

    fn send_request(
        &mut self,
        request: http::Request<WasiBody>,
        _options: Option<RequestOptions>,
        _fut: Box<dyn Future<Output = Result<(), HttpError>> + Send>,
    ) -> Box<dyn Future<Output = SendResult> + Send> {
        let policy = Arc::clone(&self.policy);
        Box::new(async move { send(&policy, request).await })
    }
}

async fn send(policy: &Policy, request: http::Request<WasiBody>) -> SendResult {
    let for_instance = match policy.judge(request.uri()) {
        Verdict::Refused(why) => {
            tracing::warn!(source = %policy.source_id, "an external adapter's request is refused: {why}");
            return Err(HttpError::ConnectionRefused);
        }
        Verdict::Allowed(for_instance) => for_instance,
    };
    let (mut parts, body) = request.into_parts();
    if for_instance {
        policy.attach(&mut parts.headers);
    }
    let body: Bytes = body.collect().await?.to_bytes();
    let request = reqwest::Request::try_from(http::Request::from_parts(parts, body))
        .map_err(|_| HttpError::HttpRequestUriInvalid)?;
    let response = client()?.execute(request).await.map_err(|error| {
        tracing::debug!(source = %policy.source_id, %error, "an external adapter's request failed");
        transport_error(&error)
    })?;
    let mut response: http::Response<reqwest::Body> = response.into();
    let body = std::mem::replace(response.body_mut(), reqwest::Body::from(Bytes::new()));
    let body = body.map_err(|_| HttpError::HttpResponseIncomplete);
    let (mut parts, ()) = response.map(|_| ()).into_parts();
    let body: WasiBody = if for_instance {
        // A body reqwest did not decode could hide the secret from the redactor.
        if parts
            .headers
            .get(http::header::CONTENT_ENCODING)
            .is_some_and(|coding| coding != "identity")
        {
            return Err(HttpError::HttpResponseContentCoding(None));
        }
        redact::headers(&mut parts.headers, &policy.needles);
        // Redaction changes the length.
        parts.headers.remove(http::header::CONTENT_LENGTH);
        RedactedBody::new(body, policy.needles.clone()).boxed_unsync()
    } else {
        body.boxed_unsync()
    };
    let done: Box<dyn Future<Output = Result<(), HttpError>> + Send> = Box::new(async { Ok(()) });
    Ok((http::Response::from_parts(parts, body), done))
}

fn transport_error(error: &reqwest::Error) -> HttpError {
    if error.is_timeout() {
        HttpError::ConnectionTimeout
    } else if error.is_connect() {
        HttpError::ConnectionRefused
    } else {
        HttpError::HttpProtocolError
    }
}

/// The process's one client for external adapters.
fn client() -> Result<&'static reqwest::Client, HttpError> {
    static CLIENT: OnceLock<Option<reqwest::Client>> = OnceLock::new();
    CLIENT
        .get_or_init(|| {
            reqwest::Client::builder()
                // Explicit, for `knobas-http`'s reason: the platform verifier.
                .use_rustls_tls()
                .redirect(reqwest::redirect::Policy::none())
                .connect_timeout(std::time::Duration::from_secs(30))
                .build()
                .inspect_err(|error| tracing::error!(%error, "the external adapters' http client"))
                .ok()
        })
        .as_ref()
        .ok_or(HttpError::ConfigurationError)
}
