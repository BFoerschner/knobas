- **`crates/knobas-http/**` — issue #1012 (2026-10-17): a streamed send, so a run's download
  and its whole log are no longer cut off at the request timeout.** Spec #808, milestone TF3,
  found while building #856. `send` gives each attempt a total timeout, and reqwest's covers the
  body, so a 1 GB artifact zip over a slow tunnel broke off at 30 s and the sink removed the file.
  No change to the SPI, the commands or any adapter's `HttpConfig`.

  **`crates/knobas-http` — additive:**

  ```rust
  pub const STALL_TIMEOUT: Duration = Duration::from_secs(30);
  pub use bytes::Bytes;
  impl HttpClient {
      pub async fn send_streaming(&self, request: Request, stall: Duration)
          -> Result<StreamedBody, SourceError>;
  }
  #[derive(Debug)] pub struct StreamedBody { /* private */ }
  impl StreamedBody {
      pub async fn chunk(&mut self) -> Result<Option<Bytes>, BodyBrokeOff>;
  }
  #[derive(Debug, Clone, PartialEq, Eq)]
  pub enum BodyBrokeOff { Stalled(Duration), Transport(String) }
  impl Display for BodyBrokeOff; // "no bytes for 30s", or the transport's words
  impl std::error::Error for BodyBrokeOff;
  ```

  The adapter wraps a `BodyBrokeOff` in `SourceError::Unreachable` with its own context.

  `send_streaming` runs `send`'s attempt loop unchanged (limiter, `MAX_ATTEMPTS`, `Retry-After`,
  classification, `SEND_BUDGET`), but each attempt's timeout covers the response headers only;
  late headers are `Unreachable` and transient. The body then has no total deadline, and `chunk`
  fails with `BodyBrokeOff::Stalled` when no byte arrives for `stall`. **One change to existing
  behaviour:** the reqwest client no longer carries a client-wide total timeout. `send` already
  set the request timeout on every attempt, so its behaviour is unchanged. A failing response's
  body is now read under the attempt timeout in both sends, so an error body that dribbles can't
  hold a streamed call open.

  **The adapter (TeamCity):** `run_reads::stream` (the whole log, `run_log`, and the preview),
  `run_reads::download` (all four `DownloadTarget`s) and `client::log_tail` (the job view's log
  tail, #665) call `send_streaming(request, STALL_TIMEOUT)`. Their "broke off" messages are as
  before, with the stall or transport detail after them.

  Pinned by: `knobas-http`'s `tests/transport.rs` against a socket that writes its body a byte at a
  time (`a_dribbled_body_outlasts_the_request_timeout`, which shows that `send` breaks the body off,
  `a_streamed_body_slower_than_the_request_timeout_arrives_whole`,
  `a_streamed_body_that_stalls_breaks_off_as_unreachable`,
  `a_streamed_send_with_no_headers_is_unreachable`). The file a broken-off download leaves is
  `knobas-app`'s `a_download_is_a_new_file_in_the_folder_named_by_its_last_segment` (none).
  Witnessed against the real server by `just teamcity-live`'s download cases.
