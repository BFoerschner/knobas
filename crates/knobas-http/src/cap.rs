//! The cap on how much of one answer's body knobas reads (#1640, SPI-4).
//!
//! A proxy that answers a listing with gigabytes, or with a gzip bomb that
//! reqwest inflates on the way in, must not be read into memory until the OS
//! kills the app and the embedded Postgres session with it. So every body
//! [`HttpClient::send`](crate::HttpClient::send) hands back is read through
//! [`Capped`]: the declared length is checked before the first byte is read,
//! and what is decoded is counted as it arrives. Over the cap, the read fails
//! with [`OverCap`], which this crate's own reads turn into
//! `SourceError::Protocol` naming the cap, and which an adapter's own
//! `response.json()` meets as a failed read.
//!
//! The cap is on the body as decoded, not as it arrived: what is held in
//! memory is the inflated bytes.

use std::pin::Pin;
use std::task::{Context, Poll};

use bytes::Bytes;
use http_body::{Body, Frame, SizeHint};

/// The read of a body ran past the cap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct OverCap {
    pub(crate) cap: u64,
}

impl std::fmt::Display for OverCap {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "the answer is larger than {}, the most knobas reads of one answer",
            Size(self.cap)
        )
    }
}

impl std::error::Error for OverCap {}

/// A byte count as a reader would say it: `32 MiB`, `64 KiB`, `100 bytes`.
struct Size(u64);

impl std::fmt::Display for Size {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        const KIB: u64 = 1024;
        const MIB: u64 = 1024 * KIB;
        match self.0 {
            0 => formatter.write_str("0 bytes"),
            n if n % MIB == 0 => write!(formatter, "{} MiB", n / MIB),
            n if n % KIB == 0 => write!(formatter, "{} KiB", n / KIB),
            n => write!(formatter, "{n} bytes"),
        }
    }
}

/// `response` with its body read through [`Capped`] at `cap`.
///
/// Lazy: nothing is read here, so a write whose answer is over the cap has
/// still been sent and answered, and only the read of that answer fails.
pub(crate) fn capped(response: reqwest::Response, cap: u64) -> reqwest::Response {
    let response: http::Response<reqwest::Body> = response.into();
    let (parts, body) = response.into_parts();
    let body = reqwest::Body::wrap(Capped {
        inner: body,
        left: cap,
        cap,
        checked: false,
    });
    reqwest::Response::from(http::Response::from_parts(parts, body))
}

/// The cap a failed read ran into, if that is why it failed: anywhere in the
/// error's source chain, since reqwest wraps a body's error once or twice.
pub(crate) fn over_cap(error: &(dyn std::error::Error + 'static)) -> Option<OverCap> {
    let mut next = Some(error);
    while let Some(error) = next {
        if let Some(over) = error.downcast_ref::<OverCap>() {
            return Some(*over);
        }
        next = error.source();
    }
    None
}

/// A body that fails once more than `cap` bytes of it are read, or at once
/// when it declares more than that.
struct Capped<B> {
    inner: B,
    /// What may still be read.
    left: u64,
    cap: u64,
    /// Whether the declared length has been checked yet.
    checked: bool,
}

impl<B> Body for Capped<B>
where
    B: Body<Data = Bytes> + Unpin,
    B::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    type Data = Bytes;
    type Error = Box<dyn std::error::Error + Send + Sync>;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Bytes>, Self::Error>>> {
        let over = OverCap { cap: self.cap };
        // The declared length, before a byte is read: a body that says it is
        // over the cap is not waited for. For a plain body this is its
        // `content-length`; an inflated one declares nothing and is counted.
        if !self.checked {
            self.checked = true;
            if self.inner.size_hint().lower() > self.cap {
                self.left = 0;
                return Poll::Ready(Some(Err(over.into())));
            }
        }
        match Pin::new(&mut self.inner).poll_frame(context) {
            Poll::Ready(Some(Ok(frame))) => {
                if let Some(data) = frame.data_ref() {
                    let len = data.len() as u64;
                    if len > self.left {
                        self.left = 0;
                        return Poll::Ready(Some(Err(over.into())));
                    }
                    self.left -= len;
                }
                Poll::Ready(Some(Ok(frame)))
            }
            Poll::Ready(Some(Err(error))) => Poll::Ready(Some(Err(error.into()))),
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }

    fn is_end_stream(&self) -> bool {
        self.inner.is_end_stream()
    }

    fn size_hint(&self) -> SizeHint {
        self.inner.size_hint()
    }
}

#[cfg(test)]
mod tests {
    use super::Size;

    #[test]
    fn a_size_is_said_in_the_largest_whole_unit() {
        assert_eq!(Size(32 * 1024 * 1024).to_string(), "32 MiB");
        assert_eq!(Size(64 * 1024).to_string(), "64 KiB");
        assert_eq!(Size(1536).to_string(), "1536 bytes");
        assert_eq!(Size(0).to_string(), "0 bytes");
    }
}
