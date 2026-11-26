//! External URL handoff: per-source settings, rule export, `knobas://` deep
//! links, Firefox Native Messaging host, and one-shot *Open in browser* bypass
//! (ADR-0061).

pub mod bridge;
pub mod bypass;
pub mod deep_link;
pub mod manifest;
pub mod native_host;
pub mod protocol;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use knobas_sync::scheduler::AdapterRegistry;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use tauri::Manager;
use ts_rs::TS;

use crate::IpcError;
use crate::commands::app::Lifecycle;
use crate::commands::entity::resolve_url_inner;
use crate::link_handoff::bridge::BridgeServer;
use crate::link_handoff::bypass::BypassStore;
use crate::link_handoff::protocol::{
    HandoffAction, HandoffOpenedTarget, HostRequest, HostResponse, IgnoreReason,
};

pub use native_host::main_if_asked;

/// How broadly a source's matching external URLs hand off into knobas.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export_to = "app.ts")]
pub enum LinkHandoffScope {
    /// Only URLs resolving to a mirrored item (`sync.item.web_url` or adapter `UrlForm`).
    #[default]
    KnownItems,
    /// Known items open in their native view; other URLs under the source's base
    /// open in the source's embedded site when *Open its pages inside knobas* is on.
    WholeSource,
}

/// Per-source *Open matching links in knobas* setting persisted in `knobas.setting`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export_to = "app.ts")]
pub struct LinkHandoffSetting {
    pub source: String,
    pub enabled: bool,
    pub scope: LinkHandoffScope,
}

/// Live rule exported to the Firefox WebExtension for one enabled source.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinkHandoffRule {
    pub source_id: String,
    pub display_name: String,
    pub adapter_kind: String,
    pub base_url: String,
    pub scope: LinkHandoffScope,
    pub open_inside: bool,
    pub url_forms: Vec<String>,
}

/// Payload emitted on `link-handoff:open` so the shell navigates to the target.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export_to = "app.ts")]
pub enum LinkHandoffTarget {
    Entity {
        entity_id: String,
        entity_kind: String,
        url: String,
    },
    Site {
        source: String,
        url: String,
    },
    Unmatched {
        url: String,
    },
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct StoredSetting {
    #[serde(default)]
    enabled: bool,
    #[serde(default)]
    scope: LinkHandoffScope,
}

/// `knobas.setting` key for one source's link handoff configuration.
#[must_use]
pub fn setting_key(source: &str) -> String {
    format!("link_handoff.source.{source}")
}

/// Whether `url` is a valid absolute `http://` or `https://` URL with a host.
#[must_use]
pub fn is_http_or_https_url(url: &str) -> bool {
    let Ok(parsed) = url::Url::parse(url.trim()) else {
        return false;
    };
    matches!(parsed.scheme(), "http" | "https") && parsed.host_str().is_some()
}

/// Normalize an `http(s)` URL using the same four rules as `web_url_normalized!`:
/// lowercase scheme + host[:port], strip `#fragment`, strip trailing `/` on the
/// path, and keep `?query` verbatim. Returns `None` for non-`http(s)` URLs.
///
/// Not a second spelling of the mirror's rule (`knobas_core::web_url`): its
/// output never meets a stored `web_url`. Both sides of every comparison it
/// serves, a bypass recorded and a bypass checked, a rule's base and the
/// rule it is matched against, pass through this one function; finding an
/// item by its address stays `resolve_url_inner`'s SQL.
#[must_use]
pub fn normalize_web_url(url: &str) -> Option<String> {
    let trimmed = url.trim();
    let parsed = url::Url::parse(trimmed).ok()?;
    if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
        return None;
    }
    let without_fragment = trimmed.split('#').next().unwrap_or_default();
    let authority_end = without_fragment.find("://").map(|i| i + 3)?;
    let rest_start = without_fragment[authority_end..]
        .find(['/', '?'])
        .map_or(without_fragment.len(), |i| authority_end + i);
    let authority = without_fragment[..rest_start].to_ascii_lowercase();
    let rest = &without_fragment[rest_start..];
    let (path, query) = match rest.find('?') {
        Some(q) => (&rest[..q], Some(&rest[q..])),
        None => (rest, None),
    };
    let trimmed_path = path.trim_end_matches('/');
    let mut out =
        String::with_capacity(authority.len() + trimmed_path.len() + query.map_or(0, str::len));
    out.push_str(&authority);
    out.push_str(trimmed_path);
    if let Some(q) = query {
        out.push_str(q);
    }
    Some(out)
}

