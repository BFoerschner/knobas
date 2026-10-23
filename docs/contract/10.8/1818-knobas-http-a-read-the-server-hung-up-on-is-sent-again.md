- **`crates/knobas-http/**` — issue #1818 (2026-10-23): a `GET` or `HEAD` the server closed its
  connection on before answering is sent once more.** Parent spec #1618; found in #1640's merge
  review. A pooled keep-alive connection the server's idle timeout closed as the next request
  went out on it (Node's 5 s, which Kuma's `live_kuma` suite met between two syncs) failed the
  read at once as `SourceError::Protocol { status: None }`, since `retry::error_is_transient`
  takes timeouts and connect errors only. No change to the SPI, the commands, `HttpConfig`, any
  adapter, the limiter, #1729's `watch_local_quota` mark, #1640's body cap or second client, or
  how any failure maps to a `SourceError` (a read sent again reports what its second attempt
  met, as every retry reports its last).

  **`crates/knobas-http/**`:**
  - `Cargo.toml`: dependency `hyper = "1"` (the copy reqwest already resolves).
  - `src/retry.rs`: new `pub fn closed_before_answering(&reqwest::Error) -> bool`, true when a
    `hyper::Error` whose `is_incomplete_message()` holds is in the error's source chain.
    `error_is_transient` is unchanged.
  - `src/lib.rs`, `send_with` (so `send` and `send_streaming`): a transport failure on a request
    that did not opt out with `no_retry`, built for `GET` or `HEAD` (the `follows` flag #1640
    added), that `closed_before_answering` matches is transient the first time in a call and not
    after it. The retry is the loop's own: it waits `retry::backoff`, passes the limiter, and is
    not made when the delay does not fit what is left of `SEND_BUDGET`. Any other method, and a
    request sent with `no_retry`, is never sent again on it and keeps its classification
    (`Protocol { status: None }`). The crate doc's Retries bullet, the `retry` module doc,
    `send`'s doc and the private `Request::follows` field's doc say so.
  - `tests/transport.rs` gains the `hung_up` module: a server that reads each request whole and
    closes the connection unanswered, and six tests (a read sent again and answered, for `GET`
    and `HEAD`; a `POST` not sent again, with and without `no_retry`; a `GET` sent with
    `no_retry` not sent again; a read sent twice only; the resend waits for the limiter; the
    resend stays inside the send budget).
