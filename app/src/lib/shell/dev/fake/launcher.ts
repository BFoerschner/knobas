/**
 * `?fake-ipc`: The launcher (#504): its home board, the Tree's search box (#430), the
 * `>` palette (#1417) and the query box's suggestions (#601).
 */
import type { SearchResponse } from "../../../ipc/search";
import type { Handlers } from "../fake-tauri";
import { fakeNotesAreShown } from "./capture";
import { CORPUS, SYNCED_AT, mirrorRow } from "./corpus";
import { FIXTURE_ESTATE, assetPathText } from "./estate";
import { fakeSmartListItems, fakeSmartLists } from "./smart-lists";
import { FIXTURE_SOURCES } from "./sources";
import { FIXTURE_QUEUE } from "./write-queue";
import { keyOf } from "../../../time/format";

/**
 * `search` for a ticket key the corpus holds (#1476), `PAY-231`: every
 * source's ticket of that key, so the launcher's rows for a key two sources
 * hold can be looked at. Any other words keep the refusal below; this is a
 * key lookup, not the corpus's full-text search.
 */
function ticketsByKey(raw: string): SearchResponse | null {
  if (!/^[A-Za-z][A-Za-z0-9]*-\d+$/.test(raw)) return null;
  const key = raw.toUpperCase();
  const hits = CORPUS.filter(
    (entry) => entry.kind === "ticket" && entry.deleted_at === null && keyOf(entry.entity_id) === key,
  ).map((entry, index) => ({ ...mirrorRow(entry), rank: 1 - index / 10, snippet: [] }));
  if (hits.length === 0) return null;
  return {
    interpreted: {
      text: raw,
      prefix: null,
      filters: { sources: [], kinds: [], updated_within_days: null, mine: false, authors: [] },
      unknown_tokens: [],
    },
    groups: [{ kind: "ticket", label: "Ticket", plural: "Tickets", monogram: "TK", total: hits.length, hits }],
    total: hits.length,
    took_ms: 2,
    coverage: [],
  };
}

/**
 * `search` for `>` (#1417): what the backend answers, the text after the
 * marker and no groups (`empty_corpus` in knobas-search: `>` is not a corpus
 * search). The launcher draws its navigation palette from that, so the
 * palette can be looked at under `?fake-ipc`.
 */
function actionPalette(raw: string): SearchResponse | null {
  if (!raw.startsWith(">")) return null;
  return {
    interpreted: {
      text: raw.slice(1).trim(),
      prefix: "action",
      filters: { sources: [], kinds: [], updated_within_days: null, mine: false, authors: [] },
      unknown_tokens: [],
    },
    groups: [],
    total: 0,
    took_ms: 0,
    coverage: [],
  };
}

/**
 * `search`, narrowed to the estate — what `assets/tree.ts`'s `estateQuery`
 * asks for.
 *
 * A **substring match over names**, where the real corpus is PostgreSQL FTS
 * over the name and the ancestor path with `corpus::ASSET`'s weights. So this
 * answers the same *shape* and a different question, which is all a browser
 * pass needs: what it certifies is that the box draws its offers and that
 * taking one opens the columns. What certifies the query itself is
 * `crates/knobas-app/tests/it/search_ipc.rs`, against a real database.
 */
function estateSearch(args: Record<string, unknown>): SearchResponse {
  const query = (args.query ?? {}) as {
    raw?: string;
    filters?: { kinds?: string[] };
  };
  // `list:<id>` routes to the list and not to the corpus, which is what
  // `Searcher::search` does with a `Prefix::List` -- and it is the only way a
  // smart list is ever opened, because `Launcher.svelte` opens one by typing
  // its id into the box. Without this branch the rail draws but every row on
  // it answers with the refusal below.
  const listed = /^list:([a-z0-9-]+)$/.exec((query.raw ?? "").trim());
  if (listed) return fakeSmartListItems({ id: listed[1]!, limit: 50 });
  const palette = actionPalette((query.raw ?? "").trim());
  if (palette !== null) return palette;
  const byKey = ticketsByKey((query.raw ?? "").trim());
  if (byKey !== null) return byKey;
  if (!(query.filters?.kinds ?? []).includes("asset")) {
    throw {
      code: "not_ready",
      message: "the fixture answers the Tree's asset search only (#430) — there is no mirror here",
      source_id: null,
    };
  }
  const needle = (query.raw ?? "").trim().toLowerCase();
  const hits = FIXTURE_ESTATE.filter(
    (asset) => needle !== "" && asset.name.toLowerCase().includes(needle),
  ).map((asset, index) => ({
    entity_id: asset.id,
    kind: "asset",
    source_id: "asset",
    updated_at: null,
    synced_at: SYNCED_AT,
    title: asset.name,
    path: assetPathText(asset),
    state: null,
    rank: 1 - index / 10,
    snippet: [],
  }));
  return {
    interpreted: {
      text: query.raw ?? "",
      prefix: null,
      filters: { sources: [], kinds: ["asset"], updated_within_days: null, mine: false, authors: [] },
      unknown_tokens: [],
    },
    groups:
      hits.length === 0
        ? []
        : [
            {
              kind: "asset",
              label: "Asset",
              plural: "Assets",
              monogram: "AS",
              total: hits.length,
              hits,
            },
          ],
    total: hits.length,
    took_ms: 2,
    coverage: [],
  };
}

