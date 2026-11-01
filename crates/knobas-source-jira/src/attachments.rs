//! An attachment's bytes, live through the stored credential (spec #2396,
//! #2405): the ticket page's preview and its *Download*.
//!
//! The address is the one the sync emitted into the mirror
//! (`fields.attachment[].content`), handed back by the app. It is checked
//! here before anything is sent: the credential goes with every request, so
//! an address on another host, or on this one outside its attachment
//! servlets, is refused unsent. The check is on the parsed URL, after `..`
//! and `.` segments are resolved, so a path that climbs out of
//! `/secure/attachment/` is seen for where it lands.

use knobas_http::{HttpClient, Method, Url};
use knobas_source::SourceError;
use knobas_source::run::DownloadSink;
use knobas_source::view::FileBytes;

/// The servlets Jira Data Center serves an attachment's bytes from: the file
/// and its thumbnail. Neither is REST, and both take the session or the
/// credential as REST does.
const SERVLETS: [&str; 2] = ["secure/attachment/", "secure/thumbnail/"];

/// `url`'s path relative to `base_url` (the instance's address, no trailing
/// slash), percent-encoded as it arrived, where it names an attachment on
/// this instance.
///
/// # Errors
///
/// [`SourceError::Protocol`] for an address that does not parse, is on
/// another scheme, host or port, carries a query, credentials or a
/// fragment, or is outside the attachment servlets.
pub(crate) fn path_on(base_url: &str, url: &str) -> Result<String, SourceError> {
    let refuse = |why: &str| {
        SourceError::protocol(format!(
            "jira: {url:?} is not an attachment of {base_url} ({why}); the credential goes only \
             to the instance's own attachments"
        ))
    };
    let base = Url::parse(base_url).map_err(|_| refuse("the instance's address does not parse"))?;
    let asked = Url::parse(url).map_err(|_| refuse("it does not parse"))?;
    if asked.scheme() != base.scheme()
        || asked.host_str() != base.host_str()
        || asked.port_or_known_default() != base.port_or_known_default()
    {
        return Err(refuse("another host"));
    }
    if asked.query().is_some()
        || asked.fragment().is_some()
        || !asked.username().is_empty()
        || asked.password().is_some()
    {
        return Err(refuse("a query, a fragment or credentials"));
    }
    let prefix = format!("{}/", base.path().trim_end_matches('/'));
    let path = asked
        .path()
        .strip_prefix(&prefix)
        .ok_or_else(|| refuse("outside the instance's path"))?;
    if !SERVLETS.iter().any(|servlet| {
        path.strip_prefix(servlet)
            .is_some_and(|rest| !rest.is_empty())
    }) {
        return Err(refuse("not under /secure/attachment/ or /secure/thumbnail/"));
    }
    Ok(path.to_owned())
}

/// The file at `path` (from [`path_on`]), whole, or refused with its size
/// where it is longer than `cap`: the length Jira announces is believed
/// before a byte is read, and a body that runs past the cap without one is
/// stopped there.
///
/// # Errors
///
/// The transport's, as [`HttpClient::send`] classifies them; a body that
/// breaks off is [`SourceError::Unreachable`].
pub(crate) async fn bytes(
    http: &HttpClient,
    path: &str,
    cap: u64,
) -> Result<FileBytes, SourceError> {
    let mut response = http.send(http.request(Method::GET, path)).await?;
    if let Some(size) = response.content_length().filter(|size| *size > cap) {
        return Ok(FileBytes::TooLarge { size });
    }
    let cap_bytes = usize::try_from(cap).unwrap_or(usize::MAX);
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|error| {
        SourceError::Unreachable(format!("jira: the read of {path} broke off: {error}"))
    })? {
        bytes.extend_from_slice(&chunk);
        if bytes.len() > cap_bytes {
            // No length announced (a compressed answer has none once undone):
            // what was read is the least the file is.
            return Ok(FileBytes::TooLarge {
                size: u64::try_from(bytes.len()).unwrap_or(u64::MAX),
            });
        }
    }
    Ok(FileBytes::Read(bytes))
}

/// The file at `path` streamed into `sink` under `name`, with no total
/// deadline and the transport's stall timeout instead, as a run's download is
/// (#1012).
///
/// # Errors
///
/// As [`bytes`], and the sink's own.
pub(crate) async fn download(
    http: &HttpClient,
    path: &str,
    name: &str,
    sink: &mut dyn DownloadSink,
) -> Result<(), SourceError> {
    let mut body = http
        .send_streaming(http.request(Method::GET, path), knobas_http::STALL_TIMEOUT)
        .await?;
    sink.start(name).await?;
    while let Some(chunk) = body.chunk().await.map_err(|error| {
        SourceError::Unreachable(format!("jira: the download of {name} broke off: {error}"))
    })? {
        sink.write(&chunk).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_attachment_on_the_instance_is_its_path_behind_a_prefix_too() {
        assert_eq!(
            path_on(
                "https://jira.example/jira",
                "https://jira.example/jira/secure/attachment/50000/a%20b.png"
            )
            .unwrap(),
            "secure/attachment/50000/a%20b.png"
        );
        assert_eq!(
            path_on(
                "https://jira.example",
                "https://jira.example:443/secure/thumbnail/50000/a.png"
            )
            .unwrap(),
            "secure/thumbnail/50000/a.png"
        );
    }

    #[test]
    fn an_address_anywhere_else_is_refused() {
        for url in [
            "http://jira.example/secure/attachment/1/a.png",
            "https://jira.example:8443/secure/attachment/1/a.png",
            "https://other.example/jira/secure/attachment/1/a.png",
            "https://jira.example/secure/attachment/1/a.png",
            "https://jira.example/jira/secure/attachment/",
            "https://jira.example/jira/secure/attachment/1/a.png?os_authType=none",
            "https://jira.example/jira/secure/attachment/1/a.png#x",
            "https://u:p@jira.example/jira/secure/attachment/1/a.png",
            "https://jira.example/jira/secure/attachment/../../rest/api/2/myself",
            "https://jira.example/jira/rest/api/2/myself",
            "not a url",
        ] {
            assert!(
                matches!(
                    path_on("https://jira.example/jira", url),
                    Err(SourceError::Protocol { status: None, .. })
                ),
                "{url}"
            );
        }
    }
}
