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
//! from its next request. Two sources at different quotas on one server,
//! syncing one after the other, would be handed a fresh burst on every
//! alternation if either kind of change started full. Both count the
//! **quiet**: the time since any of the origin's limiters last let a request
//! through (#1764). A **loosening** starts with what the looser rate refills
//! in the quiet, at most its burst, and nothing of what the limiter it
//! replaces had left (#1728, #1771). A **tightening** starts with the credit
//! the limiter it replaces had left (#1740): as many seconds of the stricter
//! rate as the old limiter's unspent requests were seconds of its own, or, if
//! more, what the stricter rate refills in the quiet, at most the stricter
//! burst. Either change after a quiet at least as long as the new burst's
//! refill therefore has the whole new burst, as in #1630; a loosening right
//! after a request starts drained, and a tightening has what was left.
//!
//! [`InstanceLimiter`] is public so a sender outside [`HttpClient`] -- the
//! wasm host's network policy (#1631) -- can wait on the same quota.
//!
//! **The clock is tokio's** (#1925): every limiter reads
//! [`tokio::time::Instant`] and waits with [`tokio::time::sleep`], as the
//! send budget and every timeout around it do. Outside a test that is the
//! monotonic clock, as before. A test that pauses tokio's clock moves the
//! quota with the budget, so it can assert what a quota lets through exactly,
//! where governor's own clock and timer would have kept wall time under it.

use std::cell::Cell;
use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::num::NonZeroU32;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex, PoisonError, Weak};
use std::time::Duration;

use governor::clock::{Clock, Reference};
use governor::middleware::NoOpMiddleware;
use governor::nanos::Nanos;
use governor::state::{InMemoryState, NotKeyed};
use governor::{Quota, RateLimiter};
use knobas_source::SourceError;
use tokio::time::Instant;

type DirectLimiter = RateLimiter<NotKeyed, InMemoryState, TokioClock, NoOpMiddleware<Tick>>;

/// Governor's clock, read from tokio's ([`tokio::time::Instant`]): the
/// monotonic clock, unless a test paused tokio's (#1925).
#[derive(Debug, Clone, Copy, Default)]
struct TokioClock;

impl Clock for TokioClock {
    type Instant = Tick;

    fn now(&self) -> Tick {
        Tick(Instant::now())
    }
}

/// One reading of [`TokioClock`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Tick(Instant);

impl std::ops::Add<Nanos> for Tick {
    type Output = Self;

    fn add(self, other: Nanos) -> Self {
        Self(self.0 + Duration::from(other))
    }
}

impl Reference for Tick {
    fn duration_since(&self, earlier: Self) -> Nanos {
        self.0.saturating_duration_since(earlier.0).into()
    }

    fn saturating_sub(&self, duration: Nanos) -> Self {
        Self(self.0.checked_sub(duration.into()).unwrap_or(self.0))
    }
}

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
        match tokio::time::timeout(within, paced.until_ready()).await {
            Ok(()) => {
                self.slot.granted();
                Ok(())
            }
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

tokio::task_local! {
    /// The quota a call under [`watch_local_quota`] stopped on, if one did.
    static STOPPED: Cell<Option<LocalQuota>>;
}

/// Run `call`, and hand back beside its output the [`LocalQuota`] an
/// [`HttpClient`](crate::HttpClient) send inside it stopped on before its
/// first attempt, if one did (#1729).
///
/// Such a send fails [`SourceError::Protocol`] with no status, which is also
/// what a refusal an adapter raises on its own account looks like. The write
/// queue has to tell the two apart -- the first sent nothing, so nothing was
/// refused -- and `SourceError` is frozen, so the mark travels here rather
/// than in the error. It is the quota itself, never a reading of the
/// message, so an adapter that wraps the message keeps it.
///
/// The mark is the call's, not the error's: it says a send inside `call`
/// stopped on the quota, and the last one to do so wins. A send that reached
/// the source and then ran out of budget on the limiter before a retry is
/// not marked: it ends with what the source answered. A send on a task
/// spawned from `call`, a wait in the wasm host, or a send outside any watch
/// is not seen. A watch inside a watch keeps its mark to itself.
pub async fn watch_local_quota<F: Future>(call: F) -> (F::Output, Option<LocalQuota>) {
    STOPPED
        .scope(Cell::new(None), async move {
            let output = call.await;
            (output, STOPPED.with(Cell::take))
        })
        .await
}

/// Mark the [`watch_local_quota`] this send runs under, if any, as stopped
/// on `quota`.
pub(crate) fn stopped_on(quota: &LocalQuota) {
    // Outside a watch nobody is asking, which is not an error.
    let _ = STOPPED.try_with(|stopped| stopped.set(Some(quota.clone())));
}

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

    /// How many requests this rate refills in `idle`, rounded down and not
    /// capped at the burst.
    fn refilled_in(self, idle: Duration) -> u128 {
        idle.as_nanos() * u128::from(self.per_second.get()) / 1_000_000_000
    }
}

