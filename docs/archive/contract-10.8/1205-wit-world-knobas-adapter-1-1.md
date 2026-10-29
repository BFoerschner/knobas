- **`knobas:adapter@1.1.0`: a download through a host-owned sink, and every call held to a
  deadline and a memory cap — issues #1205 and #1204 (2026-10-19).** `wit/adapter.wit` moves to
  1.1.0, the first minor version, under #1198's minor-version rule. No migration, no new command,
  no IPC change (`download_run_file` already reaches `Source::download`), no change to
  `crates/knobas-source/src/**`, `crates/knobas-http/**` or `error.rs`. Parent spec #1192
  (ADR-0028 *The interface*, *Every call has a deadline and a memory cap*). Ratified by the
  implementer on Opus under Björn's 2026-10-18 reserved-steps rule; the ruling is in
  `docs/archive/2026-10-unattended-rulings.md`.

  **What 1.1.0 adds, and only adds.**
  - `interface download-sink { start: func(file-name: string) -> result<_, source-error>; write:
    func(bytes: list<u8>) -> result<_, source-error>; }`, imported: `run::DownloadSink`, owned by
    the host. Each call waits until the host's sink has taken the bytes, so neither side holds a
    download whole. Outside a download, `write` before `start`, or `start` twice, fail the call.
  - `interface downloads { download: func(instance: instance, entity: string, target: json) ->
    result<bool, source-error>; }`, exported: `Source::download`, `target` a `run::DownloadTarget`
    as JSON (whose serde form is frozen through #1198's entry, as every SPI type that crosses as
    JSON). Its own interface and a free function, not a method on `adapter`, so a 1.0.0 component,
    which does not export it, still links: the host looks `downloads@1.1` up as optional, and a
    source of a component without it answers `download` with `false` (keeps no runs). An export
    of that name whose type is not this one is refused at load.
  - `world hosted { import sink; import log; import download-sink; export source; }`: what the
    host binds of every 1.x component. `world adapter` is `include hosted; export downloads;`,
    what an adapter is built against.

  No record, variant or existing function changes; the code-host diff and tree reads, and every
  run and pipeline read, already cross (`view` and its deferred sections carry `diff`, `tree` and
  `file` blocks as JSON). `INTERFACE_VERSION` is 1.1.0, so a component built against 1.0.0 or 1.1.0
  is hosted and one built against 1.2.0 is refused at load. A manifest names the version its
  artifact is built against, as before.

  **The limits** (#1204). Every call (instantiation included, and every host call it waits on: a
  request, the sink, the download sink) has a wall-clock deadline; running WASM yields to the
  executor at every epoch tick (50 ms), so a component that never returns is still stopped. Every
  call's store has a memory cap over all its linear memories and tables; a growth past it traps. A call that
  passes either fails `SourceError::Protocol` with *"the external adapter "<kind>" was stopped at
  its deadline of N s"* or *"… its memory cap of N MiB"*, which the run's ending carries and the
  source page's run list shows (ADR-0005); the row's health line says *Failed …* without it, as
  for every failed run (follow-up #1252). Defaults 600 s and 256 MiB; the manifest's `limits { deadline_secs,
  memory_mib }` may ask up to 3,600 s and 1,024 MiB, and install refuses a value above that, or
  zero, as *"the manifest's limits are refused: …"*. `knobas.external_adapter.limits` (#1202's
  migration `0051`, unchanged) is read at load and applied. Inside a call, a request's
  `request-options` timeouts are honoured (connect and first-byte together bound the wait for the
  response head; between-bytes each body frame), and an outgoing body is refused past the memory
  cap (`HTTP-request-body-size`).

  Pinned by `knobas-wasm-host`'s `tests/it/reads.rs` (every read, the deferred diff and tree, the
  downloads, a 64 MiB download through a 16 MiB cap, a 1.0.0 component still hosted) and
  `tests/it/limits.rs`, and by `knobas-app`'s `external_adapter_install` (the refusal, and a run
  past its deadline beside one that syncs, after a restart).
