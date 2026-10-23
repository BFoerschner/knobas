- **`crates/knobas-http/**` and `crates/knobas-source/src/**` — issue #1640 (2026-10-23): a cap
  on what one answer's body may hold, no redirect followed on a write, and no username or password
  in a base URL.** Parent spec #1620 (ruling C2; SPI-4, SPI-7, SPI-10). No change to the `Source`
  trait, its DTOs or the contract battery, to any IPC command, event, DTO, barrel or module-layout
  line, or to any migration. `add_source` and `update_source` gain one more reason for the
  `invalid` code they already answer with.

  **`crates/knobas-http/**`:**
  - `Cargo.toml`: dependencies `http = "1"` and `http-body = "1"` (the copies reqwest already
    resolves); dev-dependency `flate2 = "1.1.9"` (the copy the Gitea adapter already resolves).
  - New `pub const MAX_BODY: u64` (32 MiB) and a new public field `HttpConfig::max_body: u64`,
    `MAX_BODY` in `HttpConfig::default()`. An `HttpConfig` struct literal without
    `..HttpConfig::default()` now has to name it: the six in the Jira, Gitea, TeamCity,
    Confluence and Kuma adapters (`http.rs` in each, Gitea's `client.rs`, Kuma's `socket.rs`)
    each gain `max_body: knobas_http::MAX_BODY`.
  - New private module `src/cap.rs`. Every body `HttpClient::send` hands back for a success is
    read through a body that counts what is decoded and fails past `max_body` (so a gzip body is
    counted inflated); a `send_streaming` body is not. The re-wrapped response keeps its URL
    (`Response::url()` is still where the answer came from). `get_json`, `get_json_with` and the
    failing-status body read refuse a declared `content-length` over the cap before reading. Over
    the cap, `get_json` and `get_json_with` fail with `SourceError::Protocol { status: None }`
    reading `<path>: the answer is larger than <cap>, the most knobas reads of one answer`; a
    failing status keeps its class and status, and that sentence takes the body excerpt's place
    in its message.
  - New `pub fn over_the_cap(&reqwest::Error) -> bool`, for a caller that reads a sent body itself
    and names the cap in its own words.
  - `HttpClient` builds a second reqwest client with `redirect::Policy::none()`, and
    `HttpClient::request` puts every method but `GET` and `HEAD` on it (`Request` carries which,
    privately). `send` answers a redirect status on such a request with `SourceError::Protocol {
    status: Some(<3xx>) }` naming the `Location`, before any retry or `once_sent` reading; the
    write queue's `retryable` reads that as a refusal. `GET` and `HEAD` follow redirects as before.
  - New `pub fn refuse_userinfo(&str) -> Result<(), SourceError>`: `Protocol { status: None }` for
    a URL that parses with a username or a password, `Ok` for anything else. The private
    `http_url` that `HttpClient::new` and `InstanceLimiter::for_url` share applies it before the
    scheme check, and names an unparseable URL with its userinfo masked, so no message of either
    repeats a password. A source whose base URL carries userinfo gets no client and, in the wasm
    host, no limiter, as its `Target::origin_of` already gave it no origin.
  - `HttpConfig`'s `Debug` is hand-written instead of derived: the same fields, `base_url`
    through `knobas_source::instance::redact_userinfo`.
  - The crate doc gains three bullets for the above. `tests/transport.rs` gains the `bounds`
    module and one more rendering in `a_secret_never_reaches_a_debug_rendering`.

  **`crates/knobas-source/src/instance.rs`:** new `pub fn redact_userinfo(&str) -> Cow<'_, str>`,
  a string mask of everything before the authority's last `@`; `SourceInstance`'s hand-written
  `Debug` prints `base_url` through it. Its `debug_redacts_the_secret` test is extended and
  `userinfo_is_masked_and_nothing_else` added.

  **Outside the frozen list, for the record:** `knobas_app::sources::crud::add` and `update`
  refuse a userinfo base URL with `SourcesError::Invalid` (IPC `invalid`) before the keychain or
  the table is touched, through `knobas_http::refuse_userinfo`. `knobas_app::sources::remote`'s
  adapter fetch builds its client with its own cap as `max_body` (a component may be up to
  64 MiB) and reads a body over it as `FetchError::TooLarge`, its read-then-measure check
  removed. `knobas-sync`'s `retryable` test table gains a redirected write's row.
