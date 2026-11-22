//! The adapter: sign in, walk the tree, read one secret.
//!
//! **The request budget is the audit promise** (ADR-0056). A sync is one
//! sign-in (at most, per session) and one tree request. A password is one
//! request, sent once (`no_retry`). Nothing calls `GET entries/{id}`, which
//! the vendor's documentation shows answering the password inside the entry.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex, PoisonError};
use std::time::{Duration, Instant};

use knobas_http::{Auth, HttpClient, HttpConfig, Method};
use knobas_source::instance::SourceInstance;
use knobas_source::passwords::{self, SecretAnswer, SecretAsk, SecretRef, SecretText};
use knobas_source::{
    ConnectionInfo, Cursor, Sink, Source, SourceDescriptor, SourceError, WriteOp, WriteReceipt,
};

use crate::api::{ApiFolder, ServerInfo, TokenAnswer};
use crate::config::{PleasantConfig, Walk};

/// The REST API's prefix.
pub const API: &str = "api/v5/rest";

/// The oldest server REST v5 runs on.
const OLDEST: (u32, u32, u32) = (7, 9, 7);

/// How long a token is kept when the server does not say: under any
/// expiry a server would set, and a 401 drops it sooner anyway.
const TOKEN_DEFAULT: Duration = Duration::from_secs(15 * 60);

/// Signed-in tokens, by origin and account, shared by every instance built
/// for a source: building the source again costs no second sign-in, and so no
/// second audit line if the server writes one for a sign-in.
/// Origin, account, password hash: a token and when it stops being used.
type Tokens = HashMap<(String, String, u64), (String, Instant)>;

static TOKENS: LazyLock<Mutex<Tokens>> = LazyLock::new(|| Mutex::new(HashMap::new()));

fn tokens() -> std::sync::MutexGuard<'static, Tokens> {
    TOKENS.lock().unwrap_or_else(PoisonError::into_inner)
}

pub struct PleasantSource {
    id: String,
    base_url: String,
    username: String,
    password: String,
    config: PleasantConfig,
    http: HttpClient,
}

/// Build a Pleasant source from its instance: the username from the
/// configuration, the password from the keychain.
///
/// # Errors
/// [`SourceError::Protocol`] for a bad configuration or an unusable
/// address.
pub fn build(instance: SourceInstance) -> Result<Box<dyn Source>, SourceError> {
    knobas_http::refuse_userinfo(&instance.base_url)?;
    let config = PleasantConfig::from_json(&instance.config)?;
    // Both are checked at sign-in, not here: a source is built to be shown
    // and tested before it has either.
    let username = config
        .username
        .clone()
        .unwrap_or_default()
        .trim()
        .to_owned();
    let password = instance.secret.clone().unwrap_or_default();
    let base_url = instance.base_url.trim_end_matches('/').to_owned();
    let http = HttpClient::new(HttpConfig {
        base_url: base_url.clone(),
        adapter_kind: crate::ADAPTER_KIND.to_owned(),
        adapter_version: crate::ADAPTER_VERSION.to_owned(),
        auth: Auth::None,
        ..HttpConfig::default()
    })?;
    Ok(Box::new(PleasantSource {
        id: instance.id,
        base_url,
        username,
        password,
        config,
        http,
    }))
}

/// `7.11.6` as numbers; anything unreadable as zero.
fn version_of(text: &str) -> (u32, u32, u32) {
    let mut parts = text
        .split(|c: char| !c.is_ascii_digit())
        .filter(|p| !p.is_empty())
        .map(|p| p.parse().unwrap_or(0));
    (
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
    )
}

/// The key half of an entity id, for one of this source's.
fn key_of<'a>(source: &str, entity: &'a str) -> Option<&'a str> {
    entity
        .strip_prefix(source)
        .and_then(|rest| rest.strip_prefix(':'))
        .filter(|k| !k.is_empty())
}

