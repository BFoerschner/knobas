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
//! stricter wins**, field by field: the lower rate and the lower burst.
//!
//! **A quota loosens once nobody asks for the stricter one** (#1728). Each
//! [`InstanceLimiter`] -- and so each `HttpClient` -- holds a registration,
//! and its origin's quota is the stricter of the registrations still alive.
//! A client lives as long as the run, read or flush that built it, so a
//! source edited to a looser quota, a quota tried in a draft's *Test
//! connection*, or a deleted source stops holding the origin down once the
//! last client built from it is gone. The quota is worked out again when the
//! next limiter for the origin is built: one already held keeps waiting on
//! what governed when the last one was.
//!
//! A changed quota is a new governor limiter, which every holder waits on
//! from its next request. A **tightening** starts with its own (stricter)
//! burst. A **loosening** starts drained: two sources at different quotas on
//! one server, syncing one after the other, would otherwise be handed a
//! fresh burst on every alternation.
//!
//! [`InstanceLimiter`] is public so a sender outside [`HttpClient`] -- the
//! wasm host's network policy (#1631) -- can wait on the same quota.

use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::num::NonZeroU32;
use std::sync::{Arc, LazyLock, Mutex, PoisonError, Weak};
use std::time::Duration;

use governor::clock::DefaultClock;
use governor::state::{InMemoryState, NotKeyed};
use governor::{Quota, RateLimiter};
use knobas_source::SourceError;

type DirectLimiter = RateLimiter<NotKeyed, InMemoryState, DefaultClock>;

/// One instance origin's share of the process-wide rate limit.
///
/// Cloning is cheap and every clone waits on the same quota, as does every
/// other `InstanceLimiter` built for the same origin. A clone is the same
/// holder: the quota it asked for counts until the last clone is dropped.
#[derive(Clone)]
pub struct InstanceLimiter {
    slot: Arc<Slot>,
    /// What keeps this holder's quota counted ([`Slot::register`]).
    _registration: Arc<Registration>,
}

impl std::fmt::Debug for InstanceLimiter {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let rate = self.slot.current().rate;
        formatter
            .debug_struct("InstanceLimiter")
            .field("origin", &self.slot.origin)
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
    /// The origin's quota becomes the stricter of this one and every other
    /// still held (the lower of each field): asking for less tightens it for
    /// every holder, and asking for more is paced by what a live holder
    /// asked for. Once no holder of a stricter quota is left, the next
    /// limiter built for the origin loosens it.
    ///
    /// # Errors
    ///
    /// [`SourceError::Protocol`] if `url` is not an http(s) URL.
    pub fn for_url(url: &str, per_second: u32, burst: u32) -> Result<Self, SourceError> {
        Ok(Self::for_origin(
            &http_url(url)?,
            Rate::new(per_second, burst),
        ))
    }

    /// [`for_url`](Self::for_url) for a URL already parsed and checked.
    pub(crate) fn for_origin(url: &url::Url, rate: Rate) -> Self {
        /// Every origin a client was built for in this process. An entry is
        /// never removed: its limiter has to outlive the client that last
        /// used it, or the next client built a moment later starts on a
        /// fresh burst -- the bug this map exists to close.
        static INSTANCES: LazyLock<Mutex<HashMap<url::Origin, Arc<Slot>>>> =
            LazyLock::new(Mutex::default);

        let mut instances = INSTANCES.lock().unwrap_or_else(PoisonError::into_inner);
        let slot = match instances.entry(url.origin()) {
            Entry::Occupied(known) => Arc::clone(known.get()),
            Entry::Vacant(new) => Arc::clone(new.insert(Arc::new(Slot::new(
                url.origin().ascii_serialization(),
                rate,
            )))),
        };
        // The slot has a lock of its own: the map's is not needed past here.
        drop(instances);
        let registration = slot.register(rate);
        Self {
            slot,
            _registration: registration,
        }
    }

    /// Wait until the quota lets one request through, for at most `within`.
    ///
    /// # Errors
    ///
    /// [`LocalQuota`] if `within` ran out first. Nothing was sent: the wait
    /// is knobas' own, and the instance has not been asked anything.
    pub async fn until_ready_within(&self, within: Duration) -> Result<(), LocalQuota> {
        let paced = self.slot.current();
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
            origin: self.slot.origin.clone(),
            rate: self.slot.current().rate,
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

    /// The stricter of two quotas, field by field: the lower rate and the
    /// lower burst. A quota the server tolerates from one configuration says
    /// nothing about the other's, so neither field is taken from the looser.
    fn stricter(self, other: Self) -> Self {
        Self {
            per_second: self.per_second.min(other.per_second),
            burst: self.burst.min(other.burst),
        }
    }
}

/// A holder's stake in its origin's quota: the slot counts the quota it was
/// registered with for as long as this is alive. It carries nothing; only its
/// lifetime is read, through the [`Weak`] the slot keeps.
struct Registration;

/// One origin's share: its name, for the message, the limiter every holder
/// waits on, and the quotas they asked for.
struct Slot {
    origin: String,
    held: Mutex<Held>,
}

/// What a [`Slot`] guards: the limiter as it stands, and who asked for what.
struct Held {
    paced: Arc<Paced>,
    /// One per registration, alive or not: the dead ones go at the next
    /// [`Slot::register`].
    holders: Vec<(Rate, Weak<Registration>)>,
}

impl Slot {
    /// A slot with no holder yet: the first [`register`](Self::register)
    /// finds `rate` governing and changes nothing.
    fn new(origin: String, rate: Rate) -> Self {
        Self {
            origin,
            held: Mutex::new(Held {
                paced: Arc::new(Paced::new(rate)),
                holders: Vec::new(),
            }),
        }
    }