/// A holder's stake in its origin's quota: the slot counts the quota it was
/// registered with for as long as this is alive. It carries nothing; only its
/// lifetime is read, through the [`Weak`] the slot keeps.
struct Registration;

/// One origin's share: its name, for the message, the limiter every holder
/// waits on, the quotas they asked for, and when it last let a request
/// through.
struct Slot {
    origin: String,
    held: Mutex<Held>,
    /// When the slot was built: what [`last_grant`](Self::last_grant)
    /// counts from.
    born: Instant,
    /// Nanoseconds from `born` to the last request any of the origin's
    /// limiters let through, or to `born` if none has (#1764). Written by
    /// every grant without the lock, so it only ever moves forward
    /// ([`granted`](Self::granted)).
    last_grant: AtomicU64,
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
            born: Instant::now(),
            last_grant: AtomicU64::new(0),
        }
    }

    /// Count `asked` until the returned registration is dropped, and set the
    /// quota to the stricter of every live holder's.
    ///
    /// A changed quota is a new governor limiter -- governor cannot change
    /// the quota of one it has built -- and every holder waits on it from its
    /// next request. One looser in either field starts with what its rate
    /// refilled since the origin's last request ([`Paced::loosened`]), so a
    /// loosening hands out what the quiet earned and nothing the limiter it
    /// replaces had left; a stricter one takes over what the old one had
    /// left, or what the stricter rate refilled since then
    /// ([`Paced::tightened`]).
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
                Paced::loosened(rate, self.idle())
            } else {
                Paced::tightened(&held.paced, rate, self.idle())
            });
        }
        registration
    }

    /// Note that one of the origin's limiters let a request through just now.
    ///
    /// Lock-free, since every grant runs it: the latest of two racing grants
    /// wins whichever stores first.
    fn granted(&self) {
        let now = u64::try_from(self.born.elapsed().as_nanos()).unwrap_or(u64::MAX);
        self.last_grant.fetch_max(now, Ordering::Relaxed);
    }

    /// How long the origin has gone without a grant on any of its limiters,
    /// or since the slot was built if it has had none.
    fn idle(&self) -> Duration {
        let last = Duration::from_nanos(self.last_grant.load(Ordering::Relaxed));
        self.born.elapsed().saturating_sub(last)
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
            limiter: RateLimiter::direct_with_clock(
                Quota::per_second(rate.per_second).allow_burst(rate.burst),
                TokioClock,
            ),
        }
    }

    /// Wait until the limiter lets one request through: governor's
    /// `until_ready`, sleeping on tokio's timer rather than its own, so the
    /// wait keeps the clock the limiter reads (#1925).
    async fn until_ready(&self) {
        while let Err(not_until) = self.limiter.check() {
            tokio::time::sleep(not_until.wait_time_from(TokioClock.now())).await;
        }
    }

    /// A limiter at `rate`, looser than the one it replaces in at least one
    /// field, for an origin that has let nothing through for `idle`: it
    /// starts with what `rate` refills in `idle`, at most its burst (#1771).
    ///
    /// Not with what the old limiter had left (#1728): a full stricter
    /// limiter, counted in seconds of the looser rate, would hand out a fresh
    /// burst on every alternation of two quotas. The quiet itself is safe to
    /// credit: nothing was sent for that long, so the looser quota would have
    /// refilled that much had it governed throughout. Right after a request
    /// the quiet is milliseconds and the limiter starts drained.
    fn loosened(rate: Rate, idle: Duration) -> Self {
        Self::with_left(rate, rate.refilled_in(idle))
    }

    /// A limiter at `rate` with `left` requests to send at once, at most its
    /// whole burst.
    fn with_left(rate: Rate, left: u128) -> Self {
        let left = u32::try_from(left).unwrap_or(u32::MAX);
        let paced = Self::new(rate);
        // A fresh limiter has its whole burst, and this is at most that, so
        // it always fits.
        paced.spend(rate.burst.get().saturating_sub(left));
        paced
    }

    /// Send `n` requests' worth through the limiter at once if it has them,
    /// and say whether it did. Nothing waits; zero always fits.
    fn spend(&self, n: u32) -> bool {
        NonZeroU32::new(n).is_none_or(|n| matches!(self.limiter.check_n(n), Ok(Ok(()))))
    }

    /// A limiter at `rate`, no looser in either field than `old`'s, taking
    /// over from `old` with the credit it had left (#1740), for an origin
    /// that has let nothing through for `idle`.
    ///
    /// The requests `old` could still send at once are counted in seconds of
    /// its own refill, and the new limiter starts with at least as many
    /// seconds of `rate`'s, rounded down, at most its burst. `old` is left
    /// spent: what it had now belongs to the new limiter.
    ///
    /// A full `old` only says nobody sent for its own burst's worth of
    /// refill, which can be fewer seconds than `rate`'s burst takes, so the
    /// new limiter starts with what `rate` refills in `idle` instead where
    /// that is more (#1764): nothing was sent for that long, so the stricter
    /// quota would have refilled that much had it governed throughout. Either
    /// way it gets what the stricter quota would have had, which is a fresh
    /// burst only after a quiet as long as that burst's refill.
    fn tightened(old: &Self, rate: Rate, idle: Duration) -> Self {
        let left = u128::from(old.take_left()) * u128::from(rate.per_second.get())
            / u128::from(old.rate.per_second.get());
        let refilled = rate.refilled_in(idle);
        // `with_left` caps it at the new burst.
        Self::with_left(rate, left.max(refilled))
    }

    /// Take every request this limiter could send at once, and count them.
    ///
    /// Governor has no reading of what a limiter has left, so it is taken in
    /// halving steps from the largest power of two within the burst: each
    /// step that fits is kept, and what is left after it is less than the
    /// step, so the sum is the count. A token that refills between two steps
    /// can be missed, which only ever counts low.
    fn take_left(&self) -> u32 {
        let mut left = 0;
        let mut step = 1 << self.rate.burst.ilog2();
        loop {
            if self.spend(step) {
                left += step;
            }
            if step == 1 {
                return left;
            }
            step /= 2;
        }
    }
}

