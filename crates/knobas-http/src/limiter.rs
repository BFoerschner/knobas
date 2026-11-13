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
//! A changed quota is a new limiter, which every holder waits on
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
//! send budget and every timeout around it do. Outside a test that is
//! `std::time::Instant`, which the quiet a quota change is credited with
//! read already. On macOS it stops while the machine sleeps, where
//! governor's quanta clock read the raw counter, which runs on: a quota no
//! longer refills across a suspend, so the limiter wakes as it slept, and a
//! drained one hands out its next request at most one refill interval later
//! than before, never sooner. A test that pauses tokio's clock moves the
//! quota with the budget, so it can assert what a quota lets through exactly,
//! where governor's own clock and timer would have kept wall time under it.
//!
//! **A person's request goes ahead of the sync's** (the priority lane,
//! 2026-11-13). The quota is shared by the background sync and by the read a
//! person is waiting for, and a sync keeps it full, so before the lane a
//! person's read queued behind sync requests: a first open of a file on a
//! forge without a clone took five to seven seconds against a Gitea that
//! answers each request in fifty milliseconds. Every request now waits in a
//! [`Lane`], read from the task it is sent on ([`on_lane`]). A request in
//! [`Lane::Background`] -- a sync run, the write queue's tick -- yields while
//! any [`Lane::Interactive`] request for the same origin is waiting on the
//! quota or, sent through [`HttpClient`](crate::HttpClient) or the wasm
//! host, for its answer ([`Sending`]), and goes again once none is;
//! everything not marked is interactive. The answer counts because the
//! quota is not all a busy server spends: measured on the scale estate, a
//! Gitea that answers a pull request in 50 ms took 2.2 s with a backfill's
//! eight repositories in flight, so a sync that only gave up its turn at the
//! quota still loaded the server under the person's read. And the sync
//! **leaves half the burst** ([`Paced`]): a background request goes only
//! while half the burst could still go at once after it, so a page's first
//! requests go at once even while a sync keeps the quota full, rather than
//! one a refill apart. The lane only decides **who takes which request the
//! quota lets through**: every request of either lane passes the same
//! limiter, so the origin sees no more requests a second, and no bigger
//! burst, than before.
//!
//! **The sync is not starved** ([`BACKGROUND_FLOOR`]): once an origin's
//! background requests have stood aside for a second without one let
//! through, the next one stops yielding and keeps no reserve: it takes the
//! next request the quota lets through, as any request did before the lane. A person's requests
//! that trickle below the quota leave the sync the rest of it; ones that
//! fill it for longer than a second still leave the sync about one request
//! a second.

use std::cell::Cell;
use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::num::NonZeroU32;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, LazyLock, Mutex, PoisonError, Weak};
use std::time::Duration;

