//! An image at an address, loaded on a press (the XML view's design,
//! 2026-11-23, section 14): a file view that fetched because somebody opened
//! it would tell a stranger's server that they did, so an image at an address
//! is offered and loads only on the press that asks for it.
//!
//! - **One `GET`**, through the person client (ADR-0057): no cookie, no
//!   credential, no header of a source, no retry; a redirect followed on the
//!   same origin only, five at most; [`IMAGE_CAP`] at most.
//! - **The bytes decide**: an answer that is no image is refused with what it
//!   was, whatever its `Content-Type` said.
//! - **Kept in memory** until knobas quits, by its address, bounded, and
//!   nowhere else: no file, no row.
//! - **Hosts a person keeps** load when their dialog opens, still never
//!   without the press that opens it: one `knobas.setting` row,
//!   [`HOSTS_KEY`].

use std::collections::{HashMap, VecDeque};
use std::sync::{LazyLock, Mutex, PoisonError};
use std::time::Duration;

use base64::Engine as _;
use knobas_http::person::{self, Client, ClientOptions, Failure};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

use crate::IpcError;
use crate::settings::{read_value, write_value};

/// The most an image fetched may be.
pub const IMAGE_CAP: usize = 16 * 1024 * 1024;
/// The most images kept, in bytes, before the oldest goes.
const KEPT_CAP: usize = 64 * 1024 * 1024;
/// How many redirects on the image's own origin are followed.
const MAX_HOPS: usize = 5;
/// The `knobas.setting` key of the hosts whose images load when their dialog opens.
pub const HOSTS_KEY: &str = "render.image_hosts";

/// An image fetched: what it is, how big, and its bytes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, ts_rs::TS)]
#[ts(export_to = "api.ts")]
pub struct FetchedImage {
    pub url: String,
    pub mime: String,
    pub size: u64,
    pub base64: String,
}

/// The hosts kept: their images load when their dialog opens.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export_to = "api.ts")]
pub struct ImageHosts {
    pub hosts: Vec<String>,
}

struct Kept {
    order: VecDeque<String>,
    images: HashMap<String, (String, Vec<u8>)>,
    held: usize,
}

static KEPT: LazyLock<Mutex<Kept>> = LazyLock::new(|| {
    Mutex::new(Kept {
        order: VecDeque::new(),
        images: HashMap::new(),
        held: 0,
    })
});

/// What bytes are, if an image: its media type.
#[must_use]
pub fn image_type(bytes: &[u8]) -> Option<&'static str> {
    let at = |i: usize| bytes.get(i).copied();
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Some("image/png");
    }
    if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        return Some("image/jpeg");
    }
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        return Some("image/gif");
    }
    if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP".as_slice()) {
        return Some("image/webp");
    }
    if bytes.get(4..12) == Some(b"ftypavif".as_slice()) {
        return Some("image/avif");
    }
    if bytes.starts_with(b"BM") && bytes.len() > 26 {
        return Some("image/bmp");
    }
    if at(0) == Some(0) && at(1) == Some(0) && at(2) == Some(1) && at(3) == Some(0) {
        return Some("image/x-icon");
    }
    let head = String::from_utf8_lossy(&bytes[..bytes.len().min(1024)]).to_ascii_lowercase();
    let head = head.trim_start_matches('\u{feff}').trim_start();
    if head.starts_with("<svg") || (head.starts_with("<?xml") && head.contains("<svg")) {
        return Some("image/svg+xml");
    }
    None
}

/// What an answer that is no image was, in a few words.
fn what_it_was(bytes: &[u8], content_type: &str) -> String {
    let head = String::from_utf8_lossy(&bytes[..bytes.len().min(256)]).to_ascii_lowercase();
    if head.contains("<html") || head.contains("<!doctype html") {
        return "an HTML page".to_owned();
    }
    if content_type.is_empty() {
        "not an image".to_owned()
    } else {
        format!("{content_type}, not an image")
    }
}

fn origin(url: &url::Url) -> String {
    url.origin().ascii_serialization()
}

fn keep(url: &str, mime: &str, bytes: &[u8]) {
    let mut kept = KEPT.lock().unwrap_or_else(PoisonError::into_inner);
    if kept.images.contains_key(url) {
        return;
    }
    kept.held += bytes.len();
    kept.order.push_back(url.to_owned());
    kept.images
        .insert(url.to_owned(), (mime.to_owned(), bytes.to_vec()));
    while kept.held > KEPT_CAP {
        let Some(oldest) = kept.order.pop_front() else {
            break;
        };
        if let Some((_, gone)) = kept.images.remove(&oldest) {
            kept.held -= gone.len();
        }
    }
}

fn answered(url: &str, mime: &str, bytes: &[u8]) -> FetchedImage {
    FetchedImage {
        url: url.to_owned(),
        mime: mime.to_owned(),
        size: bytes.len() as u64,
        base64: base64::engine::general_purpose::STANDARD.encode(bytes),
    }
}

