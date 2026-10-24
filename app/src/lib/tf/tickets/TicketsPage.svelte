<!--
  Tickets, the ticket-first shell's home (#810, requirements §3), in three
  panes:

  - **left**, the views: *Find a view*, *All tickets* with its count, the
    ticket views in their groups with their counts (#812, R-3.2);
  - **middle**, every synced ticket as a ticket board, one column per status
    in the source's words, eight cards then *Show N more*, *Done* three, and
    fifty while the filter has words (R-3.4, #1024), split into lanes by a
    field or not (R-3.5, #813); or as a list sorted by any header (R-3.6);
  - **right**, the selected ticket's preview (R-3.9).

  The filter bar is `FilterBar` in its server-side mode (R-2.7): every
  change sends the base query, chips, dropdowns and words to `mini_board`,
  which runs the search grammar and draws only what it admits. The browser
  filters nothing; it only marks the words in what came back.

  The board is `mini_board`'s, the same read the rooms' Tickets tile makes,
  with its columns and their order (`knobas_core::mini_board`). A laned
  board is read in lanes (#939): the server groups it, caps each lane's
  column as it caps a board's and counts it whole, and sends the first ten
  lanes, biggest first; *Show N more lanes* reads ten more.

  **Keys** (R-3.10): `j`/`k` and the arrows move the selection through the
  cards on screen in reading order (lane by lane, column by column, top to
  bottom; the list's rows as sorted), and focus the card. `Enter` and a
  double click open the job.

  **The *Board / List* and *Lanes* switches hold for the session** (R-3.3):
  module state, never written anywhere, so a restart forgets them. Each view
  keeps its own, over what it was saved with.

  **Ticket views** (#812): `#/tickets/<id>` opens one. Its stored text is the
  filter bar's base query, sent to `mini_board` as it is; a text the grammar
  cannot read opens as text with a line naming the part, and no tickets.
  *+ New view*, *Edit view*, *Save as a view* (the text the bar sends now,
  with how the tickets show) and *Duplicate* open the view editor (#814);
  on a Jira view, *Save as a view* and *Make an editable copy* open it on a
  copy of your own, from the parts of its query knobas can test, as its
  source reads them (#880, #1208), and only where the source translates it;
  *Delete*, in the ⋯ menus and the editor, a toast's *Undo* takes back.
-->
<script lang="ts" module>
  import type {
    FilterSuggestions,
    QueryTranslation,
    TicketView,
    TicketViewDraft,
    TicketViewRow,
  } from "../../ipc/search";
  import type { BoardReaderPorts, ViewSessions } from "./board-reader.svelte";

  /** The reads this page makes, injectable so a test needs no bridge: the board reader's, and the views'. */
  export interface TicketsPorts extends BoardReaderPorts {
    /** Hands a Jira view's address to the browser: *Open in Jira* (#879). */
    openExternal?: (url: string) => Promise<void>;
    suggestions: () => Promise<Pick<FilterSuggestions, "fields" | "parents">>;
    /** Every ticket view, each with its count (#812). */
    views: () => Promise<TicketViewRow[]>;
    saveView: (draft: TicketViewDraft) => Promise<TicketView>;
    deleteView: (id: string) => Promise<void>;
    /** *Hide from the list* or *Show* a Jira view (#878). */
    hideView: (id: string, hidden: boolean) => Promise<void>;
    /** A Jira view was opened: it stops saying *new* (#878). */
    openView: (id: string) => Promise<void>;
    /** A Jira view's query as its source reads it, for *Make an editable copy* (#1208). */
    translateView: (id: string) => Promise<QueryTranslation | null>;
  }

  /** What the page keeps for the session: each view's switches, and the selection. */
  const session = $state<ViewSessions & { selected: string | null }>({
    /** By view id, `""` for *All tickets*. */
    views: {},
    selected: null,
  });

  /** A new session: what each test is. */
  export function forgetTicketsSession(): void {
    session.views = {};
    session.selected = null;
  }
</script>

<script lang="ts">
  import { clockLabel } from "../../time/format";
  import { flushSync, onMount, untrack } from "svelte";

  import {
    getEntity,
    jobReach,
    listEntities,
    miniBoard,
    type MiniBoardCard,
  } from "../../ipc/entity";
  import {
    deleteTicketView,
    filterSuggestions,
    hideSavedView,
    openSavedView,
    saveTicketView,
    ticketViews,
    translateTicketView,
  } from "../../ipc/search";
  import { openExternal } from "../../shell/open-external";
  import { push } from "../../shell/toasts.svelte";
  import { inTextField } from "../../shell/keys";
  import { isFiltering, toQuery, type FilterConfig, type FilterOption } from "../filter/filter";
  import FilterBar from "../filter/FilterBar.svelte";
  import { sessionFilter } from "../filter/filter-state.svelte";
  import Marked from "../filter/Marked.svelte";
  import { sourceHash } from "../settings/settings";
  import { sourceRoles, TICKET_SYSTEM, TRANSLATE_QUERY } from "../roles.svelte";
  import { columnKey, createBoardReader, messageOf, viewSession } from "./board-reader.svelte";
  import SavedViewSourceLine from "./SavedViewSourceLine.svelte";
  import { needles as readNeedles } from "./needles";
  import TicketCard from "./TicketCard.svelte";
  import Signals from "./Signals.svelte";
  import TicketPreview from "./TicketPreview.svelte";
  import { columnName, LANE_FIELDS, moreLanesLine, moreLine, nextSort, sharedKeys, sortRows, statusOrder, ticketAddress, type LaneField, type SortColumn } from "./tickets";
  import ViewEditor from "./ViewEditor.svelte";
  import ViewMenu from "./ViewMenu.svelte";
  import {
    columnLimit,
    DEFAULT_LIST_COLUMNS,
    draftCopy,
    draftEdit,
    draftFromFilter,
    draftSavedViewCopy,
    draftNew,
    draftRestore,
    drawnSavedViewColumns,
    groupViews,
    hiddenViews,
    savedViewSourceId,
    savedViewSystems,
    systemsWords,
    LIST_COLUMN_ORDER,
    OWN_GROUPS,
    savedShowing,
    type CopiedFrom,
  } from "./views";

  let {
    ready,
    timedTicket,
    ports = {
      board: (perColumn, query, savedView, lanes, role) =>
        miniBoard({ role: role ?? null, context: null, project: null }, perColumn, query, savedView, lanes),
      openExternal,
      detail: getEntity,
      listEntities,
      jobReach,
      suggestions: () => filterSuggestions(),
      views: ticketViews,
      saveView: saveTicketView,
      deleteView: deleteTicketView,
      hideView: hideSavedView,
      openView: openSavedView,
      translateView: translateTicketView,
    },
    viewId = null,
    typingPause = 90,
  }: {
    /** Whether the database answers yet. */
    ready: boolean;
    /** The ticket key the timer runs on, or `null`. */
    timedTicket: string | null;
    ports?: TicketsPorts;
    /** The ticket view `#/tickets/<id>` names, or `null` for *All tickets*. */
    viewId?: string | null;
    /** How long, in ms, the filter waits for the next key before it reads: the launcher's 90. */
    typingPause?: number;
  } = $props();

  // ---- the views ----
  let views = $state<TicketViewRow[] | null>(null);
  /** *Find a view*'s words. */
  let find = $state("");
  /** The view editor's starting point and the view it edits, or `null` while it is closed. */
  let drafting = $state<{
    draft: TicketViewDraft;
    editing: TicketViewRow | null;
    from?: CopiedFrom;
    /** View settings on a Jira view: its source's reading of the query, once it answers (#1208). */
    copy?: QueryTranslation | null;
  } | null>(null);

  function newView(draft: TicketViewDraft) {
    drafting = { draft, editing: null };
  }

  /**
   * Whether a Jira view offers *Make an editable copy* (#1208): its source
   * claims `translate_query`, the ticket system's read that turns the
   * source's query into conditions. The shell reads no query language.
   */
  function offersCopy(row: TicketViewRow): boolean {
    return row.saved_view !== null && sourceRoles.answers(TICKET_SYSTEM, savedViewSourceId(row.saved_view), TRANSLATE_QUERY);
  }

  /** The source's reading of a Jira view's query, or `null` with a toast saying why there is none. */
  async function translation(row: TicketViewRow): Promise<QueryTranslation | null> {
    try {
      const copy = await ports.translateView(row.id);
      if (copy === null) push({ text: `${row.name} cannot be copied: its source does not translate its query`, tone: "err" });
      return copy;
    } catch (error) {
      push({ text: `${row.name} was not copied: ${messageOf(error)}`, tone: "err" });
      return null;
    }
  }

  /** *Duplicate*; on a Jira view, *Make an editable copy* (#880, R-3.25), from its source's reading (#1208). */
  async function copyView(row: TicketViewRow, known: QueryTranslation | null = null) {
    if (row.saved_view_source === null) return newView(draftCopy(row));
    const savedViewSource = row.saved_view_source;
    const copy = known ?? (await translation(row));
    if (copy !== null) drafting = { ...draftSavedViewCopy(row, savedViewSource, copy), editing: null };
  }

  function editView(row: TicketViewRow) {
    const opened = { draft: draftEdit(row), editing: row, copy: null };
    drafting = opened;
    if (row.saved_view_source === null || !offersCopy(row)) return;
    // *A copy keeps …* waits for the source's answer; a failed one leaves the line out.
    ports.translateView(row.id).then(
      (copy) => {
        if (drafting?.editing === row) drafting.copy = copy;
      },
      () => {},
    );
  }

  /** *Delete view* in the editor: closes it, then deletes as the ⋯ menu does. */
  function deleteFromEditor() {
    const gone = drafting?.editing ?? null;
    drafting = null;
    if (gone !== null) void remove(gone);
  }

  /**
   * The view on screen: `null` for *All tickets*, `undefined` while the views
   * are read or for an id no view has.
   */
  const view = $derived(viewId === null ? null : (views?.find((row) => row.id === viewId) ?? undefined));
  const groups = $derived(groupViews(views ?? [], find));
  /** The Jira views hidden from the list (#878): one line at the bottom, each with *Show* when unfolded. */
  const hidden = $derived(hiddenViews(views ?? []));
  let showHidden = $state(false);
  const groupNames = $derived([
    ...new Set([...OWN_GROUPS, ...(views ?? []).map((row) => row.group)]),
  ]);
  /** The base query the board is read with: the view's text, `null` when there is nothing to read. */
  const base = $derived(view === null ? "" : view === undefined || view.unreadable !== null ? null : view.query);
  const sessionKey = $derived(viewId ?? "");
  /** The saved view the board reads (#877), or `null`: its source decides its tickets and their order. */
  const savedViewId = $derived(view?.saved_view_source ? view.saved_view : null);
  /** The ticket systems the saved views come from (#2032), for the list's foot and a view of your own. */
  const systems = $derived(savedViewSystems(views ?? []));
  /** Where *Choose which* goes: the first saved view's source, else the sources. */
  const savedViewSettingsHref = $derived.by(() => {
    const ref = (views ?? []).find((row) => row.saved_view_source !== null)?.saved_view;
    return ref ? sourceHash(savedViewSourceId(ref)) : "#/settings";
  });

  function loadViews() {
    ports
      .views()
      .then((rows) => (views = rows))
      .catch(() => (views ??= []));
  }

  const saved = $derived(savedShowing(view ?? null));
  /** The view's own columns, which gather the board's statuses (R-3.15). */
  const ownColumns = $derived(savedViewId !== null ? drawnSavedViewColumns(view?.columns ?? []) : (view?.columns ?? []));

  // ---- the board: its reads are the reader's (#1319) ----
  // svelte-ignore state_referenced_locally
  // Read once: the reader owns the typing pause and the generations.
  const reader = createBoardReader(ports, {
    typingPause,
    sessions: session,
    view: () => ({ columns: ownColumns, listed: view?.columns ?? [], layout: saved.layout, lanes: saved.lanes }),
  });
  /** How the tickets show now: the session's switches over the view's own (R-3.3). */
  const showing = $derived({
    layout: reader.layout,
    lanes: reader.lanes,
    sort: session.views[sessionKey]?.sort ?? saved.sort,
  });

  // ---- the filter bar ----
  /** The filter bar's id: its state is kept per view. */
  const filterId = $derived(viewId === null ? "tickets-all" : `tickets-${viewId}`);
  const filter = $derived(sessionFilter(filterId));
  /** What the server read from the typed text, marked on the cards (#1903). */
  const needles = $derived(readNeedles(filter.text));
  let suggested = $state<Pick<FilterSuggestions, "fields" | "parents"> | null>(null);

  /** At most this many chips a group; past it a field has too many values for chips. */
  const CHIPS_AT_MOST = 6;

  const config: FilterConfig = $derived.by(() => {
    const values = (key: string): FilterOption[] =>
      (suggested?.fields ?? [])
        .filter((field) => field.key === key)
        .map((field) => ({ value: field.value, label: field.value === "@me" ? "Me" : field.value }));
    return {
      id: filterId,
      placeholder: "Filter by key, summary, epic or person",
      noun: "tickets",
      one: "ticket",
      base: base ?? "",
      chips: [
        { key: "issuetype", label: "Type", options: values("issuetype").slice(0, CHIPS_AT_MOST) },
        { key: "priority", label: "Priority", options: values("priority").slice(0, CHIPS_AT_MOST) },
      ],
      dropdowns: [
        { key: "assignee", label: "Assignee", options: values("assignee") },
        {
          key: "under",
          label: "Epic",
          options: (suggested?.parents ?? [])
            .filter((parent) => parent.kind === "ticket")
            .map((parent) => ({ value: parent.name, label: parent.name })),
        },
      ],
    };
  });

  /** The filter bar as a read sends it. */
  function filterNow() {
    return { query: toQuery(config, filter), words: filter.text.trim() !== "" };
  }

  function sortBy(column: SortColumn) {
    viewSession(session, sessionKey).sort = nextSort(showing.sort, column);
  }

  function clearFilter() {
    filter.text = "";
    filter.chips = {};
    filter.dropdowns = {};
    reader.setFilter(filterNow());
    reader.refresh();
  }

  // ---- the views' writes ----
  /**
   * *Save as a view* (R-3.12): the text the bar sends now, and how the
   * tickets show. On a Jira view (#880), a copy: the JQL parts knobas can
   * test, then what the bar adds.
   */
  async function saveAsView() {
    if (view?.saved_view_source) {
      const [row, savedViewSource, extra, now] = [view, view.saved_view_source, toQuery({ ...config, base: "" }, filter), showing];
      const copy = await translation(row);
      if (copy !== null) drafting = { ...draftSavedViewCopy(row, savedViewSource, copy, extra, now), editing: null };
    } else newView(draftFromFilter(view ?? null, toQuery(config, filter), showing));
  }

  async function save(draft: TicketViewDraft) {
    const editing = drafting?.editing ?? null;
    const made = await ports.saveView(draft);
    drafting = null;
    push({ text: editing === null ? `Created ${made.name}` : `Saved ${made.name}` });
    loadViews();
    if (editing === null || viewId === made.id) {
      // A saved view opens as saved: the session's switches over it go.
      if (editing !== null) delete session.views[made.id];
      location.hash = `#/tickets/${made.id}`;
    }
  }

  async function remove(gone: TicketViewRow) {
    try {
      await ports.deleteView(gone.id);
    } catch (error) {
      push({ text: `${gone.name} was not deleted: ${messageOf(error)}`, tone: "err" });
      return;
    }
    if (viewId === gone.id) location.hash = "#/tickets";
    loadViews();
    push({
      text: `Deleted ${gone.name}`,
      action: {
        label: "Undo",
        run: () => {
          ports
            .saveView(draftRestore(gone))
            .then(loadViews)
            .catch((error: unknown) =>
              push({ text: `${gone.name} was not put back: ${messageOf(error)}`, tone: "err" }),
            );
        },
      },
    });
  }

  /** *Hide from the list* (`true`, with *Undo*) or *Show* (`false`) a Jira view (#878, R-3.25). */
  async function setHidden(row: TicketViewRow, hide: boolean) {
    try {
      await ports.hideView(row.id, hide);
    } catch (error) {
      push({ text: `${row.name} was not ${hide ? "hidden" : "shown"}: ${messageOf(error)}`, tone: "err" });
      return;
    }
    loadViews();
    if (!hide) return;
    if (viewId === row.id) location.hash = "#/tickets";
    push({ text: `Hid ${row.name}`, action: { label: "Undo", run: () => void setHidden(row, false) } });
  }

  // A Jira view marked *new* stops saying so once it is opened (#878, R-3.20):
  // asked once per view per visit, so an answer that still says *new* cannot loop.
  const openedNew = new Set<string>();
  $effect(() => {
    const opened = view;
    if (!opened || opened.saved_view_source?.new !== true || openedNew.has(opened.id)) return;
    openedNew.add(opened.id);
    untrack(() => {
      ports
        .openView(opened.id)
        .then(loadViews)
        .catch(() => {});
    });
  });

  // ---- what is drawn, and the selection ----
  /** The board's columns as read, before the view's own gather them. */
  const boardColumns = $derived(reader.board?.columns ?? []);
  /** The list's rows: every card read, in the header's order; each card says its own status (#1423). */
  const rows = $derived(
    sortRows(
      boardColumns.flatMap((column) => column.cards.map((card) => ({ card }))),
      showing.sort,
      statusOrder(boardColumns),
    ),
  );
  /** The keys two sources' cards hold (#1476): those cards name their source and open by it. */
  const shared = $derived(sharedKeys(boardColumns));
  /** A card's source name when its key is shared, else `null`. */
  const sourceOf = (card: MiniBoardCard) => (shared.has(card.key) ? sourceRoles.nameOf(card.source_id) : null);
  /** The list's columns: the view's, in the list's order (R-3.16). */
  const listColumns = $derived(
    LIST_COLUMN_ORDER.filter(([column]) => (view?.list_columns ?? DEFAULT_LIST_COLUMNS).includes(column)),
  );
  const SORTABLE: readonly SortColumn[] = ["key", "summary", "status", "priority", "assignee", "updated"];
  const sortable = (column: string): column is SortColumn => (SORTABLE as readonly string[]).includes(column);
  const listCut = $derived(boardColumns.some((column) => column.cards.length < column.total));
  const listRest = $derived(
    boardColumns.reduce((all, column) => all + column.total - column.cards.length, 0),
  );
  /** What is on screen, in reading order: the first is selected when the selection is not. */
  const visible = $derived(
    showing.layout === "board"
      ? reader.bands.flatMap(({ drawn }) =>
          drawn.flatMap(({ cards }) => cards.map((card) => ({ card }))),
        )
      : rows,
  );
  const current = $derived(visible.find((entry) => entry.card.entity_id === session.selected) ?? visible[0] ?? null);

  // ---- the preview ----
  /** The selected ticket's id: the preview re-reads when it changes, not when the board answers. */
  const currentId = $derived(current?.card.entity_id ?? null);

  $effect(() => {
    const id = ready ? currentId : null;
    untrack(() => reader.preview(id));
  });

  // The board, again for each view and once its text is known. Untracked,
  // so a read's own answer does not ask for the read again.
  $effect(() => {
    if (!ready) return;
    const scope = { key: sessionKey, base };
    untrack(() => reader.open({ ...scope, savedView: savedViewId }, filterNow()));
  });
  $effect(() => () => reader.close());

  // Once the database answers: the views and the suggestions.
  $effect(() => {
    if (!ready) return;
    untrack(() => {
      loadViews();
      ports
        .suggestions()
        .then((answer) => (suggested = answer))
        .catch(() => {});
    });
  });

  function select(card: MiniBoardCard) {
    session.selected = card.entity_id;
  }

  function openJob(card: MiniBoardCard) {
    session.selected = card.entity_id;
    location.hash = ticketAddress(card);
  }

  /** `j`/`k` and the arrows (R-3.10): the next or previous card on screen, selected and focused. */
  function step(by: 1 | -1) {
    if (visible.length === 0) return;
    const at = current === null ? 0 : visible.findIndex((entry) => entry.card.entity_id === current.card.entity_id);
    const next = visible[Math.min(visible.length - 1, Math.max(0, at + by))]!;
    session.selected = next.card.entity_id;
    flushSync();
    const element = document.querySelector<HTMLElement>(`.tf-tickets [data-entity="${CSS.escape(next.card.entity_id)}"]`);
    element?.focus();
    element?.scrollIntoView?.({ block: "nearest" });
  }

  // `Enter` presses the preview's button (R-3.9), from anywhere but a field
  // that types or a control of its own; a card is not one.
  onMount(() => {
    const onkey = (event: KeyboardEvent) => {
      if (event.metaKey || event.ctrlKey || event.altKey || event.defaultPrevented) return;
      const at = document.activeElement;
      if (at instanceof HTMLElement && inTextField(at)) return;
      const by = { j: 1, ArrowDown: 1, k: -1, ArrowUp: -1 }[event.key] as 1 | -1 | undefined;
      if (by !== undefined) {
        event.preventDefault();
        step(by);
        return;
      }
      if (event.key !== "Enter") return;
      const card = at instanceof HTMLElement ? at.closest<HTMLElement>("[data-entity]") : null;
      if (at instanceof HTMLElement && at !== document.body && card === null && at.closest("a, button, select") !== null)
        return;
      // A focused card is the one Enter means, selected or not: the keydown
      // comes before its click would have selected it.
      const entry = card === null ? current : (visible.find((e) => e.card.entity_id === card.dataset.entity) ?? current);
      if (entry === null) return;
      event.preventDefault();
      session.selected = entry.card.entity_id;
      location.hash = ticketAddress(entry.card);
    };
    window.addEventListener("keydown", onkey);
    return () => window.removeEventListener("keydown", onkey);
  });
</script>

<div class="tf-tickets">
  <nav class="tf-views" aria-label="Ticket views">
    <div class="tf-vfind"><input type="search" bind:value={find} placeholder="Find a view" aria-label="Find a view" /></div>
    <div class="tf-vlist">
      <div class="tf-vw-acts">
        <button type="button" class="tf-btn tf-sm" onclick={() => newView(draftNew())}>+ New view</button>
      </div>
      {#if find.trim() === ""}
        <a href="#/tickets" aria-current={viewId === null ? "true" : undefined}
          >All tickets {#if viewId === null && reader.total !== null}<span class="tf-count">{reader.total}</span>{/if}</a
        >
      {/if}
      {#each groups as group (group.name)}
        <h2>
          {group.name}{#if group.system !== null}{" "}<span class="tf-vw-sync">from the {group.system} source</span>{/if}
        </h2>
        {#each group.views as row (row.id)}
          <div class="tf-vw-row">
            <a href="#/tickets/{row.id}" aria-current={row.id === viewId ? "true" : undefined}
              ><span class="tf-vw-name"><Marked text={row.name} typed={find} /></span
              >{#if row.saved_view_source?.new}<span class="tf-vw-new">new</span>{/if}{#if row.count !== null && row.count > 0}{" "}<span
                  class="tf-count">{row.count}</span
                >{/if}</a
            >
            <ViewMenu
              name={row.name}
              savedViewKind={row.saved_view_source?.kind ?? null}
              copies={offersCopy(row)}
              onedit={() => editView(row)}
              onduplicate={() => copyView(row)}
              ondelete={() => remove(row)}
              onhide={() => void setHidden(row, true)}
            />
          </div>
        {/each}
      {/each}
      {#if find.trim() !== "" && groups.length === 0}
        <p class="tf-dim tf-vw-none">No view matches.</p>
      {/if}
      {#if hidden.length > 0}
        <button type="button" class="tf-vw-hidden" aria-expanded={showHidden} onclick={() => (showHidden = !showHidden)}
          >{hidden.length} hidden</button
        >
        {#if showHidden}
          {#each hidden as row (row.id)}
            <div class="tf-vw-row tf-vw-off">
              <a href="#/tickets/{row.id}" aria-current={row.id === viewId ? "true" : undefined}>{row.name}</a>
              <button
                type="button"
                class="tf-vw-show"
                aria-label="Show {row.name} again"
                title="Show it again"
                onclick={() => void setHidden(row, false)}>Show</button
              >
            </div>
          {/each}
        {/if}
      {/if}
      <p class="tf-vw-foot">
        {#if systems.length === 0}A ticket system's boards and favourite filters come in at every sync.{:else}Boards and favourite
          filters come in with the {systemsWords(systems)}
          {systems.length > 1 ? "sources" : "source"} at every sync.{/if}
        <a href={savedViewSettingsHref}>Choose which</a>
      </p>
    </div>
  </nav>

  <section class="tf-board-wrap">
    <div class="tf-board-head">
      <h1>{view?.name ?? (viewId === null ? "All tickets" : "")}</h1>
      {#if view && view.saved_view_source === null}<span class="tf-q"
          >{view.query.trim() === "" ? "every synced ticket" : view.query}</span
        >{/if}
      {#if view !== undefined && (view === null || view.unreadable === null)}
        <div class="tf-seg" role="group" aria-label="Layout">
          <button type="button" aria-pressed={showing.layout === "board"} onclick={() => reader.chooseLayout("board")}
            >Board</button
          >
          <button type="button" aria-pressed={showing.layout === "list"} onclick={() => reader.chooseLayout("list")}
            >List</button
          >
        </div>
      {/if}
      {#if view}
        <button type="button" class="tf-btn tf-sm" onclick={() => editView(view)}
          >{view.saved_view_source === null ? "Edit view" : "View settings"}</button
        >
        <ViewMenu
          name="this view"
          savedViewKind={view.saved_view_source?.kind ?? null}
          copies={offersCopy(view)}
          className="tf-btn tf-sm tf-ghost"
          onedit={() => editView(view)}
          onduplicate={() => copyView(view)}
          ondelete={() => remove(view)}
          onhide={() => void setHidden(view, true)}
        />
      {/if}
    </div>
    {#if view?.saved_view_source && view.saved_view}
      <SavedViewSourceLine
        savedViewSource={view.saved_view_source}
        name={view.name}
        sourceId={savedViewSourceId(view.saved_view)}
        openExternal={ports.openExternal ?? openExternal}
      />
    {/if}
    {#if base !== null}
      <FilterBar
        {config}
        {filter}
        shown={reader.shown}
        total={reader.total}
        onchange={() => reader.setFilter(filterNow())}
        extra={lanesSwitch}
        after={saveButton}
      />
    {/if}
    {#snippet lanesSwitch()}
      <!-- prototype-6 offers lanes on the ticket board only. -->
      {#if showing.layout === "board"}
        <label class="tf-fsel">
          Lanes
          <select
            aria-label="Lanes"
            value={showing.lanes}
            onchange={(event) => reader.chooseLanes(event.currentTarget.value as LaneField)}
          >
            {#each LANE_FIELDS as [value, label] (value)}
              <option {value}>{label}</option>
            {/each}
          </select>
        </label>
      {/if}
    {/snippet}
    {#snippet saveButton()}
      <!-- On a Jira view it is a copy, so a source that translates no query offers none (#1208). -->
      {#if !view?.saved_view_source || offersCopy(view)}
        <button type="button" class="tf-btn tf-sm tf-ghost tf-vw-savef" onclick={() => void saveAsView()}>Save as a view</button>
      {/if}
    {/snippet}

    {#if viewId !== null && views !== null && view === undefined}
      <div class="tf-empty">
        <p>No view is called {viewId}. It may have been deleted.</p>
        <a class="tf-btn tf-sm" href="#/tickets">All tickets</a>
      </div>
    {:else if view && view.unreadable !== null}
      <div class="tf-empty">
        <p class="tf-vw-warn">
          The search cannot read <code class="tf-mono">{view.unreadable}</code> in this view, so it shows no tickets.
        </p>
      </div>
    {:else if reader.failed !== null}
      <div class="tf-empty"><p>The tickets could not be read: {reader.failed}</p></div>
    {:else if reader.board === null}
      <div class="tf-empty" aria-busy="true"></div>
    {:else if reader.shown === 0 && isFiltering(filter)}
      <div class="tf-empty">
        <p>No tickets match.</p>
        <button type="button" class="tf-btn tf-sm" onclick={clearFilter}>Clear the filter</button>
      </div>
    {:else if reader.shown === 0}
      <div class="tf-empty"><p>No tickets synced yet. Add a ticket system in Settings and they appear here.</p></div>
    {:else if showing.layout === "board"}
      {#each reader.bands as { lane, drawn } (lane.key ?? "")}
        <div class="tf-lane">
          {#if lane.key !== null}
            <h2>{lane.title} <span class="tf-count">{lane.count}</span></h2>
          {/if}
          <div class="tf-cols">
            {#each drawn as { column, cards, done } (columnKey(column))}
              {@const more = moreLine(column, cards.length, done)}
              {@const limit = columnLimit(view?.columns ?? [], column.status)}
              <div class="tf-col">
                <h3>
                  {columnName(column.status)}
                  <span
                    class="tf-count"
                    class:tf-err={limit !== null && column.total > limit}
                    title={limit !== null ? `At most ${limit} on this column` : undefined}
                    >{column.total}{#if limit !== null}{" "}of {limit}{/if}</span
                  >
                </h3>
                <div class="tf-stack">
                  {#each cards as card (card.entity_id)}
                    <TicketCard
                      {card}
                      fields={view?.card_fields ?? null}
                      source={sourceOf(card)}
                      {needles}
                      selected={current?.card.entity_id === card.entity_id}
                      onselect={() => select(card)}
                      onopen={() => openJob(card)}
                    />
                  {:else}
                    <span class="tf-dim tf-nothing">Nothing here</span>
                  {/each}
                  {#if more}
                    <button type="button" class="tf-more" onclick={() => reader.expand(lane, column)}>{more}</button>
                  {/if}
                </div>
              </div>
            {/each}
          </div>
        </div>
      {/each}
      {@const moreLanesText = reader.laned && reader.board !== null ? moreLanesLine(reader.board.more_lanes) : null}
      {#if moreLanesText}
        <button type="button" class="tf-btn tf-sm" onclick={reader.moreLanes}>{moreLanesText}</button>
      {/if}
    {:else}
      <div class="tf-list">
        <table class="tf-tbl">
          <thead>
            <tr>
              {#each listColumns as [column, label] (column)}
                {#if sortable(column)}
                  <th aria-sort={showing.sort.column === column ? showing.sort.direction : undefined}
                    ><button type="button" onclick={() => sortBy(column)}>{label}</button></th
                  >
                {:else}
                  <th>{label}</th>
                {/if}
              {/each}
            </tr>
          </thead>
          <tbody>
            {#each rows as { card } (card.entity_id)}
              <tr
                class="tf-row"
                class:tf-card={current?.card.entity_id === card.entity_id}
                aria-current={current?.card.entity_id === card.entity_id ? "true" : undefined}
                data-entity={card.entity_id}
                tabindex="0"
                onclick={() => select(card)}
                ondblclick={() => openJob(card)}
              >
                {#each listColumns as [column] (column)}
                  {#if column === "key"}
                    {@const source = sourceOf(card)}
                    <td class="tf-mono tf-nowrap"
                      ><Marked text={card.key} {needles} />{#if source}<span class="tf-sub">{source}</span>{/if}</td
                    >
                  {:else if column === "summary"}
                    <td
                      ><Marked text={card.title} {needles} />{#if card.path}<span class="tf-sub"
                          >{card.path}</span
                        >{/if}</td
                    >
                  {:else if column === "status"}
                    <td class="tf-nowrap">{columnName(card.status)}</td>
                  {:else if column === "priority"}
                    <td>{card.priority ?? ""}</td>
                  {:else if column === "signals"}
                    <td><span class="tf-sigs"><Signals signals={card.signals} /></span></td>
                  {:else if column === "assignee"}
                    <td>{card.assignee ?? "Unassigned"}</td>
                  {:else if column === "updated"}
                    <td class="tf-dim">{clockLabel(card.updated_at)}</td>
                  {:else if column === "epic"}
                    <td class="tf-mono tf-nowrap">{card.path ?? ""}</td>
                  {:else}
                    <td>{card.labels.join(", ")}</td>
                  {/if}
                {/each}
              </tr>
            {/each}
          </tbody>
        </table>
        {#if listCut && !reader.typing}
          <button type="button" class="tf-more tf-list-more" onclick={reader.expandAll}>Show {listRest} more</button>
        {/if}
      </div>
    {/if}
  </section>

  {#if drafting !== null}
    {@const editing = drafting.editing}
    <!-- Keyed: *Make an editable copy* in a Jira view's settings opens a new editor on the copy. -->
    {#key drafting}<ViewEditor
      draft={drafting.draft}
      editing={editing !== null}
      unreadable={editing?.unreadable ?? null}
      groups={groupNames}
      {systems}
      fields={suggested?.fields ?? []}
      parents={suggested?.parents ?? []}
      savedViewSource={editing?.saved_view_source ?? null}
      copiedFrom={drafting.from ?? null}
      copy={drafting.copy ?? null}
      oncopy={editing?.saved_view_source && offersCopy(editing) ? () => void copyView(editing, drafting?.copy ?? null) : undefined}
      board={(perColumn, query) =>
        ports.board(perColumn, query, editing?.saved_view_source ? editing.saved_view : null, undefined, TICKET_SYSTEM)}
      onsave={save}
      ondelete={editing === null ? undefined : deleteFromEditor}
      onclose={() => (drafting = null)}
    />{/key}
  {/if}

  {#if current !== null && base !== null}
    <TicketPreview
      card={current.card}
      address={ticketAddress(current.card)}
      description={reader.description}
      around={reader.around}
      timerOnIt={timedTicket === current.card.key}
    />
  {:else}
    <aside class="tf-preview" aria-label="Selected ticket"><p>Select a ticket to see what is around it.</p></aside>
  {/if}
</div>
