- **An eighteenth `assets` command and four new DTOs, issue #448 (2026-09-07):** the Monitors tab's
  roster. **Ratified in advance by the spec (#427) Björn approved** — story 68, *"a Monitors tab
  with state filter chips and counts, a 24-hour bar per monitor drawn from the samples, last
  checks, uptime and certificate days"* — and by the #428 entry above, whose module-pair paragraph
  says that module is where **every** asset command lives, "including the route, import and alert
  commands #432, #439 and M4.1 add". Written with the implementing PR, per #428's, #431's, #434's,
  #435's, #439's and #443's pattern.

  **No migration.** `0021`'s `knobas.monitor_sample` (#443) is the timeseries this reads and
  `0020`'s `monitors` column is where the estate file's names live; **`0022` is still the next free
  number**, and #444's alert table is expected to take it.

  **The command and the DTOs.**

  ```rust
  #[tauri::command] pub async fn monitor_roster(..) -> Result<Vec<assets::MonitorRow>, IpcError>;

  pub struct MonitorRow {
      pub entity_id: String, pub source_id: String, pub name: String,
      pub state: Option<String>, pub monitor_type: Option<String>, pub target: Option<String>,
      pub response_time_ms: Option<f64>, pub checked_at: DateTime<Utc>,
      pub uptime: Vec<UptimeRatio>, pub cert_days_remaining: Option<f64>,
      pub web_url: Option<String>, pub tombstoned: bool,
      pub assets: Vec<MonitoredAsset>, pub samples: Vec<MonitorSample>,
  }
  pub struct UptimeRatio   { pub window: String, pub ratio: f64 }
  pub struct MonitoredAsset { pub id: String, pub name: String, pub path: Option<String> }
  pub struct MonitorSample { pub taken_at: DateTime<Utc>, pub state: Option<String> }
  ```

  Mirrored in `app/src/lib/ipc/assets.ts` as `monitorRoster()` and the four interfaces. **One line
  appended** at the foot of `crates/knobas-app/src/lib.rs`'s `generate_handler!` list, under
  #443's; neither barrel is rewritten (`app/src/lib/ipc/index.ts` already re-exports `./assets`
  wholesale) and the `commands/` + `ipc/` module **layout is untouched** — no new module pair,
  which is the thing §10.8 freezes about that directory. **No argument**: the tab is a destination
  of its own (`#/assets/monitors`) and the estate is not scoped by the room the reader came from,
  unlike `context_assets` and `source_assets`.

  **One command and not two, deliberately.** The obvious second one is a counts read for the chips.
  It is not here because the counts *are* counts of these rows — a second command would be a second
  answer that could disagree with the list beside it, and the frontend cannot draw the chips
  without the list anyway. `app/src/lib/assets/monitors.ts` derives them, and both the counts and
  the filter go through one `chipOf`, so a chip reading `warn 3` over a list of two is not a state
  the tab can reach.

  **The samples cross raw, and the bar is bucketed on the other side.** The row carries the last
  day's samples as they were recorded, oldest first; the tab folds them into forty-eight half-hour
  buckets coloured by the **worst** state in each. Forty-eight is the mockup brief's own number
  (`mockups/shared/assets.md`: *"Check interval 60 s; 24 h bar = 1 440 checks (render as a
  48-segment bar)"*), and it is why the fold is on the frontend at all: the number of segments is a
  question about a strip a few hundred pixels wide, which is not a thing the backend should own.
  The answer is bounded without a `limit` — the window is a day and the sync interval's floor is
  60 s (`knobas_sync::config::check_interval`), so at most 1 440 rows per monitor.

  **`sync.item` and not `sync.live_item`, which is the one read in this module that does not go
  through the view.** A **tombstoned** monitor is a row the roster has to draw: Kuma drops a paused
  monitor from `/metrics` and #442's adapter tombstones it, so the view's `deleted_at is null`
  clause would silently delete the *Paused* chip and take the monitor's samples with it. The other
  half of that view is kept and kept deliberately — `coalesce(s.enabled, true)`, `0012`'s rule —
  because turning a source off is a statement about what the reader wants to see and is a
  different statement from Kuma pausing a check.

  **`state` is the newest sample's, and the mirror's only where there is none.** Not a preference
  between two sources: *warn* is knobas' own, derived from the response-time threshold at sample
  time (`0021`), and exists in no Kuma payload — so a roster reading its state off the mirror would
  count no warns for ever while drawing amber in every bar. Reading the newest in-window sample is
  also what makes the chip and the bar's right-hand end one statement rather than two that can
  disagree.

  **A payload read outside an adapter, under ADR-0007's interim discipline.** `KindPaths` has no
  slot shaped like "what kind of check", "what it watches" or "a ratio per window", so declaring
  them would itself be a `crates/knobas-source/src/**` change and a §10.8 conversation of its own —
  the reasoning `knobas_sync::samples`' response-time read already records. `assets::reading_of` is
  the one named place, every field misses rather than guesses (a non-string type, a non-number
  reading, a `uptime` that is not an object, a blank hostname), and the failure direction is
  absence: a drifted key draws an empty cell, never a target that is somebody else's host. The one
  read outside it is `state`, taken with `text_at` because it is the *fallback* the samples
  normally supply, and it is the same `payload->>'state'` the pane's monitoring section already
  makes (#445).

  **`#/assets/monitors` is an address, and `AssetsTab` grows its second member.** The tab rides in
  the hash so a reader can hand somebody the roster; the chip filter deliberately does **not**,
  because a link to four of eight monitors that does not say so is worse than no link. `AssetsTab`
  was written as `"tree"` alone precisely so this would have to be a typed change rather than a
  string falling through, and it is: `MonitorsView` is a view of its own behind the tab strip both
  tabs draw from (`AssetsTabs.svelte`), because the roster needs no columns, no pane and no search
  box and one component covering both would load the whole estate to draw a list of the mirror.

  Pinned by: `commands::assets::tests::the_monitor_roster_matches_its_typescript_mirror` (all four
  shapes, with every optional field filled, plus the RFC 3339 spelling of `checked_at`) and the
  registration and argument-name loops (now eighteen commands); `tests/wiring.rs`, which proves
  every declared command is in the handler list; seven tests in `tests/assets_ipc.rs`
  (`the_roster_draws_each_monitors_type_target_reading_uptime_and_certificate`,
  `a_drifted_payload_reads_as_absence_and_never_as_a_guess`,
  `a_monitors_state_is_its_newest_sample_and_the_mirror_only_without_one`,
  `the_bar_carries_the_last_days_samples_oldest_first_and_no_older_one`,
  `a_paused_monitor_stays_on_the_roster_with_its_samples`,
  `a_disabled_sources_monitors_leave_the_roster`,
  `a_monitor_lists_the_assets_it_watches_with_their_path`); ten tests in `monitors.test.ts` for the
  bucketing and the counts; thirteen in `MonitorsView.test.svelte.ts` for the tab; and the two
  address tests in `router.test.svelte.ts`.

  **Björn keeps the gate for frozen contracts and this entry is flagged for his review**, and in
  particular the new command and the four DTOs.

