/**
 * `?fake-ipc`: the lists a reader saved (#506, #2423): `#l` and `#i` in the
 * box, `smart_lists` and the three writes; and the favourites (#2428), which
 * name a saved list or an entity and are drawn at the top of the empty box;
 * and the keys (#2429), a saved list's chord and an action's chord per kind.
 *
 * The built-in smart lists are gone (spec #2419), so this is saved lists
 * alone: what `#l` answers as kind `list`, and what Settings reads to know a
 * list exists.
 */
import type { EntityRow } from "../../../ipc/entity";
import type { ActionKey, FavouriteTarget, Keys, SearchResponse } from "../../../ipc/search";
import type { Handlers } from "../fake-tauri";
import { keepSeed } from "../fake-seed";
import { CORPUS, SYNCED_AT, mirrorRow } from "./corpus";

/**
 * The lists a reader saved -- **fixture-only**, and mutable, because saving
 * one is the gesture the walk is here to make.
 *
 * Seeded with two runnable lists and one that needs attention. The last is
 * seeded rather than made because no command can make one --
 * `knobas_search::saved::create` runs the grammar first and refuses a query
 * it could not run -- and the backend's own IPC-seam test writes its row with
 * SQL for the same reason. `broken` is carried as **data** rather than
 * re-derived here: the verdict is `saved::plan`'s, witnessed over a real
 * database in `crates/knobas-app/tests/it/search_ipc.rs`.
 */
const FIXTURE_SAVED: { id: string; label: string; query: string; broken?: boolean }[] = [
  { id: "sepa-payouts", label: "SEPA payouts", query: "sepa payouts" },
  { id: "my-reviews", label: "My reviews", query: "#p @me" },
  { id: "old-gitea", label: "Old Gitea search", query: "/gitea payout", broken: true },
];

/** `saved::Refusal::description` for the seeded broken row. */
const BROKEN =
  "Needs attention: the saved query names a source as `/gitea`, which the launcher now reads as a word; today that is `source:gitea`. Delete it and save the search again.";

function fakeSavedLists() {
  return FIXTURE_SAVED.map((list) => ({
    id: list.id,
    label: list.label,
    // No mirror to count over here; the count is the engine's, witnessed
    // over a real database.
    count: 0,
    description: list.broken ? BROKEN : list.query,
    needs_attention: list.broken === true,
  }));
}

/**
 * `search` for `#l` and `#i` (#2423), alone or with words: the saved lists as
 * kind `list` (their text the path line, a broken one's reason its state),
 * and for `#i` the kind the launcher answers from the shell's inbox store.
 * `null` for anything else. A substring match over the name and the text,
 * fixture-only: the engine's FTS is witnessed in the Rust tests.
 */
export function fakeListSearch(raw: string): SearchResponse | null {
  const found = /^#([li]+)(?:\s+(.*))?$/i.exec(raw.trim());
  if (!found) return null;
  const letters = found[1]!.toLowerCase();
  const text = (found[2] ?? "").trim();
  const kinds = [...new Set([...letters].map((letter) => (letter === "l" ? "list" : "inbox")))];
  const words = text.toLowerCase().split(/\s+/).filter(Boolean);
  const hits = kinds.includes("list")
    ? FIXTURE_SAVED.filter((list) =>
        words.every((word) => `${list.label} ${list.query}`.toLowerCase().includes(word)),
      ).map((list, index) => ({
        entity_id: list.id,
        kind: "list",
        source_id: "list",
        title: list.label,
        updated_at: null,
        synced_at: SYNCED_AT,
        path: list.query,
        state: list.broken ? { text: BROKEN, tone: "warn" as const } : null,
        rank: 1 - index / 10,
        snippet: [],
        excerpts: [],
      }))
    : [];
  return {
    interpreted: {
      text,
      prefix: null,
      filters: { sources: [], kinds, updated_within_days: null, mine: false, authors: [] },
      unknown_tokens: [],
    },
    groups:
      hits.length === 0
        ? []
        : [{ kind: "list", label: "List", plural: "Lists", monogram: "LI", total: hits.length, hits }],
    total: hits.length,
    took_ms: 1,
    coverage: [],
  };
}

/**
 * `create_smart_list`: save the box's query -- **fixture-only**. The id is
 * slugged and de-duplicated the way `knobas_search::saved`'s `slug` and
 * `free_id` do it. It does not judge the query: every refusal the real
 * command makes past a blank one is §4's grammar's, and this fixture has none.
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
  const taken = new Set(FIXTURE_SAVED.map((list) => list.id));
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

/** `delete_smart_list`; its favourite goes with it, as `0069`'s cascade does. */
function fakeDeleteSmartList(args: Record<string, unknown>) {
  const id = String(args["id"] ?? "");
  const at = FIXTURE_SAVED.findIndex((list) => list.id === id);
  if (at < 0) throw { code: "not_found", message: `no saved list ${id}`, source_id: null };
  FIXTURE_SAVED.splice(at, 1);
  const favourite = FIXTURE_FAVOURITES.findIndex((target) => "list" in target && target.list === id);
  if (favourite >= 0) FIXTURE_FAVOURITES.splice(favourite, 1);
  // ...and its key, as `0070`'s cascade does.
  const key = FIXTURE_LIST_KEYS.findIndex((bound) => bound.list_id === id);
  if (key >= 0) FIXTURE_LIST_KEYS.splice(key, 1);
  return null;
}

/**
 * The favourites (#2428) -- **fixture-only**, mutable, in the order they
 * were favourited: one saved list and one page, so the empty box shows the
 * block under `?fake-ipc`. Order, the cascade and the unknown entity are
 * witnessed over a real database in `tests/it/favourites_ipc.rs`.
 */