/// `url` parsed, if it is an http(s) URL: what [`InstanceLimiter::for_url`]
/// and [`HttpClient::new`](crate::HttpClient::new) both accept.
///
/// No message repeats a username or password the URL carries (#1640): one
/// that carries any is refused before the scheme is looked at, and one that
/// does not parse is named with its userinfo masked.
pub(crate) fn http_url(url: &str) -> Result<url::Url, SourceError> {
    let parsed = url::Url::parse(url).map_err(|error| {
        SourceError::protocol(format!(
            "base url {:?}: {error}",
            knobas_source::instance::redact_userinfo(url)
        ))
    })?;
    refuse_userinfo_in(&parsed)?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(SourceError::protocol(format!(
            "base url {url:?} is not http(s)"
        )));
    }
    Ok(parsed)
}

/// [`crate::refuse_userinfo`] on a URL already parsed.
pub(crate) fn refuse_userinfo_in(url: &url::Url) -> Result<(), SourceError> {
    if url.username().is_empty() && url.password().is_none() {
        return Ok(());
    }
    Err(SourceError::protocol(
        "the base url carries a username or password; knobas sends the credential the source is \
         configured with, so the address must not carry one",
    ))
}

/// Quotas cannot be zero; a misconfigured `0/s` means "one", not "never".
fn nonzero(value: u32) -> NonZeroU32 {
    NonZeroU32::new(value).unwrap_or(NonZeroU32::MIN)
}

#[cfg(test)]
mod tests {
    //! The quiet a quota change is credited with is a duration, so the tests
    //! that time one run on tokio's paused clock, which the limiter reads
    //! (#1925): a sleep there is exactly as long as it says, and what a quiet
    //! refilled is a literal, not a bound read from the wall clock (#1802).

    use std::sync::Arc;
    use std::time::Duration;

    use super::{InstanceLimiter, Paced, PoisonError, Rate};

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

    /// What a limiter has left is counted exactly, and counting takes it.
    #[test]
    fn what_a_limiter_has_left_is_counted_and_taken() {
        let paced = Paced::new(Rate::new(1, 13));
        assert!(paced.spend(6));

        assert_eq!(paced.take_left(), 7);
        assert_eq!(paced.take_left(), 0, "the first count took all seven");
    }

