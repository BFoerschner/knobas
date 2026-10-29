- **`crates/knobas-http/**` — issue #1740 (2026-10-23): a tightened quota starts with the credit
  the limiter it replaces had left, not with a fresh burst.** Parent spec #1618 (rulings A3, A9;
  SPI-2). Since #1728 a loosening starts drained, but a tightening still built a limiter with its
  own full burst, so two sources at different quotas on one server whose clients alternate were
  handed the stricter burst again on every loose-to-strict turn. No change to the SPI, the
  commands, `HttpConfig`, any adapter, or any public signature.

  **`crates/knobas-http/src/limiter.rs` — private, behind the public `InstanceLimiter`:** when
  `Slot::register` tightens the quota, `Paced::tightened` takes every request the old limiter
  could still send at once (governor has no reading of it, so `Paced::take_left` takes them in
  halving steps and counts them, leaving the old limiter spent), counts them in seconds of the
  old rate, and starts the new limiter with as many seconds of the stricter rate, at most its
  burst (`Paced::with_left`). A tightening of a limiter nobody spent from therefore has the whole
  stricter burst, as in #1630, wherever the old quota's burst lasts at least as many seconds as
  the new one's; one that follows a drained loosening has what the stricter rate would have
  refilled since. A loosening still starts drained (#1728). The module doc and
  `Slot::register`'s say so.
