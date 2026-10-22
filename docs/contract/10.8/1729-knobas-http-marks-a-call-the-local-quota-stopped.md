- **`crates/knobas-http/**` — issue #1729 (2026-10-22): a call the local rate limit stopped
  before its first attempt is marked, and the write queue leaves such a write waiting instead of
  refusing it.** Parent spec #1618 (story 21; SPI-11). Since #1630 a send the instance's limiter
  holds past `SEND_BUDGET` fails `SourceError::Protocol { status: None, .. }` naming the quota,
  and `knobas_sync::write_queue::retryable` read every status-less `Protocol` as the source
  refusing the write, so the write was settled refused although nothing was sent. No change to
  `SourceError`, the SPI, the commands, `HttpConfig`, any adapter, or the error the send returns.

  **`crates/knobas-http/src/limiter.rs` — one new public function, re-exported at the crate
  root:** `watch_local_quota<F: Future>(call: F) -> (F::Output, Option<LocalQuota>)` runs `call`
  under a tokio task-local and returns the `LocalQuota` an `HttpClient` send inside it stopped
  on, the last one if several did. `HttpClient::send` and `send_streaming` set it at the one
  return that fails with the quota: the first attempt never left. A retry the limiter stopped
  still ends with the source's previous answer and sets nothing. A send outside a watch, on a
  task spawned from the watched call, or in the wasm host's network policy sets nothing. A
  nested watch keeps its mark to itself.

  **`crates/knobas-sync` (not frozen), recorded here because it is the mark's reader:**
  `attempt` runs its one `Source::write` call under `watch_local_quota`. `retryable` takes the
  mark as a second argument, and a status-less `Protocol` with the mark is
  `WaitReason::Unreachable`: the write waits with the quota's message as its detail and is tried
  again on the sync ladder, as an unreachable write is since #1628. Without the mark,
  and for every `Protocol` that carries a status, the classification is unchanged. The adapter
  build's failure is classified without a mark. The crate now depends on `knobas-http`, with its
  `test-util` feature as a dev-dependency. No new `WaitReason`, migration or IPC change.
