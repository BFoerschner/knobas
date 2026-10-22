- **`crates/knobas-source/src/**` — issue #1622 (2026-10-22): the minute-resolution watermark Jira
  and Confluence both carry lives once, in `knobas_source::watermark`, and Jira gains the run-start
  ceiling clamp Confluence already had.** Parent spec #1618 (rulings A4, A1; ADPT-8). No migration,
  no IPC command or type, no `commands/` or `ipc/` module, no SPI change, no cursor version: Jira's
  `CURSOR_VERSION` stays `4` and Confluence's `3`, and each stored envelope is byte for byte what it
  was (both adapters' cursor tests assert it literally).

  **`crates/knobas-source` — additive, one new module `watermark.rs`:**

  ```rust
  pub const OVERLAP_MINUTES: i64 = 2;
  pub const DST_OVERLAP_MINUTES: i64 = 65;
  pub const SEEN_CAP: usize = 500;
  pub const MIN_UTC_OFFSET_SECS: i32 = -12 * 3600;

  pub trait MinuteLiteral {
      const FORMAT: &'static str;
      fn render(t: DateTime<Utc>, offset_secs: i32) -> String;      // provided
      fn floor(t: DateTime<Utc>, offset_secs: i32) -> DateTime<Utc>; // provided: rendered and read back
  }
  pub enum Jql {} // impl MinuteLiteral, "%Y-%m-%d %H:%M"
  pub enum Cql {} // impl MinuteLiteral, "%Y-%m-%d %H:%M"

  pub trait Entry: Clone {
      type Version: ?Sized;
      fn key(&self) -> &str;
      fn stamp(&self) -> DateTime<Utc>;
      fn is_version(&self, version: &Self::Version) -> bool;
  }
  pub fn delivered<E: Entry>(seen: &[E], key: &str, version: &E::Version) -> bool;

  pub fn since(watermark: Option<DateTime<Utc>>, recorded_offset_secs: i32,
               current_offset_secs: i32) -> Option<DateTime<Utc>>;

  #[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
  pub struct Bounds { pub previous: Option<DateTime<Utc>>, pub ceiling: Option<DateTime<Utc>> }
  #[derive(Debug, Clone, PartialEq, Eq)]
  pub struct Advanced<E> { pub watermark: DateTime<Utc>, pub seen: Vec<E> }
  pub fn advanced<L: MinuteLiteral, E: Entry>(bounds: Bounds, newest: DateTime<Utc>,
                                              tz_offset_secs: i32, seen_in_window: &[E]) -> Advanced<E>;
  ```

  `advanced`'s watermark is `max(previous, min(newest, ceiling))`: held at the ceiling, never below
  the previous watermark. Its `seen` is every entry at or after `L::floor(watermark −
  OVERLAP_MINUTES)`, newest first with the key breaking ties, truncated to `SEEN_CAP`. A zone no
  `FixedOffset` holds renders at `MIN_UTC_OFFSET_SECS`, as both adapters' copies did.

  **The adapters (not frozen surfaces, named so the entry matches the diff):**

  - Both: the `OVERLAP_MINUTES`, `DST_OVERLAP_MINUTES` and `SEEN_CAP` definitions, the literal
    rendering (`format_jql_time`/`jql_floor`, `format_cql_time`/`cql_floor`) and `zone_of` are gone;
    each `time.rs` keeps its response parsers and re-exports `MIN_UTC_OFFSET_SECS`
    (`knobas_source_confluence::MIN_UTC_OFFSET_SECS` stays public). Each `Seen` implements `Entry`:
    Jira's version is the fingerprint `h`, Confluence's the `version.number` `n` (an entry with no
    number matches none). `JiraCursor::advanced` and `ConfluenceCursor::advanced` take
    `(Bounds, newest, tz_offset_secs, seen_in_window)`.
  - Confluence: the ceiling clamp and its never-backwards floor move out of `sync.rs` into
    `watermark::advanced`, fed through `Bounds`; the run-start probe is unchanged.
  - **Jira: one more request per sync run**, the ceiling probe: `GET rest/api/2/search` with
    `jql=<scope> ORDER BY updated DESC`, `startAt=0`, `maxResults=1`, `fields=updated` and the
    walk's `expand=changelog` (`jql::build_ceiling_jql`, the walk's scope), sent after
    `serverInfo` and before the walk. Its `updated` is the run's ceiling; an empty corpus gives
    none and clamps nothing. A probe failure fails the run as a walk failure does.