    /// Count `asked` until the returned registration is dropped, and set the
    /// quota to the stricter of every live holder's.
    ///
    /// A changed quota is a new governor limiter -- governor cannot change
    /// the quota of one it has built -- and every holder waits on it from its
    /// next request. A stricter one starts with its own burst; one looser in
    /// either field starts drained, so a loosening never sends a burst the
    /// limiter it replaces would have held back.
    fn register(&self, asked: Rate) -> Arc<Registration> {
        let registration = Arc::new(Registration);
        let mut held = self.held.lock().unwrap_or_else(PoisonError::into_inner);
        held.holders.retain(|(_, holder)| holder.strong_count() > 0);
        held.holders.push((asked, Arc::downgrade(&registration)));
        let rate = held
            .holders
            .iter()
            .map(|(rate, _)| *rate)
            .fold(asked, Rate::stricter);
        if rate != held.paced.rate {
            let loosens = rate.stricter(held.paced.rate) != rate;
            held.paced = Arc::new(if loosens {
                Paced::drained(rate)
            } else {
                Paced::new(rate)
            });
        }
        registration
    }

    /// The limiter as it stands. Cloned out of the lock, so a wait never holds
    /// it.
    fn current(&self) -> Arc<Paced> {
        Arc::clone(
            &self
                .held
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .paced,
        )
    }
}

/// A governor limiter and the quota it was built with.
struct Paced {
    rate: Rate,
    limiter: DirectLimiter,
}

impl Paced {
    /// A limiter at `rate` with its whole burst to hand out.
    fn new(rate: Rate) -> Self {
        Self {
            rate,
            limiter: RateLimiter::direct(
                Quota::per_second(rate.per_second).allow_burst(rate.burst),
            ),
        }
    }

    /// A limiter at `rate` whose burst is already spent: the first request
    /// waits for the refill.
    fn drained(rate: Rate) -> Self {
        let paced = Self::new(rate);
        // A fresh limiter always has its whole burst: neither error can come.
        let _ = paced.limiter.check_n(rate.burst);
        paced
    }
}

/// `url` parsed, if it is an http(s) URL: what [`InstanceLimiter::for_url`]
/// and [`HttpClient::new`](crate::HttpClient::new) both accept.
pub(crate) fn http_url(url: &str) -> Result<url::Url, SourceError> {
    let parsed = url::Url::parse(url)
        .map_err(|error| SourceError::protocol(format!("base url {url:?}: {error}")))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(SourceError::protocol(format!(
            "base url {url:?} is not http(s)"
        )));
    }
    Ok(parsed)
}

/// Quotas cannot be zero; a misconfigured `0/s` means "one", not "never".
fn nonzero(value: u32) -> NonZeroU32 {
    NonZeroU32::new(value).unwrap_or(NonZeroU32::MIN)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::{InstanceLimiter, PoisonError, Rate};

    /// One entry per origin, however its quota moves, and a dropped holder
    /// does not stay on the books: building a limiter prunes the dead.
    #[test]
    fn one_origin_keeps_one_slot_and_only_its_live_holders() {
        let url = url::Url::parse("http://slot.invalid:1728/a").expect("url");
        let strict = InstanceLimiter::for_origin(&url, Rate::new(1, 1));
        let same_origin = url::Url::parse("http://slot.invalid:1728/b").expect("url");
        for _ in 0..50 {
            drop(InstanceLimiter::for_origin(&same_origin, Rate::new(20, 20)));
        }
        let loose = InstanceLimiter::for_origin(&url, Rate::new(20, 20));

        assert!(Arc::ptr_eq(&strict.slot, &loose.slot));
        let held = loose
            .slot
            .held
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        assert_eq!(held.holders.len(), 2, "the strict holder and the loose one");
        assert_eq!(held.paced.rate, Rate::new(1, 1), "the strict holder lives");
    }

    /// Field by field: a configuration with the higher rate but the lower
    /// burst contributes its burst, and the other its rate.
    #[test]
    fn the_stricter_quota_takes_the_lower_of_each_field() {
        assert_eq!(Rate::new(5, 10).stricter(Rate::new(10, 5)), Rate::new(5, 5));
        assert_eq!(Rate::new(10, 5).stricter(Rate::new(5, 10)), Rate::new(5, 5));
        assert_eq!(
            Rate::new(5, 10).stricter(Rate::new(5, 10)),
            Rate::new(5, 10)
        );
    }

    /// A zero reads as one, as it always has: `0/s` means "one", not "never".
    #[test]
    fn a_zero_quota_reads_as_one() {
        assert_eq!(Rate::new(0, 0), Rate::new(1, 1));
    }
}
