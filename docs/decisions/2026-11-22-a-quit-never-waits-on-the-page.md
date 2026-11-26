# 2026-11-22: A quit never waits on the page, and has a wall clock

Found by the final suite's Windows pass on Björn's PC (knobas's own WebView2 window, a debug build of d35fa646): with
XWiki's first sync running, the close box left knobas "not responding" for good. The log stopped at `site sessions
written`; a minidump showed the main thread in `block_on(Scheduler::shutdown)` and a tokio worker parked in
`tauri::ipc::Channel::send`, called by the sync's progress report.

## Why

- knobas builds Tauri with its `tracing` feature (#747, the command spans `logging::CommandLog` times). With it,
  tauri-runtime-wry's `eval_script` is a blocking round trip to the main thread, and a channel's send is one.
- Every event already hops to the main thread for that reason (`events::emit`, 2026-10-14). The progress channel,
  `sources::progress::ChannelSink`, did not: a sync sent from its worker.
- At the quit the main thread waited for the sync, and the sync for the main thread.
- With a worker blocked like that, tokio's timers stop firing, for a `block_on` caller and (measured) for a spawned
  task too, so no timeout inside `Scheduler::shutdown` could end the wait.

## The ruling (Fable, xhigh; Björn away)

Keep Tauri's `tracing` (dropping it costs #747's command timing; the macros' spans need it). Two changes:

1. **The cause.** `ChannelSink` hands each send, and its channel's last drop (which sends the page `{ end: true }`),
   to the main thread through `run_on_main_thread`. A scan test holds `Channel<` to `commands/sources.rs` and
   `sources/progress.rs`, beside `only_events_emit_names_the_emitter`.
2. **The guarantee.** The quit runs `Scheduler::shutdown` as a spawned task and waits for it by the wall clock
   (`knobas_sync::scheduler::finish_within`, a `recv_timeout`), at most `QUIT_BOUND` (the grace, the wait for aborted
   runs, the pool's close twice, and a second: 10 s); past it the quit goes on to the database and says so at error.
   Inside, the waits for aborted runs and background tasks are bounded too (`ABORTED_GONE_TIMEOUT`, 2 s), for a
   runtime whose timers still run.

## Tested

- `sources::progress` unit test: a worker's two reports and the sink's drop reach the channel only when the main
  thread's queue runs, on that thread. Red before (the worker's id on all three).
- `scheduler_loop.rs`: a run whose end report blocks its thread at the quit. Before: the quit hung for good (a run was
  stopped after 28 minutes). After: through `finish_within` it ends inside `QUIT_BOUND`; with an hour's grace and a
  300 ms bound it answers `None` in about 300 ms.

## Left

The other `block_on`s in `shutdown_app` (the clones, the backup task, the database stop) have the same timer exposure
when a worker is stuck. A cut on the database stop runs into `update::at_exit`, which starts an installer whether or not
PostgreSQL stopped, so that needs its own ticket.
