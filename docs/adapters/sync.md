# Syncing

`sync` is most of an adapter. It reads the source and hands items to the sink; knobas writes them into the mirror, indexes them and draws them. This page is the set of rules a sync is held to, each with the check that holds it. The function shapes are in [reference/interface.md](reference/interface.md#interface-source-exported-required).

## One call, one run

knobas calls `sync(cursor)` on the source's schedule, never below the descriptor's `min_sync_interval_secs` (default 60). Each call is one run under the manifest's deadline, ten minutes by default. The host makes a fresh component for the call, so the cursor you return is the only thing the next run knows.

```rust
fn sync(&self, cursor: Option<Cursor>, sink: &mut Sink) -> Result<Cursor, SourceError>
```

`None` is a **full sync**: read the source from the top. knobas asks for one on a source's first run and on a **backfill**, which a reader starts by hand from the source's page to re-fetch unchanged items after a release widens what a payload holds. It runs no periodic full sync. Every other run is incremental, from the cursor your previous run returned.

## The rules

Each rule is held by the contract battery (`knobas_source::contract::battery`), by clause number, or by the engine at run time.

1. **A full sync hands over at least one item**, every id parses as `<source id>:<key>` with the source's own id, and every kind is one the descriptor declares. Clause 1.
2. **An idle run hands nothing and returns the cursor it was given, byte for byte.** The engine compares the two to decide whether anything happened. A cursor stamped with the time of the call turns every five-minute poll into an activity line. Clause 2, and the battery checks it by running an incremental sync right after a full one.
3. **The sink's error is yours to return, never to swallow.** `sink.item(item)?`. A sink fails when the mirror cannot write; a run that carries on and returns a fresh cursor would skip what the sink dropped. Clause 7 hands you a sink that fails on the first item.
4. **Faults are classified the same way in `test_connection` and in `sync`.** A credential that worked when the source was added fails mid-sync months later, and the reader sees the class: `Unauthorized` puts *Re-enter* on screen. Clauses 3 and 4.
5. **Every declared path lands on your own records or reads as nothing.** Clause 6, and the role data check on the source's first sync. [Declared paths](#declared-paths-and-the-trap-of-a-string).
6. **A refresh answers what a sync would hand over for that id.** Clause 13. [Refresh](#refresh).

## Items

```rust
SyncItem {
    entity: EntityRef::new(&self.id, key),   // the source id, your stable key
    kind: "page".into(),                     // a kind the descriptor declares
    title,                                   // what lists and the launcher show
    body_text,                               // what full-text search indexes
    author: Some(account),                   // the account name, as `username` spells it
    updated_at,                              // the source's own change time
    payload: raw_record,                     // the source's record, for declared paths
    web_url: Some(url),                      // where a person reads it in the source
    deleted: false,
}
```

- **The key** is yours and must not move: an item whose key changes is a new item, and the old one lingers until something tombstones it. Prefer the source's immutable id where it has one. The XWiki adapter uses the page's full name, which XWiki keeps for a page's life; a rename there is a move, and the sync hands the old name over as deleted.
- **The payload** is the source's record in its own shape. Keep what your projection, view and declared paths read, and drop the bulk you never read (hypermedia links, translations). Facts the record lacks but a reader needs (a page's parent chain, display names) go in an object of your own beside the record. The XWiki adapter keeps them under `knobas`.
- **`author`** is compared with the source's `username` config for "my items", so spell it the way a person types their login.
- **`body_text`** is what search reads. For a `page` of a docs source knobas ignores it and builds the text from your projection ([docs-sources.md](docs-sources.md)).

## Cursors

The cursor is an opaque string. Make it a small JSON object with a version, so a later release can tell its own cursors from yours:

```json
{"v":2,"since":1794482667000,"pages":["Tidewater.WebHome", "..."]}
```

Read a cursor your adapter cannot parse, or one of an older version, as `None` and walk the source from the top. That is how an upgrade that changes the cursor's shape starts over. The engine still sees a run handed a position, so that walk sweeps nothing ([Deletions](#deletions)).

Take the position from **the same clock** the next incremental read filters on. XWiki stamps a page's version in its history up to a second later than the page record's own `modified`. The XWiki adapter's first release took its position from the records and its incremental read from the history, so a page edited at the boundary came back on the next idle run, and clause 2 failed. It now reads the history's newest instant before the walk.

Move the position only for changes in scope. A sync of two spaces that advanced its cursor for an edit in a third would hand nothing and return a new cursor: an idle run that looks busy.

## Deletions

There are two ways a deleted record leaves the mirror.

- **The sweep.** After a run the engine started without a cursor, knobas tombstones every item of each kind declared `full_sync_exhaustive` that the run did not touch. Declare a kind exhaustive only when a full sync really hands over all of it. A kind your full sync budgets (the newest 100 builds) is not exhaustive, or every run would tombstone the rest.
- **A tombstone you hand over.** An item with `deleted: true`, its entity and kind set, and anything for the rest. knobas tombstones it.

The sweep almost never runs for an established source. The engine starts a run without a cursor for a source's first run, when there is nothing to sweep, and for a backfill, which deliberately tombstones nothing (`knobas_sync::run_backfill`). A walk from the top that your adapter starts itself, from a cursor it cannot read, is a run handed a position to the engine, so it sweeps nothing either. And an external adapter cannot say it walked a kind whole on an incremental run: the WIT carries no `walked_whole`. So **an incremental sync must find deletions itself**, or the mirror keeps deleted records for ever. Three ways, best first:

1. The source reports them: a change feed with delete events, a trash or audit list. Hand each over as deleted.
2. The cursor remembers what the source holds, and each run lists ids and tombstones the missing ones. The XWiki adapter keeps every page's full name in its cursor, about 60 bytes a page, and lists the synced spaces on every run.
3. The cursor carries a digest per record, and each run reads everything and hands over only what changed. The [quickstart](quickstart.md)'s memo board does this.

## Progress

`sink.walked(done, of, unit)` drives the progress bar: before each unit of your outer walk and once after the last, `unit` a plural noun printed as is (`"pages"`). It returns nothing and cannot fail. A run that hands nothing still reports `0 of 0`.

## Errors

| The failure | Return |
|---|---|
| 401 or 403, or no credential to send | `SourceError::Unauthorized { status }` |
| No answer: refused, DNS, TLS, a timeout on a read | `SourceError::Unreachable(detail)` |
| An answer you cannot use: another status, a body that does not decode, a base URL that is no instance of your product | `SourceError::Protocol { status, message }` |
| The sink refused an item | the sink's own error, unchanged |

`message` is what the reader sees on the source's health line. Say what to do: the XWiki adapter answers a 404 from the REST root with *"… does not answer as XWiki's REST API: the base URL is the XWiki web app, usually ending in /xwiki"*.

Check what an unauthenticated request really gets. XWiki serves a request whose basic auth failed as the guest, with `200`, wherever guests may read. A wrong password would have synced what a guest sees, so the XWiki adapter asks whom XWiki served the request as (the `XWiki-User` response header) at the start of `test_connection`, `sync`, `write` and `refresh`, and answers `Unauthorized` when it is not the configured user.

## Declared paths, and the trap of a string

A declared path is a list of object keys from the payload root. `at(payload, path)` walks it and stops at the first missing key, `null` or non-object; anything short of the end is a miss. Clause 6 then tells "the source has no value here" from "the adapter named a key its records do not have" by one test: did the walk stop at an object without the next key?

So a path that runs into a **string** reads as the source saying nothing, on every record, and no check fails. The XWiki adapter's first descriptor declared the project at `["space", "key"]` while its pages kept XWiki's own `space`, a string, at the root: every page had no project, and the static checks and the data check passed. Only the labels path, which ran into an object, tripped clause 6. Write a unit test that resolves every declared path on a payload your sync builds; the XWiki adapter's is `the_declared_paths_land_on_a_page_payload` in `adapter/src/descriptor.rs`.

## Refresh

After a write lands, knobas reads the item back so the mirror shows the change before the next run. An adapter that can read one item by id:

1. declares `Capability::Refresh`, and `refreshes: true` on each kind it reads;
2. implements `Adapter::refresh(entity)`, answering `RefreshAnswer::Item` (exactly what `sync` would hand over for that id: same kind, title and payload), `Gone` (the source no longer has it), or `NotOffered` (not this kind).

A failed read is an error with `sync`'s classification, never `Gone`. Clause 13 refreshes every item the full sync handed over and compares, so build both through one function. The XWiki adapter's `Run::item` is called by both; it is also why a refresh lists the synced spaces first, since a page's payload names the titles of the pages it links to.
