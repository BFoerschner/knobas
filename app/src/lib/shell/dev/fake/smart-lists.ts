/**
 * `?fake-ipc`: Smart lists (#504, #506): the built-in and saved lists, their items and
 * the three writes.
 *
 * **`crates/knobas-search/src/lists.rs` reads this file** (`include_str!`)
 * and slices `fakeSmartLists` and `fakeSavedLists` out of it, with
 * `FIXTURE_SAVED` declared after `fakeSmartLists` as its negative control. It
 * is its own module so the rest of the fixture is frontend-only.
 */
import type { SearchResponse } from "../../../ipc/search";
import type { Handlers } from "../fake-tauri";
import { keepSeed } from "../fake-seed";
import { FAKE_CONTEXTS, FAKE_CONTEXT_ASSETS, SYNCED_AT } from "./corpus";
import { FIXTURE_ESTATE, assetPathText } from "./estate";
import { fakeMonitorRoster, fakeOpenAlerts, fakeUnmonitored } from "./monitors";

/**
 * The launcher's smart-list rail (#504) -- **fixture-only**.
 *
 * A restatement of `knobas_search::lists`' seven lists, in this file's own
 * terms, and a restatement is a copy that can drift: the Rust registry is the
 * authority and a divergence here is a bug in this file, never a second
 * opinion. `miniBoard` and `listEntities` carry the same warning for the same
 * reason. Nothing here proves a predicate; what it lets a browser see is the
 * rail, the counts beside it and what a row opens.
 *
 * **The three literals per list are pinned** since #506:
 * `knobas_search::lists`' `the_builtin_registry_matches_its_typescript_fixture`
 * parses the arrays below and compares the id, label and blurb triples against
 * `BUILTINS` itself. Everything the two paragraphs after this one call a
 * divergence — the counts, the badges, the row order, and what a reader with
 * no identity configured actually reads on the two `@me` rows — is outside
 * that pin, deliberately, and the test's own doc says so.
 *
 * **The four mirror lists honestly read 0.** This fixture's corpus is frozen
 * at `SYNCED_AT` -- 22 August 2026 -- so nothing in it changed today and
 * nothing synced in the last hour; and no source here carries a username, so
 * the two `@me` lists are empty for the reason the real ones would be.
 *
 * Their **descriptions** are the Rust blurbs, copied, and for the two `@me`
 * lists that is a divergence rather than a copy: with no identity configured
 * `lists::describe` replaces the blurb with `describe_missing_identity()` --
 * three sentences about *Test connection* filling a source's username in.
 * Restating that here would be the longest copied string in this file and the
 * likeliest to rot, and it is not what #504 is about. What a walk sees on
 * those two rows is therefore the blurb where the app would show the advice.
 *
 * **The three estate lists are derived from the estate this file draws**, each
 * from the nearest thing the fixture has to the rule:
 *
 * * *Not monitored* is `fakeUnmonitored` -- the assets the estate file names no
 *   monitor for -- which is what the backend's read answers over a mirror that
 *   has resolved those names into links.
 * * *Open alerts on my jobs* is the assets of `fakeOpenAlerts` that an
 *   **unarchived** fake context holds (`FAKE_CONTEXT_ASSETS`, keyed by the
 *   context's anchor), which is the registry's rule over the fixture's one
 *   membership table: an archived context holds nothing, and an alerted asset
 *   no job holds is not on the list. The fixture has no link graph, so an
 *   asset reaches a context only through that table and not through an
 *   ancestor; the ancestor walk is asserted over a real database in
 *   `search_ipc.rs`. Acked alerts stay in, as in the registry.
 * * *Certificates expiring* is the roster rows whose `cert_days_remaining` is
 *   under thirty -- the one certificate the roster invents, at nine days.
 *
 * The change badge is `false` on every row: a badge is a comparison against a
 * stamp in `knobas.setting`, and this fixture has no setting table to remember
 * one in. A badge that was always on would be the more misleading of the two.
 */
export function fakeSmartLists() {
  const mirror = [
    ["changed-today", "Changed today", "Everything a source touched since midnight."],
    [
      "mine",
      "My items",
      "Items your configured accounts are the author of, from the last 30 days.",
    ],
    ["mine-stale", "Mine, untouched 14 days", "Yours, and nothing has moved them in a fortnight."],
    ["just-synced", "Just synced", "What the last hour of syncing brought in."],
  ] as const;
  const estate = [
    [
      "not-monitored",
      "Not monitored",
      "Estate assets with no monitor attached to them.",
    ],
    [
      "alerts-in-context",
      "Open alerts on my jobs",
      "Assets a job you have not finished holds, with a monitor in trouble.",
    ],
    [
      "certs-expiring",
      "Certificates expiring",
      "Assets whose certificate runs out in under 30 days.",
    ],
  ] as const;

  return [
    ...mirror.map(([id, label, description]) => ({
      id,
      label,
      count: 0,
      changed: false,
      description,
      saved: false,
      needs_attention: false,
    })),
    ...estate.map(([id, label, description]) => ({
      id,
      label,
      count: fakeListAssets(id).length,
      changed: false,
      description,
      saved: false,
      needs_attention: false,
    })),
    ...fakeSavedLists(),
  ];
}