/// How a refused secret read says what it needs. *Unverified against a real
/// server*: the vendor says the read "will throw" when a comment is needed;
/// the recorded fixtures settle the status and the words.
fn refusal(error: &SourceError) -> Option<SecretAnswer> {
    let SourceError::Protocol {
        status: Some(status),
        message,
    } = error
    else {
        return None;
    };
    if !matches!(status, 400 | 403 | 409 | 412 | 422 | 428) {
        return None;
    }
    let lower = message.to_ascii_lowercase();
    if lower.contains("comment") {
        return Some(SecretAnswer::CommentRequired { scope: Vec::new() });
    }
    if lower.contains("approv")
        || lower.contains("access request")
        || lower.contains("request access")
    {
        return Some(SecretAnswer::ApprovalRequired { approver: None });
    }
    None
}

impl PleasantSource {
    /// The origin, the account, and a hash of the password: a credential
    /// changed in the keychain signs in anew rather than riding the old
    /// token. The hash is the process's own (`RandomState`), never stored.
    fn token_key(&self) -> (String, String, u64) {
        use std::hash::BuildHasher as _;
        static SEED: LazyLock<std::collections::hash_map::RandomState> =
            LazyLock::new(std::collections::hash_map::RandomState::new);
        (
            self.base_url.to_ascii_lowercase(),
            self.username.clone(),
            SEED.hash_one(&self.password),
        )
    }

    /// The bearer token: the one kept for this origin and account, or a new
    /// sign-in.
    async fn token(&self) -> Result<String, SourceError> {
        if self.username.is_empty() {
            return Err(SourceError::protocol(
                "a Pleasant source needs the account's username",
            ));
        }
        if self.password.is_empty() {
            return Err(SourceError::unauthorized());
        }
        if let Some((token, until)) = tokens().get(&self.token_key())
            && *until > Instant::now()
        {
            return Ok(token.clone());
        }
        let request = self
            .http
            .request(Method::POST, "OAuth2/Token")
            .form(&[
                ("grant_type", "password"),
                ("username", &self.username),
                ("password", &self.password),
            ])
            .no_retry();
        let response = self.http.send(request).await.map_err(|error| match error {
            // A refused password grant is a 400 `invalid_grant`, which is
            // the credential, not the protocol.
            SourceError::Protocol {
                status: Some(400), ref message
            } if message.contains("invalid_grant") || message.to_ascii_lowercase().contains("password") => {
                SourceError::Unauthorized { status: Some(400) }
            }
            SourceError::Protocol { ref message, .. }
                if message.contains("X-Pleasant-OTP") || message.to_ascii_lowercase().contains("two-factor") =>
            {
                SourceError::protocol(
                    "This account asks for a second factor at sign-in, which knobas cannot give yet.",
                )
            }
            other => other,
        })?;
        let answer: TokenAnswer = response.json().await.map_err(|e| {
            SourceError::protocol(format!("the sign-in answer did not decode: {e}"))
        })?;
        if answer.access_token.is_empty() {
            return Err(SourceError::protocol("the sign-in answered no token"));
        }
        let lifetime = answer
            .expires_in
            .or_else(|| answer.expiry.as_ref().and_then(serde_json::Value::as_u64))
            .map_or(TOKEN_DEFAULT, |secs| {
                Duration::from_secs(secs.saturating_sub(60).max(30))
            });
        tokens().insert(
            self.token_key(),
            (answer.access_token.clone(), Instant::now() + lifetime),
        );
        Ok(answer.access_token)
    }

    fn drop_token(&self) {
        tokens().remove(&self.token_key());
    }