    /// What a tightening starts with, for a limiter at `loose` that has had
    /// `spent` requests sent through it, tightened to `strict` (#1740).
    fn left_after_tightening(origin: &str, loose: Rate, spent: u32, strict: Rate) -> u32 {
        let url = url::Url::parse(origin).expect("url");
        let first = InstanceLimiter::for_origin(&url, loose);
        assert!(first.slot.current().spend(spent));
        let second = InstanceLimiter::for_origin(&url, strict);
        assert_eq!(second.slot.current().rate, strict, "the quota tightened");
        second.slot.current().take_left()
    }

    /// Two quotas whose bursts both last ten seconds of refill: a tightening
    /// of a limiter nobody spent from has the stricter quota's whole burst,
    /// as in #1630, and one half spent has half of it.
    #[test]
    fn a_tightening_takes_over_the_seconds_its_predecessor_had_left() {
        let unspent = left_after_tightening(
            "http://unspent.invalid:1740",
            Rate::new(2, 20),
            0,
            Rate::new(1, 10),
        );
        let half = left_after_tightening(
            "http://half.invalid:1740",
            Rate::new(2, 20),
            10,
            Rate::new(1, 10),
        );

        assert_eq!(
            unspent, 10,
            "an unspent limiter's tightening keeps its burst"
        );
        assert_eq!(
            half, 5,
            "ten of twenty left at 2/s is five seconds, five at 1/s"
        );
    }

    /// The credit is counted in seconds of refill, not as a share of the
    /// burst: twenty requests at ten a second are two seconds, which at one a
    /// second is two requests, not half of a burst of ten. A loose limiter
    /// built a moment before, which refills in two seconds, says nothing
    /// about the ten seconds the strict one takes (#1764).
    #[test]
    fn a_tightening_credits_seconds_of_refill() {
        let left = left_after_tightening(
            "http://seconds.invalid:1740",
            Rate::new(10, 20),
            0,
            Rate::new(1, 10),
        );

        assert_eq!(left, 2);
    }

    /// A limiter nobody sent on for half a second has refilled at least half
    /// a second of any stricter quota (#1764). The loose quota refills its
    /// burst of four in 40 ms, so its four unspent requests are 0.04 s, none
    /// at all at ten a second; ten a second for 0.5 s is five, so the
    /// stricter quota starts with its whole burst of four.
    #[tokio::test(start_paused = true)]
    async fn a_tightening_after_a_quiet_starts_with_what_the_quiet_refilled() {
        let url = url::Url::parse("http://quiet.invalid:1764").expect("url");
        let _loose = InstanceLimiter::for_origin(&url, Rate::new(100, 4));
        tokio::time::sleep(Duration::from_millis(500)).await;
        let strict = InstanceLimiter::for_origin(&url, Rate::new(10, 4));

        assert_eq!(strict.slot.current().rate, Rate::new(10, 4), "it tightened");
        assert_eq!(strict.slot.current().take_left(), 4);
    }

    /// The quiet is counted at the stricter rate, not the looser one (#1764):
    /// a tenth of a second is ten requests at a hundred a second, but none at
    /// one a second, and the loose limiter's four are none at one a second
    /// either. Short of a second's quiet, the stricter quota starts with
    /// nothing.
    #[tokio::test(start_paused = true)]
    async fn a_quiet_is_counted_at_the_stricter_rate() {
        let url = url::Url::parse("http://slow.invalid:1764").expect("url");
        let _loose = InstanceLimiter::for_origin(&url, Rate::new(100, 4));
        tokio::time::sleep(Duration::from_millis(100)).await;
        let strict = InstanceLimiter::for_origin(&url, Rate::new(1, 4));

        assert_eq!(strict.slot.current().take_left(), 0);
    }

    /// The quiet is counted from the last request let through, not from when
    /// the limiter was built (#1764): the same half second as above, ended by
    /// a request, leaves the stricter quota what the loose one had left in
    /// seconds -- none -- and what ten a second refills from that request to
    /// the count, which on a clock that has not moved is none either.
    /// Counted from the build it would have all four.
    #[tokio::test(start_paused = true)]
    async fn a_request_ends_the_quiet() {
        let url = url::Url::parse("http://ended.invalid:1764").expect("url");
        let loose = InstanceLimiter::for_origin(&url, Rate::new(100, 4));
        tokio::time::sleep(Duration::from_millis(500)).await;
        loose
            .until_ready_within(Duration::from_secs(1))
            .await
            .expect("a full limiter lets one through");
        let strict = InstanceLimiter::for_origin(&url, Rate::new(10, 4));

        assert_eq!(strict.slot.current().take_left(), 0);
    }