/// Read one source's link handoff setting (default: disabled, `known_items`).
///
/// # Errors
///
/// Database read error.
pub async fn read_setting(pool: &PgPool, source: &str) -> Result<LinkHandoffSetting, IpcError> {
    let stored = crate::settings::read_value(pool, &setting_key(source))
        .await?
        .and_then(|v| serde_json::from_value::<StoredSetting>(v).ok())
        .unwrap_or_default();
    Ok(LinkHandoffSetting {
        source: source.to_owned(),
        enabled: stored.enabled,
        scope: stored.scope,
    })
}

/// Persist one source's link handoff setting.
///
/// # Errors
///
/// `NotFound` when no source with `source` exists, or database write error.
pub async fn write_setting(
    pool: &PgPool,
    source: &str,
    enabled: bool,
    scope: LinkHandoffScope,
) -> Result<LinkHandoffSetting, IpcError> {
    if knobas_sync::config::get(pool, source).await?.is_none() {
        return Err(IpcError::not_found(format!("no source `{source}`")));
    }
    let stored = StoredSetting { enabled, scope };
    let value = serde_json::to_value(&stored).map_err(IpcError::internal)?;
    crate::settings::write_value(pool, &setting_key(source), &value).await?;
    Ok(LinkHandoffSetting {
        source: source.to_owned(),
        enabled,
        scope,
    })
}

/// Remove a deleted source's link handoff row from `knobas.setting`.
///
/// # Errors
///
/// Database error.
pub async fn forget_settings(pool: &PgPool, source: &str) -> Result<(), sqlx::Error> {
    sqlx::query("delete from knobas.setting where key = $1")
        .bind(setting_key(source))
        .execute(pool)
        .await?;
    Ok(())
}

/// Export live rules for every configured source whose link handoff is enabled.
///
/// # Errors
///
/// Database read error.
pub async fn export_rules(
    pool: &PgPool,
    registry: &dyn AdapterRegistry,
) -> Result<Vec<LinkHandoffRule>, IpcError> {
    let descriptors = registry.descriptors();
    let rows = knobas_sync::config::list(pool).await?;
    let mut rules = Vec::new();
    for row in rows {
        let setting = read_setting(pool, &row.id).await?;
        if !setting.enabled {
            continue;
        }
        let Some(base_url) = normalize_web_url(&row.base_url) else {
            continue;
        };
        let template = descriptors
            .iter()
            .find(|d| d.adapter_kind == row.adapter_kind);
        let passwords = template.is_some_and(|d| d.passwords.is_some());
        let open_inside_raw =
            crate::settings::read_value(pool, &crate::site::open_inside_key(&row.id))
                .await?
                .and_then(|v| v.as_bool())
                .unwrap_or(passwords);
        let open_inside = open_inside_raw && knobas_site::OFFERED;
        let url_forms = template
            .map(|d| d.url_forms.iter().map(|f| f.template.clone()).collect())
            .unwrap_or_default();
        rules.push(LinkHandoffRule {
            source_id: row.id,
            display_name: row.display_name,
            adapter_kind: row.adapter_kind,
            base_url,
            scope: setting.scope,
            open_inside,
            url_forms,
        });
    }
    Ok(rules)
}

