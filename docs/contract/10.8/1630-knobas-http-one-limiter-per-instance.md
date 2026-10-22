- **`crates/knobas-http/**` — issue #1630 (2026-10-22): one rate limiter per instance origin,
  shared by every client built for it, and a limiter wait that runs out of budget is the local
  quota.** Parent spec #1618 (ruling A3; SPI-2, SPI-11). The limiter was an `Arc` inside each
  `HttpClient`, and a client is built per sync run, per live read and per flush, so each had a
  fresh burst of its own; and a wait on it was not bounded by `SEND_BUDGET`, so a call queued past
  the budget failed `Unreachable` without a request leaving. No change to the SPI, the commands,
  `HttpConfig` or any adapter.

  **`crates/knobas-http` — one new private module `limiter.rs`, its two types re-exported at the
  crate root (`pub use limiter::{InstanceLimiter, LocalQuota}`), additive:**

  ```rust
  #[derive(Clone)]
  pub struct InstanceLimiter { /* the origin's shared slot */ } // Debug: origin, per_second, burst
  impl InstanceLimiter {
      pub fn for_url(url: &str, per_second: u32, burst: u32) -> Result<Self, SourceError>;
      pub async fn until_ready_within(&self, within: Duration) -> Result<(), LocalQuota>;
  }
  #[derive(Debug, Clone, PartialEq, Eq)]
  pub struct LocalQuota { /* origin, quota, how long it waited */ } // Display + Error

  #[cfg(feature = "test-util")]
  impl HttpClient {
      pub fn with_send_budget(self, budget: Duration) -> Self; // tests only
  }
  ```

  A process-wide map keyed by `url::Origin` (scheme, host, port) holds one governor limiter per
  origin; `HttpClient::new` takes its limiter from it, so every client for one origin, in any
  task, waits on one quota, and two sources on one server share it. Where two configurations name
  different quotas for one origin the stricter wins, field by field (the lower rate and the lower
  burst); a quota only tightens for the life of the process. `InstanceLimiter` is public so the
  wasm host's network policy can wait on the same quota (#1631).

  **`send` / `send_streaming`:** the limiter wait is bounded by what is left of `SEND_BUDGET`.
  When it runs out before the first attempt, the call fails `SourceError::Protocol` with the
  `LocalQuota` message (origin, rate, burst and how long it waited), never `Unreachable`; when it
  runs out before a retry, the call fails with the previous attempt's failure, as a retry delay
  that does not fit the budget already did. The message `no answer within 45s` is gone with the branch that made it.
