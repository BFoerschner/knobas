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

  Screenshots: `?launcher-open=sepa` opens with that query, `&launcher-sheet=1`
  opens the sheet on the row at that index once results land.
-->
<script lang="ts">
  import { onMount, tick } from "svelte";

  import {
    ackAlert,
    checkoutSettings,
    cloneRepo,
    completeInboxItem,
    createLink,
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
  import type { CheckoutApplication, InboxItem, LinkSetMemberKind, LinkSetRow } from "../../ipc/entity";
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
  import { jobHash } from "../../tf/route";
  import TfDialog from "../../tf/settings/TfDialog.svelte";
  import { clockLabel, keyOf } from "../../time/format";
  import Chips from "../Chips.svelte";
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

  /** The board without lists: the saved ones are kept aside for `#l`, the built-ins are gone. */
  async function homeNext(): Promise<LauncherHome> {
    const home = await realHome();
    lists = home.smart_lists.filter((list) => list.saved);
    listsLoaded = true;
    return { ...home, smart_lists: [] };
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
        path: list.needs_attention ? `${list.description} · needs attention` : `${list.description} · ${counted(list.count, "item", "items")}`,
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

  /** `#x`: one letter after the hash is a kind; the backend filters on six, two are answered here. */
  const KIND_OF: Record<string, string> = { t: "ticket", p: "pr", r: "repo", b: "build", m: "monitor", d: "page" };
  const LOCAL = new Set(["l", "i"]);
  const TOKEN = /^#([a-z])$/i;

  /** The search port with the prototype's grammar read first and its groups layered over the real answer. */
  async function searchNext(query: SearchQuery): Promise<SearchResponse> {
    const raw = query.raw.trim();
    const at = (now ?? new Date()).toISOString();
    if (raw.startsWith("?")) {
      return { interpreted: { text: "", prefix: "help", filters: NO_FILTERS, unknown_tokens: [] }, groups: [], total: 0, took_ms: 0, coverage: [] };
    }
    // PROTOTYPE: the frontend reads grammar here, against ruling P2.
    const kinds: string[] = [];
    const local: string[] = [];
    const rest: string[] = [];
    for (const token of raw.split(/\s+/).filter(Boolean)) {
      const letter = TOKEN.exec(token)?.[1]?.toLowerCase();
      if (letter && KIND_OF[letter]) kinds.push(KIND_OF[letter]);
      else if (letter && LOCAL.has(letter)) local.push(letter);
      else rest.push(token);
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

  // ---- the `?` card: this grammar, written here --------------------------

  interface HelpEntry {
    token: string;
    insert: string;
    summary: string;
  }
  const HELP: HelpEntry[] = [
    { token: "#t", insert: "#t ", summary: "tickets · #t sepa" },
    { token: "#p", insert: "#p ", summary: "pull requests" },
    { token: "#r", insert: "#r ", summary: "repositories" },
    { token: "#b", insert: "#b ", summary: "builds" },
    { token: "#m", insert: "#m ", summary: "monitors" },
    { token: "#d", insert: "#d ", summary: "pages, with a last row that searches the Docs area" },
    { token: "#l", insert: "#l ", summary: "your saved lists; Enter applies one's filters here" },
    { token: "#i", insert: "#i ", summary: "the inbox, newest first" },
    { token: "#t #l", insert: "#t #l ", summary: "any of them together · #t #l sepa" },
    { token: "#PAY-231", insert: "#", summary: "a ticket by its key" },
    { token: ">", insert: "> ", summary: "go to a page of the app" },
    { token: "@me", insert: "@me ", summary: "what your accounts wrote · @jonas for one person" },
    { token: "t", insert: "t ", summary: "time: worklogs and the day" },
    { token: "?", insert: "?", summary: "this card" },
  ];
  const KEYS: [string, string][] = [
    ["↵", "open the row; a saved list applies its filters here"],
    ["Tab", "the row's actions, narrowed by what you type; Esc brings the search back"],
    ["⌘S", "save the search as a list"],
    ["Esc", "clear the box, then close"],
  ];
  let helpSelected = $state(0);
  const helpMode = $derived(session.response?.interpreted.prefix === "help");

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
    /** `flow` asks the box for a pick; `rename` for a saved list's new name. */
    level: "root" | "flow" | "rename";
    flow: Flow | null;
    /** What the box holds while the sheet is open: the filter, a pick's filter, or the new name. The search words wait in `session.raw`. */
    filter: string;
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
  function itemsFor(row: LauncherRow): SheetItem[] {
    if (row.kind === "hit" && row.hit.kind === "list") {
      const id = keyOf(row.hit.entity_id);
      return [
        { id: "apply", label: "Apply", detail: "Its filters in the box, as Enter does", run: () => activate(row) },
        {
          id: "rename",
          label: "Rename",
          detail: "The box takes the new name; Enter keeps it",
          run: () => {
            if (sheet) sheet = { ...sheet, level: "rename", filter: row.hit.title };
            void tick().then(() => box?.focus({ select: true }));
          },
        },
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

  // ---- ids, scrolling, the `>` palette -----------------------------------

  const uid = $props.id();
  const optionId = (index: number) => `${uid}-opt-${index}`;
  const sheetOptionId = (index: number) => `${uid}-sheet-${index}`;
  const helpOptionId = (index: number) => `${uid}-help-${index}`;
  const resultsId = `${uid}-results`;
  const sheetId = `${uid}-sheet`;

  const activeOption = $derived.by(() => {
    if (helpMode) return helpOptionId(helpSelected);
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
      void tick().then(() => box?.focus({ select: true }));
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

  /** ⌘S: the query becomes a saved list. `true` when the key was taken. */
  function saveKey(event: KeyboardEvent): boolean {
    if (!(event.metaKey || event.ctrlKey) || event.key.toLowerCase() !== "s") return false;
    event.preventDefault();
    if (session.canSave && session.savedId === null) {
      const label = session.raw.trim();
      void session.saveCurrent(label).then(() => push({ text: `Saved “${label}” as a list` }));
    }
    return true;
  }

  const listId = $derived(sheet?.row.kind === "hit" && sheet.row.hit.kind === "list" ? keyOf(sheet.row.hit.entity_id) : null);

  function sheetKey(event: KeyboardEvent, current: Sheet) {
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

  function helpKey(event: KeyboardEvent): boolean {
    if (!helpMode) return false;
    switch (event.key) {
      case "ArrowDown":
        event.preventDefault();
        helpSelected = Math.min(HELP.length - 1, helpSelected + 1);
        return true;
      case "ArrowUp":
        event.preventDefault();
        helpSelected = Math.max(0, helpSelected - 1);
        return true;
      case "Enter": {
        event.preventDefault();
        const entry = HELP[helpSelected];
        if (entry) session.set(entry.insert);
        return true;
      }
      default:
        return false;
    }
  }

  function onkeydown(event: KeyboardEvent) {
    if (saveKey(event)) return;
    if (sheet) {
      sheetKey(event, sheet);
      return;
    }
    if (helpKey(event)) return;
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

  const placeholder = $derived(
    sheet
      ? sheet.level === "rename"
        ? "The list's new name"
        : sheet.level === "flow"
          ? (step?.title ?? "")
          : `Narrow the actions on ${sheet.title}`
      : "? for help",
  );
  const interpreted = $derived(session.response?.interpreted ?? null);
  const pinnedHit = $derived.by(() => {
    if (!sheet) return null;
    if (sheet.row.kind === "hit") return sheet.row.hit;
    if (sheet.row.kind === "recent") return { ...sheet.row.row, snippet: [] };
    return null;
  });
  /** What the footer says Tab would open: the word alone, as asked, once a row with actions is under the cursor. */
  const tabHint = $derived.by(() => {
    const row = session.current;
    return row !== null && !sheet && !helpMode && itemsFor(row).length > 0;
  });
</script>

{#if open}
  <TfDialog title="Search and actions" className="tf-launch" onclose={close}>
    {#snippet body()}
      <QueryBox
        bind:this={box}
        value={sheet ? sheet.filter : session.raw}
        {placeholder}
        pending={session.pending}
        footnote=""
        controls={controlled.list}
        active={controlled.option}
        oninput={(value) => {
          if (sheet) sheet = { ...sheet, filter: value, selected: 0 };
          else session.type(value);
        }}
        {onkeydown}
        onclose={close}
      />

      {#if sheet}
        <!-- The row the sheet acts on, lifted under the box; the box keeps its words aside. -->
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
        {#if sheet.level === "rename"}
          <p class="tf-ln-none">Type the new name in the box. Enter keeps it, Esc leaves the name as it was.</p>
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
                  <span class="tf-ln-s">{item.detail}</span>
                </button>
              {/if}
            {/each}
            {#if shown.length === 0 && !(sheet.level === "flow" && sheet.flow?.choices === null)}
              <p class="tf-ln-none">{sheet.filter === "" ? "Nothing to pick here." : `Nothing matches “${sheet.filter}”.`}</p>
            {/if}
          </div>
        {/if}
      {:else if helpMode}
        <div class="tf-ln-list" role="listbox" tabindex="-1" aria-label="Help">
          <div class="tf-ln-h"><span class="tf-ln-lab">Narrow the search</span><span class="tf-ln-n">a row inserts itself</span></div>
          <div class="tf-ln-help">
            {#each HELP as entry, i (entry.token)}
              <button
                id={helpOptionId(i)}
                class="tf-ln-opt tf-ln-syn"
                class:tf-ln-on={i === helpSelected}
                role="option"
                aria-selected={i === helpSelected}
                onclick={() => { session.set(entry.insert); box?.focus(); }}
                onmouseenter={() => (helpSelected = i)}
              >
                <span class="tf-ln-tok">{entry.token}</span>
                <span class="tf-ln-s">{entry.summary}</span>
              </button>
            {/each}
          </div>
          <div class="tf-ln-h"><span class="tf-ln-lab">Keys</span></div>
          <div class="tf-ln-help">
            {#each KEYS as [key, what] (key)}
              <p class="tf-ln-opt tf-ln-syn keyrow"><kbd>{key}</kbd><span class="tf-ln-s">{what}</span></p>
            {/each}
          </div>
        </div>
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
        <span><kbd>↵</kbd> {shown[sheet.selected]?.link ? "link" : sheet.level === "flow" ? "pick" : "do it"}</span>
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
        <span><kbd>Esc</kbd> back one step</span>
        <span class="tf-spacer"></span>
        {#if session.response && !helpMode}
          <span class="tf-ln-took">{session.response.total} matches · {session.response.took_ms} ms</span>
        {/if}
      {/if}
    {/snippet}
  </TfDialog>
{/if}

<style>
  /* PROTOTYPE rules. The shipped ones are `tf/css/launcher.css`; these would join them. */
  /* The box's own ⌘K badge is gone (Björn, round 4); `QueryBox.svelte` would lose the element. */
  :global(.tf-launch .tf-ln-q > kbd) { display: none; }
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
  .keyrow { cursor: default; }
  .keyrow kbd { justify-self: center; }
</style>