/// Find the enabled rule whose `base_url` is the longest segment-boundary
/// prefix of `url`.
#[must_use]
pub fn match_rule<'a>(rules: &'a [LinkHandoffRule], url: &str) -> Option<&'a LinkHandoffRule> {
    let target = parse_base_parts(url)?;
    let mut best: Option<(&'a LinkHandoffRule, usize)> = None;
    for rule in rules {
        let Some(base) = parse_base_parts(&rule.base_url) else {
            continue;
        };
        if base.origin != target.origin {
            continue;
        }
        let matches = if base.path.is_empty() {
            true
        } else {
            target.path == base.path
                || target
                    .path
                    .strip_prefix(&base.path)
                    .is_some_and(|rest| rest.starts_with('/'))
        };
        if matches && best.is_none_or(|(_, len)| base.path.len() > len) {
            best = Some((rule, base.path.len()));
        }
    }
    best.map(|(rule, _)| rule)
}

struct BaseParts {
    origin: String,
    path: String,
}

fn parse_base_parts(url: &str) -> Option<BaseParts> {
    let parsed = url::Url::parse(url.trim()).ok()?;
    if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
        return None;
    }
    let origin = parsed.origin().ascii_serialization();
    let path = parsed.path().trim_end_matches('/').to_owned();
    Some(BaseParts { origin, path })
}

/// Resolve an explicit `knobas://open?url=<url>` deep link into a navigation target.
///
/// # Errors
///
/// `Invalid` if `url` is not `http(s)`, or database read error.
pub async fn resolve_deep_link(
    pool: &PgPool,
    registry: &dyn AdapterRegistry,
    url: &str,
) -> Result<LinkHandoffTarget, IpcError> {
    if !is_http_or_https_url(url) {
        return Err(IpcError::invalid(format!(
            "{url:?} is not an http or https URL"
        )));
    }
    if let Some(found) = resolve_url_inner(pool, registry, url).await? {
        return Ok(LinkHandoffTarget::Entity {
            entity_id: found.entity_id,
            entity_kind: found.kind,
            url: url.to_owned(),
        });
    }
    let rules = export_rules(pool, registry).await?;
    if let Some(rule) = match_rule(&rules, url)
        && rule.scope == LinkHandoffScope::WholeSource
        && rule.open_inside
    {
        return Ok(LinkHandoffTarget::Site {
            source: rule.source_id.clone(),
            url: url.to_owned(),
        });
    }
    Ok(LinkHandoffTarget::Unmatched {
        url: url.to_owned(),
    })
}

/// Evaluate a Firefox Native Messaging `handoff` request against bypasses,
/// enabled source rules, and the local mirror.
///
/// Returns `(HandoffAction, Option<LinkHandoffTarget>)` where `Some(target)`
/// means the desktop app should focus its window and navigate to `target`.
///
/// # Errors
///
/// Database read error.
pub async fn evaluate_native_handoff(
    pool: &PgPool,
    registry: &dyn AdapterRegistry,
    bypass: &BypassStore,
    url: &str,
) -> Result<(HandoffAction, Option<LinkHandoffTarget>), IpcError> {
    if !is_http_or_https_url(url) {
        return Err(IpcError::invalid(format!(
            "{url:?} is not an http or https URL"
        )));
    }
    if bypass.check(url, true) {
        return Ok((HandoffAction::Bypass, None));
    }
    let rules = export_rules(pool, registry).await?;
    let Some(rule) = match_rule(&rules, url) else {
        return Ok((
            HandoffAction::Ignore {
                reason: IgnoreReason::NoMatchingSource,
                source_id: None,
            },
            None,
        ));
    };
    if let Some(found) = resolve_url_inner(pool, registry, url).await? {
        let target = LinkHandoffTarget::Entity {
            entity_id: found.entity_id.clone(),
            entity_kind: found.kind.clone(),
            url: url.to_owned(),
        };
        let action = HandoffAction::Opened {
            target: HandoffOpenedTarget::Entity,
            source_id: rule.source_id.clone(),
            entity_id: Some(found.entity_id),
            entity_kind: Some(found.kind),
            url: None,
        };
        return Ok((action, Some(target)));
    }
    match rule.scope {
        LinkHandoffScope::WholeSource if rule.open_inside => {
            let target = LinkHandoffTarget::Site {
                source: rule.source_id.clone(),
                url: url.to_owned(),
            };
            let action = HandoffAction::Opened {
                target: HandoffOpenedTarget::Site,
                source_id: rule.source_id.clone(),
                entity_id: None,
                entity_kind: None,
                url: Some(url.to_owned()),
            };
            Ok((action, Some(target)))
        }
        LinkHandoffScope::WholeSource => Ok((
            HandoffAction::Ignore {
                reason: IgnoreReason::SiteDisabled,
                source_id: Some(rule.source_id.clone()),
            },
            None,
        )),
        LinkHandoffScope::KnownItems => Ok((
            HandoffAction::Ignore {
                reason: IgnoreReason::NotKnownItem,
                source_id: Some(rule.source_id.clone()),
            },
            None,
        )),
    }
}

