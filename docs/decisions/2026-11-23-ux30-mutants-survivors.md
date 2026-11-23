# The ux30 end gate's mutants: each survivor, killed or explained (2026-11-23)

The ux30 end gate ran `cargo mutants --in-diff` over `v3.2.9-ux28..HEAD`, crate by crate: knobas-site, knobas-source,
knobas-source-pleasant and knobas-secrets on WSL, the rest on GitHub's hosted runners (workflow `mutants.yml`, runs
45221186058 and 45227865249), and the files Linux does not build on `windows-latest` and `macos-latest`. Fable ruled how
survivors are settled (Björn away): a mutant that changes a decision gets a test that kills it, if need be by moving the
decision where a test reaches it; a mutant in a plain call into a web view or the OS gets its own line here, saying it
needs a real web view and which real-app path drives it; "equivalent" is kept for a mutant that changes no behaviour; a
file Linux never built is "not run", not "missed".

Rerun on the Mac with the new tests (cargo-mutants 27.1.0, the same diff): knobas-http 27 caught, 0 missed;
knobas-mockd 25 caught, 0 missed; knobas-adapter-sdk 6 caught, 2 missed (the two default bodies below). Each kill below
was checked twice: the test fails with the mutation applied (cargo-mutants on the file, or the mutation
by hand) and passes on the real code.

## Killed by new tests

### knobas-http (23 missed, 0 caught)

Its 23 survivors had no test anywhere in the crate: the API explorer's client (`person.rs`, ux31) was exercised only
through knobas-app. New tests in the crate itself:

- `tests/person.rs`, against a one-exchange server on the loopback:
  - every `Failure::stage` name (`person.rs` `Failure::stage` → `""`, `"xyzzy"`);
  - a person's header goes out, `Host` and `Content-Length` are refused before anything goes out (`exchange` line
    175: `||` → `&&`, both `==` → `!=`);
  - a body exactly at the cap is whole and not cut, one byte past it is cut there (line 210: `>` → `==`, `<`, `>=`);
  - the times count from the send, against a server that waits 120 ms (`millis` → 0, 1);
  - a refused connection is the `connect` stage and its words carry every cause down to the refusal (`words_of` →
    `String::new()`, `"xyzzy"`, `delete !`);
  - a server that hangs up before answering is the `send` stage, and so is an `ftp://` address, which reqwest refuses
    as a builder error (`classify` line 277, both `||` → `&&`; the second reads `is_request || (is_body &&
    is_builder)` once swapped, so only a builder error tells it apart).
- The word lists that tell a TLS failure and an unresolved name moved, unchanged, from `classify` into `says_tls` and
  `says_resolve`, with unit tests that give each word alone (`classify` lines 260 to 262 and 267 to 270, `||` → `&&`).
  No real failure says exactly one of the words, so this is the only way a test reaches each one.
- `tests/transport.rs`, `a_form_body_and_its_content_type_both_reach_the_request`: the Pleasant token grant's form body
  and its content type (`lib.rs` `Request::form`, delete field `inner`).

### knobas-mockd (9 missed)

`tests/it/jira_issue.rs` now drives the ADR-0054 endpoints: an edit of summary, priority and labels (204, seen in later
GETs), an edit of the summary alone, the edits Jira refuses (a field off the screen under `fields` or `update`, a label
`set`, an empty summary, a priority with no name, an unknown priority: 400, nothing changed), assigning and unassigning
on `/assignee`, the assignable search by username alone (`.`) and by name alone (`a l`), and `expand=editmeta`. Kills
`edit_issue` → default, its `delete !`, `!=` → `==`, `==` → `!=`; `assign` → default; `assignable` → default and both
`||` → `&&`; `editmeta_json` → default.

### knobas-adapter-sdk (glue.rs:229, :238; 6 missed)

`glue.rs` gets a test module that calls the `tickets` guest functions on the host target with an adapter that answers
from what it was asked: the fields and the options cross as answered, for the instance and entity the host named, and
`None` stays `None`. A glue that answered `Ok(None)` or an empty or wrong string would leave every WASM adapter's
tickets read only in knobas; these now fail.

### knobas-site

