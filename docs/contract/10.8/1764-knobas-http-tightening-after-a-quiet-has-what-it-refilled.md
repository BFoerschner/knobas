- **`crates/knobas-http/**` — issue #1764 (2026-10-23): a tightening after a quiet starts with
  what the stricter quota refilled in it.** Parent spec #1618 (rulings A3, A9; SPI-2); follows
  #1740. Since #1740 a tightened quota starts with the old limiter's unspent requests counted in
  seconds of its refill, which a full limiter caps at its own burst's refill time, so where the
  looser quota refills its burst faster than the stricter one, a tightening after a long quiet
  started below the stricter burst. No change to the SPI, the commands, `HttpConfig`, any adapter,
  or any public signature.

  **`crates/knobas-http/src/limiter.rs` — private, behind the public `InstanceLimiter`:** each
  `Slot` keeps the instant of the last request any of its limiters let through, starting at the
  instant the slot was built (`Slot::born`, `Slot::last_grant`, an `AtomicU64` written with
  `fetch_max` by `Slot::granted` on every grant of `InstanceLimiter::until_ready_within`, without
  the slot's lock). `Slot::register`'s tightening branch passes the time since then
  (`Slot::idle`) to `Paced::tightened`, which now starts the new limiter with the larger of the
  seconds conversion (#1740) and what the stricter rate refills in that time, at most its burst,
  both now counted in `u128`. A loosening still starts drained (#1728) and does not restart the
  quiet. The module doc and `Slot`'s, `Slot::register`'s and `Paced::tightened`'s say so.
