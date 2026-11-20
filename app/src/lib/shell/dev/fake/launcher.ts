/**
 * `?fake-ipc`: The launcher (#504): its home board, the Tree's search box (#430), the
 * `>` palette (#1417) and the query box's suggestions (#601).
 */
import type { EverythingMatches, SearchResponse, SearchSettings } from "../../../ipc/search";
import { SYNTAX } from "../../../launcher/syntax";
import type { Handlers } from "../fake-tauri";
import { onFakeReset } from "../fake-seed";
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
import { fuzzyRank } from "../../../tf/filter/fuzzy";
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
  // The Tree's own search names its kind in the filters, not in the words: answered from the estate before the launcher's
  // word search, which knows no asset and answered every word with nothing (the asset-tree shots, 2026-11-19).
  const kinds = query.filters?.kinds ?? [];
  const treeOnly = kinds.length > 0 && kinds.every((kind) => kind === "asset");
  // `#l` and `#i` (#2423): the saved lists, and the kind the launcher
  // answers from the shell's inbox store.
  const listed = treeOnly ? null : fakeListSearch(query.raw ?? "");
  if (listed !== null) return listed;
  if (!treeOnly) {
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
  }
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
function fixtureExcerpts(title: string, text: string, words: string[], firstLine: number | null = null, lead: string | null = null): Hit["excerpts"] {
  const out: Hit["excerpts"] = [];
  // The engine's order (D10): the excerpt marking the most distinct words first, the rest as they stand.
  const marked = (excerpt: Hit["excerpts"][number]) => new Set(excerpt.segments.filter((one) => one.hit).map((one) => one.text.toLowerCase())).size;
  const ordered = () => out.map((excerpt, index) => [excerpt, index] as const).sort((a, b) => marked(b[0]) - marked(a[0]) || a[1] - b[1]).map(([excerpt]) => excerpt);
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
  let place: string | null = lead;
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
      out.push({ place: firstLine === null ? place : `Log line ${firstLine + index}`, segments: marks(piece) });
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
  return ordered();
}

