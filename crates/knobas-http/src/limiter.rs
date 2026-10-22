//! The rate limit, one per instance (#1630).
//!
//! Every [`HttpClient`](crate::HttpClient) is built for a single call's worth
//! of work -- a sync run, a live read, one flush of the write queue -- and
//! several run at once, so a limiter inside the client would give each its own
//! fresh burst and one knobas several times the quota the reader configured.
//! The limiter therefore lives here, process-wide, keyed by the instance's
//! **origin** (scheme, host, port): every client built for that origin, in any
//! task, waits on the same one. Two sources on one server share it too, since
//! the server's lockout counts per server, not per source.
//!
//! Where two configurations name different quotas for one origin, **the
//! stricter wins**, field by field: the lower rate and the lower burst. A
//! quota only tightens for the life of the process; a source edited to a
//! looser one is paced by it from the next start.
//!
//! [`InstanceLimiter`] is public so a sender outside [`HttpClient`] -- the
//! wasm host's network policy (#1631) -- can wait on the same quota.

use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::num::NonZeroU32;
use std::sync::{Arc, LazyLock, Mutex, PoisonError};
use std::time::Duration;

use governor::clock::DefaultClock;
use governor::state::{InMemoryState, NotKeyed};
use governor::{Quota, RateLimiter};
use knobas_source::SourceError;

type DirectLimiter = RateLimiter<NotKeyed, InMemoryState, DefaultClock>;

/// One instance origin's share of the process-wide rate limit.
///
/// Cloning is cheap and every clone waits on the same quota, as does every
/// other `InstanceLimiter` built for the same origin.
#[derive(Clone)]
pub struct InstanceLimiter {
    instance: Arc<Instance>,
}

impl std::fmt::Debug for InstanceLimiter {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let rate = self.instance.current().rate;
        formatter
            .debug_struct("InstanceLimiter")
            .field("origin", &self.instance.origin)
            .field("per_second", &rate.per_second)
            .field("burst", &rate.burst)
            .finish()
    }
}

impl InstanceLimiter {
    /// The limiter for `url`'s origin, asking for `per_second` sustained and
    /// `burst` at once. A zero in either reads as one: a misconfigured `0/s`
    /// means "one", not "never".
    ///
    /// The first caller for an origin sets its quota; a later caller asking for
    /// less tightens it for every holder (the lower of each field), and one
    /// asking for more is paced by what is already there.
    ///
    /// # Errors
    ///
    /// [`SourceError::Protocol`] if `url` is not an http(s) URL.
    pub fn for_url(url: &str, per_second: u32, burst: u32) -> Result<Self, SourceError> {
        let parsed = url::Url::parse(url)
            .map_err(|error| SourceError::protocol(format!("base url {url:?}: {error}")))?;
        if !matches!(parsed.scheme(), "http" | "https") {
            return Err(SourceError::protocol(format!("base url {url:?} is not http(s)")));
        }
        Ok(Self::for_origin(&parsed, Rate::new(per_second, burst)))
    }

    /// [`for_url`](Self::for_url) for a URL already parsed and checked.
    pub(crate) fn for_origin(url: &url::Url, rate: Rate) -> Self {
        /// Every origin a client was built for in this process. An entry is
        /// never removed: a quota has to outlive the client that last used
        /// it, or the next client built a moment later starts on a fresh
        /// burst -- the bug this map exists to close.
        static INSTANCES: LazyLock<Mutex<HashMap<url::Origin, Arc<Instance>>>> =
            LazyLock::new(Mutex::default);

        let mut instances = INSTANCES.lock().unwrap_or_else(PoisonError::into_inner);
        let instance = match instances.entry(url.origin()) {
            Entry::Occupied(known) => Arc::clone(known.get()),
            Entry::Vacant(new) => Arc::clone(new.insert(Arc::new(Instance::new(
                url.origin().ascii_serialization(),
                rate,
            )))),
        };
        Self { instance }
    }

    /// Wait until the quota lets one request through, for at most `within`.
    ///
    /// # Errors
    ///
    /// [`LocalQuota`] if `within` ran out first. Nothing was sent: the wait
    /// is knobas' own, and the instance has not been asked anything.
    pub async fn until_ready_within(&self, within: Duration) -> Result<(), LocalQuota> {
        let paced = self.instance.current();
        match tokio::time::timeout(within, paced.limiter.until_ready()).await {
            Ok(()) => Ok(()),
            Err(_) => Err(self.ran_out(within)),
        }
    }

    /// The [`LocalQuota`] for a wait of `waited` that ran out: also what
    /// [`HttpClient`](crate::HttpClient) reports when a wait ended with no
    /// time left to send in.
    pub(crate) fn ran_out(&self, waited: Duration) -> LocalQuota {
        LocalQuota {
            origin: self.instance.origin.clone(),
            rate: self.instance.current().rate,
            waited,
        }
    }
}

/// A wait on the local quota that ran out of time before the quota let the
/// request through ([`InstanceLimiter::until_ready_within`]).
///
/// Its `Display` names the quota, for a [`SourceError::Protocol`]: the
/// instance was never asked, so the failure is knobas' configuration, never
/// the instance being unreachable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalQuota {
    origin: String,
    rate: Rate,
    waited: Duration,
}

impl std::fmt::Display for LocalQuota {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let Rate { per_second, burst } = self.rate;
        let requests = if per_second.get() == 1 {
            "request"
        } else {
            "requests"
        };
        write!(
            formatter,
            "knobas' local rate limit for {} ({per_second} {requests} a second, burst {burst}) \
             had no room for this request within {:.1}s, so it was not sent",
            self.origin,
            self.waited.as_secs_f64()
        )
    }
}

impl std::error::Error for LocalQuota {}

/// A quota: sustained requests a second and how many may go at once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Rate {
    per_second: NonZeroU32,
    burst: NonZeroU32,
}

impl Rate {
    pub(crate) fn new(per_second: u32, burst: u32) -> Self {
        Self {
            per_second: nonzero(per_second),
            burst: nonzero(burst),
        }
    }
}

/// One origin: its name, for the message, and the limiter every holder waits
/// on.
struct Instance {
    origin: String,
    paced: Mutex<Arc<Paced>>,
}

impl Instance {
    fn new(origin: String, rate: Rate) -> Self {
        Self {
            origin,
            paced: Mutex::new(Arc::new(Paced::new(rate))),
        }
    }

    /// The limiter as it stands. Cloned out of the lock, so a wait never holds
    /// it.
    fn current(&self) -> Arc<Paced> {
        Arc::clone(&self.paced.lock().unwrap_or_else(PoisonError::into_inner))
    }
}

/// A governor limiter and the quota it was built with.
struct Paced {
    rate: Rate,
    limiter: DirectLimiter,
}

impl Paced {
    fn new(rate: Rate) -> Self {
        Self {
            rate,
            limiter: RateLimiter::direct(
                Quota::per_second(rate.per_second).allow_burst(rate.burst),
            ),
        }
    }
}

/// Quotas cannot be zero; a misconfigured `0/s` means "one", not "never".
fn nonzero(value: u32) -> NonZeroU32 {
    NonZeroU32::new(value).unwrap_or(NonZeroU32::MIN)
}