/// Managed app-wide state for link handoff (bypasses, cold-start queue, local bridge).
pub struct LinkHandoffState {
    bypass: Arc<BypassStore>,
    pending_deep_links: Mutex<Vec<String>>,
    frontend_armed: AtomicBool,
    bridge: Mutex<Option<BridgeServer>>,
}

impl Default for LinkHandoffState {
    fn default() -> Self {
        Self::new()
    }
}

impl LinkHandoffState {
    #[must_use]
    pub fn new() -> Self {
        Self {
            bypass: Arc::new(BypassStore::new()),
            pending_deep_links: Mutex::new(Vec::new()),
            frontend_armed: AtomicBool::new(false),
            bridge: Mutex::new(None),
        }
    }

    #[must_use]
    pub fn bypass(&self) -> &Arc<BypassStore> {
        &self.bypass
    }

    pub fn record_bypass(&self, url: &str) -> bool {
        self.bypass.record(url)
    }

    pub fn enqueue_deep_link(&self, url: String) {
        self.pending_deep_links
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(url);
    }

    pub fn arm_frontend(&self) {
        self.frontend_armed.store(true, Ordering::SeqCst);
    }

    #[must_use]
    pub fn is_frontend_armed(&self) -> bool {
        self.frontend_armed.load(Ordering::SeqCst)
    }

    pub fn take_pending(&self) -> Vec<String> {
        std::mem::take(
            &mut *self
                .pending_deep_links
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        )
    }

    pub fn hold_bridge(&self, server: BridgeServer) {
        let prev = self
            .bridge
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .replace(server);
        if let Some(prev) = prev {
            prev.stop();
        }
    }

    pub fn stop_bridge(&self) {
        if let Some(server) = self
            .bridge
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
        {
            server.stop();
        }
    }
}

/// Start the authenticated local bridge server for the running app instance.
pub fn start_bridge<R: tauri::Runtime>(app: &tauri::AppHandle<R>, profile: &crate::Profile) {
    let endpoint_file = std::env::var_os(bridge::BRIDGE_ENV)
        .filter(|v| !v.is_empty())
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| profile.dir.join(bridge::BRIDGE_FILE));
    // A bridge from an earlier bring-up goes first: the new one takes the
    // same endpoint file and socket, which stopping the old one removes.
    if let Some(state) = app.try_state::<LinkHandoffState>() {
        state.stop_bridge();
    }
    let handle = app.clone();
    match BridgeServer::start(endpoint_file.clone(), move |request, left| {
        handle_bridge_request(&handle, request, left)
    }) {
        Ok(server) => {
            if let Some(state) = app.try_state::<LinkHandoffState>() {
                state.hold_bridge(server);
            }
            tracing::info!(endpoint = %endpoint_file.display(), "link handoff bridge listening");
        }
        Err(error) => {
            tracing::warn!(%error, "link handoff bridge could not start");
        }
    }
}

/// Stop the local bridge server cleanly on app quit.
pub fn shutdown<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    if let Some(state) = app.try_state::<LinkHandoffState>() {
        state.stop_bridge();
    }
}