/**
 * The lists a reader saved (#506) -- **fixture-only**, and mutable, because
 * saving one is the gesture the walk is here to make.
 *
 * Seeded with **one row of each kind the rail can draw**: a runnable saved
 * query, and one that needs attention. The second is seeded rather than made
 * because no command can make one -- `knobas_search::saved::create` runs the
 * grammar first and refuses a query it could not run, so a needs-attention row
 * is what a *grammar change* leaves behind, and the backend's own IPC-seam
 * test writes its row with SQL for the same reason.
 *
 * `needs_attention` is carried as **data** rather than re-derived here. The
 * verdict is `saved::plan`'s, over §4's grammar and this installation's
 * vocabulary, and a second reading of that in TypeScript is exactly the copy
 * that drifts. What the walk sees is the two states drawn; what decides them
 * is asserted over a real database in `crates/knobas-app/tests/it/search_ipc.rs`.
 */
const FIXTURE_SAVED: { id: string; label: string; query: string; broken?: boolean }[] = [
  { id: "gitea-boxes", label: "Gitea boxes", query: "gitea" },
  { id: "todays-palette", label: "Today's palette", query: "> palette", broken: true },
];

/**
 * What one saved list stands for here: a **substring match over asset names**,
 * where the real count is PostgreSQL FTS over four corpora -- the mirror,
 * notes, assets and routes -- with `corpus::ALL`'s weights. The same stand-in
 * `estateSearch` already makes, for the same reason: this fixture has no
 * mirror to search.
 *
 * The change badge is `false` on every row, as it is on the built-ins: a badge
 * is a comparison against a stamp in `knobas.setting`, and there is no setting
 * table here to remember one in.
 */
function fakeSavedAssets(query: string) {
  const needle = query.trim().toLowerCase();
  return needle === ""
    ? []
    : FIXTURE_ESTATE.filter((asset) => asset.name.toLowerCase().includes(needle));
}

function fakeSavedLists() {
  return FIXTURE_SAVED.map((list) => ({
    id: list.id,
    label: list.label,
    count: list.broken ? 0 : fakeSavedAssets(list.query).length,
    changed: false,
    // The saved query is a saved list's own blurb, and a refused one says
    // which rule refused it -- `saved::Refusal::description`, whose heading is
    // the wording the rail draws.
    description: list.broken
      ? "Needs attention: the saved query starts with a prefix that is not a search. Delete it and save the search again."
      : list.query,
    saved: true,
    needs_attention: list.broken === true,
  }));
}

/**
 * `create_smart_list`: save the box's query -- **fixture-only**.
 *
 * The id is slugged and de-duplicated the way `knobas_search::saved`'s `slug`
 * and `free_id` do it -- **against the built-ins as well as the saved rows**,
 * because `list:<id>` is one namespace and a fixture that let a new list take
 * `mine` would draw two rows under one `{#each}` key. What it does *not* do is
 * judge the query: every refusal the real command makes past a blank one is
 * §4's grammar's, and this fixture has no grammar. The panel is what keeps an
 * unrunnable query away from here -- `Session.canSave` reads the backend's own
 * interpretation and offers the control on results only.
 */
function fakeCreateSmartList(args: Record<string, unknown>) {
  const label = String(args["label"] ?? "").trim();
  const query = String(args["query"] ?? "").trim();
  if (label === "" || query === "") {
    throw { code: "invalid", message: "a saved list needs a name and a query", source_id: null };
  }
  const slug = label
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 48)
    .replace(/-+$/, "");
  const base = slug === "" ? "list" : slug;
  const taken = new Set(fakeSmartLists().map((list) => list.id));
  let id = base;
  for (let suffix = 2; taken.has(id); suffix += 1) {
    id = `${base}-${suffix}`;
  }
  FIXTURE_SAVED.push({ id, label, query });
  const made = fakeSavedLists().find((list) => list.id === id);
  if (!made) throw { code: "internal", message: "the fixture lost the row it just made", source_id: null };
  return made;
}

/** `rename_smart_list`: the id does not move, exactly as it does not there. */
function fakeRenameSmartList(args: Record<string, unknown>) {
  const id = String(args["id"] ?? "");
  const label = String(args["label"] ?? "").trim();
  const row = FIXTURE_SAVED.find((list) => list.id === id);
  if (!row) throw { code: "not_found", message: `no saved list ${id}`, source_id: null };
  if (label === "") {
    throw { code: "invalid", message: "a saved list needs a name", source_id: null };
  }
  row.label = label;
  return null;
}

/** `delete_smart_list`. */
function fakeDeleteSmartList(args: Record<string, unknown>) {
  const id = String(args["id"] ?? "");
  const at = FIXTURE_SAVED.findIndex((list) => list.id === id);
  if (at < 0) throw { code: "not_found", message: `no saved list ${id}`, source_id: null };
  FIXTURE_SAVED.splice(at, 1);
  return null;
}

