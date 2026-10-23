- **`crates/knobas-http/**` — issue #1771 (2026-10-23): a loosening after a quiet starts with
  what the looser quota refilled in it.** Parent spec #1618 (rulings A3, A9; SPI-2); follows
  #1728 and #1764. Since #1728 a loosened quota starts drained, so after a long quiet its first
  request waited one emission interval of the looser rate though nothing had been sent. No change
  to the SPI, the commands, `HttpConfig`, any adapter, or any public signature; #1729's
  `watch_local_quota` mark is untouched.

  **`crates/knobas-http/src/limiter.rs` — private, behind the public `InstanceLimiter`:**
  `Slot::register`'s loosening branch builds `Paced::loosened(rate, Slot::idle())` in place of
  `Paced::drained(rate)`, which is removed. `Paced::loosened` starts the new limiter with what
  the looser rate refills in the time since any of the origin's limiters last let a request
  through (#1764's `Slot::last_grant`), at most its burst, and nothing of what the replaced
  limiter had left. The refill count both branches use moves into `Rate::refilled_in`, and
  `Paced::with_left` takes that `u128` and saturates it itself. The module doc, `Slot::register`'s
  and `Paced::loosened`'s say so. In `crates/knobas-http/tests/transport.rs`,
  `a_loosened_quota_starts_drained` is renamed `a_loosening_right_away_starts_drained`, since a
  loosening now starts drained only right after a request, and its comment names the credit; its
  body is unchanged. Four module tests are new
  (`a_loosening_after_a_quiet_starts_with_what_the_quiet_refilled`,
  `a_short_quiet_credits_a_loosening_in_proportion`,
  `a_loosening_right_away_takes_nothing_from_a_full_limiter`,
  `a_request_ends_the_quiet_before_a_loosening`), and `a_loosening_does_not_end_the_quiet`'s doc
  no longer says its loosening starts drained.
