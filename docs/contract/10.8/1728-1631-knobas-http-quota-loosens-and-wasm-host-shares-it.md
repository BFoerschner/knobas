- **`crates/knobas-http/**` — issues #1728 and #1631 (2026-10-22): an origin's quota loosens
  once no live client asks for the stricter one, and the wasm host's requests for an external
  adapter's instance wait on that same quota.** Parent spec #1618 (rulings A3, A9; SPI-5).
  Since #1630 a quota only tightened for the life of the process, so a raised rate on a source's
  settings, a quota tried in a draft's *Test connection*, or a deleted source's strict quota held
  the origin down until restart; and an external adapter's requests were not paced at all. No
  change to the SPI, the commands, `HttpConfig`, any adapter, or any public signature.

  **`crates/knobas-http/src/limiter.rs` — private, behind the public `InstanceLimiter`:** each
  `InstanceLimiter` (and so each `HttpClient`) holds a registration; clones share it. The slot
  per origin keeps `(Rate, Weak<registration>)` per holder, prunes the dead ones when a limiter
  for the origin is built, and sets the quota to the stricter (field by field) of the live ones
  and the new one. While any holder of the stricter quota is alive it still governs. A tighter
  quota is a new governor limiter with its own burst, as in #1630; one looser in either field is
  a new limiter that starts drained (its burst taken before anyone waits on it), so a loosening
  hands out no fresh burst. The map still holds one entry per origin, never removed; its lock is
  released before the slot's is taken, so no two locks nest, and nothing is awaited under a lock. The crate, `HttpConfig` and `InstanceLimiter::for_url` docs say so:
  a caller asking for more than the origin's quota is paced by what a live holder asked for, and
  the next limiter built once that holder is gone loosens it.

  **`crates/knobas-wasm-host` (not frozen), recorded here because it is `InstanceLimiter`'s first
  caller outside the crate:** `net::Policy::for_source` holds `InstanceLimiter::for_url(base_url,
  5, 10)` (`HttpConfig::default()`'s rate and burst) for as long as the built source lives, and
  `send` waits on it before every request for the instance, bounded by the call's deadline; a
  wait that outlasted it (the deadline ends the call first) would reach the component as
  `connection-limit-reached`. Requests to the manifest's other hosts are not paced. The crate
  now depends on `knobas-http`. The fixture component (`crates/knobas-wasm-fixture`) gains the
  config key `fetch_times`; the WIT world is unchanged.