/// Handle a `knobas://open?url=...` URI delivered by `single_instance`,
/// `tauri-plugin-deep-link`, or cold-start `argv`.
pub fn handle_deep_link<R: tauri::Runtime>(app: &tauri::AppHandle<R>, raw: &str) {
    let target_url = match deep_link::parse_deep_link(raw) {
        Ok(url) => url,
        Err(reason) => {
            tracing::warn!(uri = %raw, %reason, "ignored invalid deep link");
            return;
        }
    };
    crate::capture::focus_main_window(app);
    let Some(state) = app.try_state::<LinkHandoffState>() else {
        return;
    };
    state.enqueue_deep_link(target_url);
    drain_pending(app);
}

/// Scan forwarded CLI args (from `single_instance` or cold start) for a
/// `knobas://open?url=...` argument and dispatch it.
pub fn handle_args<R: tauri::Runtime>(app: &tauri::AppHandle<R>, args: &[String]) {
    if let Some(target_url) = deep_link::find_deep_link_in_args(args) {
        crate::capture::focus_main_window(app);
        if let Some(state) = app.try_state::<LinkHandoffState>() {
            state.enqueue_deep_link(target_url);
        }
        drain_pending(app);
    }
}

/// Resolve and emit any queued deep links once both the database and webview are ready.
pub fn drain_pending<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    let Some(state) = app.try_state::<LinkHandoffState>() else {
        return;
    };
    if !state.is_frontend_armed() {
        return;
    }
    let Some(lifecycle) = app.try_state::<Lifecycle>() else {
        return;
    };
    let Ok(pool) = lifecycle.pool() else {
        return;
    };
    let urls = state.take_pending();
    if urls.is_empty() {
        return;
    }
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let lifecycle = handle.state::<Lifecycle>();
        for url in urls {
            match resolve_deep_link(&pool, lifecycle.registry(), &url).await {
                Ok(target) => {
                    crate::capture::focus_main_window(&handle);
                    crate::events::emit(
                        &handle,
                        Some("main"),
                        crate::events::LINK_HANDOFF_OPEN,
                        target,
                    );
                }
                Err(error) => {
                    tracing::warn!(%url, %error, "deep link resolution failed");
                }
            }
        }
    });
}

/// The answer the host gives when knobas cannot answer before the host stops
/// waiting: Firefox loads the page, and knobas does nothing with the link.
fn too_slow() -> HostResponse {
    HostResponse::error("not_ready", "knobas could not answer in time")
}

