<!--
  PROTOTYPE: the ⌘K launcher rethought (clickdummy, 2026-11-02). Not production.

  One question: does "Tab on a row opens its actions in the same box" feel
  right, and do findable views, sets and lists, the `#x` filters and an inline
  link close the gaps Björn named? Mounted by `App.svelte` instead of
  `Launcher.svelte` under `?launcher-next` (dev only); everything else in the
  shell is the real thing.

  Round 4 (Björn, 2026-11-02): an empty box shows the recent items and nothing
  else; the box says only "? for help" and `?` draws this grammar's card; the
  quick filters are `#t #p #r #b #m #d #l #i`, combinable (`#t #l sepa`); Tab
  alone opens the actions (⌘K while open only gives the box its focus back);
  *Start timer* is a ticket's row only, and starting it makes the ticket the
  job (the shell already does that); a pull request can be merged, with the
  style picked; a repository's *Create pull request* picks the head branch,
  then the base, then opens the compare page; *Open in ⟨app⟩* is one row per
  application configured under Settings › Checkouts. A pick is a **flow**: a
  step's choices fill the sheet, the box narrows them, Enter takes one; the
  same flow runs *Add to a set*, *Snooze* and *Merge*.

  What is real here: `TfDialog`, `QueryBox`, `Chips`, `Results`, `Row`, the
  `Session`, the `>` palette, and every write (`create_link`, `save_link_set`,
  `submit_write` for approve, merge, re-run; `open_checkout`, `clone_repo`,
  the inbox and list writes) goes through the real IPC. Shortcuts, each
  marked PROTOTYPE below:
    - the `#x` tokens are read here and rewritten to `kind:` filters; `#l` and
      `#i` are answered here from the board's saved lists and the shell's
      inbox store (ruling P2 says the backend parses);
    - the Views and Sets groups are built here from `ticket_views` and
      `link_sets` (the real change indexes both in `knobas-search`);
    - `?` draws a card written here, not `syntax.json`;
    - the compare page takes the head branch from its address; the base is
      stashed in `sessionStorage`, which the page does not read yet;
    - *Merge* is offered on every pull request; the real row follows the
      source's declared `merge_pull_request` action, as the PR page does;
    - the sheet's rows are a fixed list per kind, not adapter-declared;
    - `BoardNext.svelte` is the recent items alone.

  Round 5 (Björn, 2026-11-02): ⌘? opens the help as an overlay from any
  search; the box empties and narrows the filters, Enter adds the picked
  one to the search that was there (letters merge: `#r` plus `#l` is `#rl`);
  the summaries are one word; the keys sit at the bottom of the card and
  the box says "⌘? for help". `#PAY-231` is gone: a key is `#t PAY-231`.

  Round 6 (Björn, 2026-11-02): ⌘S asks for the list's name in the box; a
  saved list can be given a key (*Assign key…* in its sheet) that opens it
  from anywhere; an action can be given a key (select it in the sheet, ⌘B,
  press the keys) that runs it on the selected row, kept per kind, so one
  chord means different things on a repository and on a pull request;
  *Favourite* on any entity puts it above the recents on the empty board.
  Favourites and keys live in `localStorage` here; the real change is a
  table each.

  Round 6b: a label in front of the box names what it asks for (Name, New
  name, Keys, Filter, a flow's step), highlighted and not part of the text;
  a saved list can be a favourite too, listed first in the empty box.

  Round 7 (Björn, 2026-11-02): a saved list applies by putting its own text
  back into the box (filters and words) and running it, never `list:<id>`;
  binding a key is a panel that shows the keys as they are pressed, until
  Enter or OK keeps them.

  Round 8 (Björn, 2026-11-02): one Favourites block, lists and items alike;
  and a readability pass on the rows, as `:global` rules here that would move
  to `tf/css/launcher.css`: the kind word column is gone (the monogram names
  the kind), the key is as large as the title and in ink, the second line is
  the sans at 12 px, the provenance column is as wide as its text.

  Round 9 (Björn, 2026-11-02, "less is more"): the row is three cells like
  a table (the monogram in the state's colour, a key column, the title); no
  second line, no provenance, no counts on the headings; `#b` is the build
  configurations (pipelines), and builds and commits are left out of the
  answer altogether. `RowNext.svelte` and `ResultsNext.svelte` draw it.

  Round 10 (Björn, 2026-11-02): tickets coloured by status (done green, in
  progress blue, closed grey); a colour legend per kind in the ⌘? card;
  `#p` is pipelines and `#c` pull requests; the key column is something a
  person reads (a pull request's repository, a repository's owner, a
  monitor's type, a page's space, a pipeline's project); the caret bar on
  the box is gone.

  Round 11 (Björn, 2026-11-02): a toggle between *Titles* (what is on
  screen) and *Everything* (⌘D, or the chip at the right of the box). With
  Everything on the dialog grows and splits: results at the left, and at the
  right a preview of where the words appear in the selected row: a page's
  own search hits (`docs_search`), a pipeline's latest run log, the text of
  a ticket, pull request or note from its view. Words found only in a run
  log add the pipeline to the answer. PROTOTYPE: the log search reads each
  pipeline's latest log through the IPC and keeps it in memory; the real
  change is an index in the backend. *Titles* is a client-side filter on
  the engine's answer; the real change is a flag on the query.

  Round 12 (Björn, 2026-11-02): the one box replaces the pages' own search
  fields. Opened on a section (⌘K, or `/` as the pages bound it), the box
  starts with that section's filter: `#t ` on Tickets, `#rc ` on Code,
  `#p ` on Builds, `#d ` on Docs, `#m ` on Monitors, `#i ` on the Inbox;
  the pages' own fields are hidden. PROTOTYPE: the hiding is a `:global`
  rule here; the real change removes the fields and their `/` handlers.

  Screenshots: `?launcher-open=sepa` opens with that query, `&launcher-sheet=1`
  opens the sheet on the row at that index once results land.
-->
<script lang="ts">
  import { onMount, tick, untrack } from "svelte";

  import {
    ackAlert,
    checkoutSettings,
    cloneRepo,
    completeInboxItem,
    createLink,
    docsSearch,
    pipelineRuns,
    pipelines as listPipelines,
    runLog,
    createSmartList as defaultCreateSmartList,
    deleteSmartList as defaultDeleteSmartList,
    entityView,
    filterSuggestions as defaultFilterSuggestions,
    launcherHome as defaultHome,
    ipcErrorMessage,
    linkSets,
    openCheckout,
    renameSmartList as defaultRenameSmartList,
    saveLinkSet,
    search as defaultSearch,
    snoozeInboxItem,
    submitWrite,
    ticketViews,
  } from "../../ipc";
  import type { CheckoutApplication, EntityRow, InboxItem, LinkSetMemberKind, LinkSetRow, PipelineRow } from "../../ipc/entity";
  import type { LauncherHome, ResultGroup, SearchHit, SearchQuery, SearchResponse, SmartListSummary, TicketViewRow } from "../../ipc/search";
  import type { MergeStyle } from "../../ipc/sources";
  import type { CredentialHealth, SourceSyncStatus } from "../../ipc/sources";
  import { drawn } from "../../detail/relations";
  import { snoozePresets } from "../../inbox/inbox.svelte";
  import type { HealthPhase } from "../../shell/health.svelte";
  import { canBeTarget } from "../../shell/timer";
  import { push } from "../../shell/toasts.svelte";
  import { entityAddress } from "../../tf/address";
  import { counted } from "../../tf/counted";
  import { opener, sentence } from "../../tf/inbox/inbox";
  import type { LinkTarget } from "../../tf/job/current.svelte";
  import { prHash } from "../../tf/pr/pr";
  import { branchOptions, compareHash, repoHash } from "../../tf/repo/repo";
  import { jobHash, parseTfHash, sectionOf } from "../../tf/route";
  import TfDialog from "../../tf/settings/TfDialog.svelte";
  import { clockLabel, keyOf } from "../../time/format";
  import Chips from "../Chips.svelte";
  import QueryBox from "../QueryBox.svelte";
  import { type LauncherAction, filterActions, navigationActions } from "../actions";
  import type { InboxPreview, LauncherRow, TodayTime } from "../rows";
  import { Session, type SessionPorts } from "../session.svelte";
  import BoardNext from "./BoardNext.svelte";
  import ResultsNext from "./ResultsNext.svelte";
  import { cellOf } from "./cells";
  import RowNext from "./RowNext.svelte";
  import { pipelineHash } from "../../tf/builds/builds";
  import { kindMonogram, kindSingular } from "../../shell/kinds";

  let {
    open = $bindable(false),
    sources,
    runs,
    sourcesRead,
    inbox,
    today,
    actions = [],
    onnavigate,
    onclose,
    openEntity,
    onlink,
    ontimer,
    ports,
    now,
  }: {
    open?: boolean;
    sources?: CredentialHealth[] | undefined;
    runs?: SourceSyncStatus[];
    sourcesRead?: { phase: HealthPhase; error: string | null; retry: () => void } | undefined;
    inbox?: InboxPreview | undefined;
    today?: TodayTime | undefined;
    actions?: LauncherAction[];
    onnavigate: (hash: string) => void;
    onclose: () => void;
    /** The chosen job (`breadcrumbJob`), which the sheet links to. */
    openEntity?: { entityId: string; label: string } | undefined;
    /** Unused: the sheet writes the link itself. Declared so `App.svelte` mounts both launchers alike. */
    onlink?: ((target: LinkTarget) => void) | undefined;
    ontimer: ((targetId: string, targetTitle: string) => void) | undefined;
    ports?: Partial<SessionPorts>;
    now?: Date | undefined;
  } = $props();
  // Unused since the board is the recent items alone; kept so `App.svelte` mounts both launchers alike.
  // svelte-ignore state_referenced_locally
  void onlink;
  // svelte-ignore state_referenced_locally
  void runs;
  // svelte-ignore state_referenced_locally
  void today;

  // ---- PROTOTYPE: favourites and keys, kept in localStorage; the real change is a table each ----

  const KEPT = { favourites: "knobas.prototype.favourites", listKeys: "knobas.prototype.list-keys", actionKeys: "knobas.prototype.action-keys" };
  function kept<T>(key: string, fallback: T): T {
    try {
      const raw = localStorage.getItem(key);
      return raw ? (JSON.parse(raw) as T) : fallback;
    } catch {
      return fallback;
    }
  }
  function keep(key: string, value: unknown) {
    localStorage.setItem(key, JSON.stringify(value));
  }
  /** The favourites, newest first; each an entity row as the board draws one. */
  let favourites = $state<EntityRow[]>(kept(KEPT.favourites, []));
  /** The favourite saved lists, by id, newest first; drawn first in the empty box. */
  let favouriteLists = $state<string[]>(kept("knobas.prototype.favourite-lists", []));
  /** A saved list's key: list id → chord, such as `⌘1`. */
  let listKeys = $state<Record<string, string>>(kept(KEPT.listKeys, {}));
  /** An action's key, per kind: kind → chord → the action's id in its sheet. */
  let actionKeys = $state<Record<string, Record<string, string>>>(kept(KEPT.actionKeys, {}));

  /** The chord a key event spells, `⌘⇧O`, or `null` when it is plain typing: a modifier and one letter or digit. */
  function chordOf(event: KeyboardEvent): string | null {
    if (!(event.metaKey || event.ctrlKey || event.altKey)) return null;
    const key = event.code.startsWith("Key") ? event.code.slice(3) : event.code.startsWith("Digit") ? event.code.slice(5) : null;
    if (!key) return null;
    return `${event.ctrlKey ? "⌃" : ""}${event.altKey ? "⌥" : ""}${event.shiftKey ? "⇧" : ""}${event.metaKey ? "⌘" : ""}${key}`;
  }
  /** The launcher's own chords, never bindable. */
  const RESERVED = new Set(["⌘K", "⌘S", "⌘B", "⌘D", "⌃K", "⌃S", "⌃B", "⌃D"]);

  function isFavourite(entityId: string): boolean {
    return favourites.some((row) => row.entity_id === entityId);
  }

  async function toggleFavourite(row: EntityRow) {
    favourites = isFavourite(row.entity_id) ? favourites.filter((fav) => fav.entity_id !== row.entity_id) : [row, ...favourites];
    keep(KEPT.favourites, favourites);
    sheet = null;
    push({ text: isFavourite(row.entity_id) ? `${row.title} is a favourite` : `${row.title} is no favourite any more` });
    await session.loadHome();
    box?.focus();
  }

  async function toggleFavouriteList(id: string, label: string) {
    const starred = favouriteLists.includes(id);
    favouriteLists = starred ? favouriteLists.filter((had) => had !== id) : [id, ...favouriteLists];
    keep("knobas.prototype.favourite-lists", favouriteLists);
    sheet = null;
    push({ text: starred ? `${label} is no favourite any more` : `${label} is a favourite` });
    await session.loadHome();
    session.set(session.raw);
    box?.focus();
  }

  /** The chord bound to `actionId` on `kind`, or `null`. */
  function chordFor(kind: string, actionId: string): string | null {
    return Object.entries(actionKeys[kind] ?? {}).find(([, id]) => id === actionId)?.[0] ?? null;
  }

  function bindAction(kind: string, actionId: string, chord: string | null) {
    const forKind = { ...(actionKeys[kind] ?? {}) };
    for (const [had, id] of Object.entries(forKind)) if (id === actionId) delete forKind[had];
    if (chord) forKind[chord] = actionId;
    actionKeys = { ...actionKeys, [kind]: forKind };
    keep(KEPT.actionKeys, actionKeys);
  }

  function bindList(listId: string, chord: string | null) {
    const next = { ...listKeys };
    for (const [id, had] of Object.entries(next)) if (had === chord || id === listId) delete next[id];
    if (chord) next[listId] = chord;
    listKeys = next;
    keep(KEPT.listKeys, listKeys);
  }

  // ---- PROTOTYPE: Titles or Everything (round 11) ----------------------------

  /** `true`: search everything and preview where the words appear; `false`: only what is on screen, the titles. */
  let deep = $state<boolean>(kept("knobas.prototype.deep", false));
  function setDeep(on: boolean) {
    deep = on;
    keep("knobas.prototype.deep", on);
    if (session.raw.trim() !== "") session.set(session.raw);
    box?.focus();
  }

  const WORD = (text: string) => text.toLowerCase().split(/\s+/).filter((word) => word.length >= 2 && !word.startsWith("#"));

  /** Whether every word is on screen for the hit: in its title or in its key column (a ticket's key, a repository's owner, a page's space). */
  function onScreen(hit: SearchHit, kindWord: string, words: string[]): boolean {
    const shown = cellOf(hit.kind, hit.entity_id, hit.title, hit.path, kindWord);
    const hay = `${shown.cell} ${shown.title}`.toLowerCase();
    return words.every((word) => hay.includes(word));
  }

  /** One excerpt of a preview: text around a match, the matches marked. */
  interface Excerpt {
    where: string | null;
    segments: { text: string; hit: boolean }[];
  }

  /** Every place the words appear in `text`, ±70 characters, at most `cap` of them. */
  function excerptsOf(text: string, words: string[], where: string | null, cap = 6): Excerpt[] {
    const flat = text.replace(/\s+/g, " ").trim();
    const lower = flat.toLowerCase();
    const out: Excerpt[] = [];
    let from = 0;
    while (out.length < cap) {
      let at = -1;
      let word = "";
      for (const candidate of words) {
        const found = lower.indexOf(candidate, from);
        if (found >= 0 && (at < 0 || found < at)) {
          at = found;
          word = candidate;
        }
      }
      if (at < 0) break;
      const start = Math.max(0, at - 70);
      const end = Math.min(flat.length, at + word.length + 90);
      const slice = flat.slice(start, end);
      const segments: Excerpt["segments"] = [];
      let cursor = 0;
      const sliceLower = slice.toLowerCase();
      for (;;) {
        let next = -1;
        let hitWord = "";
        for (const candidate of words) {
          const found = sliceLower.indexOf(candidate, cursor);
          if (found >= 0 && (next < 0 || found < next)) {
            next = found;
            hitWord = candidate;
          }
        }
        if (next < 0) break;
        if (next > cursor) segments.push({ text: slice.slice(cursor, next), hit: false });
        segments.push({ text: slice.slice(next, next + hitWord.length), hit: true });
        cursor = next + hitWord.length;
      }
      if (cursor < slice.length) segments.push({ text: slice.slice(cursor), hit: false });
      if (start > 0) segments.unshift({ text: "…", hit: false });
      if (end < flat.length) segments.push({ text: "…", hit: false });
      out.push({ where, segments });
      from = end;
    }
    return out;
  }

  /** Every string worth reading in a view's blocks: a prototype's reading of a page, a ticket, a pull request. */
  function textsOf(value: unknown, out: string[] = [], depth = 0): string[] {
    if (depth > 8 || value === null || value === undefined) return out;
    if (typeof value === "string") {
      if (value.length >= 12) out.push(value);
      return out;
    }
    if (Array.isArray(value)) {
      for (const item of value) textsOf(item, out, depth + 1);
      return out;
    }
    if (typeof value === "object") {
      for (const [key, inner] of Object.entries(value as Record<string, unknown>)) {
        if (["entity_id", "id", "kind", "source_id", "url", "web_url", "href", "icon", "monogram", "tone", "updated_at", "synced_at"].includes(key)) continue;
        textsOf(inner, out, depth + 1);
      }
    }
    return out;
  }

  /** The latest run's log per pipeline, read once and kept; the real change is an index. */
  const logs = new Map<string, { title: string; text: string; lines: { number: number; text: string }[] }>();
  let pipelineRows: PipelineRow[] | null = null;
  async function logOf(pipeline: PipelineRow) {
    const had = logs.get(pipeline.entity_id);
    if (had) return had;
    const runs = pipeline.recent.length > 0 ? pipeline.recent : await pipelineRuns(pipeline.entity_id, 1).catch(() => []);
    const run = runs[0];
    const log = run ? await runLog(run.entity_id).catch(() => null) : null;
    const lines = (log?.lines ?? []).map((line) => ({ number: line.number, text: line.text }));
    const entry = { title: pipeline.name, text: lines.map((line) => line.text).join("\n"), lines };
    logs.set(pipeline.entity_id, entry);
    return entry;
  }

  /** The pipelines whose latest log holds every word, as hits. */
  async function logHits(words: string[], at: string): Promise<SearchHit[]> {
    if (words.length === 0) return [];
    pipelineRows ??= await listPipelines().catch(() => []);
    const hits: SearchHit[] = [];
    for (const pipeline of pipelineRows) {
      const log = await logOf(pipeline);
      const lower = log.text.toLowerCase();
      if (!words.every((word) => lower.includes(word))) continue;
      hits.push({
        ...BLANK,
        entity_id: pipeline.entity_id,
        kind: "pipeline",
        source_id: pipeline.source_id,
        title: pipeline.name,
        synced_at: at,
        path: pipeline.path.join(" › "),
        state: pipeline.recent[0]?.state ?? null,
      });
    }
    return hits;
  }

  /** The filter a section's own search would have applied: what the box starts with when opened there. */
  function sectionFilter(): string {
    const section = sectionOf(parseTfHash(location.hash));
    switch (section) {
      case "tickets":
        return "#t ";
      case "inbox":
        return "#i ";
      case "code":
        return "#rc ";
      case "builds":
        return "#p ";
      case "docs":
        return "#d ";
      case "monitors":
        return "#m ";
      default:
        return "";
    }
  }

  // ---- PROTOTYPE: the local corpus the backend does not index yet ----------

  let views: TicketViewRow[] = [];
  let sets = $state<LinkSetRow[]>([]);
  /** The saved lists, as the board read them; the board itself no longer draws them. */
  let lists = $state<SmartListSummary[]>([]);
  let listsLoaded = false;
  /** Settings › Checkouts: the applications *Open in ⟨app⟩* offers, and the default's id. */
  let apps = $state<CheckoutApplication[]>([]);
  let defaultApp = $state<string | null>(null);

  async function loadLocal() {
    const [v, s, c] = await Promise.allSettled([ticketViews(), linkSets(), checkoutSettings()]);
    views = v.status === "fulfilled" ? v.value : [];
    sets = s.status === "fulfilled" ? s.value : [];
    if (c.status === "fulfilled") {
      apps = c.value.applications;
      defaultApp = c.value.default_application;
    }
  }

  // svelte-ignore state_referenced_locally
  const realHome = ports?.launcherHome ?? defaultHome;

  /** Kinds nobody looks for in the box (Björn, round 9): a run and a commit are reached from their pipeline and repository. */
  const LEFT_OUT = new Set(["build", "commit"]);

  /** The board without lists: the saved ones are kept aside for `#l`, the built-ins are gone. */
  async function homeNext(): Promise<LauncherHome> {
    const home = await realHome();
    lists = home.smart_lists.filter((list) => list.saved);
    listsLoaded = true;
    const starred = new Set(favourites.map((row) => row.entity_id));
    const starredLists = favouriteLists.map((id) => lists.find((list) => list.id === id)).filter((list): list is SmartListSummary => list !== undefined);
    const recent = home.recent.filter((row) => !starred.has(row.entity_id) && !LEFT_OUT.has(row.kind));
    return { ...home, smart_lists: starredLists, recent: [...favourites, ...recent] };
  }

  const BLANK = { updated_at: null, state: null, rank: 1, snippet: [] as SearchHit["snippet"] };
  const NO_FILTERS = { sources: [], kinds: [], updated_within_days: null, mine: false, authors: [] };

  function viewGroup(needle: string, at: string): ResultGroup | null {
    const hits: SearchHit[] = views
      .filter((view) => `${view.name} ${view.group} ${view.query}`.toLowerCase().includes(needle))
      .map((view) => ({
        ...BLANK,
        entity_id: `view:${view.id}`,
        kind: "view",
        source_id: "local",
        title: view.name,
        synced_at: at,
        path: [view.group, view.layout, view.query].filter((part) => part !== "").join(" · "),
      }));
    return hits.length === 0 ? null : { kind: "view", label: "View", plural: "Views", monogram: "VW", total: hits.length, hits };
  }

  function setGroup(needle: string, at: string): ResultGroup | null {
    const hits: SearchHit[] = sets
      .filter((set) => `${set.name} ${set.description}`.toLowerCase().includes(needle))
      .map((set) => ({
        ...BLANK,
        entity_id: `set:${set.id}`,
        kind: "set",
        source_id: "local",
        title: set.name,
        synced_at: at,
        path: `${counted(set.members.length, "member", "members")} · ${counted(set.patterns.length, "pattern", "patterns")} · on ${counted(set.tickets, "ticket", "tickets")}`,
      }));
    return hits.length === 0 ? null : { kind: "set", label: "Set", plural: "Sets", monogram: "LS", total: hits.length, hits };
  }

  /** `#l ⟨text⟩`: the saved lists whose name or description holds the text; all of them for no text. */
  function listGroup(needle: string, at: string): ResultGroup | null {
    const hits: SearchHit[] = lists
      .filter((list) => `${list.label} ${list.description}`.toLowerCase().includes(needle))
      .map((list) => ({
        ...BLANK,
        entity_id: `list:${list.id}`,
        kind: "list",
        source_id: "local",
        title: list.label,
        synced_at: at,
        path: `${listKeys[list.id] ? `${listKeys[list.id]} · ` : ""}${list.needs_attention ? `${list.description} · needs attention` : `${list.description} · ${counted(list.count, "item", "items")}`}`,
      }));
    return hits.length === 0 ? null : { kind: "list", label: "Saved list", plural: "Saved lists", monogram: "LI", total: hits.length, hits };
  }

  /** Which inbox item a row stands for, by the row's id. */
  const inboxKeys = new Map<string, string>();

  /** `#i ⟨text⟩`: the inbox's lines holding the text, newest first; all of them for no text. */
  function inboxGroup(needle: string, at: string): ResultGroup | null {
    const items = inbox?.items ?? [];
    inboxKeys.clear();
    const seen = new Map<string, number>();
    const hits: SearchHit[] = items
      .filter((item) => `${sentence(item)} ${item.reason}`.toLowerCase().includes(needle))
      .map((item) => {
        // The row's key column is the line's time; the item's own key is kept aside.
        const when = clockLabel(item.occurred_at, now ?? new Date(), true);
        const n = (seen.get(when) ?? 0) + 1;
        seen.set(when, n);
        const id = `inbox:${when}${n > 1 ? ` (${n})` : ""}`;
        inboxKeys.set(id, item.key);
        return { ...BLANK, entity_id: id, kind: "inbox", source_id: item.source_id, title: sentence(item), synced_at: at, path: item.reason };
      });
    return hits.length === 0 ? null : { kind: "inbox", label: "Inbox line", plural: "Inbox", monogram: "IN", total: hits.length, hits };
  }

  function inboxItemOf(row: LauncherRow): InboxItem | null {
    if (row.kind !== "hit" || row.hit.kind !== "inbox") return null;
    const key = inboxKeys.get(row.hit.entity_id);
    return key === undefined ? null : (inbox?.items.find((item) => item.key === key) ?? null);
  }

  function docsRow(response: SearchResponse, at: string): SearchResponse {
    const text = response.interpreted.text.trim();
    if (text === "") return response;
    const hit: SearchHit = {
      ...BLANK,
      entity_id: `docs:${text}`,
      kind: "docs",
      source_id: "local",
      title: `Search Docs for “${text}”`,
      synced_at: at,
      path: "Every space, with the outline and the hits in the page",
    };
    const group: ResultGroup = { kind: "docs-search", label: "Docs", plural: "Docs area", monogram: "PG", total: 1, hits: [hit] };
    return { ...response, groups: [...response.groups, group] };
  }

  // svelte-ignore state_referenced_locally
  const realSearch = ports?.search ?? defaultSearch;

  /** `#x`: letters after the hash are kinds, and they combine (`#rl`); the backend filters on six, two are answered here. */
  const KIND_OF: Record<string, string> = { t: "ticket", c: "pr", r: "repo", p: "pipeline", m: "monitor", d: "page" };
  const LOCAL = new Set(["l", "i"]);
  const TOKEN = /^#([tcrpmdli]+)$/i;

  /** The search port with the prototype's grammar read first and its groups layered over the real answer. */
  async function searchNext(query: SearchQuery): Promise<SearchResponse> {
    const raw = query.raw.trim();
    const at = (now ?? new Date()).toISOString();
    // PROTOTYPE: the frontend reads grammar here, against ruling P2.
    const kinds: string[] = [];
    const local: string[] = [];
    const rest: string[] = [];
    for (const token of raw.split(/\s+/).filter(Boolean)) {
      const letters = TOKEN.exec(token)?.[1]?.toLowerCase();
      if (letters) {
        for (const letter of new Set(letters)) {
          if (KIND_OF[letter]) kinds.push(KIND_OF[letter]);
          else if (LOCAL.has(letter)) local.push(letter);
        }
      } else {
        // `#PAY-231` is no lookup any more: the hash is dropped and the key is a word (`#t PAY-231`).
        rest.push(token.replace(/^#/, ""));
      }
    }
    const text = rest.join(" ");
    const needle = text.toLowerCase();
    const localGroups: ResultGroup[] = [];
    if (local.includes("l")) {
      if (!listsLoaded) await homeNext();
      const group = listGroup(needle, at);
      if (group) localGroups.push(group);
    }
    if (local.includes("i")) {
      const group = inboxGroup(needle, at);
      if (group) localGroups.push(group);
    }
    if (kinds.length === 0 && local.length > 0) {
      return {
        interpreted: { text, prefix: null, filters: NO_FILTERS, unknown_tokens: [] },
        groups: localGroups,
        total: localGroups.reduce((n, group) => n + group.total, 0),
        took_ms: 0,
        coverage: [],
      };
    }
    const sent = kinds.length === 0 ? query : { ...query, raw: [...kinds.map((kind) => `kind:${kind}`), text].join(" ") };
    let response = await realSearch(sent);
    response = { ...response, groups: response.groups.filter((group) => !LEFT_OUT.has(group.kind)) };
    const words = WORD(text);
    if (!deep) {
      // Titles: only what is on screen matches. PROTOTYPE: a client-side filter; the real change is a flag on the query.
      const groups = response.groups
        .map((group) => ({ ...group, hits: group.hits.filter((hit) => onScreen(hit, group.label, words)) }))
        .filter((group) => group.hits.length > 0)
        .map((group) => ({ ...group, total: group.hits.length }));
      response = { ...response, groups, total: groups.reduce((n, group) => n + group.total, 0) };
    } else if (words.length > 0) {
      // Everything: the Docs area's own search for pages, and the pipelines whose latest log holds the words.
      const wantPages = kinds.length === 0 || kinds.includes("page");
      const wantPipelines = kinds.length === 0 || kinds.includes("pipeline");
      const [docs, fromLogs] = await Promise.all([
        wantPages ? docsSearch(text, null).catch(() => null) : Promise.resolve(null),
        wantPipelines ? logHits(words, at) : Promise.resolve([] as SearchHit[]),
      ]);
      const groups = response.groups.map((group) => ({ ...group, hits: [...group.hits] }));
      const into = (kind: string, label: string, plural: string, monogram: string, hits: SearchHit[]) => {
        if (hits.length === 0) return;
        let group = groups.find((candidate) => candidate.kind === kind);
        if (!group) {
          group = { kind, label, plural, monogram, total: 0, hits: [] };
          groups.push(group);
        }
        const had = new Set(group.hits.map((hit) => hit.entity_id));
        for (const hit of hits) if (!had.has(hit.entity_id)) group.hits.push(hit);
        group.total = group.hits.length;
      };
      into(
        "page",
        "Page",
        "Pages",
        "PG",
        (docs?.hits ?? []).map((hit) => ({
          ...BLANK,
          entity_id: hit.entity_id,
          kind: "page",
          source_id: "confluence",
          title: hit.title,
          synced_at: at,
          path: hit.path ?? hit.space?.name ?? null,
          snippet: hit.snippet.segments,
        })),
      );
      into("pipeline", "Pipeline", "Pipelines", "PL", fromLogs);
      response = { ...response, groups, total: groups.reduce((n, group) => n + group.hits.length, 0) };
    }
    if (kinds.length === 1 && kinds[0] === "page") response = docsRow(response, at);
    if (localGroups.length > 0) {
      response = { ...response, groups: [...response.groups, ...localGroups], total: response.total + localGroups.reduce((n, group) => n + group.total, 0) };
    }
    if (kinds.length > 0 || local.length > 0 || response.interpreted.prefix !== null || needle === "") return response;
    const found = [viewGroup(needle, at), setGroup(needle, at)].filter((group): group is ResultGroup => group !== null);
    if (found.length === 0) return response;
    return { ...response, groups: [...found, ...response.groups], total: response.total + found.reduce((n, group) => n + group.total, 0) };
  }

  // svelte-ignore state_referenced_locally
  const session = new Session({
    createSmartList: defaultCreateSmartList,
    renameSmartList: defaultRenameSmartList,
    deleteSmartList: defaultDeleteSmartList,
    filterSuggestions: defaultFilterSuggestions,
    ...ports,
    search: searchNext,
    launcherHome: homeNext,
  });

  const health = $derived(sources ?? session.home?.sources ?? []);
  const sourcesPhase = $derived(sourcesRead?.phase ?? "ready");

  let box = $state<ReturnType<typeof QueryBox> | null>(null);

  // ---- the ⌘? card: this grammar, written here, as an overlay the box narrows ----

  interface HelpEntry {
    token: string;
    insert: string;
    summary: string;
  }
  const HELP: HelpEntry[] = [
    { token: "#t", insert: "#t", summary: "tickets" },
    { token: "#c", insert: "#c", summary: "pull requests" },
    { token: "#r", insert: "#r", summary: "repositories" },
    { token: "#p", insert: "#p", summary: "pipelines" },
    { token: "#m", insert: "#m", summary: "monitors" },
    { token: "#d", insert: "#d", summary: "pages" },
    { token: "#l", insert: "#l", summary: "lists" },
    { token: "#i", insert: "#i", summary: "inbox" },
    { token: ">", insert: "> ", summary: "go to" },
    { token: "@me", insert: "@me ", summary: "mine" },
    { token: "t", insert: "t ", summary: "time" },
  ];
  /** The colour legend, per kind: what the monogram's colour says. The tones are the adapters' own. */
  const LEGEND: { kind: string; monogram: string; tones: [string, string][] }[] = [
    { kind: "tickets", monogram: "TK", tones: [["ok", "done"], ["info", "in progress"], ["muted", "closed"]] },
    { kind: "pull requests", monogram: "PR", tones: [["info", "open"], ["ok", "merged"], ["muted", "closed"]] },
    { kind: "pipelines", monogram: "PL", tones: [["ok", "last run passed"], ["err", "last run failed"], ["info", "running"]] },
    { kind: "monitors", monogram: "MO", tones: [["ok", "up"], ["err", "down"], ["muted", "paused"]] },
  ];
  const KEYS: [string, string][] = [
    ["↵", "open"],
    ["Tab", "actions"],
    ["⌘S", "save as list"],
    ["⌘?", "this card; letters combine: #rl is repositories and lists"],
    ["Esc", "back"],
  ];

  /** The help overlay: the box holds its filter; the search words wait in `session.raw`. */
  let help = $state<{ filter: string; selected: number } | null>(null);

  function fuzzyHelp(filter: string): HelpEntry[] {
    const needle = filter.toLowerCase().replace(/\s+/g, "");
    if (needle === "") return HELP;
    return HELP.filter((entry) => {
      const hay = `${entry.token} ${entry.summary}`.toLowerCase();
      let at = -1;
      for (const char of needle) {
        at = hay.indexOf(char, at + 1);
        if (at < 0) return false;
      }
      return true;
    });
  }
  const helpShown = $derived(help ? fuzzyHelp(help.filter) : []);

  /** Put a picked filter into the search: a `#` token merges its letters into one already there, the rest go first. */
  function withFilter(raw: string, insert: string): string {
    const base = raw.trim();
    if (insert.startsWith("#")) {
      const letter = insert.slice(1);
      const tokens = base === "" ? [] : base.split(/\s+/);
      const at = tokens.findIndex((token) => TOKEN.test(token));
      if (at >= 0) {
        const have = tokens[at]!.slice(1).toLowerCase();
        tokens[at] = have.includes(letter) ? tokens[at]! : `#${have}${letter}`;
        return `${tokens.join(" ")} `;
      }
      return `${insert} ${base}`.trim() + " ";
    }
    return `${insert}${base}`;
  }

  function openHelp() {
    help = { filter: "", selected: 0 };
    void tick().then(() => box?.focus());
  }

  function leaveHelp() {
    help = null;
    box?.focus();
  }

  function helpKey(event: KeyboardEvent, current: { filter: string; selected: number }): void {
    switch (event.key) {
      case "ArrowDown":
        event.preventDefault();
        current.selected = Math.min(helpShown.length - 1, current.selected + 1);
        return;
      case "ArrowUp":
        event.preventDefault();
        current.selected = Math.max(0, current.selected - 1);
        return;
      case "Enter": {
        event.preventDefault();
        const entry = helpShown[current.selected];
        help = null;
        if (entry) session.set(withFilter(session.raw, entry.insert));
        box?.focus();
        return;
      }
      case "Escape":
        event.preventDefault();
        event.stopPropagation();
        leaveHelp();
        return;
      case "Backspace":
        if (current.filter === "") {
          event.preventDefault();
          leaveHelp();
        }
        return;
      default:
    }
  }

  /** ⌘? (⌘⇧/ on most keyboards): the card, from any search; pressed again, back to it. */
  function helpChord(event: KeyboardEvent): boolean {
    if (!(event.metaKey || event.ctrlKey) || !(event.key === "?" || (event.key === "/" && event.shiftKey))) return false;
    event.preventDefault();
    if (sheet) return true;
    if (help) leaveHelp();
    else openHelp();
    return true;
  }

  // ---- the sheet: what Tab opens -----------------------------------------

  const RELATIONS: { id: string; label: string }[] = [
    { id: "related", label: "related" },
    { id: "implements", label: "implements" },
    { id: "documents", label: "documents" },
    { id: "runs-on", label: "runs on" },
    { id: "blocks", label: "blocks" },
    { id: "depends-on", label: "depends on" },
  ];

  interface SheetItem {
    id: string;
    label: string;
    detail: string;
    /** The one item with the relation chips on it. */
    link?: boolean;
    run: () => void | Promise<void>;
  }

  /** One step of a flow: what the sheet asks, and the choices the box narrows, given the picks so far. */
  interface Step {
    title: string;
    choices: (picks: string[]) => Promise<SheetItem[]>;
  }

  /** A flow: steps asked in order, each pick kept; `finish` runs with all of them. */
  interface Flow {
    /** Which flow an answer belongs to: `sheet.flow` is a reactive proxy, so identity cannot say. */
    id: number;
    steps: Step[];
    picks: string[];
    finish: (picks: string[]) => void | Promise<void>;
    /** The current step's choices once read; `null` while reading. */
    choices: SheetItem[] | null;
  }

  interface Sheet {
    rowId: string;
    row: LauncherRow;
    title: string;
    selected: number;
    relation: number;
    /** `flow` asks the box for a pick; `rename` for a saved list's new name; `bind` waits for the keys of the selected action, `bind-list` for the list's. */
    level: "root" | "flow" | "rename" | "bind" | "bind-list";
    flow: Flow | null;
    /** What the box holds while the sheet is open: the filter, a pick's filter, or the new name. The search words wait in `session.raw`. */
    filter: string;
    /** While binding: the chord pressed so far, `null` before the first full one. */
    chord?: string | null;
  }

  /** The modifiers held right now, for the panel to show while a chord is being pressed. */
  let held = $state<string[]>([]);
  function heldOf(event: KeyboardEvent): string[] {
    const out: string[] = [];
    if (event.ctrlKey) out.push("⌃");
    if (event.altKey) out.push("⌥");
    if (event.shiftKey) out.push("⇧");
    if (event.metaKey) out.push("⌘");
    return out;
  }
  /** A chord split into the chips the panel draws: `⌘⇧O` is ⌘, ⇧, O. */
  function chipsOf(chord: string): string[] {
    return [...chord];
  }

  /** Keep the chord shown in the panel: on the action, or on the list. */
  function confirmBind(current: Sheet) {
    const chord = current.chord ?? null;
    const kind = kindOfRow(current.row);
    if (current.level === "bind-list" && listId !== null) {
      bindList(listId, chord);
      push({ text: chord ? `${chord} opens ${current.title}` : "Key removed" });
      sheet = { ...current, level: "root", chord: null };
      session.set(session.raw);
      return;
    }
    const target = shown[current.selected];
    if (target) {
      bindAction(kind, target.id, chord);
      push({ text: chord ? `${chord} runs ${target.label} on ${kind === "pr" ? "pull requests" : `${kind}s`}` : `Key removed from ${target.label}` });
    }
    sheet = { ...current, level: "root", chord: null };
  }

  let sheet = $state<Sheet | null>(null);

  interface Entity {
    entityId: string;
    title: string;
    kind: string;
  }

  function entityOf(row: LauncherRow): Entity | null {
    switch (row.kind) {
      case "hit":
        if (["view", "set", "list", "inbox"].includes(row.hit.kind) || row.group.kind === "docs-search") return null;
        return { entityId: row.hit.entity_id, title: row.hit.title, kind: row.hit.kind };
      case "recent":
        return { entityId: row.row.entity_id, title: row.row.title, kind: row.row.kind };
      default:
        return null;
    }
  }

  /** What an action key is kept under: the entity's kind, or the row's own. */
  function kindOfRow(row: LauncherRow): string {
    const entity = entityOf(row);
    if (entity) return entity.kind;
    if (row.kind === "hit") return row.group.kind === "docs-search" ? "docs" : row.hit.kind;
    return row.kind;
  }

  /** The entity row a hit or a recent line stands for, as the board draws one. */
  function entityRowOf(row: LauncherRow): EntityRow | null {
    if (row.kind === "recent") return row.row;
    if (row.kind !== "hit" || !entityOf(row)) return null;
    const { entity_id, kind, source_id, title, updated_at, synced_at, path, state } = row.hit;
    return { entity_id, kind, source_id, title, updated_at, synced_at, path, state };
  }

  function nameOf(row: LauncherRow): string {
    const entity = entityOf(row);
    if (entity) return entity.kind === "ticket" ? keyOf(entity.entityId) : entity.title;
    if (row.kind === "hit") return row.hit.title;
    if (row.kind === "list") return row.list.label;
    return "";
  }

  function memberKindOf(kind: string): LinkSetMemberKind | null {
    switch (kind) {
      case "page":
      case "repo":
      case "pipeline":
      case "asset":
      case "monitor":
        return kind;
      default:
        return null;
    }
  }

  async function writeLink(entity: Entity, relationIndex: number) {
    const job = openEntity;
    if (!job) return;
    const relation = RELATIONS[relationIndex]?.id ?? "related";
    const link = drawn(relation, job.entityId, entity.entityId);
    try {
      await createLink(link.fromId, link.toId, link.relation);
      const name = entity.kind === "ticket" ? keyOf(entity.entityId) : entity.title;
      push({ text: `Linked ${name} to ${job.label} as ${RELATIONS[relationIndex]?.label ?? "related"}` });
      close();
    } catch (cause) {
      sheet = null;
      session.error = ipcErrorMessage(cause);
    }
  }

  async function putInSet(set: LinkSetRow, entity: Entity) {
    const member = memberKindOf(entity.kind);
    const draft = {
      id: set.id,
      name: set.name,
      description: set.description,
      members: set.members.map((row) => ({ kind: row.kind, entity_id: row.entity_id })),
      patterns: set.patterns.map((row) => ({ kind: row.kind, query: row.query })),
      rules: set.rules.map((row) => ({ kind: row.kind, value: row.value })),
    };
    if (member) draft.members.push({ kind: member, entity_id: entity.entityId });
    else draft.rules.push({ kind: "manual", value: entity.entityId });
    await doing(member ? `Added ${entity.title} to ${set.name}` : `${set.name} now shows on ${keyOf(entity.entityId)}`, () => saveLinkSet(draft));
  }

  /** A write that answers at once, said in the toast; the launcher closes either way the write went. */
  async function doing(said: string, write: () => Promise<unknown>) {
    try {
      await write();
      push({ text: said });
      close();
    } catch (cause) {
      sheet = null;
      session.error = ipcErrorMessage(cause);
    }
  }

  async function copy(text: string, said: string) {
    try {
      await navigator.clipboard.writeText(text);
      push({ text: said });
    } catch {
      push({ text: "The clipboard refused.", tone: "err" });
    }
    close();
  }

  async function renameList(id: string, name: string) {
    const label = name.trim();
    sheet = null;
    if (label === "") return;
    await session.renameList(id, label);
    await session.loadHome();
    session.set(session.raw);
    box?.focus();
    push({ text: `Renamed to ${label}` });
  }

  async function deleteList(id: string) {
    sheet = null;
    await session.deleteList(id);
    await session.loadHome();
    session.set(session.raw);
    box?.focus();
  }

  /** Start a flow in the sheet: the first step's choices are read and the box narrows them. */
  let flows = 0;
  function startFlow(steps: Step[], finish: Flow["finish"]) {
    if (!sheet) return;
    const flow: Flow = { id: ++flows, steps, picks: [], finish, choices: null };
    sheet = { ...sheet, level: "flow", flow, filter: "", selected: 0 };
    void readStep(flow);
  }

  async function readStep(flow: Flow) {
    const step = flow.steps[flow.picks.length];
    if (!step) return;
    try {
      const choices = await step.choices(flow.picks);
      if (sheet?.flow?.id === flow.id && sheet.flow.picks.length === flow.picks.length) sheet = { ...sheet, flow: { ...flow, choices } };
    } catch (cause) {
      sheet = null;
      session.error = ipcErrorMessage(cause);
    }
  }

  /** Take `pick` for the current step: the next step's choices, or the finish. */
  function pick(flow: Flow, value: string) {
    const picks = [...flow.picks, value];
    if (picks.length >= flow.steps.length) {
      sheet = null;
      void flow.finish(picks);
      return;
    }
    const next: Flow = { ...flow, picks, choices: null };
    if (sheet) sheet = { ...sheet, flow: next, filter: "", selected: 0 };
    void readStep(next);
  }

  /** One step over fixed choices, each `id` the pick. */
  const fixed = (title: string, items: { id: string; label: string; detail: string }[]): Step => ({
    title,
    choices: () => Promise.resolve(items.map((item) => ({ ...item, run: () => {} }))),
  });

  /** The branches of a repository, from its view's Branches section, the default first; `except` names the pick to leave out. */
  const branchesOf = (repoId: string, title: string, except?: (picks: string[]) => string | undefined): Step => ({
    title,
    choices: async (picks) => {
      const view = await entityView(repoId, "branches");
      const skip = except?.(picks);
      return branchOptions(view?.sections[0] ?? null)
        .filter((option) => option.name !== skip)
        .map((option) => ({ id: option.name, label: option.name, detail: option.isDefault ? "the default branch" : "", run: () => {} }));
    },
  });

  /** *Open in ⟨app⟩*: one row per application configured under Settings › Checkouts, the default first. */
  function openInRows(entity: Entity, where: string): SheetItem[] {
    const ordered = [...apps].sort((left, right) => Number(right.id === defaultApp) - Number(left.id === defaultApp));
    if (ordered.length === 0) {
      return [{ id: "open-default", label: "Open in editor", detail: where, run: () => doing(`Opened ${entity.title}`, () => openCheckout(entity.entityId)) }];
    }
    return ordered.map((app) => ({
      id: `open-${app.id}`,
      label: `Open in ${app.name}`,
      detail: where,
      run: () => doing(`Opened ${entity.title} in ${app.name}`, () => openCheckout(entity.entityId, { application: app.id })),
    }));
  }

  /** The sheet's rows for `row`, in the order they are drawn. Empty: no sheet. */
  /** A saved list's id, name and the text it keeps, from a result row or a board row; `null` for anything else. */
  function listOf(row: LauncherRow): { id: string; label: string; query: string | null } | null {
    const summary = row.kind === "hit" && row.hit.kind === "list" ? lists.find((list) => list.id === keyOf(row.hit.entity_id)) : row.kind === "list" ? row.list : undefined;
    if (!summary) return null;
    // A saved list's description is the text it was saved with (`saved.rs`); a refused one carries the refusal instead.
    return { id: summary.id, label: summary.label, query: summary.needs_attention ? null : summary.description };
  }

  /** Apply a saved list: its own text back in the box, filters and words, and the search run again. */
  function applyList(list: { query: string | null; label: string }) {
    sheet = null;
    if (list.query === null) {
      push({ text: `${list.label} needs attention; its text cannot run`, tone: "err" });
      return;
    }
    session.set(list.query);
    box?.focus();
  }

  function itemsFor(row: LauncherRow): SheetItem[] {
    const list = listOf(row);
    if (list) {
      const { id, label } = list;
      const starred = favouriteLists.includes(id);
      return [
        { id: "apply", label: "Apply", detail: list.query ? `${list.query} in the box, as Enter does` : "It needs attention; nothing to apply", run: () => applyList(list) },
        {
          id: "rename",
          label: "Rename",
          detail: "The box takes the new name; Enter keeps it",
          run: () => {
            if (sheet) sheet = { ...sheet, level: "rename", filter: label };
            void tick().then(() => box?.focus({ select: true }));
          },
        },
        { id: "fav", label: starred ? "Unfavourite" : "Favourite", detail: starred ? "Leaves the top of the empty box" : "First in the empty box", run: () => toggleFavouriteList(id, label) },
        { id: "delete", label: "Delete", detail: "The saved query is gone; there is no undo", run: () => deleteList(id) },
        {
          id: "key",
          label: listKeys[id] ? `Key: ${listKeys[id]}` : "Assign key…",
          detail: listKeys[id] ? "Press new keys, or Backspace to remove them" : "A chord that opens this list from anywhere",
          run: () => { if (sheet) sheet = { ...sheet, level: "bind-list", filter: "", chord: listKeys[id] ?? null }; },
        },
      ];
    }
    if (row.kind === "hit" && row.hit.kind === "view") {
      const id = keyOf(row.hit.entity_id);
      return [
        { id: "open", label: "Open", detail: "The Tickets page on this view", run: () => activate(row) },
        { id: "copy", label: "Copy address", detail: `#/tickets/${id}`, run: () => void copy(`#/tickets/${id}`, "Copied the view's address") },
      ];
    }
    if (row.kind === "hit" && row.hit.kind === "set") {
      const id = keyOf(row.hit.entity_id);
      const items: SheetItem[] = [{ id: "open", label: "Open", detail: "The set, its members and its rules", run: () => activate(row) }];
      const job = openEntity;
      const set = sets.find((candidate) => candidate.id === id);
      if (job && set) {
        items.push({ id: "show", label: `Show on ${job.label}`, detail: "A rule for this one ticket, by hand", run: () => putInSet(set, { entityId: job.entityId, title: job.label, kind: "ticket" }) });
      }
      return items;
    }
    const item = inboxItemOf(row);
    if (item) {
      const items: SheetItem[] = [{ id: "open", label: "Open", detail: opener(item)?.href ? "What the line is about" : "The Inbox page", run: () => activate(row) }];
      const entity = item.entity_id;
      if (item.kind === "build" && entity) {
        items.push({ id: "rerun", label: "Re-run", detail: "Queued as a write to the source", run: () => doing(`Re-run queued for ${item.title}`, () => submitWrite({ RerunBuild: { entity } })) });
      }
      if (item.category === "alert" && entity) {
        items.push({ id: "ack", label: "Acknowledge", detail: "The alert stops asking", run: () => doing(`Acknowledged ${item.title}`, () => ackAlert(entity)) });
      }
      items.push({
        id: "snooze",
        label: "Snooze…",
        detail: "Until tomorrow morning, or next Monday",
        run: () => {
          const presets = snoozePresets(now ?? new Date());
          startFlow(
            [fixed("Snooze until", presets.map((preset) => ({ id: preset.at.toISOString(), label: preset.label, detail: preset.at.toLocaleString() })))],
            ([until]) => doing(`Snoozed ${item.title}`, () => snoozeInboxItem(item.key, until ?? "")),
          );
        },
      });
      items.push({ id: "done", label: "Done", detail: "Leaves the inbox", run: () => doing(`Done: ${item.title}`, () => completeInboxItem(item.key)) });
      return items;
    }
    const entity = entityOf(row);
    if (!entity) return [];
    const items: SheetItem[] = [];
    const job = openEntity;
    const id = entity.entityId;
    const key = keyOf(id);
    // The pair first: what this item and the job can be to each other.
    if (job && id !== job.entityId) {
      items.push({ id: "link", label: `Link to ${job.label}`, detail: "", link: true, run: () => writeLink(entity, sheet?.relation ?? 0) });
    }
    if (sets.length > 0) {
      const member = memberKindOf(entity.kind);
      if (member || entity.kind === "ticket") {
        items.push({
          id: "set",
          label: member ? "Add to a set…" : `Show a set on ${key}…`,
          detail: member ? "A fixed member, shown on every ticket the set applies to" : "The set's members appear on this ticket",
          run: () =>
            startFlow(
              [fixed(member ? "Add to" : `Show on ${key}`, sets.map((set) => ({ id: set.id, label: set.name, detail: set.description })))],
              ([setId]) => {
                const set = sets.find((candidate) => candidate.id === setId);
                if (set) void putInSet(set, entity);
              },
            ),
        });
      }
    }
    // A ticket's own: the timer, which also makes it the job (the shell does that), and the job itself.
    if (entity.kind === "ticket" && ontimer && canBeTarget({ entityId: id })) {
      const start = ontimer;
      items.push({ id: "timer", label: "Start timer", detail: `Stops the running one first; ${key} becomes the job`, run: () => { start(id, entity.title); close(); } });
    }
    // Then what the kind can do: a real address of the app, or a real write.
    const go = (hash: string) => () => { close(); onnavigate(hash); };
    switch (entity.kind) {
      case "ticket":
        items.push({ id: "work", label: "Work on this", detail: `Makes ${key} the job and opens its page`, run: go(jobHash(id)) });
        break;
      case "pr":
        items.push({ id: "diff", label: "Review the files", detail: "Files changed", run: go(prHash(id, "diff")) });
        items.push({ id: "talk", label: "Conversation", detail: "The review thread", run: go(prHash(id, "conversation")) });
        items.push({ id: "approve", label: "Approve", detail: "Queued as a write to the source", run: () => doing(`Approval queued for ${entity.title}`, () => submitWrite({ Approve: { entity: id, body: "" } })) });
        items.push({
          id: "merge",
          label: "Merge…",
          detail: "Pick the style; queued as a write to the source",
          run: () =>
            startFlow(
              [
                fixed("Merge as", [
                  { id: "merge", label: "Merge commit", detail: "Keeps every commit and adds one" },
                  { id: "squash", label: "Squash", detail: "One commit on the base" },
                  { id: "rebase", label: "Rebase", detail: "The commits replayed on the base" },
                ]),
              ],
              ([style]) => doing(`Merge queued for ${entity.title}`, () => submitWrite({ MergePullRequest: { entity: id, style: (style ?? "merge") as MergeStyle, delete_branch: false } })),
            ),
        });
        items.push(...openInRows(entity, "The checkout, at this pull request's branch"));
        break;
      case "repo":
        items.push({
          id: "newpr",
          label: "Create pull request…",
          detail: "Pick the branch with the change, then the branch it goes into",
          run: () =>
            startFlow(
              [branchesOf(id, "From branch"), branchesOf(id, "Into branch", (picks) => picks[0])],
              ([from, into]) => {
                // PROTOTYPE: the compare page takes the head from its address; the base waits here for the page to learn to read it.
                sessionStorage.setItem("knobas.prototype.compare-base", into ?? "");
                close();
                onnavigate(compareHash(id, from ?? ""));
              },
            ),
        });
        items.push({ id: "prs", label: "Pull requests", detail: "Open ones first", run: go(repoHash(id, "prs")) });
        items.push({ id: "branches", label: "Branches", detail: "Every branch and where it stands", run: go(repoHash(id, "branches")) });
        items.push(...openInRows(entity, "The checkout under the clones root"));
        items.push({ id: "clone", label: "Clone", detail: "Into a new folder under the clones root", run: () => doing(`Cloning ${entity.title}`, () => cloneRepo(id)) });
        break;
      case "pipeline":
        items.push({ id: "run", label: "Run", detail: "Queued as a write to the source", run: () => doing(`Run queued for ${entity.title}`, () => submitWrite({ TriggerBuild: { entity: id } })) });
        break;
      case "build":
        items.push({ id: "rerun", label: "Re-run", detail: "Queued as a write to the source", run: () => doing(`Re-run queued for ${entity.title}`, () => submitWrite({ RerunBuild: { entity: id } })) });
        break;
      case "monitor":
        items.push({ id: "ack", label: "Acknowledge alert", detail: "If one is open on it", run: () => doing(`Acknowledged ${entity.title}`, () => ackAlert(id)) });
        break;
      case "commit":
      case "branch":
        items.push(...openInRows(entity, "The checkout, at this point"));
        break;
      default:
    }
    const asRow = entityRowOf(row);
    if (asRow) {
      const starred = isFavourite(id);
      items.push({ id: "fav", label: starred ? "Unfavourite" : "Favourite", detail: starred ? "Leaves the top of the empty box" : "Above the recents in the empty box", run: () => toggleFavourite(asRow) });
    }
    if (entity.kind === "ticket") items.push({ id: "copy", label: "Copy key", detail: key, run: () => void copy(key, `Copied ${key}`) });
    else items.push({ id: "copy", label: "Copy title", detail: entity.title, run: () => void copy(entity.title, "Copied the title") });
    return items;
  }

  /**
   * The fuzzy filter: every typed character in order, somewhere in the label
   * or the detail; the tightest match first. Empty: everything, in order.
   */
  function fuzzy(items: SheetItem[], filter: string): SheetItem[] {
    const needle = filter.toLowerCase().replace(/\s+/g, "");
    if (needle === "") return items;
    const scored = items.flatMap((item) => {
      const hay = `${item.label} ${item.detail}`.toLowerCase();
      let at = -1;
      let first = -1;
      for (const char of needle) {
        at = hay.indexOf(char, at + 1);
        if (at < 0) return [];
        if (first < 0) first = at;
      }
      return [{ item, score: at - first + first / 100 }];
    });
    return scored.sort((left, right) => left.score - right.score).map((entry) => entry.item);
  }

  /** The first level, derived live: the chosen job, the sets and the applications can arrive after the sheet opened. */
  const rootItems = $derived(sheet ? itemsFor(sheet.row) : []);
  const shown = $derived(sheet ? fuzzy(sheet.level === "flow" ? (sheet.flow?.choices ?? []) : rootItems, sheet.filter) : []);
  const step = $derived(sheet?.flow ? sheet.flow.steps[sheet.flow.picks.length] : undefined);

  function openSheet() {
    const row = session.current;
    if (!row) return;
    if (itemsFor(row).length === 0) return;
    sheet = { rowId: row.id, row, title: nameOf(row), selected: 0, relation: 0, level: "root", flow: null, filter: "" };
  }

  /** Leave the sheet: the box shows the search words again, untouched. */
  function leaveSheet() {
    sheet = null;
    box?.focus();
  }

  // The sheet belongs to one row: moving off it closes it.
  $effect(() => {
    if (sheet && session.current?.id !== sheet.rowId) sheet = null;
  });

  // ---- the preview pane (round 11): where the words appear in the selected row ----

  interface Preview {
    rowId: string;
    /** The words the excerpts were read for, so a new search reads again. */
    words: string;
    title: string;
    kind: string;
    excerpts: Excerpt[];
    loading: boolean;
  }
  let preview = $state<Preview | null>(null);
  let previewTimer: ReturnType<typeof setTimeout> | null = null;

  /** The words of the search, for the preview to mark. */
  const searchWords = $derived(WORD(session.response?.interpreted.text ?? session.raw.replace(/#\S+/g, "")));

  async function readPreview(row: LauncherRow, words: string[]): Promise<Excerpt[]> {
    if (row.kind === "hit" && row.hit.kind === "pipeline") {
      pipelineRows ??= await listPipelines().catch(() => []);
      const pipeline = pipelineRows.find((candidate) => candidate.entity_id === row.hit.entity_id);
      if (!pipeline) return [];
      const log = await logOf(pipeline);
      const lines = log.lines.filter((line) => words.some((word) => line.text.toLowerCase().includes(word))).slice(0, 12);
      return lines.flatMap((line) => excerptsOf(line.text, words, `line ${line.number}`, 1));
    }
    if (row.kind === "hit" && row.hit.kind === "page") {
      const docs = await docsSearch(words.join(" "), null).catch(() => null);
      const own = docs?.hits.filter((hit) => hit.entity_id === row.hit.entity_id) ?? [];
      const fromDocs = own.map((hit) => ({ where: hit.snippet.section, segments: hit.snippet.segments }));
      if (fromDocs.length > 0) return fromDocs;
    }
    const entity = entityOf(row);
    if (entity) {
      const view = await entityView(entity.entityId, null).catch(() => null);
      const texts = view ? textsOf(view.sections) : [];
      const seen = new Set<string>();
      const out: Excerpt[] = [];
      for (const text of texts) {
        if (seen.has(text)) continue;
        seen.add(text);
        out.push(...excerptsOf(text, words, null, 3));
        if (out.length >= 10) break;
      }
      if (out.length > 0) return out.slice(0, 10);
    }
    if (row.kind === "hit" && row.hit.snippet.length > 0) return [{ where: null, segments: row.hit.snippet }];
    if (row.kind === "hit" && row.hit.path) return excerptsOf(`${row.hit.title} ${row.hit.path}`, words, null, 1);
    return [];
  }

  // The preview follows the selection, a breath after it settles.
  $effect(() => {
    const row = session.current;
    const words = searchWords;
    const wanted = deep && !sheet && !help && session.mode === "results" && row !== null;
    if (previewTimer) clearTimeout(previewTimer);
    if (!wanted || !row) {
      if (untrack(() => preview) !== null) preview = null;
      return;
    }
    // Read without tracking: this effect sets `preview`, and must not follow its own write.
    const had = untrack(() => preview);
    if (had?.rowId === row.id && had.words === words.join(" ")) return;
    preview = { rowId: row.id, words: words.join(" "), title: nameOf(row) || (row.kind === "hit" ? row.hit.title : ""), kind: kindOfRow(row), excerpts: [], loading: true };
    previewTimer = setTimeout(() => {
      void readPreview(row, words).then((excerpts) => {
        const current = untrack(() => preview);
        if (current?.rowId === row.id) preview = { ...current, excerpts, loading: false };
      });
    }, 120);
  });

  // ---- ids, scrolling, the `>` palette -----------------------------------

  const uid = $props.id();
  const optionId = (index: number) => `${uid}-opt-${index}`;
  const sheetOptionId = (index: number) => `${uid}-sheet-${index}`;
  const helpOptionId = (index: number) => `${uid}-help-${index}`;
  const resultsId = `${uid}-results`;
  const sheetId = `${uid}-sheet`;

  const helpId = `${uid}-help`;
  const activeOption = $derived.by(() => {
    const row = session.current;
    if (!row || session.error) return undefined;
    return optionId(session.selected);
  });
  const controlled = $derived(
    help
      ? { list: helpId, option: helpOptionId(help.selected) }
      : sheet
        ? { list: sheetId, option: sheetOptionId(sheet.selected) }
        : { list: resultsId, option: activeOption },
  );

  $effect(() => {
    void session.rows;
    if (activeOption) document.getElementById(activeOption)?.scrollIntoView?.({ block: "nearest" });
  });
  $effect(() => {
    if (sheet) document.getElementById(sheetOptionId(sheet.selected))?.scrollIntoView?.({ block: "nearest" });
  });

  // The board is the recent items alone: no inbox rows, no time row.
  $effect.pre(() => {
    session.extras = { inbox: null, today: null };
  });

  $effect(() => {
    const typed = session.response?.interpreted.text ?? "";
    session.actions = filterActions([...navigationActions(), ...actions], typed);
  });

  $effect(() => {
    if (open) {
      void session.loadHome();
      void loadLocal();
      // Opened on a section with nothing in the box: the section's filter, as its own search would have had it.
      const filter = sectionFilter();
      if (session.raw.trim() === "" && filter !== "") session.set(filter);
      void tick().then(() => box?.focus({ select: filter === "" }));
    } else {
      sheet = null;
      session.dispose();
    }
  });

  // The inbox store seeds after the shell mounts: an `#i` typed before that is asked again once it has.
  let inboxSeen = 0;
  $effect(() => {
    const count = inbox?.items.length ?? 0;
    if (count > 0 && inboxSeen === 0 && open && /(^|\s)#i(\s|$)/i.test(session.raw)) session.set(session.raw);
    inboxSeen = count;
  });

  // PROTOTYPE: the screenshot affordances.
  const params = new URLSearchParams(location.search);
  let autoSheet = params.has("launcher-sheet");
  $effect(() => {
    if (autoSheet && open && session.mode === "results" && session.response && session.response.groups.length > 0 && !sheet) {
      autoSheet = false;
      session.selected = Number(params.get("launcher-sheet") || 0);
      openSheet();
    }
  });

  onMount(() => {
    function onkeydown(event: KeyboardEvent) {
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "k") {
        event.preventDefault();
        // Opens; while open it only gives the box its focus back, wherever a click left it.
        if (!open) open = true;
        else box?.focus();
        return;
      }
      // A saved list's chord opens the launcher on that list, from anywhere; while open the box answers it.
      if (open) return;
      const chord = chordOf(event);
      const list = chord ? Object.entries(listKeys).find(([, had]) => had === chord)?.[0] : undefined;
      if (list) {
        event.preventDefault();
        open = true;
        void tick().then(async () => {
          if (!listsLoaded) await homeNext();
          const summary = lists.find((candidate) => candidate.id === list);
          if (summary) applyList({ query: summary.needs_attention ? null : summary.description, label: summary.label });
        });
      }
    }
    window.addEventListener("keydown", onkeydown);
    // `/` on a page opened its own search field; it opens this box with the page's filter instead. Capture phase, before the page's handler.
    function onslash(event: KeyboardEvent) {
      if (event.key !== "/" || event.metaKey || event.ctrlKey || event.altKey || open) return;
      const target = event.target as HTMLElement | null;
      if (target && (target.tagName === "INPUT" || target.tagName === "TEXTAREA" || target.isContentEditable)) return;
      event.preventDefault();
      event.stopPropagation();
      open = true;
    }
    window.addEventListener("keydown", onslash, true);
    const onkeyup = () => { if (held.length > 0) held = []; };
    window.addEventListener("keyup", onkeyup);
    const preset = params.get("launcher-open");
    if (preset !== null) {
      open = true;
      if (preset !== "") void tick().then(() => session.set(preset));
    }
    return () => {
      window.removeEventListener("keydown", onkeydown);
      window.removeEventListener("keydown", onslash, true);
      window.removeEventListener("keyup", onkeyup);
      session.dispose();
    };
  });

  function close() {
    open = false;
    sheet = null;
    session.dispose();
    onclose();
  }

  function addressOf(row: LauncherRow): string | null {
    switch (row.kind) {
      case "hit":
        // PROTOTYPE: a view and a set are addresses the search does not know yet.
        if (row.hit.kind === "view") return `#/tickets/${encodeURIComponent(keyOf(row.hit.entity_id))}`;
        if (row.hit.kind === "set") return `#/sets/${encodeURIComponent(keyOf(row.hit.entity_id))}`;
        if (row.hit.kind === "inbox") {
          const item = inboxItemOf(row);
          return item ? (opener(item)?.href ?? "#/inbox") : "#/inbox";
        }
        if (row.hit.kind === "pipeline") return pipelineHash(row.hit.entity_id);
        if (row.group.kind === "docs-search") {
          // PROTOTYPE: the Docs page does not read this yet; the hand-off is the implementer's.
          sessionStorage.setItem("knobas.prototype.docs-search", keyOf(row.hit.entity_id));
          return "#/docs";
        }
        return entityAddress(row.hit.kind, row.hit.entity_id, row.hit.title);
      case "recent":
        return entityAddress(row.row.kind, row.row.entity_id, row.row.title);
      case "action":
        return row.action.hash;
      case "inbox":
        return opener(row.item)?.href ?? "#/inbox";
      case "today":
        return "#/time";
      case "list":
      case "syntax":
        return null;
    }
  }

  function activate(row: LauncherRow) {
    // A saved list applies its own text in the box and keeps the reader here.
    const list = listOf(row);
    if (list) {
      applyList(list);
      return;
    }
    if (row.kind === "syntax") {
      session.set(row.entry.insert);
      box?.focus();
      return;
    }
    const hash = addressOf(row);
    if (!hash) return;
    close();
    onnavigate(hash);
  }

  /** ⌘S: the box asks for the list's name, the query prefilled. `true` when the key was taken. */
  function saveKey(event: KeyboardEvent): boolean {
    if (!(event.metaKey || event.ctrlKey) || event.key.toLowerCase() !== "s") return false;
    event.preventDefault();
    if (session.canSave && session.savedId === null && !sheet && !help) {
      naming = session.raw.trim();
      void tick().then(() => box?.focus({ select: true }));
    }
    return true;
  }

  const listId = $derived(sheet ? (listOf(sheet.row)?.id ?? null) : null);

  /** A list's chord opens it; an action's chord runs it on the selected row. `true` when the keys were a chord that did something. */
  function chordKey(event: KeyboardEvent): boolean {
    const chord = chordOf(event);
    if (!chord || RESERVED.has(chord)) return false;
    const list = Object.entries(listKeys).find(([, had]) => had === chord)?.[0];
    if (list) {
      event.preventDefault();
      sheet = null;
      help = null;
      naming = null;
      const summary = lists.find((candidate) => candidate.id === list);
      if (summary) applyList({ query: summary.needs_attention ? null : summary.description, label: summary.label });
      return true;
    }
    if (help || naming) return false;
    const row = sheet?.row ?? session.current;
    if (!row) return false;
    const id = actionKeys[kindOfRow(row)]?.[chord];
    if (!id) return false;
    const item = itemsFor(row).find((candidate) => candidate.id === id);
    if (!item) return false;
    event.preventDefault();
    // A flow and the link need the sheet under them.
    if (!sheet || sheet.rowId !== row.id) sheet = { rowId: row.id, row, title: nameOf(row), selected: 0, relation: 0, level: "root", flow: null, filter: "" };
    void item.run();
    return true;
  }

  /** The panel's keys: every press is shown; a full chord is kept in the panel, Enter or OK keeps it for good, Backspace clears it, Esc leaves things as they were. */
  function bindKey(event: KeyboardEvent, current: Sheet): void {
    held = heldOf(event);
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      held = [];
      sheet = { ...current, level: "root", chord: null };
      return;
    }
    if (event.key === "Enter" && !event.metaKey && !event.ctrlKey && !event.altKey) {
      event.preventDefault();
      held = [];
      confirmBind(current);
      return;
    }
    if (event.key === "Backspace" && !event.metaKey && !event.ctrlKey && !event.altKey) {
      event.preventDefault();
      sheet = { ...current, chord: null };
      return;
    }
    event.preventDefault();
    const chord = chordOf(event);
    if (!chord) return;
    if (RESERVED.has(chord)) {
      push({ text: `${chord} is the launcher's own`, tone: "err" });
      return;
    }
    sheet = { ...current, chord };
  }

  /** ⌘S: the box takes the list's name; Enter saves. `null` when not naming. */
  let naming = $state<string | null>(null);

  function namingKey(event: KeyboardEvent, name: string): void {
    if (event.key === "Enter") {
      event.preventDefault();
      const label = name.trim();
      naming = null;
      if (label === "") return;
      void session.saveCurrent(label).then(() => push({ text: `Saved as ${label}` }));
    } else if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      naming = null;
    }
  }

  function sheetKey(event: KeyboardEvent, current: Sheet) {
    if (current.level === "bind" || current.level === "bind-list") {
      bindKey(event, current);
      return;
    }
    if (current.level === "root" && chordOf(event) === "⌘B" && shown[current.selected] && !shown[current.selected]?.link) {
      event.preventDefault();
      sheet = { ...current, level: "bind", chord: chordFor(kindOfRow(current.row), shown[current.selected]!.id) };
      return;
    }
    if (current.level === "rename") {
      if (event.key === "Enter") {
        event.preventDefault();
        if (listId !== null) void renameList(listId, current.filter);
      } else if (event.key === "Escape") {
        event.preventDefault();
        event.stopPropagation();
        sheet = { ...current, level: "root", filter: "" };
      }
      return;
    }
    const items = shown;
    switch (event.key) {
      case "ArrowDown":
        event.preventDefault();
        current.selected = Math.min(items.length - 1, current.selected + 1);
        return;
      case "ArrowUp":
        event.preventDefault();
        current.selected = Math.max(0, current.selected - 1);
        return;
      case "ArrowRight":
      case "ArrowLeft":
        if (items[current.selected]?.link) {
          event.preventDefault();
          const stepBy = event.key === "ArrowRight" ? 1 : -1;
          current.relation = (current.relation + stepBy + RELATIONS.length) % RELATIONS.length;
        }
        return;
      case "Enter": {
        event.preventDefault();
        const chosen = items[current.selected];
        if (!chosen) return;
        if (current.level === "flow" && current.flow) {
          void chosen.run();
          pick(current.flow, chosen.id);
        } else {
          void chosen.run();
        }
        return;
      }
      case "Tab":
        event.preventDefault();
        leaveSheet();
        return;
      case "Escape":
        // Never the dialog's: a flow goes back to the row's actions, the actions back to the search.
        event.preventDefault();
        event.stopPropagation();
        if (current.level === "flow") sheet = { ...current, level: "root", flow: null, filter: "", selected: 0 };
        else leaveSheet();
        return;
      case "Backspace":
        // An empty box and one more Backspace: a step back, or back to the results with the words still there.
        if (current.filter === "") {
          event.preventDefault();
          if (current.level === "flow") sheet = { ...current, level: "root", flow: null, selected: 0 };
          else leaveSheet();
        }
        return;
      default:
    }
  }

  function onkeydown(event: KeyboardEvent) {
    if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "d" && !event.shiftKey && !event.altKey) {
      event.preventDefault();
      setDeep(!deep);
      return;
    }
    if (naming !== null) {
      namingKey(event, naming);
      return;
    }
    if (sheet?.level === "bind" || sheet?.level === "bind-list") {
      bindKey(event, sheet);
      return;
    }
    if (helpChord(event)) return;
    if (saveKey(event)) return;
    if (chordKey(event)) return;
    if (help) {
      helpKey(event, help);
      return;
    }
    if (sheet) {
      sheetKey(event, sheet);
      return;
    }
    switch (event.key) {
      case "ArrowDown":
        event.preventDefault();
        session.move(1);
        return;
      case "ArrowUp":
        event.preventDefault();
        session.move(-1);
        return;
      case "Enter": {
        event.preventDefault();
        const row = session.current;
        if (row) activate(row);
        return;
      }
      case "Tab":
        event.preventDefault();
        openSheet();
        return;
      case "Escape":
        event.preventDefault();
        event.stopPropagation();
        if (session.raw !== "") session.set("");
        else close();
        return;
      default:
    }
  }

  /** The highlighted word in front of the box: what it is asking for. Nothing while it is the search. */
  const mode = $derived(
    naming !== null
      ? "Name"
      : help
        ? "Filter"
        : sheet
          ? sheet.level === "bind" || sheet.level === "bind-list"
            ? "Keys"
            : sheet.level === "rename"
              ? "New name"
              : sheet.level === "flow"
                ? (step?.title ?? "Pick")
                : "Actions"
          : null,
  );
  const placeholder = $derived(
    naming !== null
      ? "the list's name"
      : help
        ? "narrow the filters"
        : sheet
          ? sheet.level === "bind"
            ? `press the keys for ${shown[sheet.selected]?.label ?? "the action"}`
            : sheet.level === "bind-list"
              ? "press the keys that open this list"
              : sheet.level === "rename"
                ? ""
                : sheet.level === "flow"
                  ? "narrow the choices"
                  : `narrow the actions on ${sheet.title}`
          : "⌘? for help",
  );
  const interpreted = $derived(session.response?.interpreted ?? null);
  const pinnedHit = $derived.by(() => {
    if (!sheet) return null;
    if (sheet.row.kind === "hit") return sheet.row.hit;
    if (sheet.row.kind === "recent") return sheet.row.row;
    return null;
  });
  /** What the footer says Tab would open: the word alone, as asked, once a row with actions is under the cursor. */
  const tabHint = $derived.by(() => {
    const row = session.current;
    return row !== null && !sheet && !help && itemsFor(row).length > 0;
  });
</script>

{#if open}
  <TfDialog title="Search and actions" className={deep ? "tf-launch tf-launch-deep" : "tf-launch"} onclose={close}>
    {#snippet body()}
      <!-- The label is its own element in front of the box: highlighted, never part of the text, never deletable. -->
      <div class="boxrow" class:labelled={mode !== null}>
        {#if mode !== null}<span class="mode">{mode}</span>{/if}
      <QueryBox
        bind:this={box}
        value={naming !== null ? naming : help ? help.filter : sheet ? sheet.filter : session.raw}
        {placeholder}
        pending={session.pending}
        footnote=""
        controls={controlled.list}
        active={controlled.option}
        oninput={(value) => {
          if (naming !== null) naming = value;
          else if (help) help = { filter: value, selected: 0 };
          else if (sheet) sheet = { ...sheet, filter: value, selected: 0 };
          else session.type(value);
        }}
        {onkeydown}
        onclose={close}
      />
        <button type="button" class="scope" class:on={deep} onclick={() => setDeep(!deep)} title="⌘D">
          {deep ? "Everything" : "Titles"}
        </button>
      </div>

      {#if sheet}
        <!-- The row the sheet acts on, lifted under the box; the box keeps its words aside. -->
        <div class="pinned">
          {#if pinnedHit}
            {@const cell = cellOf(pinnedHit.kind, pinnedHit.entity_id, pinnedHit.title, pinnedHit.path, sheet.row.kind === "hit" ? sheet.row.group.label : kindSingular(pinnedHit.kind))}
            <RowNext
              monogram={sheet.row.kind === "hit" ? sheet.row.group.monogram : kindMonogram(pinnedHit.kind)}
              tone={pinnedHit.state?.tone ?? null}
              kindName={kindSingular(pinnedHit.kind)}
              cell={cell.cell}
              mono={cell.mono}
              title={cell.title}
              id={`${uid}-pinned`}
              selected={false}
              onopen={() => {}}
              onhover={() => {}}
            />
          {:else}
            <p class="lead">{sheet.title}</p>
          {/if}
        </div>
        {#if sheet.level === "rename"}
          <p class="tf-ln-none">Type the new name in the box. Enter keeps it, Esc leaves the name as it was.</p>
        {:else if sheet.level === "bind" || sheet.level === "bind-list"}
          {@const current = sheet}
          <div class="bindpanel" role="group" aria-label="Keys">
            <p class="bindwhat">
              {#if current.level === "bind-list"}
                The keys that open <strong>{current.title}</strong> from anywhere
              {:else}
                The keys that run <strong>{shown[current.selected]?.label ?? ""}</strong> on any {kindOfRow(current.row) === "pr" ? "pull request" : kindOfRow(current.row)} you have selected
              {/if}
            </p>
            <div class="bindkeys" aria-live="polite">
              {#if current.chord}
                {#each chipsOf(current.chord) as chip, i (i)}<kbd class="bigkey">{chip}</kbd>{/each}
              {:else if held.length > 0}
                {#each held as chip, i (i)}<kbd class="bigkey dim">{chip}</kbd>{/each}
                <span class="bindhint">… and a letter or digit</span>
              {:else}
                <span class="bindhint">Press a modifier and a letter or digit, such as ⌘⇧O</span>
              {/if}
            </div>
            <div class="bindbtns">
              <button type="button" class="tf-btn tf-primary" onclick={() => confirmBind(current)}>{current.chord ? "OK" : "Remove the key"}</button>
              <button type="button" class="tf-btn" onclick={() => { sheet = { ...current, chord: null }; box?.focus(); }}>Clear</button>
              <button type="button" class="tf-btn" onclick={() => { sheet = { ...current, level: "root", chord: null }; box?.focus(); }}>Cancel</button>
            </div>
            <p class="bindhint">Enter keeps the keys, Backspace clears them, Esc leaves things as they were.</p>
          </div>
        {:else}
          <!-- Keys from a clicked chip land here first, so Esc never reaches the dialog; a click gives the box its focus back. -->
          <div id={sheetId} class="sheet" role="listbox" tabindex="-1" aria-label="Actions on {sheet.title}" {onkeydown} onclick={() => box?.focus()}>
            {#if sheet.level === "flow" && sheet.flow}
              <div class="tf-ln-h">
                <span class="tf-ln-lab">{step?.title ?? ""}</span>
                {#if sheet.flow.picks.length > 0}<span class="tf-ln-n">{sheet.flow.picks.join(" → ")}</span>{/if}
              </div>
              {#if sheet.flow.choices === null}
                <p class="tf-ln-none">Reading…</p>
              {/if}
            {/if}
            {#each shown as item, index (item.id)}
              {#if index > 0 && sheet.level === "root" && sheet.filter === "" && (item.id === "copy" || ["link", "set", "timer"].includes(shown[index - 1]?.id ?? "") !== ["link", "set", "timer"].includes(item.id))}
                <div class="cut"></div>
              {/if}
              {#if item.link}
                <!-- Not a button: the relation chips inside it are buttons of their own. -->
                <div
                  id={sheetOptionId(index)}
                  class="tf-ln-opt linkrow"
                  class:tf-ln-on={index === sheet.selected}
                  role="option"
                  tabindex="-1"
                  aria-selected={index === sheet.selected}
                  onmouseenter={() => sheet && (sheet.selected = index)}
                >
                  <button type="button" class="tf-ln-nm linkbtn" onclick={() => void item.run()}>Link to <span class="job">{openEntity?.label}</span></button>
                  <span class="rel" role="radiogroup" aria-label="Relation">
                    {#each RELATIONS as relation, r (relation.id)}
                      <button
                        type="button"
                        class:pick={r === sheet.relation}
                        role="radio"
                        aria-checked={r === sheet.relation}
                        tabindex="-1"
                        onclick={() => { if (sheet) sheet.relation = r; }}
                      >{relation.label}</button>
                    {/each}
                  </span>
                </div>
              {:else}
                <button
                  id={sheetOptionId(index)}
                  class="tf-ln-opt"
                  class:tf-ln-on={index === sheet.selected}
                  role="option"
                  aria-selected={index === sheet.selected}
                  onclick={() => {
                    if (sheet?.level === "flow" && sheet.flow) {
                      void item.run();
                      pick(sheet.flow, item.id);
                    } else {
                      void item.run();
                    }
                  }}
                  onmouseenter={() => sheet && (sheet.selected = index)}
                >
                  <span class="tf-ln-nm">{item.label}</span>
                  <span class="tf-ln-s">{item.detail}{#if sheet.level === "root" && chordFor(kindOfRow(sheet.row), item.id)}{" "}<kbd class="bound">{chordFor(kindOfRow(sheet.row), item.id)}</kbd>{/if}</span>
                </button>
              {/if}
            {/each}
            {#if shown.length === 0 && !(sheet.level === "flow" && sheet.flow?.choices === null)}
              <p class="tf-ln-none">{sheet.filter === "" ? "Nothing to pick here." : `Nothing matches “${sheet.filter}”.`}</p>
            {/if}
          </div>
        {/if}
      {:else if help}
        <div id={helpId} class="tf-ln-list" role="listbox" tabindex="-1" aria-label="Filters" {onkeydown} onclick={() => box?.focus()}>
          <div class="tf-ln-h"><span class="tf-ln-lab">Filters</span><span class="tf-ln-n">Enter adds one to the search</span></div>
          <div class="tf-ln-help">
            {#each helpShown as entry, i (entry.token)}
              <button
                id={helpOptionId(i)}
                class="tf-ln-opt tf-ln-syn"
                class:tf-ln-on={i === help.selected}
                role="option"
                aria-selected={i === help.selected}
                onclick={() => { help = null; session.set(withFilter(session.raw, entry.insert)); box?.focus(); }}
                onmouseenter={() => help && (help.selected = i)}
              >
                <span class="tf-ln-tok">{entry.token}</span>
                <span class="tf-ln-s">{entry.summary}</span>
              </button>
            {/each}
            {#if helpShown.length === 0}
              <p class="tf-ln-none">No filter matches “{help.filter}”.</p>
            {/if}
          </div>
          <div class="tf-ln-h"><span class="tf-ln-lab">Colours</span><span class="tf-ln-n">what a monogram's colour says</span></div>
          <div class="legend">
            {#each LEGEND as entry (entry.kind)}
              <div class="legendrow">
                <span class="legendkind">{entry.kind}</span>
                {#each entry.tones as [tone, meaning] (tone)}
                  <span class="legenditem"><span class="mg toned" data-tone={tone}>{entry.monogram}</span>{meaning}</span>
                {/each}
              </div>
            {/each}
          </div>
          <div class="keys">
            {#each KEYS as [key, what] (key)}
              <span><kbd>{key}</kbd> {what}</span>
            {/each}
          </div>
        </div>
      {:else}
        {#if interpreted}
          <Chips {interpreted} sources={health} sourcesRead={sourcesPhase === "ready"} />
        {/if}

        <div class="split" class:two={deep && session.mode === "results"}>
        <div id={resultsId} class="tf-ln-list" role="listbox" tabindex="-1" aria-label="Results">
          {#if session.error}
            <p class="tf-ln-none tf-ln-err">{session.error}</p>
            {#if !session.home}
              <button class="tf-ln-opt" onclick={() => void session.loadHome()}>
                <span class="tf-ln-nm">Retry</span>
                <span class="tf-ln-s">Read the board again.</span>
              </button>
            {/if}
          {:else if session.mode === "board"}
            {#if session.home}
              <BoardNext
                home={session.home}
                favourites={favourites.length}
                sources={health}
                rows={session.rows}
                selected={session.selected}
                {optionId}
                {now}
                onopen={activate}
                onhover={(index) => (session.selected = index)}
              />
            {:else}
              <p class="tf-ln-none">Reading the mirror…</p>
            {/if}
          {:else if session.mode === "actions"}
            <div class="tf-ln-h"><span class="tf-ln-lab">Go to</span></div>
            {#each session.rows as row, i (row.id)}
              {#if row.kind === "action"}
                <button
                  id={optionId(i)}
                  class="tf-ln-opt"
                  class:tf-ln-on={i === session.selected}
                  role="option"
                  aria-selected={i === session.selected}
                  onclick={() => activate(row)}
                  onmouseenter={() => (session.selected = i)}
                >
                  <span class="tf-ln-nm">{row.action.label}</span>
                  <span class="tf-ln-s">{row.action.detail}</span>
                </button>
              {/if}
            {/each}
            {#if session.rows.length === 0}
              <p class="tf-ln-none">No action matches.</p>
            {/if}
          {:else if session.response && session.response.groups.length === 0 && /(^|\s)#[li](\s|$)/i.test(session.raw) && session.response.interpreted.filters.kinds.length === 0}
            <p class="tf-ln-none">Nothing in your lists or the inbox matches.</p>
          {:else if session.response}
            <ResultsNext
              response={session.response}
              rows={session.rows}
              selected={session.selected}
              {optionId}
              onopen={activate}
              onhover={(index) => (session.selected = index)}
            />
          {:else}
            <p class="tf-ln-none">Searching…</p>
          {/if}
        </div>
        {#if deep && session.mode === "results"}
          <aside class="preview" aria-label="Where it matches">
            {#if preview}
              <p class="previewhead">{preview.title}</p>
              {#if preview.loading && preview.excerpts.length === 0}
                <p class="tf-ln-none">Reading…</p>
              {:else if preview.excerpts.length === 0}
                <p class="tf-ln-none">Only the title holds the words.</p>
              {:else}
                {#each preview.excerpts as excerpt, i (i)}
                  <button type="button" class="excerpt" onclick={() => { const row = session.current; if (row) activate(row); }}>
                    {#if excerpt.where}<span class="where">{excerpt.where}</span>{/if}
                    <span class="text">{#each excerpt.segments as seg, j (j)}{#if seg.hit}<mark>{seg.text}</mark>{:else}{seg.text}{/if}{/each}</span>
                  </button>
                {/each}
              {/if}
            {:else}
              <p class="tf-ln-none">Select a row to see where the words appear.</p>
            {/if}
          </aside>
        {/if}
        </div>
      {/if}
    {/snippet}

    {#snippet footer()}
      {#if naming !== null}
        <span><kbd>↵</kbd> save the list</span>
        <span><kbd>Esc</kbd> keep searching</span>
      {:else if help}
        <span><kbd>↑↓</kbd> move</span>
        <span><kbd>↵</kbd> add the filter</span>
        <span><kbd>Esc</kbd> back to the search</span>
      {:else if sheet}
        <span><kbd>↑↓</kbd> move</span>
        {#if shown[sheet.selected]?.link}<span><kbd>←→</kbd> relation</span>{/if}
        <span><kbd>↵</kbd> {shown[sheet.selected]?.link ? "link" : sheet.level === "flow" ? "pick" : "do it"}</span>
        {#if sheet.level === "root" && !shown[sheet.selected]?.link}<span><kbd>⌘B</kbd> bind a key</span>{/if}
        <span><kbd>Esc</kbd> {sheet.level === "root" ? "back to the search" : "back"}</span>
        <span class="tf-spacer"></span>
        <span class="tf-ln-took">{sheet.title}{#if sheet.level === "flow"} · {step?.title ?? ""}{:else} · {counted(shown.length, "action", "actions")}{/if}</span>
      {:else}
        <span><kbd>↑↓</kbd> move</span>
        <span><kbd>↵</kbd> open</span>
        {#if tabHint}
          <span><kbd>Tab</kbd> actions</span>
        {/if}
        {#if session.canSave}
          <span><kbd>⌘S</kbd> {session.savedId === null ? "save as list" : "saved as list"}</span>
        {/if}
        <span><kbd>⌘?</kbd> help</span>
        <span><kbd>⌘D</kbd> {deep ? "titles only" : "everything"}</span>
        <span><kbd>Esc</kbd> back one step</span>
        <span class="tf-spacer"></span>
      {/if}
    {/snippet}
  </TfDialog>
{/if}

<style>
  /* PROTOTYPE rules. The shipped ones are `tf/css/launcher.css`; these would join them. */
  :global(.tf-launch .tf-ln-h) { padding-top: 10px; }
  /* The box's own ⌘K badge is gone (Björn, round 4); `QueryBox.svelte` would lose the element. */
  :global(.tf-launch .tf-ln-q > kbd) { display: none; }
  /* The pages' own search fields (Docs, Builds): the one box, with the page's filter, is the way in. */
  :global(.tf main label.tf-fsearch) { display: none; }
  /* Everything on: the dialog grows and the body splits, results at the left, the preview at the right. */
  :global(.tf-dialog.tf-launch-deep) { width: min(1280px, calc(100vw - 32px)); height: min(840px, 92vh); }
  .scope {
    flex: none; margin-right: 12px; padding: 3px 10px; border: 1px solid var(--rule); border-radius: 4px;
    font-size: var(--t-xs); font-weight: 600; color: var(--ink-2); background: var(--sheet);
  }
  .scope:hover { color: var(--ink); border-color: var(--ink-2); }
  .scope.on { background: var(--ink); color: var(--sheet); border-color: var(--ink); }
  .split { flex: 1; min-height: 0; display: flex; }
  .split > :global(.tf-ln-list) { flex: 1; min-width: 0; }
  .split.two > :global(.tf-ln-list) { flex: 0 0 46%; border-right: 1px solid var(--rule); }
  .preview { flex: 1; min-width: 0; overflow: auto; padding: 8px 0 12px; }
  .previewhead { padding: 8px 18px 6px; font-size: var(--t-md); font-weight: 600; color: var(--ink); }
  .excerpt {
    display: block; width: 100%; text-align: left; padding: 7px 18px; font-size: var(--t-sm); line-height: 1.5; color: var(--ink-2);
    border-bottom: 1px solid var(--rule-2);
  }
  .excerpt:hover { background: var(--sheet-2); }
  .excerpt .where { display: block; font-size: var(--t-2xs); color: var(--ink-2); margin-bottom: 2px; }
  .excerpt .text { color: var(--ink); white-space: normal; }
  .excerpt mark { background: var(--mark); color: var(--ink); border-radius: 3px; padding: 0 1px; }
  .boxrow { display: flex; align-items: center; flex: none; border-bottom: 1px solid var(--rule); }
  .boxrow :global(.tf-ln-q) { flex: 1; min-width: 0; border-bottom: 0; }
  .mode {
    flex: none; margin-left: 15px; padding: 2px 8px; border-radius: 4px; background: var(--ink); color: var(--sheet);
    font-size: var(--t-sm); font-weight: 600; line-height: 1.4; user-select: none;
  }
  .labelled :global(.tf-ln-q input) { padding-left: 10px; }
  :global(.tf-launch .tf-ln-caret) { display: none; }
  .boxrow :global(.tf-ln-q input) { padding-left: 18px; }
  .legend { display: flex; flex-direction: column; gap: 6px; padding: 4px 18px 10px; }
  .legendrow { display: flex; align-items: center; gap: 14px; font-size: var(--t-xs); color: var(--ink-2); }
  .legendkind { width: 100px; color: var(--ink); }
  .legenditem { display: inline-flex; align-items: center; gap: 6px; }
  .mg {
    display: inline-flex; align-items: center; justify-content: center; width: 28px; height: 18px; border: 1px solid var(--rule);
    border-radius: 3px; font-size: var(--t-2xs); font-weight: 600; letter-spacing: .04em; color: var(--ink-2);
  }
  .mg.toned[data-tone="ok"] { color: var(--ok); background: var(--ok-bg); border-color: transparent; }
  .mg.toned[data-tone="err"] { color: var(--err); background: var(--err-bg); border-color: transparent; }
  .mg.toned[data-tone="info"] { color: var(--info); background: var(--info-bg); border-color: transparent; }
  .mg.toned[data-tone="muted"] { color: var(--muted); background: var(--muted-bg); border-color: transparent; }
  .pinned { flex: none; border-bottom: 1px solid var(--rule); }
  .lead { padding: 10px 18px; font-size: var(--t-sm); color: var(--ink); }
  .sheet { flex: 1; min-height: 0; overflow: auto; padding: 0 0 4px; outline: none; }
  .cut { height: 1px; background: var(--rule); margin: 4px 0; }
  .job {
    display: inline-block; background: var(--card); color: var(--card-ink);
    font-family: var(--mono); font-variant-ligatures: none; font-size: var(--t-xs); font-weight: 600;
    padding: 0 6px; border-radius: 3px;
  }
  .linkrow { cursor: default; }
  .linkbtn { text-align: left; padding: 0; cursor: pointer; }
  .rel { display: flex; flex-wrap: wrap; gap: 4px; align-items: center; }
  .rel button {
    font-size: var(--t-xs); padding: 1px 8px; border: 1px solid var(--rule); border-radius: 3px; color: var(--ink-2);
    cursor: pointer;
  }
  .rel button:hover { border-color: var(--ink-2); color: var(--ink); }
  .rel .pick, .rel .pick:hover { background: var(--ink); color: var(--sheet); border-color: var(--ink); }
  .bindpanel { display: flex; flex-direction: column; align-items: center; gap: 14px; padding: 28px 18px; }
  .bindwhat { font-size: var(--t-sm); color: var(--ink-2); text-align: center; }
  .bindwhat strong { color: var(--ink); }
  .bindkeys { display: flex; align-items: center; gap: 8px; min-height: 48px; }
  .bigkey {
    display: inline-flex; align-items: center; justify-content: center; min-width: 44px; height: 44px; padding: 0 12px;
    border: 1px solid var(--rule); border-bottom-width: 3px; border-radius: 6px; background: var(--sheet-2);
    font: 600 var(--t-2xl)/1 var(--sans); color: var(--ink);
  }
  .bigkey.dim { color: var(--ink-2); }
  .bindhint { font-size: var(--t-xs); color: var(--ink-2); }
  .bindbtns { display: flex; gap: var(--control-gap); }
  .bound { font-size: var(--t-2xs); padding: 0 4px; border: 1px solid var(--rule); border-radius: 3px; color: var(--ink); margin-left: 4px; }
  .keys { display: flex; flex-wrap: wrap; gap: 6px 14px; padding: 10px 18px; margin-top: 6px; border-top: 1px solid var(--rule); font-size: var(--t-xs); color: var(--ink-2); }
</style>
