- **`crates/knobas-http/**` — issue #1925 (2026-10-24): the rate limiter and the send budget
  read tokio's clock, so a test can pause it.** The limiter read governor's default clock and
  waited on governor's own timer (`futures-timer`), and the send budget read `std::time::Instant`,
  so no test could move a quota or a budget: each read the wall clock with sub-second slack and
  went red under load. No change to the SPI, the commands, `HttpConfig`, any adapter, any public
  signature, how any failure maps to a `SourceError`, #1729's `watch_local_quota` mark, or what a
  quota lets through while the machine is awake; outside a test that pauses tokio's clock,
  `tokio::time::Instant` is `std::time::Instant`, which the send budget's timeouts and the quiet a
  quota change is credited with read already. One difference, across a system sleep on macOS:
  std's clock (`CLOCK_UPTIME_RAW`) stops while the machine sleeps, and governor's quanta clock
  read the raw counter (`cntvct_el0` on Apple silicon), which runs on, so a quota no longer
  refills while the machine sleeps. The limiter wakes as it slept: a drained quota hands out its
  next request at most one refill interval later than before, never sooner. Stricter, never
  looser.

  **`crates/knobas-http/src/limiter.rs` — private, behind the public `InstanceLimiter`:** a
  private governor clock, `TokioClock`, reads `tokio::time::Instant` through a private `Tick`
  reference; `Paced` builds its limiter with `RateLimiter::direct_with_clock(.., TokioClock)` and
  waits in its own `Paced::until_ready`, governor's loop with `tokio::time::sleep` in place of
  governor's timer. `Slot::born` and the quiet it counts are a `tokio::time::Instant`. The module
  doc says so. The module tests that slept a real quiet run on tokio's paused clock and assert
  literals, and `refill_bound_since`, the wall-clock bound #1802 added, is gone.

  **`crates/knobas-http/src/lib.rs`:** `send_with`'s `started`, which `budget_left` and
  `attempt_timeout` read, is a `tokio::time::Instant`.

  **`crates/knobas-http/Cargo.toml`:** the dev-dependency on tokio adds `sync` and `test-util`.

  **`crates/knobas-http/tests/transport.rs`:** the burst, quota-change, budget and quota-mark
  tests (`two_clients_for_one_instance_share_one_burst`,
  `the_stricter_of_two_quotas_for_one_instance_governs`,
  `a_quota_loosens_once_the_stricter_client_is_gone`, `a_loosening_right_away_starts_drained`,
  `a_tightening_after_a_loosening_starts_with_what_was_left`,
  `a_limiter_wait_that_outlasts_the_budget_is_the_local_quota`,
  `a_limiter_wait_never_outlasts_the_budget`, `a_watched_call_the_quota_stopped_is_marked`,
  `a_retry_the_quota_stopped_is_not_marked`, and `hung_up::a_read_sent_again_stays_inside_the_send_budget`)
  run on tokio's paused clock, held still by a running blocking task (`HeldClock`) while a request
  is on the loopback and moved only once `CountingServer::exchanges_over` says nothing is left on
  the wire, and assert what went exactly. `hits_within_the_burst` and `queued_sends` are gone.
  New helpers: `HeldClock`; `real_backstop`, a 20 s real-time hang backstop for a wait on the held
  clock; `start_sends`, `ended`, `went` and `the_rest_stopped_on_the_quota` over `Ended`, what one
  send ended in and when; `queued_behind_one_a_second` and `ten_on_the_loose_client`; and the
  budgets `SHORT_BUDGET` (now a `Duration` in scope), `UNDER_A_REFILL`, `TWO_A_SECOND_WINDOW` and
  `ONE_REFILLED`. `CountingServer` reads each connection to its end and counts the exchanges that
  are over (`exchanges_over`). No test is added or removed.