/**
 * The assets one estate list answers with -- **fixture-only**, see
 * `fakeSmartLists` for what each stands in for and what it drops.
 */
function fakeListAssets(id: string): (typeof FIXTURE_ESTATE)[number][] {
  const byId = (assetId: string) => FIXTURE_ESTATE.find((asset) => asset.id === assetId);
  switch (id) {
    case "not-monitored":
      return fakeUnmonitored()
        .map((row) => byId(row.id))
        .filter((asset) => asset !== undefined);
    case "alerts-in-context":
      return fakeJobAlerts()
        .map((assetId) => byId(assetId))
        .filter((asset) => asset !== undefined);
    case "certs-expiring":
      return fakeMonitorRoster()
        .filter((row) => row.cert_days_remaining !== null && row.cert_days_remaining < 30)
        .flatMap((row) => row.assets)
        .map((asset) => byId(asset.id))
        .filter((asset) => asset !== undefined);
    default:
      return [];
  }
}

/**
 * The ids of the alerted assets an unarchived fake context holds (#1503),
 * each once, in the alerts' order.
 */
function fakeJobAlerts(): string[] {
  const held = new Set(
    FAKE_CONTEXTS.filter((context) => context.archived_at === null).flatMap(
      (context) => FAKE_CONTEXT_ASSETS[context.anchor_id ?? ""] ?? [],
    ),
  );
  const alerted = fakeOpenAlerts().flatMap((alert) => alert.assets.map((asset) => asset.id));
  return [...new Set(alerted)].filter((id) => held.has(id));
}

/**
 * One list's rows, `search`'s answer to `list:<id>`, in the shape a search
 * answers in -- which is the whole point of a smart list on this side of the
 * wire, and what lets the launcher render one with the component it renders
 * results with.
 *
 * An unknown id is refused the way the real search refuses it, so a typo in
 * `list:` under `?fake-ipc` looks like a typo and not like an empty list.
 *
 * **The order is the fixture's, not the statement's** -- the last of the
 * divergences `fakeSmartLists` lists. `rows_over_estate!` returns an estate
 * list `order by coalesce(path,'') asc, title asc, entity_id asc`; these rows
 * come back in whatever order the roster and the alert list already build,
 * because reproducing that ordering here would be a fourth restatement to keep
 * in step and the walk asserts nothing about it. What a browser sees is which
 * rows, not which first.
 */
export function fakeSmartListItems(args: Record<string, unknown>): SearchResponse {
  const id = String(args["id"] ?? "");
  const known = fakeSmartLists().some((list) => list.id === id);
  if (!known) {
    throw { code: "invalid", message: `unknown smart list: ${id}`, source_id: null };
  }
  const limit = Number(args["limit"] ?? 20);
  // A saved list answers with its own query's rows, and a built-in with its
  // rule's -- one id namespace and one shape, which is what lets the launcher
  // open either by typing `list:<id>`.
  const saved = FIXTURE_SAVED.find((list) => list.id === id);
  const assets = saved ? fakeSavedAssets(saved.query) : fakeListAssets(id);
  const hits = assets.slice(0, limit).map((asset) => ({
    entity_id: asset.id,
    kind: "asset",
    source_id: "asset",
    updated_at: null,
    synced_at: SYNCED_AT,
    title: asset.name,
    path: assetPathText(asset),
    state: null,
    // A list row was matched against nothing, so it carries no rank and no
    // excerpt -- the rule the real response follows.
    rank: 0,
    snippet: [],
  }));
  return {
    interpreted: {
      text: "",
      prefix: "list",
      filters: { sources: [], kinds: [], updated_within_days: null, mine: false, authors: [] },
      unknown_tokens: [],
    },
    groups:
      assets.length === 0
        ? []
        : [
            {
              kind: "asset",
              label: "Asset",
              plural: "Assets",
              monogram: "AS",
              // The list's total, never the page's.
              total: assets.length,
              hits,
            },
          ],
    total: assets.length,
    took_ms: 2,
    coverage: [],
  };
}

/** This feature's commands. */
export function handlers(): Handlers {
  return {
    smart_lists: () => fakeSmartLists(),
    // *Save as list* and the two writes that keep the rail tidy (#506).
    // **Fixture-only**: the rows live in a module array, not in a database,
    // and what they certify is the panel -- the rail drawing a saved row
    // beside the built-ins, the rename field, the two-press delete. The table,
    // the counts, the badge and the grammar's verdict are witnessed at the IPC
    // seam over a real PostgreSQL, in `crates/knobas-app/tests/it/search_ipc.rs`.
    create_smart_list: (args) => fakeCreateSmartList(args),
    rename_smart_list: (args) => fakeRenameSmartList(args),
    delete_smart_list: (args) => fakeDeleteSmartList(args),
  };
}

// Back to the seed after every test (#2004, `dev/fake-seed.ts`).
keepSeed(FIXTURE_SAVED);
