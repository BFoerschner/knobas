//! Keeping the credential out of what the instance answers (#1203).
//!
//! The host attaches the credential to a request for the instance, and a
//! server can echo a request's headers back: in a body (an echo endpoint,
//! an error page that quotes the request) or in a header. So every response
//! from the instance reaches the component with each spelling of the
//! secret the host sent replaced by [`REDACTED`], in the body as it streams
//! and in the headers, and a header holding one dropped whole.

use std::pin::Pin;
use std::task::{Context, Poll};

use bytes::Bytes;
use http_body::{Body, Frame};

/// What a spelling of the secret becomes.
pub const REDACTED: &[u8] = b"[redacted]";

/// The spellings of a secret to take out, longest first.
#[derive(Debug, Clone, Default)]
pub struct Needles(Vec<Vec<u8>>);

impl Needles {
    /// Every non-empty spelling in `spellings`, without repeats.
    #[must_use]
    pub fn new(spellings: impl IntoIterator<Item = String>) -> Self {
        let mut needles: Vec<Vec<u8>> = spellings
            .into_iter()
            .filter(|s| !s.is_empty())
            .map(String::into_bytes)
            .collect();
        needles.sort_by_key(|n| std::cmp::Reverse(n.len()));
        needles.dedup();
        Self(needles)
    }

    fn longest(&self) -> usize {
        self.0.first().map_or(0, Vec::len)
    }

    /// Whether `bytes` holds any spelling.
    #[must_use]
    pub fn found_in(&self, bytes: &[u8]) -> bool {
        self.0
            .iter()
            .any(|n| bytes.windows(n.len()).any(|w| w == n.as_slice()))
    }

    /// Redact `buf` into `out`. Unless `last`, stop where fewer bytes are
    /// left than the longest spelling, and answer where: those bytes may be
    /// the start of a spelling that the next chunk completes.
    fn redact(&self, buf: &[u8], out: &mut Vec<u8>, last: bool) -> usize {
        let keep = self.longest().saturating_sub(1);
        let mut at = 0;
        while at < buf.len() {
            if !last && buf.len() - at <= keep {
                return at;
            }
            if let Some(needle) = self.0.iter().find(|n| buf[at..].starts_with(n)) {
                out.extend_from_slice(REDACTED);
                at += needle.len();
            } else {
                out.push(buf[at]);
                at += 1;
            }
        }
        at
    }
}

/// A response body with every spelling of the secret replaced, across
/// chunk boundaries. Trailers are dropped: nothing knobas hosts reads them,
/// and a trailer could carry the secret as a header does.
pub struct RedactedBody<B> {
    inner: B,
    needles: Needles,
    /// Bytes held back from the last chunk.
    carry: Vec<u8>,
    done: bool,
}

impl<B> RedactedBody<B> {
    pub fn new(inner: B, needles: Needles) -> Self {
        Self {
            inner,
            needles,
            carry: Vec::new(),
            done: false,
        }
    }
}

impl<B> Body for RedactedBody<B>
where
    B: Body<Data = Bytes> + Unpin,
{
    type Data = Bytes;
    type Error = B::Error;

    fn poll_frame(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Bytes>, Self::Error>>> {
        let this = self.get_mut();
        loop {
            if this.done {
                return Poll::Ready(None);
            }
            match Pin::new(&mut this.inner).poll_frame(cx) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(Some(Err(error))) => return Poll::Ready(Some(Err(error))),
                Poll::Ready(Some(Ok(frame))) => {
                    let Ok(data) = frame.into_data() else {
                        continue;
                    };
                    this.carry.extend_from_slice(&data);
                    let mut out = Vec::with_capacity(this.carry.len());
                    let used = this.needles.redact(&this.carry, &mut out, false);
                    this.carry.drain(..used);
                    if !out.is_empty() {
                        return Poll::Ready(Some(Ok(Frame::data(Bytes::from(out)))));
                    }
                }
                Poll::Ready(None) => {
                    this.done = true;
                    let mut out = Vec::with_capacity(this.carry.len());
                    this.needles.redact(&this.carry, &mut out, true);
                    this.carry.clear();
                    if !out.is_empty() {
                        return Poll::Ready(Some(Ok(Frame::data(Bytes::from(out)))));
                    }
                }
            }
        }
    }
}

/// Take every header holding a spelling out of `headers`.
pub fn headers(headers: &mut http::HeaderMap, needles: &Needles) {
    let leaking: Vec<http::HeaderName> = headers
        .iter()
        .filter(|(_, value)| needles.found_in(value.as_bytes()))
        .map(|(name, _)| name.clone())
        .collect();
    for name in leaking {
        headers.remove(name);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use http_body_util::BodyExt;

    fn needles() -> Needles {
        Needles::new(["s3cret-token".to_owned(), "c2VjcmV0".to_owned()])
    }

    /// A body that answers `chunks`, one frame each.
    struct Chunks(std::vec::IntoIter<Bytes>);

    impl Body for Chunks {
        type Data = Bytes;
        type Error = std::convert::Infallible;

        fn poll_frame(
            mut self: Pin<&mut Self>,
            _: &mut Context<'_>,
        ) -> Poll<Option<Result<Frame<Bytes>, Self::Error>>> {
            Poll::Ready(self.0.next().map(|c| Ok(Frame::data(c))))
        }
    }

    async fn through(chunks: &[&str]) -> String {
        let chunks: Vec<Bytes> = chunks.iter().map(|c| Bytes::from(c.to_string())).collect();
        let body = RedactedBody::new(Chunks(chunks.into_iter()), needles());
        let bytes = body.collect().await.unwrap().to_bytes();
        String::from_utf8(bytes.to_vec()).unwrap()
    }

    #[tokio::test]
    async fn a_spelling_split_across_chunks_is_redacted() {
        assert_eq!(
            through(&["auth: Bearer s3cr", "et-tok", "en; basic c2Vj", "cmV0."]).await,
            "auth: Bearer [redacted]; basic [redacted]."
        );
    }

    #[tokio::test]
    async fn a_body_without_the_secret_passes_unchanged() {
        assert_eq!(
            through(&["s3cret-to", "ken-less? no: s3cret-tokeN"]).await,
            "[redacted]-less? no: s3cret-tokeN"
        );
        assert_eq!(through(&["short", ""]).await, "short");
    }

    #[test]
    fn a_header_holding_the_secret_is_dropped() {
        let mut map = http::HeaderMap::new();
        map.insert("x-echo", "Bearer s3cret-token".parse().unwrap());
        map.insert("content-type", "text/plain".parse().unwrap());
        headers(&mut map, &needles());
        assert!(map.get("x-echo").is_none());
        assert_eq!(map.get("content-type").unwrap(), "text/plain");
    }
}