use knobas_source::SourceError;
use tokio::time::Instant;

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
    ///
    /// The wait is in the [`Lane`] of the task it runs on ([`current_lane`]):
    /// a background one yields to interactive ones for the same origin. An
    /// interactive request counts as waiting until this returns; one that
    /// goes on counting until its answer is in is
    /// [`until_ready_sending`](Self::until_ready_sending)'s.
    pub async fn until_ready_within(&self, within: Duration) -> Result<(), LocalQuota> {
        self.until_ready_sending(within).await.map(drop)
    }

    /// [`until_ready_within`](Self::until_ready_within), handing back the
    /// [`Sending`] that keeps an interactive request counted until it is
    /// dropped: held over the request until its answer is in, it keeps the
    /// origin's background requests standing aside for the whole of what a
    /// person waits for, not only its wait on the quota (the priority lane,
    /// 2026-11-13). A background request's holds nothing.
    ///
    /// # Errors
    ///
    /// As [`until_ready_within`](Self::until_ready_within).
    pub async fn until_ready_sending(&self, within: Duration) -> Result<Sending, LocalQuota> {
        let lane = current_lane();
        let sending = match lane {
            Lane::Interactive => Sending::enter(&self.slot),
            Lane::Background => Sending(None),
        };
        match tokio::time::timeout(within, self.slot.until_ready(lane)).await {
            Ok(()) => {
                self.slot.granted();
                Ok(sending)
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

/// Who a request is for, which decides who takes the next request the
/// quota lets through: a person's requests go first ([module](self) docs).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Lane {
    /// A request somebody is waiting for: a live read behind an open page or
    /// a hover's prefetch, a *Test connection*, a write they pressed and
    /// watch a toast for. What every request not marked otherwise is.
    #[default]
    Interactive,
    /// A request nobody is waiting on: a sync run, the write queue's tick.
    /// It yields while an interactive request for its origin waits, for at
    /// most [`BACKGROUND_FLOOR`].
    Background,
}

/// The longest an origin's background requests yield to interactive ones
/// without one let through: past it, the next takes the next request the
/// quota lets through like any other, so the sync goes on, about a request a
/// second, even while a person's fill the quota.
pub const BACKGROUND_FLOOR: Duration = Duration::from_secs(1);

tokio::task_local! {
    /// The lane of the requests sent on this task, where [`on_lane`] set one.
    static LANE: Lane;
}

/// Run `call` with every request sent inside it -- through an
/// [`HttpClient`](crate::HttpClient) or an [`InstanceLimiter`] -- waiting in
/// `lane`. A lane inside a lane holds for its own call.
///
/// Like any task-local, it holds on the task `call` is polled on: a task
/// spawned from `call` starts outside it, in [`Lane::Interactive`]. A sender
/// that drives its requests elsewhere takes [`current_lane`] with it.
pub async fn on_lane<F: Future>(lane: Lane, call: F) -> F::Output {
    LANE.scope(lane, call).await
}

/// The lane a request sent here and now waits in: the innermost
/// [`on_lane`]'s, or [`Lane::Interactive`] outside any.
#[must_use]
pub fn current_lane() -> Lane {
    LANE.try_with(|lane| *lane).unwrap_or_default()
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

    /// How many of the burst a background request leaves for a person's
    /// ([`Paced`]): half, rounded down, so a burst of one keeps none.
    fn reserve(self) -> u32 {
        self.burst.get() / 2
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
    /// Nanoseconds from `born` to the last [`Lane::Background`] request let
    /// through, or `0`: what [`BACKGROUND_FLOOR`] counts from.
    last_background: AtomicU64,
    /// How many [`Lane::Interactive`] requests are waiting on this origin's
    /// quota or for its answer right now ([`Sending`]): while any is, a
    /// background one yields.
    interactive: AtomicUsize,
    /// Told when the last interactive request is answered or gives up: what
    /// a yielding background request wakes on.
    yielded: tokio::sync::Notify,
}

/// An interactive request's place among its origin's
/// ([`InstanceLimiter::until_ready_sending`]): while it is held, the
/// origin's background requests stand aside. Given back when dropped,
/// however the request ends: answered, out of time, or cancelled. A
/// background request's holds no place.
#[must_use = "dropping it ends the request's place at once"]
pub struct Sending(Option<Arc<Slot>>);

impl Sending {
    fn enter(slot: &Arc<Slot>) -> Self {
        slot.interactive.fetch_add(1, Ordering::SeqCst);
        Self(Some(Arc::clone(slot)))
    }
}

impl std::fmt::Debug for Sending {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_tuple("Sending")
            .field(&self.0.as_ref().map(|slot| &slot.origin))
            .finish()
    }
}

impl Drop for Sending {
    fn drop(&mut self) {
        if let Some(slot) = self.0.take()
            && slot.interactive.fetch_sub(1, Ordering::SeqCst) == 1
        {
            slot.yielded.notify_waiters();
        }
    }
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
            last_background: AtomicU64::new(0),
            interactive: AtomicUsize::new(0),
            yielded: tokio::sync::Notify::new(),
        }
    }

    /// Wait until the quota lets one request of `lane` through.
    ///
    /// Both lanes wait on the limiter as it stood when the wait began, as
    /// every wait did before the lane (#1728): a quota changed meanwhile
    /// governs from the next request. An interactive request waits as
    /// before, counted by its caller's [`Sending`]. A background one checks
    /// for waiting interactive requests before each try at the limiter, and
    /// stands aside while there are any ([`yield_to_interactive`]), so a
    /// request the quota lets through while an interactive one waits for it
    /// goes to that one.
    ///
    /// [`yield_to_interactive`]: Self::yield_to_interactive
    async fn until_ready(&self, lane: Lane) {
        let paced = self.current();
        match lane {
            // Counted by the caller's [`Sending`], from before this wait.
            Lane::Interactive => paced.until_ready().await,
            Lane::Background => {
                let mut stood_aside = None;
                let began = Instant::now();
                loop {
                    self.yield_to_interactive(&mut stood_aside).await;
                    // The reserve is kept until the floor: past it, the sync
                    // takes what any request would.
                    let floor = began.max(self.last_background()) + BACKGROUND_FLOOR;
                    let reserve = if Instant::now() >= floor {
                        0
                    } else {
                        paced.rate.reserve()
                    };
                    match paced.take(1, reserve) {
                        Ok(()) => {
                            self.background_granted();
                            return;
                        }
                        // Woken at the floor at the latest, where the reserve
                        // is no longer kept from it.
                        Err(at) => {
                            tokio::time::sleep_until(at.min(floor.max(Instant::now()))).await
                        }
                    }
                }
            }
        }
    }

    /// Return once no interactive request waits on this origin, or once the
    /// origin's background requests have gone [`BACKGROUND_FLOOR`] without
    /// one let through while this one stood aside, whichever is first.
    ///
    /// The floor counts from the later of the origin's last background
    /// request let through and when this one first stood aside
    /// (`stood_aside`, kept across its tries): a sync request that only
    /// queued behind its own run's is not starved, and a sync that just
    /// started after a quiet still yields its first request.
    async fn yield_to_interactive(&self, stood_aside: &mut Option<Instant>) {
        loop {
            let notified = self.yielded.notified();
            let mut notified = std::pin::pin!(notified);
            // Registered before the count is read, so a last interactive
            // request that goes between the read and the wait still wakes it.
            notified.as_mut().enable();
            if self.interactive.load(Ordering::SeqCst) == 0 {
                return;
            }
            let since = *stood_aside.get_or_insert_with(Instant::now);
            let until = since.max(self.last_background()) + BACKGROUND_FLOOR;
            if Instant::now() >= until {
                return;
            }
            // Woken at `until`, the floor is read again: another background
            // request let through meanwhile moved it.
            tokio::select! {
                () = notified => {}
                () = tokio::time::sleep_until(until) => {}
            }
        }
    }

    /// Note that a background request was let through just now.
    fn background_granted(&self) {
        let now = u64::try_from(self.born.elapsed().as_nanos()).unwrap_or(u64::MAX);
        self.last_background.fetch_max(now, Ordering::Relaxed);
    }

    /// When the origin last let a background request through, or when the
    /// slot was built if it has not.
    fn last_background(&self) -> Instant {
        self.born + Duration::from_nanos(self.last_background.load(Ordering::Relaxed))
    }

    /// Count `asked` until the returned registration is dropped, and set the
    /// quota to the stricter of every live holder's.
    ///
    /// A changed quota is a new limiter ([`Paced`] keeps the quota it was
    /// built with), and every holder waits on it from its
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

/// One quota's limiter: the generic cell rate algorithm (GCRA) on tokio's
/// clock, the arithmetic governor did before it (2026-11-13), with a
/// tolerance a caller may take smaller.
///
/// It keeps one instant, the **theoretical arrival time**: when the next
/// request would be on time if every request so far had gone exactly one
/// [interval](Self::interval) apart. A request goes when that is at most
/// `burst - 1` intervals ahead of now, and moves it one interval on: so a
/// full limiter lets `burst` through at once and then one an interval. A
/// caller that keeps a **reserve** of `r` asks for it to be at most
/// `burst - 1 - r` intervals ahead: it goes only while `r` more could still
/// go at once after it, which no other caller is held to. The background
/// lane keeps half the burst that way ([`Rate::reserve`]), so a person's
/// first requests go at once even while a sync keeps the quota full; the
/// server sees no more at once, nor more a second, than the burst and rate
/// allow whoever sends them.
struct Paced {
    rate: Rate,
    /// One request's share of a second: a second over the rate, rounded
    /// down to the nanosecond as governor's did.
    interval: Duration,
    /// The theoretical arrival time; `None` for a limiter nothing has gone
    /// through, which has its whole burst.
    tat: Mutex<Option<Instant>>,
}

impl Paced {
    /// A limiter at `rate` with its whole burst to hand out.
    fn new(rate: Rate) -> Self {
        Self {
            rate,
            interval: Duration::from_secs(1) / rate.per_second.get(),
            tat: Mutex::new(None),
        }
    }

    /// Let `n` requests through at once if they fit with `reserve` more
    /// still able to go after them, or say when they would.
    ///
    /// # Errors
    ///
    /// The instant they would fit, if no one else went before; `None`-like
    /// far future where `n` can never fit (more than the burst).
    fn take(&self, n: u32, reserve: u32) -> Result<(), Instant> {
        let now = Instant::now();
        let mut tat = self.tat.lock().unwrap_or_else(PoisonError::into_inner);
        let burst = self.rate.burst.get();
        let tolerance = self.interval * burst.saturating_sub(1).saturating_sub(reserve);
        let from = tat.unwrap_or(now).max(now);
        // The last of the `n` is on time when its own arrival time is within
        // the tolerance of now.
        let last = from + self.interval * n.saturating_sub(1);
        let ahead = last.saturating_duration_since(now);
        if n <= burst && ahead <= tolerance {
            *tat = Some(from + self.interval * n);
            Ok(())
        } else {
            Err(now + ahead.saturating_sub(tolerance))
        }
    }

    /// How many requests could go at once right now.
    fn left(&self) -> u32 {
        let now = Instant::now();
        let tat = *self.tat.lock().unwrap_or_else(PoisonError::into_inner);
        let ahead = tat.unwrap_or(now).saturating_duration_since(now);
        let tolerance = self.interval * self.rate.burst.get();
        let room = tolerance.saturating_sub(ahead);
        u32::try_from(room.as_nanos() / self.interval.as_nanos().max(1)).unwrap_or(u32::MAX)
    }

    /// Wait until the limiter lets one request through, on tokio's timer, so
    /// the wait keeps the clock the limiter reads (#1925).
    async fn until_ready(&self) {
        while let Err(at) = self.take(1, 0) {
            tokio::time::sleep_until(at).await;
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
        n == 0 || self.take(n, 0).is_ok()
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
    fn take_left(&self) -> u32 {
        let left = self.left();
        // What `left` counted fits unless another caller took some between
        // the two, and then it is counted again.
        if self.spend(left) {
            left
        } else {
            self.take_left()
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

    mod lane {
        //! The priority lane (2026-11-13), on tokio's paused clock: a sleep
        //! is exactly as long as it says, so when each request is let
        //! through is a literal.

        use std::sync::{Arc, Mutex};
        use std::time::Duration;

        use tokio::time::Instant;

        use super::super::{BACKGROUND_FLOOR, InstanceLimiter, Lane, current_lane, on_lane};
        use super::Rate;

        /// Who was let through, and when, counted from the test's start.
        type Grants = Arc<Mutex<Vec<(Lane, Duration)>>>;

        /// One request in `lane` on `limiter`, sent `after` from `start`,
        /// recorded in `grants` once let through.
        fn send(
            limiter: &InstanceLimiter,
            lane: Lane,
            start: Instant,
            after: Duration,
            grants: &Grants,
        ) -> tokio::task::JoinHandle<()> {
            let (limiter, grants) = (limiter.clone(), Arc::clone(grants));
            tokio::spawn(on_lane(lane, async move {
                tokio::time::sleep_until(start + after).await;
                limiter
                    .until_ready_within(Duration::from_secs(600))
                    .await
                    .expect("let through");
                grants.lock().unwrap().push((lane, start.elapsed()));
            }))
        }

        async fn all(handles: Vec<tokio::task::JoinHandle<()>>) {
            for handle in handles {
                handle.await.expect("the request's task");
            }
        }

        fn ms(n: u64) -> Duration {
            Duration::from_millis(n)
        }

        /// Whether `grants`, sorted, are requests a server held to `rate`
        /// a second with a burst of `burst` could have been sent: replayed
        /// through the quota's own arithmetic, none early. The first one
        /// early, where one is.
        fn early(grants: &[Duration], rate: u32, burst: u32) -> Option<Duration> {
            let interval = Duration::from_secs(1) / rate;
            let mut tat = Duration::ZERO;
            grants.iter().copied().find(|&at| {
                let from = tat.max(at);
                let early = from - at > interval * (burst - 1);
                tat = from + interval;
                early
            })
        }

        /// A sync's twenty requests queued at the start, and a person's
        /// eight arriving a few at a time while they wait, at ten a second
        /// with a burst of five: run once with the person's requests in the
        /// interactive lane and once in the background one, beside the sync
        /// as they waited before the lane. Both runs are within the quota,
        /// replayed through its arithmetic: the origin sees no more a second
        /// and no bigger burst whoever sends. With the lane, the person's
        /// first two of each few go the moment they arrive, out of the half
        /// of the burst the sync leaves, and the third at the next refill;
        /// the sync ends when it would have with the burst its own but for
        /// the two it left, 200 ms at ten a second.
        #[tokio::test(start_paused = true)]
        async fn the_lane_changes_who_goes_and_never_how_many() {
            async fn run(origin: &str, person: Lane) -> Vec<(Lane, Duration)> {
                let url = url::Url::parse(origin).expect("url");
                let limiter = InstanceLimiter::for_origin(&url, Rate::new(10, 5));
                let grants = Grants::default();
                let start = Instant::now();
                let mut handles = Vec::new();
                for _ in 0..20 {
                    handles.push(send(&limiter, Lane::Background, start, ms(0), &grants));
                }
                // Never at an instant the quota lets one through: who of two
                // arriving together goes first is the runtime's, not the lane's.
                for at in [350, 350, 360, 905, 910, 1_501, 1_501, 1_502] {
                    handles.push(send(&limiter, person, start, ms(at), &grants));
                }
                all(handles).await;
                let mut grants = grants.lock().unwrap().clone();
                grants.sort_by_key(|g| g.1);
                grants
            }

            let laned = run("http://laned.invalid:2479", Lane::Interactive).await;
            let unlaned = run("http://unlaned.invalid:2479", Lane::Background).await;

            let when = |grants: &[(Lane, Duration)]| grants.iter().map(|g| g.1).collect::<Vec<_>>();
            assert_eq!((laned.len(), unlaned.len()), (28, 28));
            assert_eq!(early(&when(&laned), 10, 5), None, "{laned:?}");
            assert_eq!(early(&when(&unlaned), 10, 5), None, "{unlaned:?}");
            assert_eq!(laned.last().map(|g| g.1), Some(ms(2_500)));
            let person: Vec<Duration> = laned
                .iter()
                .filter(|g| g.0 == Lane::Interactive)
                .map(|g| g.1)
                .collect();
            assert_eq!(
                person,
                [350, 350, 400, 905, 910, 1_501, 1_501, 1_600].map(ms)
            );
        }

        /// The checker above is no rubber stamp: a sixth request at once, or
        /// one sooner than a refill after the burst, is early.
        #[test]
        fn a_burst_past_the_quota_is_early() {
            assert_eq!(early(&[ms(0); 5], 10, 5), None);
            assert_eq!(early(&[ms(0); 6], 10, 5), Some(ms(0)));
            let mut steady = vec![ms(0); 5];
            steady.extend([ms(100), ms(150)]);
            assert_eq!(early(&steady, 10, 5), Some(ms(150)));
        }

        /// A sync keeping the quota full leaves half the burst: a person's
        /// page of two requests goes at once, where it waited a refill for
        /// each before the reserve (the priority lane, 2026-11-13).
        #[tokio::test(start_paused = true)]
        async fn a_persons_first_requests_go_at_once_beside_a_full_sync() {
            let url = url::Url::parse("http://reserve.invalid:2479").expect("url");
            let limiter = InstanceLimiter::for_origin(&url, Rate::new(10, 4));
            let grants = Grants::default();
            let start = Instant::now();
            let sync = {
                let (limiter, grants) = (limiter.clone(), Arc::clone(&grants));
                tokio::spawn(on_lane(Lane::Background, async move {
                    while start.elapsed() < ms(2_000) {
                        limiter
                            .until_ready_within(Duration::from_secs(60))
                            .await
                            .expect("let through");
                        grants
                            .lock()
                            .unwrap()
                            .push((Lane::Background, start.elapsed()));
                    }
                }))
            };
            let person: Vec<_> = [1_050, 1_050]
                .into_iter()
                .map(|at| send(&limiter, Lane::Interactive, start, ms(at), &grants))
                .collect();
            all(person).await;
            sync.await.expect("the sync");

            let mut grants = grants.lock().unwrap().clone();
            grants.sort_by_key(|g| g.1);
            let person: Vec<Duration> = grants
                .iter()
                .filter(|g| g.0 == Lane::Interactive)
                .map(|g| g.1)
                .collect();
            assert_eq!(person, [ms(1_050), ms(1_050)]);
            assert_eq!(
                early(&grants.iter().map(|g| g.1).collect::<Vec<_>>(), 10, 4),
                None
            );
        }

        /// The quota spent and four sync requests queued behind it: a
        /// person's request that arrives next is let through next, at the
        /// next refill, ahead of all four.
        #[tokio::test(start_paused = true)]
        async fn a_persons_request_takes_the_next_request_ahead_of_a_queued_sync() {
            let url = url::Url::parse("http://next.invalid:2479").expect("url");
            let limiter = InstanceLimiter::for_origin(&url, Rate::new(10, 1));
            let grants = Grants::default();
            let start = Instant::now();
            let mut handles = Vec::new();
            for _ in 0..5 {
                handles.push(send(&limiter, Lane::Background, start, ms(0), &grants));
            }
            handles.push(send(&limiter, Lane::Interactive, start, ms(10), &grants));
            all(handles).await;

            let grants = grants.lock().unwrap().clone();
            assert_eq!(grants[0], (Lane::Background, ms(0)), "the burst");
            assert_eq!(grants[1], (Lane::Interactive, ms(100)), "the next one");
            assert!(grants[2..].iter().all(|g| g.0 == Lane::Background));
            assert_eq!(grants[5].1, ms(500), "and the sync goes on after it");
        }

        /// A person's requests trickling at two a second, under a quota of
        /// ten, leave the sync the other eight: over three seconds a sync
        /// that always has a request waiting is let through 25 times
        /// besides the person's six, and each of those waits at most one
        /// refill.
        #[tokio::test(start_paused = true)]
        async fn the_sync_goes_on_while_a_persons_requests_trickle() {
            let url = url::Url::parse("http://trickle.invalid:2479").expect("url");
            let limiter = InstanceLimiter::for_origin(&url, Rate::new(10, 1));
            let grants = Grants::default();
            let start = Instant::now();
            let sync = {
                let (limiter, grants) = (limiter.clone(), Arc::clone(&grants));
                tokio::spawn(on_lane(Lane::Background, async move {
                    while start.elapsed() < ms(3_000) {
                        limiter
                            .until_ready_within(Duration::from_secs(60))
                            .await
                            .expect("let through");
                        grants
                            .lock()
                            .unwrap()
                            .push((Lane::Background, start.elapsed()));
                    }
                }))
            };
            let person: Vec<_> = [250, 750, 1_250, 1_750, 2_250, 2_750]
                .into_iter()
                .map(|at| send(&limiter, Lane::Interactive, start, ms(at), &grants))
                .collect();
            all(person).await;
            sync.await.expect("the sync");

            let grants = grants.lock().unwrap().clone();
            let synced = grants.iter().filter(|g| g.0 == Lane::Background).count();
            assert_eq!(synced, 25, "{grants:?}");
            let person: Vec<Duration> = grants
                .iter()
                .filter(|g| g.0 == Lane::Interactive)
                .map(|g| g.1)
                .collect();
            assert_eq!(person, [300, 800, 1_300, 1_800, 2_300, 2_800].map(ms));
        }

        /// Fifty of a person's requests, five seconds of the quota, do not
        /// hold a sync request for five seconds: it stands aside for
        /// [`BACKGROUND_FLOOR`] and then takes the next request the quota
        /// lets through.
        #[tokio::test(start_paused = true)]
        async fn the_sync_is_not_starved_by_a_person_who_fills_the_quota() {
            let url = url::Url::parse("http://starved.invalid:2479").expect("url");
            let limiter = InstanceLimiter::for_origin(&url, Rate::new(10, 1));
            let grants = Grants::default();
            let start = Instant::now();
            let mut handles = Vec::new();
            for _ in 0..50 {
                handles.push(send(&limiter, Lane::Interactive, start, ms(0), &grants));
            }
            handles.push(send(&limiter, Lane::Background, start, ms(0), &grants));
            all(handles).await;

            let grants = grants.lock().unwrap().clone();
            let synced = grants
                .iter()
                .find(|g| g.0 == Lane::Background)
                .expect("the sync's request")
                .1;
            assert!(
                synced >= BACKGROUND_FLOOR && synced <= BACKGROUND_FLOOR + ms(100),
                "{synced:?}: {grants:?}"
            );
        }

        /// A person's request whose answer is not in holds the sync off as
        /// one waiting on the quota does ([`super::super::Sending`]): the
        /// quota has room, but a sync request sent while the person's is in
        /// flight goes the moment its answer is in. One whose answer takes
        /// five seconds holds it [`BACKGROUND_FLOOR`] and no longer.
        #[tokio::test(start_paused = true)]
        async fn the_sync_stands_aside_until_a_persons_answer_is_in() {
            async fn held_for(origin: &str, flight: Duration) -> Duration {
                let url = url::Url::parse(origin).expect("url");
                let limiter = InstanceLimiter::for_origin(&url, Rate::new(10, 5));
                let start = Instant::now();
                let sending = on_lane(
                    Lane::Interactive,
                    limiter.until_ready_sending(Duration::from_secs(60)),
                )
                .await
                .expect("the burst has room");
                let sync = {
                    let limiter = limiter.clone();
                    tokio::spawn(on_lane(Lane::Background, async move {
                        tokio::time::sleep(ms(10)).await;
                        limiter
                            .until_ready_within(Duration::from_secs(60))
                            .await
                            .expect("let through");
                        start.elapsed()
                    }))
                };
                tokio::time::sleep(flight).await;
                drop(sending);
                sync.await.expect("the sync")
            }

            assert_eq!(
                held_for("http://flight.invalid:2479", ms(300)).await,
                ms(300)
            );
            assert_eq!(
                held_for("http://long.invalid:2479", ms(5_000)).await,
                ms(10) + BACKGROUND_FLOOR,
                "stood aside from 10 ms, a second without one let through"
            );
        }

        /// A sync request's wait holds no place: a person's request behind
        /// it is held by nothing but the quota.
        #[tokio::test(start_paused = true)]
        async fn a_sync_requests_answer_holds_nobody() {
            let url = url::Url::parse("http://nobody.invalid:2479").expect("url");
            let limiter = InstanceLimiter::for_origin(&url, Rate::new(10, 5));
            let sync = on_lane(
                Lane::Background,
                limiter.until_ready_sending(Duration::from_secs(60)),
            )
            .await
            .expect("the burst has room");
            let start = Instant::now();
            on_lane(
                Lane::Interactive,
                limiter.until_ready_within(Duration::from_secs(60)),
            )
            .await
            .expect("the burst has room");
            assert_eq!(start.elapsed(), Duration::ZERO);
            drop(sync);
        }

        /// A person's request yields nothing on another origin: the sync of
        /// a second server goes on at its own quota.
        #[tokio::test(start_paused = true)]
        async fn a_person_on_one_origin_holds_no_sync_on_another() {
            let one = url::Url::parse("http://one.invalid:2479").expect("url");
            let other = url::Url::parse("http://other.invalid:2479").expect("url");
            let busy = InstanceLimiter::for_origin(&one, Rate::new(1, 1));
            let quiet = InstanceLimiter::for_origin(&other, Rate::new(10, 1));
            let grants = Grants::default();
            let start = Instant::now();
            // The first takes `one`'s burst; the second waits a second on it.
            let person = vec![
                send(&busy, Lane::Interactive, start, ms(0), &grants),
                send(&busy, Lane::Interactive, start, ms(0), &grants),
            ];
            let sync = vec![
                send(&quiet, Lane::Background, start, ms(10), &grants),
                send(&quiet, Lane::Background, start, ms(10), &grants),
            ];
            all(sync).await;
            assert_eq!(start.elapsed(), ms(110), "the other origin's own pace");
            all(person).await;
        }

        /// Outside any lane a request is a person's; a lane inside a lane
        /// holds for its own call.
        #[tokio::test]
        async fn a_request_is_a_persons_unless_marked() {
            assert_eq!(current_lane(), Lane::Interactive);
            on_lane(Lane::Background, async {
                assert_eq!(current_lane(), Lane::Background);
                on_lane(Lane::Interactive, async {
                    assert_eq!(current_lane(), Lane::Interactive);
                })
                .await;
                assert_eq!(current_lane(), Lane::Background);
            })
            .await;
        }
    }
}