fn failed(failure: &Failure) -> IpcError {
    IpcError::invalid(match failure {
        Failure::Resolve(said)
        | Failure::Connect(said)
        | Failure::Tls(said)
        | Failure::Send(said)
        | Failure::Read(said) => {
            format!("The image could not be loaded: {said}")
        }
        Failure::Timeout => "The image did not load in 30 s.".to_owned(),
        Failure::Stopped => "The load was stopped.".to_owned(),
    })
}

/// The image at `address`: kept, or asked for once.
///
/// # Errors
///
/// No `http` or `https` address; the request failed; the server answered no
/// `200`; the answer is past [`IMAGE_CAP`] or is no image.
pub async fn fetch(address: &str) -> Result<FetchedImage, IpcError> {
    if let Some((mime, bytes)) = KEPT
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .images
        .get(address)
    {
        return Ok(answered(address, mime, bytes));
    }
    let url = url::Url::parse(address)
        .map_err(|error| IpcError::invalid(format!("Not an address: {error}")))?;
    if url.scheme() != "http" && url.scheme() != "https" {
        return Err(IpcError::invalid(
            "knobas loads images from http and https addresses only.",
        ));
    }
    let direct = url.host_str().is_some_and(super::send::is_this_computer);
    let client = Client::new(&ClientOptions {
        connect_timeout: Duration::from_secs(10),
        extra_roots_pem: None,
        direct,
    })
    .map_err(IpcError::internal)?;
    let from = origin(&url);
    let mut request = person::Request {
        method: knobas_http::Method::GET,
        url: url.clone(),
        headers: Vec::new(),
        body: None,
    };
    let mut hops = 0;
    let answer = loop {
        let answer = client
            .send(
                request.clone(),
                Duration::from_secs(30),
                IMAGE_CAP + 1,
                std::future::pending(),
            )
            .await
            .map_err(|failure| failed(&failure))?;
        let next = if (300..400).contains(&answer.status) {
            answer
                .headers
                .iter()
                .find(|(n, _)| n.eq_ignore_ascii_case("location"))
                .and_then(|(_, v)| request.url.join(v).ok())
        } else {
            None
        };
        match next {
            Some(next) if origin(&next) == from && hops < MAX_HOPS => {
                hops += 1;
                request = person::Request {
                    url: next,
                    ..request
                };
            }
            Some(next) => {
                return Err(IpcError::invalid(format!(
                    "The address sends elsewhere, to {}: knobas follows a redirect on the image's own site only.",
                    next.host_str().unwrap_or("another site")
                )));
            }
            None => break answer,
        }
    };
    if answer.status != 200 {
        return Err(IpcError::invalid(format!(
            "The server answered {} {}.",
            answer.status, answer.reason
        )));
    }
    if answer.cut || answer.body.len() > IMAGE_CAP {
        return Err(IpcError::invalid(
            "The image is larger than 16 MiB, the most knobas loads.",
        ));
    }
    let content_type = answer
        .headers
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case("content-type"))
        .map(|(_, v)| v.clone())
        .unwrap_or_default();
    let Some(mime) = image_type(&answer.body) else {
        return Err(IpcError::invalid(format!(
            "The address answered {}.",
            what_it_was(&answer.body, &content_type)
        )));
    };
    keep(address, mime, &answer.body);
    Ok(answered(address, mime, &answer.body))
}

/// The hosts kept.
///
/// # Errors
///
/// The read fails.
pub async fn hosts(pool: &PgPool) -> Result<ImageHosts, IpcError> {
    Ok(read_value(pool, HOSTS_KEY)
        .await?
        .and_then(|value| serde_json::from_value(value).ok())
        .unwrap_or_default())
}

/// Stores the hosts kept, each once, lower case.
///
/// # Errors
///
/// The write fails.
pub async fn set_hosts(pool: &PgPool, hosts: ImageHosts) -> Result<ImageHosts, IpcError> {
    let mut out: Vec<String> = Vec::new();
    for host in hosts.hosts {
        let host = host.trim().to_ascii_lowercase();
        if !host.is_empty() && !out.contains(&host) {
            out.push(host);
        }
    }
    let kept = ImageHosts { hosts: out };
    let value = serde_json::to_value(&kept).map_err(IpcError::internal)?;
    write_value(pool, HOSTS_KEY, &value).await?;
    Ok(kept)
}

#[cfg(test)]
mod tests {
    use super::image_type;

    #[test]
    fn the_bytes_say_what_an_image_is() {
        assert_eq!(image_type(b"\x89PNG\r\n\x1a\n...."), Some("image/png"));
        assert_eq!(image_type(&[0xff, 0xd8, 0xff, 0xe0]), Some("image/jpeg"));
        assert_eq!(image_type(b"GIF89a"), Some("image/gif"));
        assert_eq!(image_type(b"RIFF\0\0\0\0WEBPVP8 "), Some("image/webp"));
        assert_eq!(
            image_type(b"\0\0\0\x1cftypavif\0\0\0\0"),
            Some("image/avif")
        );
        assert_eq!(
            image_type(b"<?xml version='1.0'?><svg xmlns='a'/>"),
            Some("image/svg+xml")
        );
        assert_eq!(image_type(b"<!doctype html><html>"), None);
        assert_eq!(image_type(b"%PDF-1.4"), None);
    }
}