/** The title's letters the words match, as the picker's marks are (UTF-16 pairs, ascending, never overlapping): a row its name explains draws no evidence line. */
function fixtureMarks(title: string, words: string[]): number[] {
  const lower = title.toLowerCase();
  const spans = words
    .map((word) => [lower.indexOf(word), word.length] as const)
    .filter(([at]) => at >= 0)
    .map(([at, length]) => [at, at + length] as const)
    .sort((a, b) => a[0] - b[0]);
  const out: number[] = [];
  for (const [start, end] of spans) {
    const last = out.length - 1;
    if (last > 0 && start <= out[last]!) out[last] = Math.max(out[last]!, end);
    else out.push(start, end);
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
 * kinds (`SYNTAX`'s letters), so a section's search can be looked at too, and a
 * `#` word that is no filter is the word, hash dropped (#2420). Any other
 * prefix or a `:` token is not answered here.
 */
/** Settings › Search's boxes (the everything-search decisions, D13), as the fixture keeps them: every one on until set. */
const EVERY_BOX: EverythingMatches = { ticket_text: true, pr_text: true, page_text: true, build_logs: true, file_names: true, file_text: true };
let everything: EverythingMatches = EVERY_BOX;
/** *Keep at most* (D13), in MB. */
let kept = 2048;
/** `?fake-long-paths`: the repositories' files under a deep folder with a long name (`long-paths.mjs`). */
let longFiles = false;
onFakeReset(() => {
  everything = EVERY_BOX;
  kept = 2048;
  longFiles = false;
});

/**
 * The fixture's repositories found by their files (the everything-search decisions, D16): the renamed repository, whose
 * classes and image still say its old name `SettleBridge` while its name, README and pom do not, and a client of it that
 * only imports it. **Fixture-only**, beside the corpus rather than in it, so no pinned count of the corpus moves; the
 * engine's read is `knobas-files`', witnessed in `knobas-search`'s ranking table.
 */
export const FAKE_FILE_REPOS: readonly { entity_id: string; title: string; files: readonly (readonly [string, string])[] }[] = [
  {
    entity_id: "mock:tidewater/ledger-gateway",
    title: "tidewater/ledger-gateway",
    files: [
      [
        "src/main/java/com/tidewater/settlebridge/SettleBridgeServer.java",
        "package com.tidewater.settlebridge;\n\nimport com.tidewater.settlebridge.config.SettleBridgeConfig;\n\n/** Terminates TLS and forwards settlement batches to the ledger. */\npublic final class SettleBridgeServer {\n    private final SettleBridgeConfig config;\n}",
      ],
      ["src/main/java/com/tidewater/settlebridge/config/SettleBridgeConfig.java", "public record SettleBridgeConfig(String host, int port) {\n    public static SettleBridgeConfig fromEnv() {"],
      ["src/test/java/com/tidewater/settlebridge/SettleBridgeServerTest.java", "class SettleBridgeServerTest {\n    void startsOnTheConfiguredPort() {}"],
      ["deploy/k8s/deployment.yaml", "spec:\n  containers:\n    - name: gateway\n      image: registry.tidewater.example/settle-bridge:4.2.0"],
      ["README.md", "# Ledger gateway\nForwards settlement batches to the ledger."],
    ],
  },
  {
    entity_id: "mock:tidewater/ledger-api",
    title: "tidewater/ledger-api",
    files: [
      ["src/main/java/com/tidewater/ledger/SettleClient.java", "import com.tidewater.settlebridge.client.Client;\npublic class SettleClient {}"],
      ["src/main/java/com/tidewater/ledger/LedgerClient.java", "public class LedgerClient {}"],
    ],
  },
];

/**
 * `?fake-long-paths` (the everything-search build, 2026-11-20): each file under a deep folder with a long name, so the
 * evidence line and the pane hold a long path to its box. The folder holds none of the words, so the answers are the same.
 */
const LONG_FILE_FOLDER = "services/the-repository-name-that-is-long/modules/with/folders/nested/very/deep/inside/it/";

/** The fixture's file repositories as the switches draw them. */
function fileRepos(): typeof FAKE_FILE_REPOS {
  if (!longFiles) return FAKE_FILE_REPOS;
  return FAKE_FILE_REPOS.map((repo) => ({ ...repo, files: repo.files.map(([path, text]) => [LONG_FILE_FOLDER + path, text] as const) }));
}

/** An identifier's words as the engine reads them, cut down: lower-cased runs and their capital parts, each to the run's end. */
function fileWords(text: string): string[] {
  const out: string[] = [];
  for (const run of text.match(/[\p{L}\p{N}_-]+/gu) ?? []) {
    const bare = run.replace(/[_-]/g, "");
    out.push(bare.toLowerCase());
    for (const at of [...bare.matchAll(/(?<=[a-z0-9])[A-Z]|(?<=[A-Z])[A-Z](?=[a-z])/g)].map((m) => m.index)) out.push(bare.slice(at).toLowerCase());
  }
  return out;
}

/** The fixture's files answer to `words` (D8 in miniature): each repository whose files hold every word of four letters and more. */
export function fakeFilesFound(words: readonly string[]) {
  const asked = words.filter((word) => word.length >= 3).map((word) => word.replace(/[_-]/g, ""));
  if (asked.length === 0) return [];
  const holds = (text: string) => {
    const said = fileWords(text);
    return asked.every((term) => said.some((word) => (term.length <= 3 ? word === term : word.startsWith(term))));
  };
  return fileRepos().flatMap((repo) => {
    const byName = repo.files.filter(([path]) => holds(path));
    const byText = repo.files.filter(([, text]) => holds(text));
    const shown = [...byName, ...byText].sort((a, b) => Number(!byName.includes(a)) - Number(!byName.includes(b)) || a[0].length - b[0].length)[0];
    if (!shown) return [];
    const lines = shown[1].split("\n");
    const at = lines.findIndex((line) => holds(line) && /class |record |interface /.test(line));
    const line = at >= 0 ? at : lines.findIndex((line) => holds(line));
    return [{ repo, byName: byName.length, byText: byText.length, path: shown[0], lines, line: line >= 0 && byText.includes(shown) ? line : null }];
  });
}

/** The letters of `text` the words cover, as segments: a run whose word begins with one, marked over the word's length. */
function fileMarks(text: string, words: readonly string[]): Hit["snippet"] {
  const asked = words.filter((word) => word.length >= 3);
  const out: Hit["snippet"] = [];
  let at = 0;
  const lower = text.toLowerCase();
  while (at < text.length) {
    let found: [number, number] | null = null;
    for (const word of asked) {
      for (let i = at; i < text.length; i++) {
        let j = i;
        let k = 0;
        while (j < text.length && k < word.length) {
          if (lower[j] === "_" || lower[j] === "-") j++;
          else if (lower[j] === word[k]) (j++, k++);
          else break;
        }
        if (k === word.length && (i === 0 || !/[a-z0-9]/.test(lower[i - 1]!) || /[A-Z]/.test(text[i]!))) {
          if (found === null || i < found[0]) found = [i, j];
          break;
        }
      }
    }
    if (found === null) break;
    if (found[0] > at) out.push({ text: text.slice(at, found[0]), hit: false });
    out.push({ text: text.slice(found[0], found[1]), hit: true });
    at = found[1];
  }
  if (at < text.length) out.push({ text: text.slice(at), hit: false });
  return out;
}

/** A found repository's first excerpt, as `knobas_search::files::excerpt` makes it. */
export function fakeFileExcerpt(found: ReturnType<typeof fakeFilesFound>[number], words: readonly string[]): Hit["excerpts"][number] {
  if (found.line === null) return { place: null, segments: fileMarks(found.path, words), file: { path: found.path, line: null, before: "", after: "" } };
  const text = found.lines[found.line] ?? "";
  return {
    place: `${found.path}:${found.line + 1}`,
    segments: fileMarks(text.trim(), words),
    file: { path: found.path, line: found.line + 1, before: found.lines[found.line - 1] ?? "", after: found.lines[found.line + 1] ?? "" },
  };
}

/** What a box covers, read off the fixture as the backend counts the mirror. */
function searchSettingsAnswer(): SearchSettings {
  const live = CORPUS.filter((entry) => entry.deleted_at === null);
  const count = (kind: string) => live.filter((entry) => entry.kind === kind).length;
  const files = fileRepos().flatMap((repo) => repo.files);
  const textBytes = files.reduce((all, [, text]) => all + text.length, 0);
  return {
    everything,
    tickets: count("ticket"),
    pull_requests: count("pr"),
    pages: count("page"),
    pipelines: fakePipelines().length,
    files: files.length,
    text_files: kept === 0 ? 0 : files.length,
    kept_mb: kept,
    text_bytes: kept === 0 ? 0 : textBytes,
    repositories_read: FAKE_FILE_REPOS.length,
    repositories: live.filter((entry) => entry.kind === "repo").length + FAKE_FILE_REPOS.length,
    left_out: { lock_file: 1, binary: 2 },
  };
}

/** Whether a cleared box leaves `kind` to its names (D13). */
const namesOnly = (kind: string) => (kind === "ticket" && !everything.ticket_text) || (kind === "pr" && !everything.pr_text) || (kind === "page" && !everything.page_text);

function wordSearch(raw: string, titles: boolean): SearchResponse | null {
  const typed = raw.trim();
  const named = /^#([a-z]+)(?:\s+(.*))?$/i.exec(typed);
  const lettersOf = [...(named?.[1]?.toLowerCase() ?? "")];
  const kindsOf = lettersOf.flatMap((letter) => SYNTAX.find((row) => row.token === `#${letter}`)?.kind ?? []);
  const filter = kindsOf.length === lettersOf.length ? named : null;
  const kinds = filter ? kindsOf : [];
  const asked = filter ? typed : typed.replace(/^#(?=[^\s])/, "");
  const text = filter ? (filter[2] ?? "").trim() : asked;
  if ((text === "" && filter === null) || /^[>?#@]/.test(text) || /\S+:/.test(text)) return null;
  const words = text.toLowerCase().split(/\s+/).filter(Boolean);
  const byKind = new Map<string, SearchResponse["groups"][number]["hits"]>();
  const wanted = (kind: string) => filter === null || kinds.includes(kind);
  for (const { haystack, hit, log } of launcherKindHits()) {
    if (!wanted(hit.kind)) continue;
    // Everything (#2431): a pipeline is found by its newest run's log too, while *Build logs* is set (D13).
    const body = titles || !everything.build_logs ? "" : (log ?? "");
    if (!words.every((word) => `${haystack} ${body}`.toLowerCase().includes(word))) continue;
    const hits = byKind.get(hit.kind) ?? [];
    hits.push({ ...hit, rank: 1 - hits.length / 100, excerpts: titles ? [] : fixtureExcerpts("", body, words, 1) });
    byKind.set(hit.kind, hits);
  }
  for (const entry of CORPUS) {
    if (entry.deleted_at !== null || LEFT_OUT.has(entry.kind) || !wanted(entry.kind)) continue;
    const row = mirrorRow(entry);
    const body = titles || namesOnly(entry.kind) ? "" : entry.body_text;
    const haystack = `${keyOf(entry.entity_id)} ${entry.title} ${row.path ?? ""} ${row.cell ?? ""} ${body}`.toLowerCase();
    if (!words.every((word) => haystack.includes(word))) continue;
    const hits = byKind.get(entry.kind) ?? [];
    const lead = entry.kind === "ticket" || entry.kind === "pr" ? "Description" : null;
    const excerpts = titles ? [] : fixtureExcerpts(entry.title, body, words, null, lead);
    const marks = fixtureMarks(entry.title, words);
    hits.push({ ...row, rank: 1 - hits.length / 100, snippet: [], excerpts, ...(marks.length > 0 ? { marks } : {}) });
    byKind.set(entry.kind, hits);
  }
  // Everything finds a repository by its files too (D8): named by them before only said, after the corpus's own rows.
  if (!titles && wanted("repo") && (everything.file_names || everything.file_text)) {
    const found = fakeFilesFound(words).filter((one) => (everything.file_names && one.byName > 0) || (everything.file_text && one.byText > 0));
    found.sort((a, b) => b.byName - a.byName || b.byText - a.byText);
    for (const one of found) {
      const hits = byKind.get("repo") ?? [];
      hits.push({
        entity_id: one.repo.entity_id,
        kind: "repo",
        source_id: "mock",
        title: one.repo.title,
        updated_at: SYNCED_AT,
        synced_at: SYNCED_AT,
        path: null,
        state: null,
        rank: one.byName > 0 ? 0.5 : -2,
        snippet: [],
        excerpts: [fakeFileExcerpt(one, words)],
        files: { by_text: one.byText, by_name: one.byName, branch: "main" },
      });
      byKind.set("repo", hits);
    }
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
/**
 * `?fake-projects=<n>`: `n` more projects among the ticket keys' values, to
 * look at the view editor's Project condition at a real Jira's size
 * (`&fake-projects=40`), where its values are a list to search.
 */
function manyProjects(): { key: string; value: string; detail: string }[] {
  const count = Number(new URLSearchParams(globalThis.location?.search).get("fake-projects") ?? 0) || 0;
  const areas = ["Billing", "Customs", "Tracking", "Warehouse", "Fleet", "Partner", "Returns", "Pricing", "Identity", "Analytics"];
  const kinds = ["Platform", "Mobile", "Integrations", "Support", "Data", "Web"];
  return Array.from({ length: count }, (_, at) => {
    const area = areas[at % areas.length]!;
    const kind = kinds[Math.floor(at / areas.length) % kinds.length]!;
    return { key: "project", value: `${area.slice(0, 2)}${kind.slice(0, 1)}${at >= 60 ? at : ""}`.toUpperCase(), detail: `${area} ${kind}` };
  });
}

/**
 * `fuzzy_rank` (the fuzzy-search pass, 2026-11-19): the view editor's letters-in-order ranking
 * (`tf/filter/fuzzy.ts`) over the texts sent, its marks as UTF-16 pairs. **Fixture-only**: the
 * backend's rules are `knobas_search::picker::rank`'s, witnessed in its tests.
 */
export function fakeFuzzyRank(text: string, haystacks: readonly string[]): { index: number; loose: boolean; marks: number[] }[] {
  const indexed = haystacks.map((haystack, index) => ({ haystack, index }));
  return fuzzyRank(indexed, text, (entry) => entry.haystack).map(({ option, runs }) => {
    const marks: number[] = [];
    let at = 0;
    for (const run of runs) {
      if (run.hit) marks.push(at, at + run.text.length);
      at += run.text.length;
    }
    return { index: option.index, loose: false, marks };
  });
}

export function handlers(params: URLSearchParams = new URLSearchParams()): Handlers {
  longFiles = params.has("fake-long-paths");
  return {
    fuzzy_rank: (args) => fakeFuzzyRank(String(args["text"] ?? ""), (args["haystacks"] as string[] | undefined) ?? []),
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
    // The pane's files of a repository the fixture's files found (D11).
    search_files: (args) => {
      const words = String(args["raw"] ?? "").toLowerCase().split(/\s+/).filter(Boolean);
      const found = fakeFilesFound(words).find((one) => one.repo.entity_id === args["repository"]);
      if (!found) return { files: [], by_text: 0, by_name: 0, more: false, branch: "main" };
      const files = found.repo.files.flatMap(([path]) => {
        const one = fakeFilesFound(words).find((each) => each.repo === found.repo);
        return one ? [{ ...one, path, lines: (found.repo.files.find(([p]) => p === path)?.[1] ?? "").split("\n") }] : [];
      });
      const shown = files
        .map((one) => {
          const at = one.lines.findIndex((line) => words.some((word) => line.toLowerCase().replace(/[_-]/g, "").includes(word)));
          const named = words.some((word) => one.path.toLowerCase().replace(/[_-]/g, "").includes(word));
          return at < 0 && !named ? null : fakeFileExcerpt({ ...one, line: at < 0 ? null : at }, words);
        })
        .filter((one) => one !== null);
      return { files: shown.slice(0, 8), by_text: found.byText, by_name: found.byName, more: shown.length > 8, branch: "main" };
    },
    // Settings › Search (D13): the boxes, kept for the session, which the word search above honours.
    search_settings: () => searchSettingsAnswer(),
    set_search_files_kept: (args) => {
      kept = Number(args["mb"]);
      return searchSettingsAnswer();
    },
    reread_files: () => searchSettingsAnswer(),
    set_search_everything: (args) => {
      everything = { ...everything, ...(args["matches"] as Partial<EverythingMatches>) };
      return searchSettingsAnswer();
    },
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
        ...manyProjects(),
        ...["In Progress", "To Do", "In Review", "Done"].map((value) => ({
          key: "status",
          value,
          detail: "",
        })),
        ...["Story", "Bug", "Task"].map((value) => ({ key: "issuetype", value, detail: "" })),
        ...["High", "Medium", "Low"].map((value) => ({ key: "priority", value, detail: "" })),
        { key: "assignee", value: "@me", detail: "you" },
        { key: "assignee", value: "jonas", detail: "" },
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
