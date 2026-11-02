/**
 * `?fake-ipc`: the lists a reader saved (#506, #2423): `#l` and `#i` in the
 * box, `smart_lists` and the three writes.
 *
 * The built-in smart lists are gone (spec #2419), so this is saved lists
 * alone: what `#l` answers as kind `list`, and what Settings reads to know a
 * list exists.
 */
import type { SearchResponse } from "../../../ipc/search";
import type { Handlers } from "../fake-tauri";
import { keepSeed } from "../fake-seed";
import { SYNCED_AT } from "./corpus";

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
  { id: "sepa-tickets", label: "SEPA tickets", query: "#t sepa" },
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

/** `delete_smart_list`. */
function fakeDeleteSmartList(args: Record<string, unknown>) {
  const id = String(args["id"] ?? "");
  const at = FIXTURE_SAVED.findIndex((list) => list.id === id);
  if (at < 0) throw { code: "not_found", message: `no saved list ${id}`, source_id: null };
  FIXTURE_SAVED.splice(at, 1);
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
  };
}

keepSeed(FIXTURE_SAVED);
