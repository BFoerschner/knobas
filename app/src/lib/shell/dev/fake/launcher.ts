/**
 * `?fake-ipc`: The launcher (#504): its home board, the Tree's search box (#430), the
 * `>` palette (#1417) and the query box's suggestions (#601).
 */
import type { SearchResponse } from "../../../ipc/search";
import { SYNTAX } from "../../../launcher/syntax";
import type { Handlers } from "../fake-tauri";
import { fakePipelines, fixtureRun } from "./builds";
import { fakeNotesAreShown } from "./capture";
import { CORPUS, SYNCED_AT, mirrorRow } from "./corpus";
import { FIXTURE_ESTATE, assetPathText } from "./estate";
import { fakeLinkSetLines } from "./link-sets";
import { fakeLog } from "./run-reads";
import { fakeFavourites, fakeListSearch } from "./smart-lists";
import { FIXTURE_SOURCES } from "./sources";
import { fakeTicketViewLines } from "./ticket-views";
import { FIXTURE_QUEUE } from "./write-queue";
import { keyOf } from "../../../time/format";
import { kindLabel, kindMonogram, kindSingular } from "../../kinds";

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
  ).map((entry, index) => ({ ...mirrorRow(entry), rank: 1 - index / 10, snippet: [], excerpts: [] }));
  if (hits.length === 0) return null;
  return {
    interpreted: {
      text: raw,
      prefix: null,
      filters: { sources: [], kinds: [], updated_within_days: null, mine: false, authors: [] },
      unknown_tokens: [],
      left_out_tokens: [],
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
      left_out_tokens: [],
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
    titles?: boolean;
    filters?: { kinds?: string[] };
  };
  // `#l` and `#i` (#2423): the saved lists, and the kind the launcher
  // answers from the shell's inbox store.
  const listed = fakeListSearch(query.raw ?? "");
  if (listed !== null) return listed;
  const palette = actionPalette((query.raw ?? "").trim());
  if (palette !== null) return palette;
  const help = helpCard((query.raw ?? "").trim());
  if (help !== null) return help;
  const leftOut = leftOutAnswer((query.raw ?? "").trim());
  if (leftOut !== null) return leftOut;
  const byKey = ticketsByKey((query.raw ?? "").trim());
  if (byKey !== null) return byKey;
  const words = wordSearch(query.raw ?? "", query.titles !== false);
  if (words !== null) return words;
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
    excerpts: [],
  }));
  return {
    interpreted: {
      text: query.raw ?? "",
      prefix: null,
      filters: { sources: [], kinds: ["asset"], updated_within_days: null, mine: false, authors: [] },
      unknown_tokens: [],
      left_out_tokens: [],
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

/**
 * `search` for `?` (#2421): the help prefix and no groups, as the backend
 * answers it, so the `?` card and its colour legend draw under `?fake-ipc`.
 */
function helpCard(raw: string): SearchResponse | null {
  if (raw !== "?") return null;
  return {
    interpreted: {
      text: "",
      prefix: "help",
      filters: { sources: [], kinds: [], updated_within_days: null, mine: false, authors: [] },
      unknown_tokens: [],
      left_out_tokens: [],
    },
    groups: [],
    total: 0,
    took_ms: 0,
    coverage: [],
  };
}

/** `knobas_search::group`'s order, unknown kinds last. */
const GROUP_ORDER = ["view", "set", "ticket", "asset", "repo", "pr", "pipeline", "build_config", "build", "page", "note", "commit", "branch", "route", "monitor", "list"];

/** What the engine leaves out of the answer (#2422, `knobas_search::frames`): runs, commits, a Jira view's mirror row. */
const LEFT_OUT = new Set(["build", "commit", "jira_view"]);

type Hit = SearchResponse["groups"][number]["hits"][number];

/**
 * Everything's excerpts (#2431) as the fixture makes them: each line (a run
 * log's, numbered from `firstLine`) or sentence of `text` that holds a word,
 * the words marked, a page's `\u001e` section, `\u001f` comment and
 * `\u001d` link marks as
 * places, and quoted themselves when they hold a word. **A substring match,
 * fixture-only**: the engine's are `knobas_search::excerpt`'s, witnessed over
 * PostgreSQL.
 */
function fixtureExcerpts(title: string, text: string, words: string[], firstLine: number | null = null): Hit["excerpts"] {
  const out: Hit["excerpts"] = [];
  const marks = (piece: string) => {
    const lower = piece.toLowerCase();
    const segments: Hit["snippet"] = [];
    let at = 0;
    while (at < piece.length) {
      const next = words
        .map((word) => [lower.indexOf(word, at), word.length] as const)
        .filter(([found]) => found >= 0)
        .sort((a, b) => a[0] - b[0])[0];
      if (!next) break;
      if (next[0] > at) segments.push({ text: piece.slice(at, next[0]), hit: false });
      segments.push({ text: piece.slice(next[0], next[0] + next[1]), hit: true });
      at = next[0] + next[1];
    }
    if (at < piece.length) segments.push({ text: piece.slice(at), hit: false });
    return segments;
  };
  const quoteMark = (at: string | null, lead: string, name: string) => {
    const lower = name.toLowerCase();
    const word = words.find((candidate) => lower.includes(candidate));
    if (word === undefined || out.length >= 8) return;
    const from = lower.indexOf(word);
    const segments: Hit["snippet"] = [];
    if (lead !== "" || from > 0) segments.push({ text: lead + name.slice(0, from), hit: false });
    segments.push({ text: name.slice(from, from + word.length), hit: true });
    if (from + word.length < name.length) segments.push({ text: name.slice(from + word.length), hit: false });
    if (!out.some((excerpt) => excerpt.place === at && JSON.stringify(excerpt.segments) === JSON.stringify(segments))) {
      out.push({ place: at, segments });
    }
  };
  const links: [string, string][] = [];
  let place: string | null = null;
  const lines = text.startsWith(title) ? text.slice(title.length).split("\n") : text.split("\n");
  lines.forEach((line, index) => {
    if (line.startsWith("\u001e")) {
      quoteMark("Heading", "", line.slice(1).trim());
      return void (place = line.slice(1).trim());
    }
    if (line.startsWith("\u001f")) {
      quoteMark(null, "Comment by ", line.slice(1).trim());
      return void (place = `Comment by ${line.slice(1).trim()}`);
    }
    if (line.startsWith("\u001d")) {
      const [space = "", title = ""] = line.replaceAll("\u001d", "\n").trim().split("\n");
      return void links.push([space, title]);
    }
    const pieces = firstLine === null ? line.split(/(?<=[.!?])\s+/) : [line];
    for (const piece of pieces) {
      const lower = piece.toLowerCase();
      if (out.length >= 8 || !words.some((word) => lower.includes(word))) continue;
      out.push({ place: firstLine === null ? place : `line ${firstLine + index}`, segments: marks(piece) });
    }
  });
  // A link's line, once no other place shows the word (#2491).
  const quoted = out.flatMap((excerpt) => excerpt.segments.filter((one) => one.hit).map((one) => one.text.toLowerCase()));
  const unquoted = words.filter((word) => !quoted.includes(word));
  for (const [space, title] of links) {
    const lower = `${title} ${space}`.toLowerCase();
    if (out.length >= 8 || !unquoted.some((word) => lower.includes(word))) continue;
    out.push({
      place: null,
      segments: [{ text: "Links to ", hit: false }, ...marks(title), { text: " in ", hit: false }, ...marks(space)],
    });
  }
  return out;
}

/** A pipeline's newest run's log under the fixture, as `run_log` answers it. */
function fixtureLogText(pipeline: ReturnType<typeof fakePipelines>[number]): string {
  const newest = pipeline.recent[0];
  const run = newest ? fixtureRun(newest.entity_id) : undefined;
  return run ? fakeLog(run.config, run.no, run.st).lines.map((line) => line.text).join("\n") : "";
}

/**
 * The launcher's own kinds (#2422) as the fixture holds them: the ticket
 * views, the link sets and the pipelines, each with its path line, and a
 * pipeline in its newest run's state. Fixture-only, like the word search.
 */
function launcherKindHits(): { haystack: string; hit: Hit; log?: string }[] {
  const hit = (kind: string, entity_id: string, title: string, path: string, state: Hit["state"] = null): Hit => ({
    entity_id,
    kind,
    source_id: kind,
    title,
    updated_at: SYNCED_AT,
    synced_at: SYNCED_AT,
    path,
    state,
    rank: 1,
    snippet: [],
    excerpts: [],
  });
  return [
    ...fakeTicketViewLines().map((view) => ({ haystack: `${view.name} ${view.path}`, hit: hit("view", view.id, view.name, view.path) })),
    ...fakeLinkSetLines().map((set) => ({ haystack: `${set.name} ${set.description}`, hit: hit("set", set.id, set.name, set.path) })),
    ...fakePipelines().map((pipeline) => ({
      haystack: `${pipeline.name} ${pipeline.path.join(" ")}`,
      log: fixtureLogText(pipeline),
      hit: { ...hit("pipeline", pipeline.entity_id, pipeline.name, pipeline.path.join(" › "), pipeline.recent[0]?.state ?? null), source_id: pipeline.source_id },
    })),
  ];
}

/**
 * `search` for plain words (#2421): the corpus rows whose key, title, path or
 * key cell (#2452) holds every word, grouped in the engine's order, each with its state. So a
 * free search can be looked at under `?fake-ipc`, the launcher's rows and
 * their colours with it.
 *
 * **A substring match, fixture-only**, like the estate search below: it
 * answers the shape and not the engine's question. The ranking, the excerpt
 * and the grammar are the engine's, witnessed over a real PostgreSQL in
 * `knobas-search`'s integration tests. A leading `#` filter narrows to its
 * kinds (`SYNTAX`'s letters), so a section's search can be looked at too. Any
 * other prefix or a `:` token is not answered here.
 */
function wordSearch(raw: string, titles: boolean): SearchResponse | null {
  const asked = raw.trim();
  const filter = /^#([a-z]+)(?:\s+(.*))?$/i.exec(asked);
  const letters = [...(filter?.[1]?.toLowerCase() ?? "")];
  const kinds = letters.flatMap((letter) => SYNTAX.find((row) => row.token === `#${letter}`)?.kind ?? []);
  if (kinds.length !== letters.length) return null;
  const text = filter ? (filter[2] ?? "").trim() : asked;
  if ((text === "" && filter === null) || /^[>?#@]/.test(text) || /\S+:/.test(text)) return null;
  const words = text.toLowerCase().split(/\s+/).filter(Boolean);
  const byKind = new Map<string, SearchResponse["groups"][number]["hits"]>();
  const wanted = (kind: string) => filter === null || kinds.includes(kind);
  for (const { haystack, hit, log } of launcherKindHits()) {
    if (!wanted(hit.kind)) continue;
    // Everything (#2431): a pipeline is found by its newest run's log too.
    const body = titles ? "" : (log ?? "");
    if (!words.every((word) => `${haystack} ${body}`.toLowerCase().includes(word))) continue;
    const hits = byKind.get(hit.kind) ?? [];
    hits.push({ ...hit, rank: 1 - hits.length / 100, excerpts: titles ? [] : fixtureExcerpts("", body, words, 1) });
    byKind.set(hit.kind, hits);
  }
  for (const entry of CORPUS) {
    if (entry.deleted_at !== null || LEFT_OUT.has(entry.kind) || !wanted(entry.kind)) continue;
    const row = mirrorRow(entry);
    const body = titles ? "" : entry.body_text;
    const haystack = `${keyOf(entry.entity_id)} ${entry.title} ${row.path ?? ""} ${row.cell ?? ""} ${body}`.toLowerCase();
    if (!words.every((word) => haystack.includes(word))) continue;
    const hits = byKind.get(entry.kind) ?? [];
    const excerpts = titles ? [] : fixtureExcerpts(entry.title, body, words);
    hits.push({ ...row, rank: 1 - hits.length / 100, snippet: [], excerpts });
    byKind.set(entry.kind, hits);
  }
  const at = (kind: string) => (GROUP_ORDER.indexOf(kind) < 0 ? GROUP_ORDER.length : GROUP_ORDER.indexOf(kind));
  const groups = [...byKind]
    .sort(([left], [right]) => at(left) - at(right) || left.localeCompare(right))
    .map(([kind, hits]) => ({
      kind,
      label: kindSingular(kind),
      plural: kindLabel(kind),
      monogram: kindMonogram(kind),
      total: hits.length,
      hits: hits.slice(0, 8),
    }));
  return {
    interpreted: {
      text,
      prefix: null,
      filters: { sources: [], kinds, updated_within_days: null, mine: false, authors: [] },
      unknown_tokens: [],
      left_out_tokens: [],
    },
    groups,
    total: groups.reduce((sum, group) => sum + group.total, 0),
    took_ms: 3,
    coverage: [],
  };
}

/**
 * What the engine answers for a query the launcher leaves empty (#2488), so
 * the token saying why can be looked at under `?fake-ipc`. `kind:<a left-out
 * kind>` and one made-up name only commits sit beneath; the rule itself is
 * witnessed over a real PostgreSQL in `knobas-search`'s `launcher_kinds`.
 */
function leftOutAnswer(raw: string): SearchResponse | null {
  const named = /^kind:(build|commit|jira_view)$/.test(raw) || raw === "under:retired-service";
  if (!named) return null;
  return {
    interpreted: {
      text: "",
      prefix: null,
      filters: { sources: [], kinds: raw.startsWith("kind:") ? [raw.slice(5)] : [], updated_within_days: null, mine: false, authors: [] },
      unknown_tokens: [],
      left_out_tokens: [raw],
    },
    groups: [],
    total: 0,
    took_ms: 2,
    coverage: [],
  };
}

/** This feature's commands. */
export function handlers(): Handlers {
  return {
    // The launcher board (#504): the recent items alone since #2421 and
    // #2423. **Fixture-only**; the rules are witnessed at the IPC seam over a
    // real PostgreSQL, in `crates/knobas-app/tests/it/search_ipc.rs`.
    launcher_home: () => ({
      // The newest of the fixture's corpus, in the shape `list_entities`
      // answers in -- the board draws recent rows with the same component.
      recent: CORPUS.filter((entry) => entry.deleted_at === null && !LEFT_OUT.has(entry.kind))
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
      // What the reader favourited (#2428), above the recent rows.
      favourites: fakeFavourites(),
      sources: FIXTURE_SOURCES.map((source) => source.health),
      pending_writes: FIXTURE_QUEUE.filter((row) => row.state === "pending").length,
      notes_shown: fakeNotesAreShown(),
    }),
    // The Tree's search box (#430), a ticket key (#1476), `>` (#1417) and
    // plain words over the corpus by substring (#2421). Anything else is
    // refused rather than answered from the estate: a handler that answered
    // everything with assets would make a QA pass over the launcher look like
    // it worked.
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