    /// A loosening does not end the quiet (#1764): a new limiter was built,
    /// but nothing was sent, so the stricter quota coming back half a second
    /// later has refilled its whole burst all the same.
    #[tokio::test(start_paused = true)]
    async fn a_loosening_does_not_end_the_quiet() {
        let url = url::Url::parse("http://loosened.invalid:1764").expect("url");
        drop(InstanceLimiter::for_origin(&url, Rate::new(10, 4)));
        tokio::time::sleep(Duration::from_millis(500)).await;
        let loose = InstanceLimiter::for_origin(&url, Rate::new(100, 4));
        assert_eq!(loose.slot.current().rate, Rate::new(100, 4), "it loosened");
        let strict = InstanceLimiter::for_origin(&url, Rate::new(10, 4));

        assert_eq!(strict.slot.current().take_left(), 4);
    }

    /// A loosening after a quiet starts with what the looser quota refilled
    /// in it (#1771): its burst of four takes 0.4 s at ten a second, and
    /// nothing was sent for half a second, so it starts with all four.
    #[tokio::test(start_paused = true)]
    async fn a_loosening_after_a_quiet_starts_with_what_the_quiet_refilled() {
        let url = url::Url::parse("http://quiet.invalid:1771").expect("url");
        drop(InstanceLimiter::for_origin(&url, Rate::new(1, 1)));
        tokio::time::sleep(Duration::from_millis(500)).await;
        let loose = InstanceLimiter::for_origin(&url, Rate::new(10, 4));

        assert_eq!(loose.slot.current().rate, Rate::new(10, 4), "it loosened");
        assert_eq!(loose.slot.current().take_left(), 4);
    }

    /// A loosening after a short quiet is credited in proportion to it, not
    /// with its whole burst (#1771): 150 ms at ten a second is one request of
    /// fifty. Counted in the wrong unit either way, the credit would be none
    /// or all fifty.
    #[tokio::test(start_paused = true)]
    async fn a_short_quiet_credits_a_loosening_in_proportion() {
        let url = url::Url::parse("http://short.invalid:1771").expect("url");
        drop(InstanceLimiter::for_origin(&url, Rate::new(1, 1)));
        tokio::time::sleep(Duration::from_millis(150)).await;
        let loose = InstanceLimiter::for_origin(&url, Rate::new(10, 50));

        assert_eq!(loose.slot.current().rate, Rate::new(10, 50), "it loosened");
        assert_eq!(
            loose.slot.current().take_left(),
            1,
            "of fifty, after 150 ms at ten a second"
        );
    }

    /// A loosening right after the stricter limiter was built hands out
    /// nothing, though that limiter was full (#1728): its ten requests are
    /// ten seconds at one a second, a thousand at a hundred a second, and
    /// crediting them would be the fresh burst on every alternation. What a
    /// hundred a second refills from the slot's build to the count is all it
    /// may have, and on a clock that has not moved that is nothing.
    #[tokio::test(start_paused = true)]
    async fn a_loosening_right_away_takes_nothing_from_a_full_limiter() {
        let url = url::Url::parse("http://full.invalid:1771").expect("url");
        drop(InstanceLimiter::for_origin(&url, Rate::new(1, 10)));
        let loose = InstanceLimiter::for_origin(&url, Rate::new(100, 20));

        assert_eq!(loose.slot.current().rate, Rate::new(100, 20), "it loosened");
        assert_eq!(loose.slot.current().take_left(), 0, "of twenty");
    }

    /// The quiet a loosening is credited with ends at the last request let
    /// through (#1771): half a second, then a request, then the loosening,
    /// leaves what twenty a second refills from that request to the count:
    /// nothing, on a clock that has not moved. Counted from the slot's build
    /// it would have all eight.
    #[tokio::test(start_paused = true)]
    async fn a_request_ends_the_quiet_before_a_loosening() {
        let url = url::Url::parse("http://ended.invalid:1771").expect("url");
        let strict = InstanceLimiter::for_origin(&url, Rate::new(10, 4));
        tokio::time::sleep(Duration::from_millis(500)).await;
        strict
            .until_ready_within(Duration::from_secs(1))
            .await
            .expect("a full limiter lets one through");
        drop(strict);
        let loose = InstanceLimiter::for_origin(&url, Rate::new(20, 8));

        assert_eq!(loose.slot.current().rate, Rate::new(20, 8), "it loosened");
        assert_eq!(loose.slot.current().take_left(), 0, "of eight");
    }

    /// A zero reads as one, as it always has: `0/s` means "one", not "never".
    #[test]
    fn a_zero_quota_reads_as_one() {
        assert_eq!(Rate::new(0, 0), Rate::new(1, 1));
    }
}
