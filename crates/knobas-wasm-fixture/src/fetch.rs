//! The fixture's one network read (#1203): `GET` a URL through
//! `wasi:http/outgoing-handler`, following redirects itself, and keep every
//! hop's status, headers and body, so a test can see all the component was
//! handed. The host decides which requests leave, and with what credential.
//!
//! [`Options`] bend the request for the host's bounds (#1204): a first-byte
//! timeout in its `request-options`, or a `POST` of a body of some MiB,
//! written 4 KiB at a time so the component never holds it.

use wasip2::http::outgoing_handler;
use wasip2::http::types::{Fields, Method, OutgoingBody, OutgoingRequest, RequestOptions, Scheme};
use wasip2::io::streams::StreamError;

/// How the fetch's first request is sent.
#[derive(Clone, Copy, Default)]
pub struct Options {
    /// `request-options`' first-byte timeout, in milliseconds.
    pub first_byte_ms: Option<u64>,
    /// `POST` this many MiB instead of a `GET`.
    pub post_mib: Option<u64>,
}

/// Redirects the fixture follows before it gives up.
const MAX_HOPS: usize = 4;

/// One request and what came back.
pub struct Hop {
    pub url: String,
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

/// Every hop up to the last, and the error that ended the walk, if any.
pub struct Trace {
    pub hops: Vec<Hop>,
    pub error: Option<String>,
}

impl Trace {
    /// Everything the component received, as JSON.
    pub fn to_json(&self) -> serde_json::Value {
        let hops: Vec<serde_json::Value> = self
            .hops
            .iter()
            .map(|hop| {
                serde_json::json!({
                    "url": hop.url,
                    "status": hop.status,
                    "headers": hop.headers,
                    "body": hop.body,
                })
            })
            .collect();
        serde_json::json!({ "hops": hops, "error": self.error })
    }
}

/// `GET url` (or what `options` make of it), following up to [`MAX_HOPS`]
/// absolute redirects.
pub fn get(url: &str, options: Options) -> Trace {
    let mut trace = Trace {
        hops: Vec::new(),
        error: None,
    };
    let mut next = url.to_owned();
    for _ in 0..MAX_HOPS {
        match one(&next, options) {
            Err(error) => {
                trace.error = Some(format!("{next}: {error}"));
                return trace;
            }
            Ok(hop) => {
                let location = (300..400).contains(&hop.status).then(|| {
                    hop.headers
                        .iter()
                        .find(|(name, _)| name.eq_ignore_ascii_case("location"))
                        .map(|(_, value)| value.clone())
                });
                trace.hops.push(hop);
                match location.flatten() {
                    Some(location) => next = location,
                    None => return trace,
                }
            }
        }
    }
    trace.error = Some(format!("more than {MAX_HOPS} redirects"));
    trace
}

fn one(url: &str, options: Options) -> Result<Hop, String> {
    let (scheme, rest) = url
        .split_once("://")
        .ok_or_else(|| format!("{url:?} is not an absolute URL"))?;
    let (authority, path) = match rest.find('/') {
        Some(at) => rest.split_at(at),
        None => (rest, "/"),
    };
    let request = OutgoingRequest::new(Fields::new());
    let scheme = match scheme {
        "http" => Scheme::Http,
        "https" => Scheme::Https,
        other => Scheme::Other(other.to_owned()),
    };
    request
        .set_scheme(Some(&scheme))
        .map_err(|()| "scheme refused".to_owned())?;
    request
        .set_authority(Some(authority))
        .map_err(|()| "authority refused".to_owned())?;
    request
        .set_path_with_query(Some(path))
        .map_err(|()| "path refused".to_owned())?;
    if options.post_mib.is_some() {
        request
            .set_method(&Method::Post)
            .map_err(|()| "method refused".to_owned())?;
    }
    let body = request.body().map_err(|()| "no request body".to_owned())?;
    let request_options = match options.first_byte_ms {
        None => None,
        Some(ms) => {
            let set = RequestOptions::new();
            set.set_first_byte_timeout(Some(ms * 1_000_000))
                .map_err(|()| "first-byte timeout refused".to_owned())?;
            Some(set)
        }
    };
    let pending =
        outgoing_handler::handle(request, request_options).map_err(|e| format!("{e:?}"))?;
    if let Some(mib) = options.post_mib {
        let out = body.write().map_err(|()| "no body stream".to_owned())?;
        let chunk = [b'q'; 4096];
        // A write the host refused ends the body; the response says why.
        for _ in 0..mib * 256 {
            if out.blocking_write_and_flush(&chunk).is_err() {
                break;
            }
        }
        drop(out);
    }
    // A body the host stopped reading cannot finish; the response says why.
    let _ = OutgoingBody::finish(body, None);
    pending.subscribe().block();
    let response = pending
        .get()
        .ok_or("no response after the wait")?
        .map_err(|()| "the response was taken twice".to_owned())?
        .map_err(|e| format!("{e:?}"))?;
    let headers = response
        .headers()
        .entries()
        .into_iter()
        .map(|(name, value)| (name, String::from_utf8_lossy(&value).into_owned()))
        .collect();
    let status = response.status();
    let incoming = response.consume().map_err(|()| "no body".to_owned())?;
    let stream = incoming
        .stream()
        .map_err(|()| "no body stream".to_owned())?;
    let mut body = Vec::new();
    loop {
        match stream.blocking_read(64 * 1024) {
            Ok(chunk) => body.extend_from_slice(&chunk),
            Err(StreamError::Closed) => break,
            Err(StreamError::LastOperationFailed(error)) => {
                return Err(format!("reading the body: {}", error.to_debug_string()));
            }
        }
    }
    drop(stream);
    Ok(Hop {
        url: url.to_owned(),
        status,
        headers,
        body: String::from_utf8_lossy(&body).into_owned(),
    })
}
