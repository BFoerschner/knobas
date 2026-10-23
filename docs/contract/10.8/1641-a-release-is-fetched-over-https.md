- **IPC command semantics, issue #1641 (2026-10-23): an external adapter's release is fetched
  over https, or over plain http only from this machine.** Parent spec #1620 (LIFE-7). No
  signature change, no new IPC variant, no migration, no new event, no new module in `commands/`
  or `ipc/`. The generated mirror does not change.

  `install_adapter` refuses a manifest URL over plain `http://` whose host is not a loopback
  address (`127.0.0.0/8`, `::1`) or `localhost`, before anything is fetched. It answers with the
  `invalid` code it already answers a bad origin with, and the message reads "`<url>` is plain
  http: a release is fetched over https, or over http only from this machine (127.0.0.1, ::1,
  localhost)". The new `knobas_app::sources::remote::FetchError::NotHttps` variant carries it.
  Every fetch of a release (the manifest, a release API's answer, an asset its answer names, and
  the artifact) refuses the same way when the URL it is given, or the URL the answer came from
  after any redirects, is not https off this machine. A plain-http hop between the two is not
  checked (#1830).
  A GitHub repository URL is not checked: nothing is fetched from it, and its release is read
  through the API.

  `check_adapter_updates` reports the same message as the adapter's failed check, and
  `update_adapter` refuses with it, for an adapter installed earlier from a plain-http URL off
  this machine. So does a backup restore that has to fetch such an adapter again. An `https://`
  origin, a local manifest and the test forge on `http://127.0.0.1` behave as before.

  Witnessed by `crates/knobas-app/src/sources/remote.rs`
  (`a_plain_http_origin_off_this_machine_is_refused`,
  `a_plain_http_origin_on_this_machine_still_parses`,
  `a_fetch_over_plain_http_off_this_machine_is_refused`,
  `a_redirect_to_plain_http_off_this_machine_is_refused`) and the unchanged
  `crates/knobas-app/tests/it/external_adapter_install.rs` and `external_adapter_release.rs`.

  The same PR bounds what an external adapter writes to the app's log (each line cut to 4 KiB; 100
  lines a call, then one a second) and widens the rebinding guard to every address the IANA
  special-purpose registries mark not globally reachable. Neither changes the WIT world or any IPC
  surface: both are inside `crates/knobas-wasm-host`.