    /// One API request with the token. A 401 drops the token, signs in once
    /// and repeats the request once: nothing was read under a refused token.
    async fn call(
        &self,
        method: Method,
        path: &str,
        build: impl Fn(knobas_http::Request) -> knobas_http::Request,
    ) -> Result<knobas_http::Response, SourceError> {
        for attempt in 0..2 {
            let token = self.token().await?;
            let bearer = knobas_http::header::HeaderValue::from_str(&format!("Bearer {token}"))
                .map_err(|_| {
                    SourceError::protocol("the sign-in answered a token no header can carry")
                })?;
            let request = build(
                self.http
                    .request(method.clone(), &format!("{API}/{path}"))
                    .header(knobas_http::header::AUTHORIZATION, bearer),
            );

            match self.http.send(request).await {
                Err(SourceError::Unauthorized { status: Some(401) }) if attempt == 0 => {
                    self.drop_token()
                }
                other => return other,
            }
        }
        unreachable!("the second attempt returns")
    }

    /// The whole tree in one answer, parsed off the runtime, with its
    /// fingerprint for the cursor.
    async fn tree(&self) -> Result<(ApiFolder, String), SourceError> {
        let response = self.call(Method::GET, "folders", |r| r).await?;
        let bytes = response
            .bytes()
            .await
            .map_err(|e| SourceError::Unreachable(format!("the tree broke off: {e}")))?;
        tokio::task::spawn_blocking(move || {
            let raw: serde_json::Value = serde_json::from_slice(&bytes)
                .map_err(|e| SourceError::protocol(format!("the tree did not decode: {e}")))?;
            let print = knobas_source::fingerprint::of(&raw);
            let tree: ApiFolder = serde_json::from_value(raw)
                .map_err(|e| SourceError::protocol(format!("the tree did not decode: {e}")))?;
            Ok((tree, print))
        })
        .await
        .map_err(|e| SourceError::protocol(format!("the tree's parse failed: {e}")))?
    }

    /// One folder and what it holds, one level down.
    async fn folder(&self, id: &str) -> Result<ApiFolder, SourceError> {
        let response = self
            .call(Method::GET, &format!("folders/{id}"), |r| {
                r.query(&[("recurseLevel", "1")])
            })
            .await?;
        response
            .json()
            .await
            .map_err(|e| SourceError::protocol(format!("folder {id} did not decode: {e}")))
    }

    /// The tree a folder at a time, eight in flight: [`Walk::Folders`].
    async fn tree_by_folders(&self) -> Result<(ApiFolder, String), SourceError> {
        let root_id: String = self
            .call(Method::GET, "folders/root", |r| r)
            .await?
            .json()
            .await
            .map_err(|e| SourceError::protocol(format!("the root's id did not decode: {e}")))?;
        let mut root = self.folder(&root_id).await?;
        // Breadth first: each level's folders are read eight at a time, then
        // put in place.
        let mut level: Vec<Vec<usize>> = (0..root.children.len()).map(|i| vec![i]).collect();
        while !level.is_empty() {
            let mut next = Vec::new();
            for batch in level.chunks(8) {
                let mut reads = Vec::new();
                for path in batch {
                    let id = node(&root, path).id.clone();
                    reads.push(async move { (path.clone(), self.folder(&id).await) });
                }
                for (path, read) in futures_join(reads).await {
                    let full = read?;
                    let slot = node_mut(&mut root, &path);
                    slot.credentials = full.credentials;
                    slot.children = full.children;
                    for i in 0..slot.children.len() {
                        let mut child = path.clone();
                        child.push(i);
                        next.push(child);
                    }
                }
            }
            level = next;
        }
        let print = knobas_source::fingerprint::of(&serde_json::json!(format!("{root:?}")));
        Ok((root, print))
    }
}