- `win.rs` `kind_name` and `origin_text` (32 Windows survivors among them) moved, unchanged, to `glue.rs` as
  `resource_kind(i32)` and `origin_text`, with tests for every kind number and for an origin without path or query (the
  vendor's news address carries the install id in its query).
- The confined site's refusal, `rules.is_some_and(|rule| !rule.may_request(url))`, written twice in `win.rs`'s two
  handlers (lines 228 and 266, `delete !`), is `glue::refuses`, tested with a rule and without one.
- `wry_host.rs` `Confined` gets tests on the Mac and Windows: a refusal not in place shuts the gate and reports
  `NOT_CONFINED` (`Confined::refused` → `()`); `WryView::confine`'s "same rules, nothing to apply" is `Confined::take`
  (line 293, `==` → `!=`); the Windows "register the request filter once, only with rules" is `Confined::registers`
  (line 87, `&&` → `||`, `delete !`).
- `mac.rs` `confine`'s "a rule list that lands after a newer one was asked for is dropped" (line 494, `+` → `-`, `*`;
  line 524, `!=` → `==`) is `Confined::next_generation` and `Confined::is_current`, tested with two lists in a row.
- On the Mac (cargo-mutants 27.1.0 on wry_host.rs, mac.rs and glue.rs, in place): 58 caught and 2 timeouts of 91 on the
  first pass; the `forget` and `remove_store` timeouts are `tests/mac_forget.rs` waiting on a forget that never
  answers, so caught. `has_data_stores` (the Linux run's "missed" mac.rs:55) is caught by its existing test: Linux
  never built it.

### knobas-app

Run 45227865249 ran 9 of knobas-app's 16 shards (the other 7 failed their baselines on the two tests below) and the
macOS leg (run 45236096794) 27 of the 128 mutants in the app's macOS files before it was cancelled. New tests for
what survived there:

- `api/kept.rs:19` (`BODY_BYTES`, `64 * 1024 * 1024`, either `*` → `+`): `answers_up_to_64_mib_in_all_are_kept` keeps
  two 20 MiB answers and drops the oldest at 70 MiB. The test that was there scales with the constant, so it could not
  see a bound of about 1 MiB.
- `api/send.rs:309` (the client built without the CA file's roots): `a_ca_file_without_a_certificate_fails_the_send_before_it_goes`.
- `settings.rs:163` (`source_count` → `Ok(1)`): `the_source_count_is_how_many_sources_are_configured` (0, then 2).
- `site.rs:439` (`passwords || reads.is_none()` → `&&`, a site whose role is unknown left unconfined) and `site.rs:480`
  (`other != source` → `==`, a site's rules listing itself instead of the others):
  `a_site_is_confined_until_its_role_is_read_and_its_rules_name_the_others`.
- `site.rs:424` (`delete !`: the source's own sign-in counted as saved while turned off): the existing own-credential
  test now reads the site's sign-in rule back through `Sites::sign_in_rule`, a read-only getter beside `Sites::rules`.
- `sources/progress.rs:74` (`ChannelSink`'s drop → `()`): `a_sink_that_never_reported_lets_its_channel_go_on_the_main_thread`.
  The test that was there reported first, and the reports' copies of the channel let it go on the main thread anyway.

`lib.rs:1103` (`spawn_bring_up`, `*` → `+` in `KEEP_DAYS * 24 * 3600`): the scratch folders' age limit is
`api::scratch::keep()`, and the sweep test now keeps a folder 29 days old and sweeps one 31 days old with it. The
mutant made the limit 72 minutes, which would have removed every API-explorer folder a person had not opened that hour.

### knobas-core (run 45248101534: 62 caught, 1 missed)

`checkout/open.rs:825` (`settle`, `delete !` in "the API command is not blank"): an API client may leave its
repository command blank, and only an API client. `only_an_api_client_may_leave_its_repository_command_blank` saves
Bruno with a blank repository command and refuses an entry whose API command is blank as well; with the mutation applied
by hand it fails, and passes on the real code.

### knobas-db (Windows baseline)

Not a survivor: the Windows baseline failed on `embedded.rs` `a_watch_sees_a_killed_postmaster_whose_lock_file_stayed`
and `process_liveness_tells_a_running_process_from_a_reaped_one`. Both test the liveness check, and off unix there is
no `ps`: `process_alive` answers `false` there and `ServerWatch::is_gone` asks it only under `cfg!(unix)`, on purpose. A
test bug, not a product bug; both tests are now `#[cfg(unix)]`, as the code they test is.

### Baselines that failed on the runners (test bugs, no product change)

- knobas-app `tests/it/assets_ipc.rs`, `the_bar_is_each_half_hours_worst_and_the_samples_come_only_when_asked` (three
  of run 45227865249's app shards): the bar is on the UTC half-hour grid, and the test wrote samples at `now()` less 1
  to 300 minutes, so in a half-hour's first minute the newest half-hour held none and the bar ended in no reading. The
  samples now start at minute 0. Run on the Mac at 21:00:00: the old test failed (`...uuuuduuuud-`), the new one passed.
- knobas-app `tests/it/file_pipelines.rs`, `a_value_nested_deep_at_run_time_fails_the_step_not_the_app` (four
  shards): building two values a million levels deep for `deep == deep` passed the ten-second step limit on the runner,
  so the step timed out instead of failing on the stack. The test is about how the step fails, so it gives the step a
  minute.
- knobas-search `tests/it/coverage_plans.rs` (run 45227865249): read 258 child rows against a line of 257, one
  withdrawn row ahead of the live ones at the head of the range. Handed to its own fix agent by the gate.
- knobas-core (run 45221186058, Ubuntu 26.04): embedded PostgreSQL 18.6.0 needs `libxml2.so.2`; rerun on 24.04 by the
  gate.

### knobas-sync (run 45227865249: 42 caught, 2 missed, 1 timeout)

- `refresh.rs:241` (`==` → `!=`, a walk read as the parent gone when any other item under it was deleted): killed by
  knobas-app's `tests/it/refresh.rs` `a_walk_is_gone_only_when_it_answers_the_parent_deleted`, which drives the refresh
  through the scheduler; knobas-sync's own suite has no source that walks. Checked by hand: the app test fails with the
  mutation in knobas-sync and passes without it.
- `write_queue.rs:901` (`attempt` → `Ok(true)`): a timeout, so caught.

## Equivalent: no behaviour changes

- knobas-sync `scheduler.rs:1718` (`2 * POOL_CLOSE_TIMEOUT.as_secs()` → `2 + ...`): `POOL_CLOSE_TIMEOUT` is 2 s, and
  2 × 2 = 2 + 2, so `QUIT_BOUND` is the same.
- knobas-app `passwords/otp.rs:77`, `:78`, `:79` (`|` → `^` in RFC 4226's truncation): the four bytes are shifted into
  bit ranges that do not overlap (24 to 30, 16 to 23, 8 to 15, 0 to 7), so or and xor give the same number. The RFC's
  test vectors stay green either way.
- knobas-app `lib.rs:1105` (`gone > 0` → `<`, `>=`): only whether the sweep writes its log line.
- knobas-app `commands/entity/fields.rs:32` (`behind == Some(true)` → `!=`): only marks the command's span as a read
  behind, which changes the level a slow command is logged at, nothing it answers.

- knobas-source `lib.rs:2362` (`Source::ticket_fields` → `Ok(None)`) and `:2375` (`Source::field_options` →
  `Ok(None)`): the default bodies already answer `Ok(None)`.
- knobas-source `lib.rs:2891` (`Sink::otp_key` → `()`): the trait's default body is empty.
- knobas-adapter-sdk `lib.rs:460` (`Adapter::ticket_fields` → `Ok(None)`) and `:474` (`Adapter::field_options` →
  `Ok(None)`): the default bodies already answer `Ok(None)`.
- knobas-site `host.rs:134` (`SiteView::confine` → `()`) and `:189` (`SiteHost::flush` → `()`): default no-op bodies.
- knobas-site `fake.rs:248` (`FakeHost::flush`, `>` → `>=`): the test host records whether the quit's deadline is
  still ahead. The two differ only when the deadline is the very `Instant::now()` the comparison reads, which no caller
  can hand it; the fake is test code, not the product.
- knobas-source-pleasant `source.rs:210` (`PleasantSource::token`, `>` → `>=`): a kept token is used while its expiry
  `until` is after now. The two differ only at the one instant `until == Instant::now()`, which a test cannot stop the
  monotonic clock on; on either side of it both read the same.

- knobas-db `test_util.rs:393` (`supervise` → `Some(Default::default())`, run 45227865249): the `cfg(not(unix))` stub
  of the test harness's supervisor. Linux does not compile it, so the mutant changes nothing there; on Windows it would
  not build (`ChildStdin` has no `Default`). The unix supervisor has its own four tests (`tests/embedded/supervisor.rs`),
  all caught. Its six timeouts in the same run are servers that never start once their start is replaced: caught.

## Needs a real web view: the plain calls into WebView2, WKWebView and the OS

Each of these hands a value to the web view or the OS and decides nothing a test can reach without one. The path that
drives each in the real app is named; the Windows ones are owed on Windows (the PC pass list) where that pass has not
run them yet.

### wry_host.rs (Windows and macOS)

- `placed` → `Default::default()`: a site's rectangle. Path: open any site (Gitea, XWiki) and resize the window; the site
  fills its rectangle under knobas's bar.
- `Confined::apply` → `()`: puts the rules in place (WKContentRuleList on macOS, the request filter on Windows). Path:
  a Pleasant source's *Open web interface* against the probe page (`front.mjs`): the sink logs nothing.
- `load_now` → `()`, `WryView::load` → `()`: the load itself. Path: open any site.
- `remakes_to_confine` → `true` / `false`: `cfg!(windows)`, a constant per platform. Path: change a password site's
  allowed origins on Windows; the view is made again with new browser arguments.
- `flush` → `()`: the quit wait. Path: the Windows hand mutants of 2026-11-22 (sign in, quit, start: still signed in;
  probe checks 11, 11b, 12, settled at 80d6cbb9).
- `forget` → `()`: *Forget…* on a site; the next open is signed out (settled on Windows and the Mac, 2026-11-22).
- `WryView::confine` → `()`: as `Confined::apply`.
- `set_bounds` → `()`, `reload` → `()`, `url` → `None` / `Some("")` / `Some("xyzzy")`, `sign_in` → `()`: the page's
  rectangle, *Reload*, the address shown, the saved sign-in's fill. Path: open a site, reload it, follow a link (the
  address updates), sign in with a saved sign-in (settled for Gitea, Forgejo, XWiki on Windows and the Mac).
- `browser_process` → `None` / `Some(0)` / `Some(1)`: the process the quit waits for. Path: as `flush` (hand mutant
  settled on Windows).

### mac.rs (macOS)

- `flush` → `()`: its body is `let _ = deadline;` (WebKit's network process writes the cookies itself, measured ux29):
  equivalent.
- `under_title` → `0.0`, `1.0`, `-1.0`: an AppKit read of the title bar's height; what is done with it is
  `glue::title_room` and `glue::under_bar`, tested. Path: open a site in the Mac window; it starts under knobas's bar,
  not under the title bar (ux29).
- `confine` → `()` and line 522 (`delete !` on "is this list still current"): compiling and installing the
  WKContentRuleList. Path: a Pleasant source's *Open web interface* against the probe page in the Mac window; the sink
  logs nothing (2026-11-22).
- `peer_connections` → `()`: WebRTC off for a confined site. Path: the WebRTC probe page in the Mac window (2026-11-22:
  STUN left before, nothing after). Line 585 (`delete !`): only which call logs that WebKit has no switch.
- `load` → `()`: the load after the gate opens. Path: open a password site; it loads once its rules are in place.
- `make` → `Ok(Box::new(Default::default()))` (wry_host.rs) and `wrap` → `Default::default()` (mac.rs): unviable, they
  do not build.

### knobas-app

- `api/send.rs:310` (the client built without `direct`): `direct` only turns the system proxy and `HTTPS_PROXY` off for
  a request to this computer; with no proxy set, as in every test process, both clients are the same. A test would have
  to set a proxy in the process's environment, which every other test in the binary reads. Path: the explorer sending to
  `localhost` with `HTTPS_PROXY` set to a dead proxy; it still answers.
- `site.rs:678` (`TauriShell::sign_in` → `()`): the Tauri side of the saved sign-in's fill. Path: sign in with a saved
  sign-in (settled on Windows and the Mac, 2026-11-22).
- `capture.rs:448` (`open_window` → `()`, macOS leg): builds the capture window. Path: the capture shortcut opens it.

### win.rs (Windows)

- `browser_process` → `None`, `Some(0)`, `Some(1)` and `wait_for_exit` → `()`, `==` → `!=` (line 85): as
  `wry_host::flush` above; the hand mutants ran in the Windows window (80d6cbb9).
- `top_of` → `None` / `""` / `"xyzzy"` and line 244 (`!=` → `==`): which page a refused-request log line belongs to;
  they change only how often the log line repeats.
- `confine` → `Ok(())`, line 229 (`delete !`): the request filter's registration and its 403. Path: the Pleasant probe
  page in knobas's Windows window; owed on Windows ("check that WebResourceRequested's 403 covers every load kind").

## Not run

- knobas-app's Windows-only files: its test targets do not compile on Windows. Eighteen of its test files are written
  for unix (`std::os::unix` permissions and symlinks, `sh` scripts standing in for git), so making them build there is
  not a small change. The Windows code in knobas-app is driven by the Windows passes' real-app paths.
- The macOS leg's rerun (run 45236096794, cancelled at 21:32 after 3 h 14 min): knobas-db `socket.rs` 23 of 23
  caught, knobas-secrets `keyring_store.rs` 4 of 4 caught, knobas-site complete (its survivors are the ones above), and
  knobas-app 27 of its 128 macOS mutants (20 caught, 6 unviable, `capture.rs:448` above). The other 101 are not run.
- knobas-app's 7 shards whose baselines failed on the two tests fixed above (run 45227865249), and knobas-search, whose
  baseline failed on `coverage_plans` (its own fix): not run until the gate's rerun.