/// Answer one host request within `left`, the time before the host stops
/// waiting (`bridge::budget_left`). A `handoff` that would open the link acts
/// only when it finished inside that time and the webview listens for
/// `link-handoff:open`; otherwise Firefox keeps the page.
fn handle_bridge_request<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    request: HostRequest,
    left: Duration,
) -> HostResponse {
    match request {
        HostRequest::Ping => HostResponse::pong(true),
        HostRequest::CheckBypass { url, consume } => {
            let bypassed = app
                .try_state::<LinkHandoffState>()
                .is_some_and(|state| state.bypass().check(&url, consume));
            HostResponse::bypass(bypassed)
        }
        HostRequest::GetRules => {
            let Some(lifecycle) = app.try_state::<Lifecycle>() else {
                return HostResponse::error("not_ready", "knobas is starting");
            };
            let Ok(pool) = lifecycle.pool() else {
                return HostResponse::error("not_ready", "database is not ready");
            };
            let registry = lifecycle.registry();
            match tauri::async_runtime::block_on(tokio::time::timeout(
                left,
                export_rules(&pool, registry),
            )) {
                Ok(Ok(rules)) => HostResponse::rules(rules),
                Ok(Err(error)) => HostResponse::error("internal", error.message),
                Err(_) => too_slow(),
            }
        }
        HostRequest::Handoff { url } => {
            if !is_http_or_https_url(&url) {
                return HostResponse::error("invalid_url", "url must be http or https");
            }
            let Some(state) = app.try_state::<LinkHandoffState>() else {
                return HostResponse::error("not_ready", "link handoff is not ready");
            };
            // Before `frontend_ready` nothing hears `link-handoff:open`: an
            // `opened` answer would close the tab and open nothing.
            if !state.is_frontend_armed() {
                return HostResponse::error("not_ready", "the knobas window is starting");
            }
            let Some(lifecycle) = app.try_state::<Lifecycle>() else {
                return HostResponse::error("not_ready", "knobas is starting");
            };
            let Ok(pool) = lifecycle.pool() else {
                return HostResponse::error("not_ready", "database is not ready");
            };
            let registry = lifecycle.registry();
            match tauri::async_runtime::block_on(tokio::time::timeout(
                left,
                evaluate_native_handoff(&pool, registry, state.bypass(), &url),
            )) {
                Err(_) => too_slow(),
                Ok(Ok((action, maybe_target))) => {
                    if let Some(target) = maybe_target {
                        crate::capture::focus_main_window(app);
                        crate::events::emit(
                            app,
                            Some("main"),
                            crate::events::LINK_HANDOFF_OPEN,
                            target,
                        );
                    }
                    HostResponse::handoff(action)
                }
                Ok(Err(error)) => HostResponse::error("internal", error.message),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_web_url_matches_web_url_normalized_rules() {
        assert_eq!(
            normalize_web_url("HTTPS://JIRA.Example/browse/PAY-231/#comment-4").as_deref(),
            Some("https://jira.example/browse/PAY-231")
        );
        assert_eq!(
            normalize_web_url("https://Wiki.Example/pages/viewpage.action?pageId=42#top")
                .as_deref(),
            Some("https://wiki.example/pages/viewpage.action?pageId=42")
        );
        assert_eq!(
            normalize_web_url("https://Example.com/wiki/").as_deref(),
            Some("https://example.com/wiki")
        );
        assert_eq!(normalize_web_url("file:///etc/passwd"), None);
        assert_eq!(normalize_web_url("javascript:alert(1)"), None);
    }

    #[test]
    fn match_rule_prefers_longest_segment_prefix() {
        let rules = vec![
            LinkHandoffRule {
                source_id: "root".into(),
                display_name: "Root".into(),
                adapter_kind: "jira".into(),
                base_url: "https://corp.example".into(),
                scope: LinkHandoffScope::KnownItems,
                open_inside: false,
                url_forms: vec![],
            },
            LinkHandoffRule {
                source_id: "wiki".into(),
                display_name: "Wiki".into(),
                adapter_kind: "confluence".into(),
                base_url: "https://corp.example/wiki".into(),
                scope: LinkHandoffScope::WholeSource,
                open_inside: true,
                url_forms: vec!["/display/{}/{}".into()],
            },
        ];
        assert_eq!(
            match_rule(&rules, "https://CORP.example/wiki/display/ENG/Page")
                .map(|r| r.source_id.as_str()),
            Some("wiki")
        );
        // Segment boundary: `/wiki2` must NOT match `/wiki`, falls back to `root`.
        assert_eq!(
            match_rule(&rules, "https://corp.example/wiki2/page").map(|r| r.source_id.as_str()),
            Some("root")
        );
        assert_eq!(match_rule(&rules, "https://other.example/wiki/page"), None);
    }

    fn error_message(response: &HostResponse) -> Option<(&str, &str)> {
        match &response.body {
            protocol::HostResponseBody::Error { error, message } => {
                Some((error.as_str(), message.as_str()))
            }
            _ => None,
        }
    }

    #[test]
    fn a_handoff_before_the_window_listens_leaves_the_page_in_firefox() {
        let app = tauri::test::mock_app();
        app.manage(LinkHandoffState::new());
        let handoff = || HostRequest::Handoff {
            url: "https://jira.example/browse/PAY-231".into(),
        };

        let early = handle_bridge_request(app.handle(), handoff(), bridge::ANSWER_BUDGET);
        assert_eq!(
            error_message(&early),
            Some(("not_ready", "the knobas window is starting"))
        );

        // Armed, it goes on to the database (not managed in this test).
        app.state::<LinkHandoffState>().arm_frontend();
        let armed = handle_bridge_request(app.handle(), handoff(), bridge::ANSWER_BUDGET);
        assert_eq!(
            error_message(&armed),
            Some(("not_ready", "knobas is starting"))
        );
    }
}