/// Run reads side by side on this task, so the request counter sees them.
async fn futures_join<F: std::future::Future>(reads: Vec<F>) -> Vec<F::Output> {
    let mut pinned: Vec<std::pin::Pin<Box<F>>> = reads.into_iter().map(Box::pin).collect();
    let mut out: Vec<Option<F::Output>> = (0..pinned.len()).map(|_| None).collect();
    std::future::poll_fn(|cx| {
        let mut pending = false;
        for (i, read) in pinned.iter_mut().enumerate() {
            if out[i].is_none() {
                match read.as_mut().poll(cx) {
                    std::task::Poll::Ready(v) => out[i] = Some(v),
                    std::task::Poll::Pending => pending = true,
                }
            }
        }
        if pending {
            std::task::Poll::Pending
        } else {
            std::task::Poll::Ready(())
        }
    })
    .await;
    out.into_iter()
        .map(|v| v.expect("every read finished"))
        .collect()
}

fn node<'a>(root: &'a ApiFolder, path: &[usize]) -> &'a ApiFolder {
    path.iter().fold(root, |f, i| &f.children[*i])
}

fn node_mut<'a>(root: &'a mut ApiFolder, path: &[usize]) -> &'a mut ApiFolder {
    path.iter().fold(root, |f, i| &mut f.children[*i])
}

#[async_trait::async_trait]
impl Source for PleasantSource {
    fn descriptor(&self) -> SourceDescriptor {
        SourceDescriptor {
            id: self.id.clone(),
            ..crate::descriptor::descriptor_template()
        }
    }

    /// The token, then the server's version. Reads no entry.
    async fn test_connection(&self) -> Result<ConnectionInfo, SourceError> {
        let info: ServerInfo = self
            .call(Method::GET, "GetServerInfo", |r| r)
            .await?
            .json()
            .await
            .unwrap_or_default();
        let version = info.version.or(info.server_version);
        if let Some(v) = &version
            && version_of(v) < OLDEST
            && version_of(v) != (0, 0, 0)
        {
            return Err(SourceError::protocol(format!(
                "Password Server {v} is older than 7.9.7, the first with the REST API v5 knobas reads"
            )));
        }
        Ok(ConnectionInfo {
            account: Some(self.username.clone()),
            server_version: version.map(|v| format!("Password Server {v}")),
            detail: Some("It read no entries.".to_owned()),
            ..ConnectionInfo::default()
        })
    }

    /// One request for the whole tree. A tree whose fingerprint is the
    /// cursor's emits nothing and hands the cursor back; any other emits
    /// everything, its code keys to the sink's side channel, and says both
    /// kinds were walked whole so the engine sweeps what is gone.
    async fn sync(
        &self,
        cursor: Option<Cursor>,
        sink: &mut (dyn Sink + Send),
    ) -> Result<Cursor, SourceError> {
        let (tree, print) = match self.config.walk {
            Walk::Tree => self.tree().await?,
            Walk::Folders => self.tree_by_folders().await?,
        };
        let next = format!("tree:{print}");
        if cursor.as_deref() == Some(next.as_str()) {
            return Ok(next);
        }
        let walked = crate::map::walk(&self.id, &tree);
        let of = walked.items.len() as u64;
        for (done, item) in walked.items.into_iter().enumerate() {
            sink.item(item).await?;
            if done % 200 == 199 {
                sink.walked(done as u64 + 1, Some(of), "items").await;
            }
        }
        for (entity, key) in walked.keys {
            let entity = knobas_source::EntityRef::parse(&entity)
                .map_err(|e| SourceError::protocol(e.to_string()))?;
            sink.otp_key(&entity, key).await;
        }
        sink.walked(of, Some(of), "items").await;
        sink.walked_whole(passwords::ENTRY_KIND).await;
        sink.walked_whole(passwords::FOLDER_KIND).await;
        Ok(next)
    }

    async fn write(&self, op: WriteOp) -> Result<WriteReceipt, SourceError> {
        Err(SourceError::protocol(format!(
            "a Pleasant source takes no queued writes ({:?} was not sent): its changes are direct",
            op.identifier()
        )))
    }

    async fn reachable_transitions(&self, _entity: &str) -> Result<Vec<String>, SourceError> {
        Err(SourceError::protocol(
            "pleasant: a password manager has no workflow, so there are no transitions",
        ))
    }

