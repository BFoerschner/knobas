- **`crates/knobas-http/**` and IPC command semantics — issues #1828 and #1830 (2026-10-23): a
  client whose reads follow only the redirects a check allows, and a release fetch that checks
  every hop and allows plain-http loopback only when it started on this machine.** Parent spec
  #1620. **Amends #1641's entry**, under which a plain-http hop between the first URL and the
  last was not checked, and plain http to this machine passed in any release fetch. Recorded
  under Björn's 2026-10-22 waiver (an Opus implementer rules the reserved step). No change to the
  SPI, `HttpConfig`, `HttpClient::new`, any adapter's client, the limiter, #1640's body cap and
  refused write redirect, #1818's resend of a read, or #1821's `Unconfirmed` write and
  `WRITE_POOL_IDLE`. No signature change on an IPC command, no new IPC variant, no migration, no
  new event, no new module in `commands/` or `ipc/`. The generated mirror does not change.

  **`crates/knobas-http/**`:**
  - `src/redirect.rs` (new, private): reqwest's own redirect policy (at most 10 hops) for every
    hop a caller's check allows, and an error naming the hop and the redirect's status for one
    it refuses.
  - `src/lib.rs`: new `pub fn HttpClient::with_redirect_check(config: HttpConfig, check: impl
    Fn(&Url) -> bool + Send + Sync + 'static) -> Result<Self, SourceError>`. The reading client
    (`GET`, `HEAD`) asks `check` about each hop before requesting it, the last one included; the
    write client follows none, as on every client. `HttpClient::new` builds exactly what it built
    before. `send_with`: a hop the check refused is `SourceError::Protocol { status: Some(<3xx>),
    message: "HTTP <status>: the read was redirected to <url>, which this client does not
    follow" }`, not transient, so not sent again. `pub use reqwest::Url` joins the re-exports,
    because the check's signature names it.
  - Docs: the crate doc's redirect bullet and the `inner` field say so.
  - `tests/transport.rs`, `bounds` module: new
    `a_checked_read_is_refused_at_the_first_hop_its_check_refuses`,
    `a_checked_read_follows_every_hop_its_check_allows`,
    `a_checked_read_follows_at_most_ten_hops`.

  **IPC command semantics (`install_adapter`, `check_adapter_updates`, `update_adapter`, a
  backup restore that fetches an adapter again):** every read of a release goes out on a client
  built with `with_redirect_check`, whose check runs `remote::require_https` on the hop and
  keeps the `FetchError` it refused with for `remote::get` to return. A redirect hop over plain
  http off this machine is refused before it is requested, wherever it falls in the chain
  (#1830), and the separate check on the answer's final URL is gone: the last hop is checked
  before it is requested. Plain http to this machine (a loopback address or `localhost`) is allowed only
  when the release's reads started on this machine: the pasted manifest URL, or the release API
  knobas asks (`Endpoints::github_api` for a GitHub repository or a github.com download, the
  forge's own API otherwise) (#1828). A release found elsewhere that names an asset at, or
  redirects to, `http://127.0.0.1:…` is refused before anything is sent there. The refusal is
  `FetchError::NotHttps`, with the `invalid` code it already answered with; its message now
  ends "…or over http only from this machine (127.0.0.1, ::1, localhost) when the release is on
  this machine too". The test forge on `http://127.0.0.1` and an https origin behave as before.

  Witnessed by `crates/knobas-app/src/sources/remote.rs`
  (`plain_http_to_this_machine_is_allowed_only_when_the_reads_started_there`,
  `a_remote_release_naming_an_asset_on_this_machine_is_refused`,
  `a_plain_http_hop_inside_a_redirect_chain_is_refused`, and the #1641 tests) and the unchanged
  `crates/knobas-app/tests/it/external_adapter_install.rs` and `external_adapter_release.rs`.