/** This feature's commands. */
export function handlers(): Handlers {
  return {
    // The launcher board (#504). Until now `launcher_home` had no handler at
    // all, so `⌘K` under `?fake-ipc` drew a red "command not found" over the
    // whole board -- which is why the three estate lists this ticket adds
    // could not be looked at in a browser.
    //
    // **Everything here is fixture-only** and none of it certifies the bridge.
    // What it is for is the walk: that the rail draws a row per list with its
    // count, and that Enter on an estate row hands the shell the Tree's
    // address. The rules themselves are witnessed at the IPC seam over a real
    // PostgreSQL, in `crates/knobas-app/tests/it/search_ipc.rs`.
    launcher_home: () => ({
      smart_lists: fakeSmartLists(),
      // The newest of the fixture's corpus, in the shape `list_entities`
      // answers in -- the board draws recent rows with the same component.
      recent: CORPUS.filter((entry) => entry.deleted_at === null)
        // `recent_sql!`'s order, because the comment above has to be true:
        // newest first, a row the source never dated last, and the id to
        // break a tie. CORPUS is in reading order, not date order.
        .sort(
          (left, right) =>
            (right.updated_at ?? "").localeCompare(left.updated_at ?? "") ||
            left.entity_id.localeCompare(right.entity_id),
        )
        .slice(0, 8)
        .map(mirrorRow),
      sources: FIXTURE_SOURCES.map((source) => source.health),
      pending_writes: FIXTURE_QUEUE.filter((row) => row.state === "pending").length,
      notes_shown: fakeNotesAreShown(),
    }),
    // The Tree's search box (#430), and **only** the Tree's: a query that is
    // not narrowed to assets is refused rather than answered from the estate,
    // because the launcher's corpus is the mirror's and this fixture has no
    // mirror to search. A handler that answered everything with assets would
    // make a QA pass over the launcher look like it worked. `>` is answered
    // (#1417) because the backend answers it without a corpus too.
    search: (args) => estateSearch(args),
    // The query box's suggestions (#601). **Fixture-only**: the kinds the
    // corpus holds and the configured sources, so `kind:` and `source:` can be
    // looked at under `?fake-ipc`; no parents, because this corpus declares
    // none. "Only values that exist" is witnessed over a real PostgreSQL, in
    // `crates/knobas-search/tests/it/suggest.rs`.
    filter_suggestions: () => ({
      kinds: [...new Set(CORPUS.filter((e) => e.deleted_at === null).map((e) => e.kind))]
        .sort()
        .map((kind) => ({ kind, label: kind })),
      sources: FIXTURE_SOURCES.map((source) => ({
        id: source.id,
        display_name: source.display_name,
      })),
      parents: [],
      // The ticket keys' values (#811), as the Tidewater fixture's Jira holds
      // them: fixture-only, like the lists above.
      fields: [
        { key: "project", value: "PAY", detail: "Payments" },
        { key: "project", value: "OPS", detail: "Operations" },
        ...["In Progress", "To Do", "In Review", "Done"].map((value) => ({
          key: "status",
          value,
          detail: "",
        })),
        ...["Story", "Bug", "Task"].map((value) => ({ key: "issuetype", value, detail: "" })),
        ...["High", "Medium", "Low"].map((value) => ({ key: "priority", value, detail: "" })),
        { key: "assignee", value: "@me", detail: "you" },
        { key: "assignee", value: "jonas.becker", detail: "" },
        { key: "label", value: "sepa", detail: "" },
        { key: "sprint", value: "current", detail: "the active sprint" },
        { key: "assigned", value: "today", detail: "assigned to you, changed in the last day" },
        { key: "assigned", value: "week", detail: "assigned to you, changed in the last 7 days" },
        // `has:`'s four (#815), which every installation has.
        { key: "has", value: "failing-build", detail: "a build around it failed" },
        { key: "has", value: "blocker", detail: "something blocks it" },
        { key: "has", value: "open-pr", detail: "a pull request of it is open" },
        { key: "has", value: "review-for-me", detail: "a review is asked of you" },
      ],
    }),
  };
}