const FIXTURE_FAVOURITES: FavouriteTarget[] = [{ list: "sepa-payouts" }, { entity: "confluence:sepa" }];

/** `favourites`: each target as the board's row, a list as its `#l` hit is drawn; what resolves nowhere is skipped. */
export function fakeFavourites(): EntityRow[] {
  return FIXTURE_FAVOURITES.flatMap((target): EntityRow[] => {
    if ("list" in target) {
      const list = FIXTURE_SAVED.find((saved) => saved.id === target.list);
      if (!list) return [];
      const state = list.broken ? { text: BROKEN, tone: "warn" as const } : null;
      return [{ entity_id: list.id, kind: "list", source_id: "list", title: list.label, updated_at: null, synced_at: SYNCED_AT, path: list.query, state }];
    }
    const entry = CORPUS.find((row) => row.entity_id === target.entity && row.deleted_at === null);
    return entry ? [mirrorRow(entry) as EntityRow] : [];
  });
}

const sameTarget = (a: FavouriteTarget, b: FavouriteTarget) =>
  ("list" in a && "list" in b && a.list === b.list) || ("entity" in a && "entity" in b && a.entity === b.entity);

/** `add_favourite`: last in the block, once; a list nobody saved is `not_found`. */
function fakeAddFavourite(args: Record<string, unknown>) {
  const target = args["target"] as FavouriteTarget;
  if ("list" in target && !FIXTURE_SAVED.some((list) => list.id === target.list)) {
    throw { code: "not_found", message: `no saved list ${target.list}`, source_id: null };
  }
  if (!FIXTURE_FAVOURITES.some((kept) => sameTarget(kept, target))) FIXTURE_FAVOURITES.push(target);
  return null;
}

/** `remove_favourite`: not a favourite is no error. */
function fakeRemoveFavourite(args: Record<string, unknown>) {
  const target = args["target"] as FavouriteTarget;
  const at = FIXTURE_FAVOURITES.findIndex((kept) => sameTarget(kept, target));
  if (at >= 0) FIXTURE_FAVOURITES.splice(at, 1);
  return null;
}

/**
 * The keys (#2429) -- **fixture-only**, mutable: *SEPA payouts* opens on ⌘1,
 * and *Approve* runs on a pull request on ⌘⇧P, so the rows draw their chords
 * under `?fake-ipc`. The tables' rules (one chord per list and per action on
 * a kind, the cascade, the chord's spelling) are witnessed over a real
 * database in `tests/it/keys_ipc.rs`.
 */
const FIXTURE_LIST_KEYS: { list_id: string; chord: string }[] = [{ list_id: "sepa-payouts", chord: "⌘1" }];
const FIXTURE_ACTION_KEYS: ActionKey[] = [{ kind: "pr", chord: "⌘⇧P", action_id: "approve" }];

/** `keys`: each list's chord with its name and text, the actions' by kind then chord. */
function fakeKeys(): Keys {
  return {
    lists: FIXTURE_LIST_KEYS.flatMap((key) => {
      const list = FIXTURE_SAVED.find((saved) => saved.id === key.list_id);
      return list ? [{ ...key, label: list.label, text: list.query }] : [];
    }),
    actions: [...FIXTURE_ACTION_KEYS].sort((a, b) => a.kind.localeCompare(b.kind) || a.chord.localeCompare(b.chord)),
  };
}

/** `bind_list_key`: the chord moves to this list; `null` takes it away; a list nobody saved is `not_found`. */
function fakeBindListKey(args: Record<string, unknown>) {
  const listId = String(args["listId"] ?? "");
  const chord = (args["chord"] as string | null) ?? null;
  if (!FIXTURE_SAVED.some((list) => list.id === listId)) {
    throw { code: "not_found", message: `no saved list ${listId}`, source_id: null };
  }
  const kept = FIXTURE_LIST_KEYS.filter((key) => key.list_id !== listId && key.chord !== chord);
  FIXTURE_LIST_KEYS.splice(0, FIXTURE_LIST_KEYS.length, ...kept, ...(chord ? [{ list_id: listId, chord }] : []));
  return null;
}

/** `bind_action_key`: on the kind, the chord moves to this action and the action holds one chord. */
function fakeBindActionKey(args: Record<string, unknown>) {
  const kind = String(args["kind"] ?? "");
  const actionId = String(args["actionId"] ?? "");
  const chord = (args["chord"] as string | null) ?? null;
  const kept = FIXTURE_ACTION_KEYS.filter((key) => key.kind !== kind || (key.action_id !== actionId && key.chord !== chord));
  FIXTURE_ACTION_KEYS.splice(0, FIXTURE_ACTION_KEYS.length, ...kept, ...(chord ? [{ kind, chord, action_id: actionId }] : []));
  return null;
}

/** This feature's commands. */
export function handlers(): Handlers {
  return {
    smart_lists: () => fakeSavedLists(),
    // *Save as list* and the two writes. **Fixture-only**: the rows live in a
    // module array, not in a database.
    create_smart_list: (args) => fakeCreateSmartList(args),
    rename_smart_list: (args) => fakeRenameSmartList(args),
    delete_smart_list: (args) => fakeDeleteSmartList(args),
    favourites: () => fakeFavourites(),
    add_favourite: (args) => fakeAddFavourite(args),
    remove_favourite: (args) => fakeRemoveFavourite(args),
    keys: () => fakeKeys(),
    bind_list_key: (args) => fakeBindListKey(args),
    bind_action_key: (args) => fakeBindActionKey(args),
  };
}

keepSeed(FIXTURE_SAVED, FIXTURE_FAVOURITES, FIXTURE_LIST_KEYS, FIXTURE_ACTION_KEYS);
