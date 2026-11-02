<!--
  PROTOTYPE: the ⌘K launcher rethought (clickdummy, 2026-11-02). Not production.

  One question: does "⌘K again opens the selected item's actions" feel right,
  and do findable views and lists, `d ` and `l `, and an inline link close the
  gaps Björn named? Mounted by `App.svelte` instead of `Launcher.svelte` under
  `?launcher-next` (dev only); everything else in the shell is the real thing.

  Round 2 (Björn, 2026-11-02): `d ` for pages and `l ` for saved lists, one
  letter and a space like `t `; no mirror footnote; no lists block on the
  board (lists are `l `, renamed and deleted from the sheet); Esc in the sheet
  closes the sheet, never the dialog; typing in the sheet filters its actions;
  ⌘S saves the query as a list.

  What is real here: `TfDialog`, `QueryBox`, `Chips`, `Results`, `Row`,
  `Help`, the `Session`, the `>` palette, the `?` card, and every write
  (`create_link`, `save_link_set`, the list writes) goes through the real IPC.
  What is a shortcut, each marked PROTOTYPE below:
    - `d ` is rewritten to `kind:page` and `l ` is answered here from the
      board's saved lists (ruling P2 says the backend parses; the real change
      is two prefixes in `query.rs`);
    - the Views and Sets groups are built here from `ticket_views` and
      `link_sets` (the real change indexes both in `knobas-search`);
    - the "Search Docs for …" row stores its words and opens `#/docs`; the
      Docs page does not read them yet;
    - the sheet's actions are a fixed list per kind, not adapter-declared;
    - `BoardNext.svelte` is `Board.svelte` without the lists block.

  Round 3 (Björn, 2026-11-02): no `/` for a source and no sources strip on
  the board; one-letter prefixes for the kinds he reaches for (`p r b m d i
  l`, `t` as shipped); actions per kind that are real addresses or real
  writes of the app (a PR's Files changed, a repo's compare page for a new
  pull request, re-run, approve, ack, snooze, done, open the checkout,
  clone); ⌘K while open gives the box its focus back when a click took it.

  Screenshots: `?launcher-open=sepa` opens with that query, `&launcher-sheet=1`
  opens the sheet on the row at that index once results land.