    async fn view(
        &self,
        _entity: &str,
        _section: Option<&str>,
    ) -> Result<Option<knobas_source::view::SourceView>, SourceError> {
        Ok(None)
    }

    /// The password: one request to `entries/{id}/password`, sent once. A
    /// `GET`, or a `POST` carrying the usage comment. A field the listing did
    /// not send: the entry's folder listed once more, as the sync lists it.
    async fn read_secret(&self, ask: &SecretAsk) -> Result<SecretAnswer, SourceError> {
        let Some(key) = key_of(&self.id, &ask.entity) else {
            return Ok(SecretAnswer::NotOffered);
        };
        match &ask.which {
            SecretRef::Password => {
                let path = format!("entries/{key}/password");
                let answer = match &ask.comment {
                    Some(comment) => {
                        self.call(Method::POST, &path, |r| {
                            r.json(&serde_json::json!({ "Comment": comment }))
                                .no_retry()
                        })
                        .await
                    }
                    None => self.call(Method::GET, &path, |r| r.no_retry()).await,
                };
                let response = match answer {
                    Ok(response) => response,
                    Err(SourceError::Protocol {
                        status: Some(404), ..
                    }) => return Ok(SecretAnswer::NotOffered),
                    Err(error) => return refusal(&error).ok_or(error),
                };
                let text = response
                    .text()
                    .await
                    .map_err(|e| SourceError::Unreachable(format!("the answer broke off: {e}")))?;
                let value = serde_json::from_str::<String>(&text).unwrap_or(text);
                Ok(SecretAnswer::Value(SecretText::new(value)))
            }
            SecretRef::Field { name } => {
                let folder = match ask.folder.as_deref().and_then(|f| key_of(&self.id, f)) {
                    Some(folder) => folder.to_owned(),
                    None => self
                        .call(Method::GET, "folders/root", |r| r)
                        .await?
                        .json::<String>()
                        .await
                        .map_err(|e| {
                            SourceError::protocol(format!("the root's id did not decode: {e}"))
                        })?,
                };
                let listing = self.folder(&folder).await?;
                let value = listing
                    .credentials
                    .iter()
                    .find(|e| e.id.eq_ignore_ascii_case(key))
                    .and_then(|e| e.custom_user_fields.get(name))
                    .and_then(|v| match v {
                        Some(serde_json::Value::String(s)) if !s.is_empty() => Some(s.clone()),
                        _ => None,
                    });
                Ok(value.map_or(SecretAnswer::NotOffered, |v| {
                    SecretAnswer::Value(SecretText::new(v))
                }))
            }
            SecretRef::Earlier { .. } => Ok(SecretAnswer::NotOffered),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_compare_as_numbers() {
        assert!(version_of("7.9.6") < OLDEST);
        assert!(version_of("7.9.7") >= OLDEST);
        assert!(version_of("7.11.6.1234") > OLDEST);
        assert_eq!(version_of("unknown"), (0, 0, 0));
    }

    #[test]
    fn a_refusal_is_read_by_its_words() {
        let refused = |status, message: &str| SourceError::Protocol {
            status: Some(status),
            message: message.to_owned(),
        };
        assert_eq!(
            refusal(&refused(400, "A comment is required to view this password")),
            Some(SecretAnswer::CommentRequired { scope: vec![] })
        );
        assert_eq!(
            refusal(&refused(
                403,
                "This entry requires an access request approval"
            )),
            Some(SecretAnswer::ApprovalRequired { approver: None })
        );
        assert_eq!(refusal(&refused(500, "comment")), None);
        assert_eq!(refusal(&refused(400, "bad input")), None);
        assert_eq!(key_of("pw", "pw:abc"), Some("abc"));
        assert_eq!(key_of("pw", "other:abc"), None);
        assert_eq!(key_of("pw", "pw:"), None);
    }
}