-->
<script lang="ts">
  import { onMount, tick } from "svelte";

  import {
    ackAlert,
    cloneRepo,
    completeInboxItem,
    createLink,
    createSmartList as defaultCreateSmartList,
    deleteSmartList as defaultDeleteSmartList,
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
  import type { InboxItem } from "../../ipc/entity";
  import { pickedMorning, tomorrowValue } from "../../inbox/inbox.svelte";
  import { prHash } from "../../tf/pr/pr";
  import { repoHash } from "../../tf/repo/repo";
  import { clockLabel } from "../../time/format";
  import type { LauncherHome, ResultGroup, SearchHit, SearchQuery, SearchResponse, SmartListSummary, TicketViewRow } from "../../ipc/search";
  import type { LinkSetMemberKind, LinkSetRow } from "../../ipc/entity";
  import type { CredentialHealth, SourceSyncStatus } from "../../ipc/sources";
  import { drawn } from "../../detail/relations";
  import type { HealthPhase } from "../../shell/health.svelte";
  import { canBeTarget } from "../../shell/timer";
  import { push } from "../../shell/toasts.svelte";
  import { entityAddress } from "../../tf/address";
  import { counted } from "../../tf/counted";
  import { opener, sentence } from "../../tf/inbox/inbox";
  import type { LinkTarget } from "../../tf/job/current.svelte";
  import { jobHash } from "../../tf/route";
  import TfDialog from "../../tf/settings/TfDialog.svelte";
  import { keyOf } from "../../time/format";
  import Chips from "../Chips.svelte";
  import Help from "../Help.svelte";
  import QueryBox from "../QueryBox.svelte";
  import Results from "../Results.svelte";
  import Row from "../Row.svelte";
  import { type LauncherAction, filterActions, navigationActions } from "../actions";
  import type { InboxPreview, LauncherRow, TodayTime } from "../rows";
  import { Session, type SessionPorts } from "../session.svelte";
  import BoardNext from "./BoardNext.svelte";

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
    /** Unused: the sheet writes the link itself, which is the thing being tried. Declared so `App.svelte` mounts both launchers alike. */
    onlink?: ((target: LinkTarget) => void) | undefined;
    ontimer: ((targetId: string, targetTitle: string) => void) | undefined;
    ports?: Partial<SessionPorts>;
    now?: Date | undefined;
  } = $props();
  // svelte-ignore state_referenced_locally
  void onlink;
  // Unused since round 3: the board draws no source strip. Kept so `App.svelte` mounts both launchers alike.
  // svelte-ignore state_referenced_locally
  void runs;

  // ---- PROTOTYPE: the local corpus the backend does not index yet ----------

  let views: TicketViewRow[] = [];
  let sets = $state<LinkSetRow[]>([]);
  /** The saved lists, as the board read them; the board itself no longer draws them. */
  let lists = $state<SmartListSummary[]>([]);
  let listsLoaded = false;

  async function loadLocal() {
    try {
      [views, sets] = await Promise.all([ticketViews(), linkSets()]);
    } catch {
      views = [];
      sets = [];
    }
  }

  // svelte-ignore state_referenced_locally
  const realHome = ports?.launcherHome ?? defaultHome;

  /** The board without lists: the saved ones are kept aside for `l `, the built-ins are gone. */
  async function homeNext(): Promise<LauncherHome> {
    const home = await realHome();
    lists = home.smart_lists.filter((list) => list.saved);
    listsLoaded = true;
    return { ...home, smart_lists: [] };
  }

  const BLANK = { updated_at: null, state: null, rank: 1, snippet: [] as SearchHit["snippet"] };

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

  /** `l ⟨text⟩`: the saved lists whose name or description holds the text; `l` alone is all of them. */
  function listsAnswer(text: string, at: string): SearchResponse {
    const needle = text.toLowerCase();
    const hits: SearchHit[] = lists
      .filter((list) => `${list.label} ${list.description}`.toLowerCase().includes(needle))
      .map((list) => ({
        ...BLANK,
        entity_id: `list:${list.id}`,
        kind: "list",
        source_id: "local",
        title: list.label,
        synced_at: at,
        path: list.needs_attention ? `${list.description} · needs attention` : `${list.description} · ${counted(list.count, "item", "items")}`,
      }));
    return {
      interpreted: { text, prefix: "list", filters: { sources: [], kinds: [], updated_within_days: null, mine: false, authors: [] }, unknown_tokens: [] },
      groups: hits.length === 0 ? [] : [{ kind: "list", label: "Saved list", plural: "Saved lists", monogram: "LI", total: hits.length, hits }],
      total: hits.length,
      took_ms: 0,
      coverage: [],
    };
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

  /**
   * The one-letter prefixes, `t ` style: a letter, a space, the words. Five
   * are a kind the backend already filters on; `l` and `i` are answered here.
   */
  const KIND_OF: Record<string, string> = { d: "page", r: "repo", p: "pr", b: "build", m: "monitor" };
  const KINDED = /^([drpbm])(?:\s+|$)/i;
  const LISTS = /^l(?:\s+|$)/i;
  const INBOX = /^i(?:\s+|$)/i;

  /** `i ⟨text⟩`: the inbox's items whose sentence holds the text, newest first; `i` alone is all of them. */
  function inboxAnswer(text: string, at: string): SearchResponse {
    const needle = text.toLowerCase();
    const items = inbox?.items ?? [];
    // The row's key column is the line's time, as the board shows it; the item's own key is kept aside.
    inboxKeys.clear();
    const seen = new Map<string, number>();
    const hits: SearchHit[] = items
      .filter((item) => `${sentence(item)} ${item.reason}`.toLowerCase().includes(needle))
      .map((item) => {
        const when = clockLabel(item.occurred_at, now ?? new Date(), true);
        const n = (seen.get(when) ?? 0) + 1;
        seen.set(when, n);
        const id = `inbox:${when}${n > 1 ? ` (${n})` : ""}`;
        inboxKeys.set(id, item.key);
        return { ...BLANK, entity_id: id, kind: "inbox", source_id: item.source_id, title: sentence(item), synced_at: at, path: item.reason };
      });
    return {
      interpreted: { text, prefix: null, filters: { sources: [], kinds: [], updated_within_days: null, mine: false, authors: [] }, unknown_tokens: [] },
      groups: hits.length === 0 ? [] : [{ kind: "inbox", label: "Inbox item", plural: "Inbox", monogram: "IN", total: hits.length, hits }],
      total: hits.length,
      took_ms: 0,
      coverage: [],
    };
  }

  /** Which inbox item a row stands for, by the row's id. */
  const inboxKeys = new Map<string, string>();

  function inboxItemOf(row: LauncherRow): InboxItem | null {
    if (row.kind !== "hit" || row.hit.kind !== "inbox") return null;
    const key = inboxKeys.get(row.hit.entity_id);
    return key === undefined ? null : (inbox?.items.find((item) => item.key === key) ?? null);
  }

  /** The search port with the prototype's additions layered over the real answer. */
  async function searchNext(query: SearchQuery): Promise<SearchResponse> {
    const raw = query.raw.trim();
    const at = (now ?? new Date()).toISOString();
    // PROTOTYPE: the frontend reads grammar here, against ruling P2.
    if (LISTS.test(raw)) {
      // The board's read may still be in flight on the first keystroke.
      if (!listsLoaded) await homeNext();
      return listsAnswer(raw.replace(LISTS, "").trim(), at);
    }
    if (INBOX.test(raw)) return inboxAnswer(raw.replace(INBOX, "").trim(), at);
    const letter = KINDED.exec(raw)?.[1]?.toLowerCase();
    const kind = letter ? KIND_OF[letter] : undefined;
    const sent = kind ? { ...query, raw: raw.replace(KINDED, `kind:${kind} `) } : query;
    const response = await realSearch(sent);
    if (kind === "page") return docsRow(response, at);
    if (response.interpreted.prefix !== null) return response;
    const needle = response.interpreted.text.trim().toLowerCase();
    if (needle === "") return response;
    const local = [viewGroup(needle, at), setGroup(needle, at)].filter((group): group is ResultGroup => group !== null);
    if (local.length === 0) return response;
    return { ...response, groups: [...local, ...response.groups], total: response.total + local.reduce((n, group) => n + group.total, 0) };
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

  // ---- the sheet: what the second ⌘K opens -------------------------------

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

  interface Sheet {
    rowId: string;
    row: LauncherRow;
    title: string;
    selected: number;
    relation: number;
    /** `sets` picks a set; `rename` holds the field for a saved list's new name. */
    level: "root" | "sets" | "rename";
    /** What was typed since the sheet opened: the fuzzy filter over its actions. */
    filter: string;
    /** The rename field's starting text. */
    name: string;
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

  function nameOf(row: LauncherRow): string {
    const entity = entityOf(row);
    if (entity) return entity.kind === "ticket" ? keyOf(entity.entityId) : entity.title;
    if (row.kind === "hit") return row.hit.title;
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
    try {
      await saveLinkSet(draft);
      push({ text: member ? `Added ${entity.title} to ${set.name}` : `${set.name} now shows on ${keyOf(entity.entityId)}` });
      close();
    } catch (cause) {
      sheet = null;
      session.error = ipcErrorMessage(cause);
    }
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

  /** A saved list's rename, committed from the sheet's field: the lists are re-read and the `l ` answer redrawn. */
  async function renameList(id: string, name: string) {
    const label = name.trim();
    sheet = null;
    if (label === "") return;
    await session.renameList(id, label);
    await session.loadHome();
    session.set(session.raw);
    box?.focus();
  }

  async function deleteList(id: string) {
    sheet = null;
    await session.deleteList(id);
    await session.loadHome();
    session.set(session.raw);
    box?.focus();
  }

  /** The sheet's rows for `row`, in the order they are drawn. Empty: no sheet. */
  function itemsFor(row: LauncherRow): SheetItem[] {
    if (row.kind === "hit" && row.hit.kind === "list") {
      const id = keyOf(row.hit.entity_id);
      return [
        { id: "apply", label: "Apply", detail: "Its filters in the box, as Enter does", run: () => activate(row) },
        { id: "rename", label: "Rename", detail: "Give the list a new name", run: () => { if (sheet) sheet = { ...sheet, level: "rename", name: row.hit.title }; } },
        { id: "delete", label: "Delete", detail: "The saved query is gone; there is no undo", run: () => deleteList(id) },
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
        label: "Snooze until tomorrow 9:00",
        detail: "Back in the inbox then",
        run: () => doing(`Snoozed ${item.title}`, () => snoozeInboxItem(item.key, (pickedMorning(tomorrowValue(now ?? new Date()), now) ?? new Date()).toISOString())),
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
          run: () => { if (sheet) sheet = { ...sheet, level: "sets", selected: 0, filter: "" }; },
        });
      }
    }
    if (ontimer && canBeTarget({ entityId: id })) {
      const start = ontimer;
      items.push({ id: "timer", label: "Start timer", detail: "Stops the running one first", run: () => { start(id, entity.title); close(); } });
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
        items.push({ id: "editor", label: "Open in editor", detail: "The checkout, at this pull request's branch", run: () => doing(`Opened ${entity.title} in the editor`, () => openCheckout(id)) });
        break;
      case "repo":
        items.push({ id: "newpr", label: "Create pull request", detail: "The compare page: pick the branch, open the request", run: go(repoHash(id, "compare")) });
        items.push({ id: "prs", label: "Pull requests", detail: "Open ones first", run: go(repoHash(id, "prs")) });
        items.push({ id: "branches", label: "Branches", detail: "Every branch and where it stands", run: go(repoHash(id, "branches")) });
        items.push({ id: "editor", label: "Open in editor", detail: "The checkout under the clones root", run: () => doing(`Opened ${entity.title} in the editor`, () => openCheckout(id)) });
        items.push({ id: "clone", label: "Clone", detail: "Into a new folder under the clones root", run: () => doing(`Cloning ${entity.title}`, () => cloneRepo(id)) });
        break;
      case "build":
        items.push({ id: "rerun", label: "Re-run", detail: "Queued as a write to the source", run: () => doing(`Re-run queued for ${entity.title}`, () => submitWrite({ RerunBuild: { entity: id } })) });
        break;
      case "monitor":
        items.push({ id: "ack", label: "Acknowledge alert", detail: "If one is open on it", run: () => doing(`Acknowledged ${entity.title}`, () => ackAlert(id)) });
        break;
      case "commit":
      case "branch":
        items.push({ id: "editor", label: "Open in editor", detail: "The checkout, at this point", run: () => doing(`Opened ${entity.title} in the editor`, () => openCheckout(id)) });
        break;
      default:
    }
    if (entity.kind === "ticket") items.push({ id: "copy", label: "Copy key", detail: key, run: () => void copy(key, `Copied ${key}`) });
    else items.push({ id: "copy", label: "Copy title", detail: entity.title, run: () => void copy(entity.title, "Copied the title") });
    return items;
  }

  const setItems = $derived.by((): SheetItem[] => {
    if (!sheet) return [];
    const entity = entityOf(sheet.row);
    if (!entity) return [];
    return sets.map((set) => ({ id: `set:${set.id}`, label: set.name, detail: set.description, run: () => putInSet(set, entity) }));
  });

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

  /** The first level, derived live: the chosen job and the sets can arrive after the sheet opened. */
  const rootItems = $derived(sheet ? itemsFor(sheet.row) : []);
  const shown = $derived(sheet ? fuzzy(sheet.level === "sets" ? setItems : rootItems, sheet.filter) : []);

  function openSheet() {
    const row = session.current;
    if (!row) return;
    if (itemsFor(row).length === 0) return;
    sheet = { rowId: row.id, row, title: nameOf(row), selected: 0, relation: 0, level: "root", filter: "", name: "" };
  }

  // The sheet belongs to one row: moving off it closes it.
  $effect(() => {
    if (sheet && session.current?.id !== sheet.rowId) sheet = null;
  });

  // ---- ids, scrolling, the board's extras, the `>` palette ---------------

  const uid = $props.id();
  const optionId = (index: number) => `${uid}-opt-${index}`;
  const sheetOptionId = (index: number) => `${uid}-sheet-${index}`;
  const resultsId = `${uid}-results`;
  const sheetId = `${uid}-sheet`;

  const activeOption = $derived.by(() => {
    const row = session.current;
    if (!row || session.error) return undefined;
    return optionId(session.selected);
  });
  const controlled = $derived(sheet ? { list: sheetId, option: sheetOptionId(sheet.selected) } : { list: resultsId, option: activeOption });

  $effect(() => {
    void session.rows;
    if (activeOption) document.getElementById(activeOption)?.scrollIntoView?.({ block: "nearest" });
  });
  $effect(() => {
    if (sheet) document.getElementById(sheetOptionId(sheet.selected))?.scrollIntoView?.({ block: "nearest" });
  });

  $effect.pre(() => {
    session.extras = { inbox: inbox ?? null, today: today ?? null };
  });

  $effect(() => {
    const typed = session.response?.interpreted.text ?? "";
    session.actions = filterActions([...navigationActions(), ...actions], typed);
  });

  $effect(() => {
    if (open) {
      void session.loadHome();
      void loadLocal();
      void tick().then(() => box?.focus({ select: true }));
    } else {
      sheet = null;
      session.dispose();
    }
  });

  // The inbox store seeds after the shell mounts: an `i` typed before that is asked again once it has.
  let inboxSeen = 0;
  $effect(() => {
    const count = inbox?.items.length ?? 0;
    if (count > 0 && inboxSeen === 0 && open && INBOX.test(session.raw.trim())) session.set(session.raw);
    inboxSeen = count;
  });

  // PROTOTYPE: the screenshot affordances.
  const params = new URLSearchParams(location.search);
  let autoSheet = params.has("launcher-sheet");
  $effect(() => {
    if (autoSheet && open && session.mode === "results" && session.response && !sheet) {
      autoSheet = false;
      session.selected = Number(params.get("launcher-sheet") || 0);
      openSheet();
    }
  });

  onMount(() => {
    function onkeydown(event: KeyboardEvent) {
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "k") {
        event.preventDefault();
        if (!open) {
          open = true;
          return;
        }
        // A click on a chip or a row took the focus: the first ⌘K only gives it back.
        if (!document.activeElement?.closest(".tf-ln-q")) {
          box?.focus();
          return;
        }
        if (sheet) {
          sheet = null;
        } else {
          openSheet();
        }
      }
    }
    window.addEventListener("keydown", onkeydown);
    const preset = params.get("launcher-open");
    if (preset !== null) {
      open = true;
      void tick().then(() => session.set(preset));
    }
    return () => {
      window.removeEventListener("keydown", onkeydown);
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
    // A saved list applies its filters in the box and keeps the reader here.
    if (row.kind === "hit" && row.hit.kind === "list") {
      sheet = null;
      session.set(`list:${keyOf(row.hit.entity_id)}`);
      box?.focus();
      return;
    }
    if (row.kind === "list") {
      if (row.list.needs_attention) return;
      session.set(`list:${row.list.id}`);
      box?.focus();
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

  /** ⌘S: the query becomes a saved list, from either panel. `true` when the key was taken. */
  function saveKey(event: KeyboardEvent): boolean {
    if (!(event.metaKey || event.ctrlKey) || event.key.toLowerCase() !== "s") return false;
    event.preventDefault();
    if (session.canSave && session.savedId === null) {
      const label = session.raw.trim();
      void session.saveCurrent(label).then(() => push({ text: `Saved “${label}” as a list` }));
    }
    return true;
  }

  function sheetKey(event: KeyboardEvent, current: Sheet) {
    if (current.level === "rename") {
      // The field answers its own keys; Esc here is the field's too, so it never reaches the dialog.
      if (event.key === "Escape") {
        event.preventDefault();
        event.stopPropagation();
        sheet = { ...current, level: "root" };
        box?.focus();
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
          const step = event.key === "ArrowRight" ? 1 : -1;
          current.relation = (current.relation + step + RELATIONS.length) % RELATIONS.length;
        }
        return;
      case "Enter":
        event.preventDefault();
        void items[current.selected]?.run();
        return;
      case "Tab":
        event.preventDefault();
        sheet = null;
        return;
      case "Escape":
        // One rung per press, and never the dialog's: the filter, then the level, then the sheet.
        event.preventDefault();
        event.stopPropagation();
        if (current.filter !== "") sheet = { ...current, filter: "", selected: 0 };
        else if (current.level === "sets") sheet = { ...current, level: "root", selected: Math.max(0, rootItems.findIndex((item) => item.id === "set")) };
        else sheet = null;
        box?.focus();
        return;
      case "Backspace":
        event.preventDefault();
        sheet = { ...current, filter: current.filter.slice(0, -1), selected: 0 };
        return;
      default:
        // A printable key narrows the actions; the box keeps its words.
        if (event.key.length === 1 && !event.metaKey && !event.ctrlKey && !event.altKey) {
          event.preventDefault();
          sheet = { ...current, filter: current.filter + event.key, selected: 0 };
        }
    }
  }

  function onkeydown(event: KeyboardEvent) {
    if (saveKey(event)) return;
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

  /** The rename field takes focus and selects the old name as it opens. */
  function takeFocus(node: HTMLInputElement) {
    node.focus();
    node.select();
  }

  const placeholder = "Search everything · ? lists the prefixes · > # @ p r b m d i l t";
  const interpreted = $derived(session.response?.interpreted ?? null);
  const sheetHint = $derived.by(() => {
    const row = session.current;
    if (!row || sheet) return null;
    return itemsFor(row).length > 0 ? nameOf(row) : null;
  });
  const pinned = $derived(sheet ? entityOf(sheet.row) : null);
  const pinnedHit = $derived.by(() => {
    if (!sheet) return null;
    if (sheet.row.kind === "hit") return sheet.row.hit;
    if (sheet.row.kind === "recent") return { ...sheet.row.row, snippet: [] };
    return null;
  });
  const listId = $derived(sheet?.row.kind === "hit" && sheet.row.hit.kind === "list" ? keyOf(sheet.row.hit.entity_id) : null);
</script>

{#if open}
  <TfDialog title="Search and actions" className="tf-launch" onclose={close}>
    {#snippet body()}
      <QueryBox
        bind:this={box}
        value={session.raw}
        {placeholder}
        pending={session.pending}
        footnote=""
        controls={controlled.list}
        active={controlled.option}
        oninput={(value) => { sheet = null; session.type(value); }}
        {onkeydown}
        onclose={close}
      />

      {#if sheet}
        <!-- The row the sheet acts on, lifted under the box; the box keeps its words. -->
        <div class="pinned">
          {#if pinnedHit}
            <Row
              entityId={pinnedHit.entity_id}
              kind={pinnedHit.kind}
              sourceId={pinnedHit.source_id}
              title={pinnedHit.title}
              syncedAt={pinnedHit.synced_at}
              path={pinnedHit.path}
              state={pinnedHit.state}
              sources={health}
              {now}
              id={`${uid}-pinned`}
              selected={false}
              onopen={() => {}}
              onhover={() => {}}
            />
          {:else}
            <p class="lead">{sheet.title}</p>
          {/if}
        </div>
        {#if sheet.level === "rename" && listId !== null}
          {@const id = listId}
          <div class="renamerow">
            <label class="tf-ln-s" for="{uid}-rename">New name</label>
            <input
              id="{uid}-rename"
              class="tf-ln-ren"
              type="text"
              use:takeFocus
              value={sheet.name}
              onkeydown={(event) => {
                if (event.key === "Enter") {
                  event.preventDefault();
                  event.stopPropagation();
                  void renameList(id, event.currentTarget.value);
                } else if (event.key === "Escape") {
                  event.preventDefault();
                  event.stopPropagation();
                  if (sheet) sheet = { ...sheet, level: "root" };
                  box?.focus();
                }
              }}
            />
          </div>
        {:else}
          <!-- Keys from a clicked chip land here first, so Esc never reaches the dialog; a click gives the box its focus back. -->
          <div id={sheetId} class="sheet" role="listbox" tabindex="-1" aria-label="Actions on {sheet.title}" {onkeydown} onclick={() => box?.focus()}>
            <p class="filter" class:empty={sheet.filter === ""}>
              {#if sheet.filter === ""}Type to narrow the actions{:else}{sheet.filter}<span class="caret"></span>{/if}
            </p>
            {#if sheet.level === "sets"}
              <div class="tf-ln-h"><span class="tf-ln-lab">{pinned && memberKindOf(pinned.kind) ? "Add to" : "Show on this ticket"}</span></div>
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
                  onclick={() => void item.run()}
                  onmouseenter={() => sheet && (sheet.selected = index)}
                >
                  <span class="tf-ln-nm">{item.label}</span>
                  <span class="tf-ln-s">{item.detail}</span>
                </button>
              {/if}
            {/each}
            {#if shown.length === 0}
              <p class="tf-ln-none">No action matches “{sheet.filter}”.</p>
            {/if}
          </div>
        {/if}
      {:else}
        {#if interpreted}
          <Chips {interpreted} sources={health} sourcesRead={sourcesPhase === "ready"} />
        {/if}

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
                inboxCount={inbox?.count ?? 0}
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
          {:else if session.mode === "help"}
            <Help rows={session.rows} selected={session.selected} {optionId} onopen={activate} onhover={(index) => (session.selected = index)} />
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
          {:else if session.response && session.response.interpreted.prefix === "list" && session.response.groups.length === 0}
            <p class="tf-ln-none">{lists.length === 0 ? "No saved list yet. Search for something and press ⌘S." : "No saved list matches."}</p>
          {:else if session.response && INBOX.test(session.raw.trim()) && session.response.groups.length === 0}
            <p class="tf-ln-none">{(inbox?.items.length ?? 0) === 0 ? "The inbox is empty." : "No inbox line matches."}</p>
          {:else if session.response}
            <Results
              response={session.response}
              rows={session.rows}
              selected={session.selected}
              {optionId}
              sources={health}
              {now}
              onopen={activate}
              onhover={(index) => (session.selected = index)}
            />
          {:else}
            <p class="tf-ln-none">Searching…</p>
          {/if}
        </div>
      {/if}
    {/snippet}

    {#snippet footer()}
      {#if sheet}
        <span><kbd>↑↓</kbd> move</span>
        {#if shown[sheet.selected]?.link}<span><kbd>←→</kbd> relation</span>{/if}
        <span><kbd>↵</kbd> {shown[sheet.selected]?.link ? "link" : "do it"}</span>
        <span><kbd>Esc</kbd> {sheet.filter !== "" ? "clear" : sheet.level === "root" ? "back to results" : "back"}</span>
        <span class="tf-spacer"></span>
        <span class="tf-ln-took">{sheet.title} · {counted(shown.length, "action", "actions")}</span>
      {:else}
        <span><kbd>↑↓</kbd> move</span>
        <span><kbd>↵</kbd> open</span>
        {#if sheetHint}
          <span><kbd>⌘K</kbd> actions on {sheetHint}</span>
        {/if}
        {#if session.canSave}
          <span><kbd>⌘S</kbd> {session.savedId === null ? "save as list" : "saved as list"}</span>
        {/if}
        <span><kbd>Esc</kbd> back one step</span>
        <span class="tf-spacer"></span>
        {#if session.response}
          <span class="tf-ln-took">{session.response.total} matches · {session.response.took_ms} ms</span>
        {/if}
      {/if}
    {/snippet}
  </TfDialog>
{/if}

<style>
  /* PROTOTYPE rules. The shipped ones are `tf/css/launcher.css`; these would join them. */
  .pinned { flex: none; border-bottom: 1px solid var(--rule); }
  .lead { padding: 10px 18px; font-size: var(--t-sm); color: var(--ink); }
  .sheet { flex: 1; min-height: 0; overflow: auto; padding: 0 0 4px; outline: none; }
  .filter { padding: 8px 18px 6px; font-size: var(--t-sm); color: var(--ink); border-bottom: 1px solid var(--rule); margin-bottom: 4px; }
  .filter.empty { color: var(--ink-2); }
  .caret { display: inline-block; width: 1px; height: 1em; background: var(--ink); vertical-align: -2px; margin-left: 1px; }
  .renamerow { display: flex; align-items: center; gap: 10px; padding: 10px 18px; }
  .renamerow input { flex: 1; margin: 0; }
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
</style>
