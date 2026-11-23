<!--
  Tickets, the ticket-first shell's home (#810, requirements §3), in two
  panes:

  - **left**, the views: *Find a view*, *All tickets* with its count, the
    ticket views in their groups with their counts (#812, R-3.2);
  - **middle**, every synced ticket as a ticket board, one column per status
    in the source's words, eight cards then *Show N more*, *Done* three, and
    fifty while the filter has words (R-3.4, #1024), split into lanes by a
    field or not (R-3.5, #813); or as a list sorted by any header (R-3.6).
    A view stored with *Show every ticket when the view opens* (ux18) draws
    every card and row, with no *Show more*.

  The selected ticket's preview on the right is gone (ux18, Björn: "Remove the
  right sidebar from the tickets view"): the board or list takes the width, and
  a click or `Enter` opens the ticket's own page, which holds what the preview
  showed (the facts, the description, *Around this ticket*, *Start work*).

  Two marks on the board and the list (spec #2396, #2398): yellow follows
  the chosen job (`job/chosen.svelte.ts`), selected or not; the selection is
  a two-pixel ink outline, and on the board the lift.

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
  click open the ticket page (`#/ticket/<id>`, spec #2227), which chooses
  nothing; the right-click menu's *Work on this* and *Start timer* take the
  ticket on. A long list, and a board column of
  24 cards or more (`TicketStack`), draw only the rows near the viewport
  (`shell/windowed.svelte.ts`, #2209); `j`/`k` draws the next first.

  **Moving a ticket** (ADR-0054, ticket-edit decisions sections 3 to 13): a
  card is dragged to a column, which lights where it can go (`card-drag.svelte.ts`;
  `s` carries the focused or selected card by key); a column of several statuses asks
  which, at the drop point. A card's right-click menu offers *Move to ▸* with
  the statuses it can reach,
  and the list's Status cell is a button that opens them (`s` on the selected
  row). A move is drawn at once, the card in its new column with *Queued*
  then *Sent* (`moves.svelte.ts`, laid over the board's read by
  `echo-board.ts`); a refused or held one puts the card back with a note.
  `⌘Z` (`Ctrl+Z`) undoes the last move while its toast stands.

  **The *Board / List* and *Lanes* switches hold for the session** (R-3.3):
  module state, never written anywhere, so a restart forgets them. Each view
  keeps its own, over what it was saved with.

  **Ticket views** (#812): `#/tickets/<id>` opens one. Its stored text is the
  filter bar's base query, sent to `mini_board` as it is; a text the grammar
  cannot read opens as text with a line naming the part, and no tickets.
  *+ New view*, *Edit view*, *Save as a view* (the text the bar sends now,
  with how the tickets show) and *Duplicate* open the view editor (#814);
  on a saved view, *Save as a view* and *Make an editable copy* open it on a
  copy of your own, from the parts of its query knobas can test, as its
  source reads them (#880, #1208), and only where the source translates it;
  *Delete*, in the ⋯ menus and the editor, a toast's *Undo* takes back.

  **Back from a ticket page** (spec #2227): `Esc` on a ticket page Tickets
  opened returns to the ticket view it was opened from, with that ticket
  selected again ({@link selectTicket}, which the shell calls on that step).
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
  import type { MovePorts, NotePorts } from "./moves.svelte";
  import type { EchoListen } from "../queued.svelte";
  import type { CardFieldsHost } from "./card-fields.svelte";

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
    /** A move's reads and writes, and the queue's open rows its notes are read from (ADR-0054); the bridge's when left out. */
    moves?: Partial<MovePorts> & Partial<NotePorts>;
    /** Hears `item:refreshed` and `sync:state`, which read the board again for its moves' echoes; the bridge's when left out. */
    listen?: EchoListen;
    /** A card's field edits' reads and write (ADR-0054, section 26); the bridge's when left out. */
    fields?: Pick<CardFieldsHost, "submitWrite" | "refused" | "fields" | "follower">;
  }

  /** What the page keeps for the session: each view's switches, and the selection. */
  const session = $state<ViewSessions & { selected: string | null }>({
    /** By view id, `""` for *All tickets*. */
    views: {},
    selected: null,
  });

  /**
   * Select `entityId` on Tickets (spec #2227): the shell's `Esc` from a ticket
   * page that Tickets opened, so the reader lands where they left, even when
   * the selection moved meanwhile (`⌘[`, another pick, `⌘]`).
   */
  export function selectTicket(entityId: string): void {
    session.selected = entityId;
  }

  /** A new session: what each test is. */
  export function forgetTicketsSession(): void {
    session.views = {};
    session.selected = null;
  }
</script>

<script lang="ts">
  import TfSideEdge from "../TfSideEdge.svelte";
  import { sideWidth } from "../side-width.svelte";
  import { clockLabel } from "../../time/format";
  import { flushSync, onMount, tick, untrack } from "svelte";
  import { innerWidth } from "svelte/reactivity/window";
  import { fonts } from "../fonts.svelte";
  import { stamped, stampOf } from "../opener";
  import { HeldOrder } from "../held-order.svelte";
  import { rowInView } from "../return-place";
  import { parseTfHash, ticketHash } from "../route";
  import { menuThing } from "../context/thing";
  import type { RoleFrameProps } from "../page-entry";

  import {
    miniBoard,
    type MiniBoard,
    type MiniBoardCard,
    type MiniBoardColumn,
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
  import { routeWeb as openExternal } from "../../shell/open-web";
  import { push } from "../../shell/toasts.svelte";
  import { inTextField } from "../../shell/keys";
  import { Windowed } from "../../shell/windowed.svelte";
  import { isFiltering, toQuery, type FilterConfig, type FilterOption } from "../filter/filter";
  import FilterBar from "../filter/FilterBar.svelte";
  import TfKey from "../TfKey.svelte";
  import TfKeyed from "../TfKeyed.svelte";
  import TfScroller from "../TfScroller.svelte";
  import { sessionFilter } from "../filter/filter-state.svelte";
  import Marked from "../filter/Marked.svelte";
  import { ADD_SOURCE_HASH, settingsHash, sourceHash } from "../settings/settings";
  import TfLoadDemo from "../settings/TfLoadDemo.svelte";
  import { health as sharedHealth, type Health } from "../../shell/health.svelte";
  import { lifecycle } from "../../shell/lifecycle.svelte";
  import { sourceRoles, TICKET_SYSTEM, TRANSLATE_QUERY } from "../roles.svelte";
  import { columnKey, createBoardReader, messageOf, viewSession } from "./board-reader.svelte";
  import SavedViewSourceLine from "./SavedViewSourceLine.svelte";
  import { needles as readNeedles } from "./needles";
  import { nearEnd } from "./near-end";
  import { fbarHeight } from "./fbar-height";
  import TicketCard from "./TicketCard.svelte";
  import TicketStack, { STACK_WINDOW_FROM } from "./TicketStack.svelte";
  import Signals from "./Signals.svelte";
  import { assigneeName, assigneeOptions, cardFloor, COLUMN_CAP, columnHead, columnWidth, columnName, columnWords, keepColumnArrays, LANE_FIELDS, listName, listPointer, moreLanesLine, moreLine, nextSort, sentenceCase, sharedKeys, shortKey, sortRows, statusOrder, ticketAddress, type CardPerson, type LaneField, type SortColumn } from "./tickets";
  import ViewEditor from "./ViewEditor.svelte";
  import ViewMenu from "./ViewMenu.svelte";
  import {
    columnLimit,
    draftCopy,
    draftEdit,
    draftFromFilter,
    draftSavedViewCopy,
    listColumnsOf,
    draftNew,
    draftRestore,
    drawnSavedViewColumns,
    groupViews,
    hiddenViews,
    savedViewSourceId,
    savedViewsFoot,
    savedViewSystems,
    LIST_COLUMN_ORDER,
    OWN_GROUPS,
    savedShowing,
    type CopiedFrom,
  } from "./views";
  import { pageVisible } from "../kept-pages.svelte";
  import { chosenJob } from "../job/chosen.svelte";
  import { scopedTicket } from "../job/scoped.svelte";
  import { layers } from "../layers.svelte";
  import { keyOf } from "../job/job";
  import TfCrumbs from "../TfCrumbs.svelte";
  import TfRailBar from "../TfRailBar.svelte";
  import type { EscTarget } from "../escape";
  import { filterKeys } from "../filter-keys";
  import { echoedEntities, echoOn, settleEchoes, watchEchoes } from "../queued.svelte";
  import { sameStatus } from "./drop";
  import { withFieldEchoes, withFieldRows, withStatusEchoes, withStatusRows, type LanedBy } from "./echo-board";
  import { createCardFields, offerFields } from "./card-fields.svelte";
  import TfPeoplePick from "../job/TfPeoplePick.svelte";
  import { askReach, asking as reachAsking, moveRefusal, noteObserved, reachOf } from "./reach.svelte";
  import { dismissNote, movedStatus, moveTicket, moveWaits, MOVE_KEY, offerMoves, ticketNotes, undoLastMove, watchTicketNotes, type MoveOffer } from "./moves.svelte";
  import StatusMenu from "./StatusMenu.svelte";
  import { createCardDrag } from "./card-drag.svelte";

  let {
    ready,
    ports = {
      board: (perColumn, query, savedView, lanes, role, page) =>
        miniBoard({ role: role ?? null, context: null, project: null }, perColumn, query, savedView, lanes, page),
      openExternal,
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
    back = null,
    sync,
    syncRevision = 0,
    syncAsked = 0,
    health = sharedHealth,
    demo = lifecycle.status?.demo ?? false,
  }: {
    /** Whether the database answers yet. */
    ready: boolean;
    ports?: TicketsPorts;
    /** The ticket view `#/tickets/<id>` names, or `null` for *All tickets*. */
    viewId?: string | null;
    /** How long, in ms, the filter waits for the next key before it reads: the launcher's 90. */
    typingPause?: number;
    /** Where `Esc` goes, which the sub-tasks bar's hint names. */
    back?: EscTarget | null;
    /** The sources knobas knows, which says when there are none at all (ux31: a fresh profile's first page). */
    health?: Pick<Health, "phase" | "all" | "reseed">;
    /** The `--demo` profile (P13), which offers the Tidewater dataset while it has no source. */
    demo?: boolean;
  } & RoleFrameProps = $props();

  /**
   * No source at all, as an answer rather than a store not read yet (#746: an empty store is a claim only in
   * `ready`). What a fresh profile opens on since the first-run wizard went (ux31): the way to add one, and to
   * restore a backup first, which knobas takes only into a database with no data.
   */
  const noSources = $derived(health.phase === "ready" && health.all.length === 0);

  /** The key whose sub-tasks a ticket card's row narrowed this view to (`job/scoped.svelte.ts`), or `null`. */
  const subtasksOf = $derived(viewId === null && scopedTicket.subtasksOf !== null ? keyOf(scopedTicket.subtasksOf) : null);
  /**
   * The preview's *Sub-tasks* row narrows this page where it stands, so no history entry holds the way back that a
   * ticket card's row has. `Esc` still undoes it, here as a panel on the layer stack, and the bar says so.
   */
  const narrowedHere = $derived(subtasksOf !== null && back === null);
  $effect(() => {
    if (!narrowedHere || !pageShown()) return;
    return untrack(() =>
      layers.add({
        kind: "panel",
        close: () => {
          scopedTicket.clear();
          // The list is read again and its rows drawn anew, so a keyboard's place goes to the page's heading.
          shownHeading()?.focus({ preventScroll: true });
        },
      }),
    );
  });
  const shownHeading = () => [...document.querySelectorAll<HTMLElement>(".tf-board-title h1")].find((heading) => !heading.closest(".tf-kept[hidden]"));
  /**
   * Narrowed where it stands, the page's own history entry says so, as an entry a card's row opened does, so a ticket
   * opened from here and `Esc` come back to the same narrowed list (F-141). The shell reads this stamp on arriving.
   */
  $effect(() => {
    const trip = subtasksOf !== null ? scopedTicket.trip : null;
    if (back !== null || !pageShown() || parseTfHash(location.hash).view !== "tickets") return;
    const had = stampOf(history.state);
    if (had === null || (had.trip?.entityId ?? null) === (trip?.entityId ?? null)) return;
    history.replaceState(stamped(history.state, { ...had, trip }), "");
  });
  // Arriving narrowed, the press's own control has gone or is hidden: focus takes the page's heading, so Tab starts here (F-148).
  // A mouse's press left focus on its link, in a page now hidden, which the browser only drops at the next frame.
  $effect(() => {
    if (subtasksOf === null || !pageShown()) return;
    void tick().then(() => {
      const at = document.activeElement;
      if (at !== document.body && at !== null && at.closest("[hidden]") === null) return;
      shownHeading()?.focus({ preventScroll: true });
    });
  });

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
  const base = $derived(view === null ? (subtasksOf === null ? "" : `under:${subtasksOf}`) : view === undefined || view.unreadable !== null ? null : view.query);
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
  // ---- a card's fields (ADR-0054, section 26): *Assign to…*, *Priority ▸*, `a`, and lanes that take a drop ----
  /** The card the people picker stands at, and the element it opened from. */
  let picking = $state<{ card: MiniBoardCard; opener: HTMLElement } | null>(null);
  let pickOpen = $state(false);
  const cardFields = createCardFields({
    cardOf: (entityId) => cardById(entityId),
    sourceName: (sourceId) => sourceRoles.nameOf(sourceId),
    edits: (sourceId) => sourceRoles.writes(sourceId, "edit_ticket"),
    openPicker: (card) => openPicker(card),
    get submitWrite() {
      return ports.fields?.submitWrite;
    },
    get refused() {
      return ports.fields?.refused;
    },
    get fields() {
      return ports.fields?.fields;
    },
    get follower() {
      return ports.fields?.follower;
    },
  });
  /** What the board's lanes are by, `null` without lanes. */
  function lanedBy(): LanedBy {
    const by = reader.lanes;
    return by === "none" ? null : by;
  }
  /** The people picker at `card`'s card or row, its fields asked. */
  function openPicker(card: MiniBoardCard) {
    if (cardFields.why(card, "assignee") !== null) return;
    const opener = document.querySelector<HTMLElement>(`.tf-tickets [data-entity="${CSS.escape(card.entity_id)}"]`);
    if (opener === null) return;
    cardFields.ask(card.entity_id);
    picking = { card, opener };
    pickOpen = true;
  }
  // The picker's list, once the ticket's assignee field is known.
  $effect(() => {
    const at = picking;
    if (at === null || !pickOpen) return;
    const field = cardFields.assignee(at.card);
    if (field !== null && field.kind.type === "search") untrack(() => cardFields.search(at.card, field, ""));
  });

  // svelte-ignore state_referenced_locally
  // Read once, on purpose: the reader owns the typing pause and the generations, so a bridge or a pause swapped
  // mid-life would drop both. Nothing changes these props: production omits them and tests pass fakes at mount.
  const reader = createBoardReader(ports, {
    typingPause,
    sessions: session,
    view: () => ({
      columns: ownColumns,
      listed: view?.columns ?? [],
      layout: saved.layout,
      lanes: saved.lanes,
      showAll: view?.show_all ?? false,
      sort: session.views[sessionKey]?.sort ?? saved.sort,
    }),
    // A move is drawn in its new column at once, over every read (ADR-0054); a field edit on its card, in its new lane.
    overlay: {
      // Only an entity with an echo is asked after its echoes: one read of which have one, not one a card.
      board: (board) => {
        const echoed = echoedEntities();
        const statusOf = (id: string) => (echoed.has(id) ? movedStatus(id) : null);
        const fieldsOf = (id: string) => (echoed.has(id) ? cardFields.echoed(id) : null);
        return withFieldEchoes(withStatusEchoes(board, statusOf), fieldsOf, lanedBy());
      },
      rows: (rows) => withFieldRows(withStatusRows(rows, movedStatus), cardFields.echoed),
    },
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
  /**
   * What the server read from the typed text, marked on the cards (#1903): the text the board drawn was read with, so a
   * key marks nothing until its answer draws (the ux19 overs pass).
   */
  const needles = $derived(readNeedles(reader.drawnFilter.text ?? filter.text));
  let suggested = $state<Pick<FilterSuggestions, "fields" | "parents"> | null>(null);
  let cardPeople = $state.raw<ReadonlyMap<string, CardPerson>>(new Map());

  /** At most this many chips a group; past it a field has too many values for chips. */
  const CHIPS_AT_MOST = 6;

  const config: FilterConfig = $derived.by(() => {
    const values = (key: string): FilterOption[] =>
      (suggested?.fields ?? [])
        .filter((field) => field.key === key)
        .map((field) => ({ value: field.value, label: field.value }));
    return {
      id: filterId,
      placeholder: "Filter tickets",
      noun: "tickets",
      one: "ticket",
      base: base ?? "",
      chips: [
        { key: "issuetype", label: "Type", options: values("issuetype").slice(0, CHIPS_AT_MOST) },
        { key: "priority", label: "Priority", options: values("priority").slice(0, CHIPS_AT_MOST) },
      ],
      dropdowns: [
        { key: "assignee", label: "Assignee", options: assigneeOptions(values("assignee").map((option) => option.value), cardPeople) },
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
    return { query: toQuery(config, filter), words: filter.text.trim() !== "", text: filter.text };
  }

  function sortBy(column: SortColumn) {
    viewSession(session, sessionKey).sort = nextSort(showing.sort, column);
    // A view that shows every ticket sorts its list on the server: its first page again.
    reader.resort();
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
  // asked once per view per visit, so an answer that still says *new* cannot loop. Only once it is shown: a page the
  // shell holds ahead of its press (`ahead.ts`) has opened nothing for the reader.
  const openedNew = new Set<string>();
  $effect(() => {
    const opened = view;
    if (!opened || opened.saved_view_source?.new !== true || openedNew.has(opened.id) || !pageShown()) return;
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
  // Who the cards name, so *Assignee* says what the rows say and offers who they show (F-161). Kept as a filter narrows the cards.
  $effect(() => {
    const known = untrack(() => cardPeople);
    const found = reader.bands
      .flatMap(({ drawn }) => drawn.flatMap((column) => column.cards))
      .filter((card) => card.assignee != null && (!known.has(card.assignee) || (card.assignee_name != null && known.get(card.assignee)?.name !== card.assignee_name)));
    if (found.length > 0) cardPeople = new Map([...known, ...found.map((card): [string, CardPerson] => [card.assignee ?? "", { name: card.assignee_name ?? null, mine: card.mine }])]);
  });
  /**
   * The list's rows: every card read, in the header's order; each card says its own status (#1423). A view that
   * shows every ticket draws its pages in the server's order of the sort (ux18).
   */
  const rows = $derived(
    reader.rows !== null
      ? reader.rows.map((card) => ({ card }))
      : sortRows(
          boardColumns.flatMap((column) => column.cards.map((card) => ({ card }))),
          showing.sort,
          statusOrder(boardColumns),
        ),
  );
  // -- cards and rows stay where they first drew (the still-rows pass, 2026-11-11) --
  /**
   * What the person chose, and each read they caused: a change sorts the cards again. A sync's read holds them. The
   * filter is the one the board drawn answers, not the bar's: a key sorted the board it drew before its answer, which
   * drew cards a sync had held back in place of others, 5 of 48 at 5120 px on Björn's PC (the ux19 overs pass).
   */
  const heldBasis = $derived(JSON.stringify([filterId, reader.drawnFilter.query, showing, reader.lanesShown, reader.asked]));
  type HeldCard = { tag: string; card: MiniBoardCard };
  /**
   * Every drawn card in reading order, each in the lane and column it drew in. The columns stand side by side, so a card
   * a later read brings below a column's drawn ones joins that column's foot (`byTag`): the whole read behind a view
   * that shows every ticket, or behind a key on an expanded board, moves nothing drawn (ux18).
   */
  const boardHold = new HeldOrder<HeldCard>(({ card }) => card.entity_id, ({ tag }) => tag, { byTag: true });
  const placeAt = (lane: { key: string | null }, column: MiniBoardColumn) => `${lane.key ?? ""}\u0001${columnKey(column)}`;
  const heldBoard = $derived(
    showing.layout === "board" && reader.board !== null
      ? boardHold.place(
          reader.bands.flatMap(({ lane, drawn }) => drawn.flatMap(({ column, cards }) => cards.map((card) => ({ tag: placeAt(lane, column), card })))),
          heldBasis,
        )
      : { rows: [] as HeldCard[], waiting: 0, stale: false },
  );
  /** The columns too: a status a sync brings is a column that would land left of the drawn ones and slide them all. */
  type Drawn = (typeof reader.bands)[number]["drawn"][number];
  type HeldColumn = { lane: string; entry: Drawn };
  const columnHold = new HeldOrder<HeldColumn>(({ lane, entry }) => `${lane}\u0001${columnKey(entry.column)}`, ({ lane }) => lane);
  const heldColumns = $derived(
    showing.layout === "board" && reader.board !== null
      ? columnHold.place(reader.bands.flatMap(({ lane, drawn }) => drawn.map((entry) => ({ lane: lane.key ?? "", entry }))), heldBasis)
      : { rows: [] as HeldColumn[], waiting: 0, stale: false },
  );
  const columnsOf = (lane: { key: string | null }) => heldColumns.rows.filter((held) => columnHold.tagOf(held) === (lane.key ?? "")).map(({ entry }) => entry);
  /** A column with no card drawn: one array, so it is the same one each time. */
  const NO_CARDS: MiniBoardCard[] = [];
  /** The arrays the columns were last handed: a column whose cards did not change keeps its own (`keepColumnArrays`). */
  let drawnCards = new Map<string, MiniBoardCard[]>();
  const heldCards = $derived.by(() => {
    const by = new Map<string, MiniBoardCard[]>();
    for (const entry of heldBoard.rows) {
      const at = boardHold.tagOf(entry);
      const column = by.get(at);
      if (column === undefined) by.set(at, [entry.card]);
      else column.push(entry.card);
    }
    drawnCards = keepColumnArrays(drawnCards, by);
    return drawnCards;
  });
  // -- a column that grows draws what the screen shows first (the broad-filters pass, 2026-11-13) --
  /**
   * The first key of a filter takes each column from the eight cards it drew to fifty (R-3.4): 300 new cards in one
   * frame, about 45 ms of a quarter-speed main thread, where the screen shows five to eight a column. In the frame
   * the answer lands such a column draws its first {@link FIRST_FRAME_CARDS}, and the rest the frame after, below
   * them: nothing drawn moves. A column that already drew that many draws whole at once, so an answer that changes
   * what a full column holds never takes cards away from under the pointer.
   */
  const FIRST_FRAME_CARDS = 16;
  /** The board every card of which has drawn; until the frame after an answer lands, the one before it. */
  let wholeBoard = $state.raw<MiniBoard | null>(null);
  /** How many cards each column (by its place) drew when the last board drew whole. */
  const drewWhole = new Map<string, number>();
  const firstFrame = $derived(reader.board !== null && reader.board !== wholeBoard);
  $effect(() => {
    const board = reader.board;
    if (board === null || board === wholeBoard) return;
    // After the frame that draws the first cards has painted: a task queued from inside the frame's callback.
    let task: ReturnType<typeof setTimeout> | undefined;
    const frame = requestAnimationFrame(() => (task = setTimeout(() => (wholeBoard = board), 0)));
    return () => {
      cancelAnimationFrame(frame);
      clearTimeout(task);
    };
  });
  $effect(() => {
    if (firstFrame) return;
    drewWhole.clear();
    for (const [place, cards] of heldCards) drewWhole.set(place, cards.length);
  });
  /** The cards a column draws now: the first frame's share of a column that grows past it, or every one. */
  function drawnNow(place: string, cards: MiniBoardCard[]): MiniBoardCard[] {
    if (!firstFrame || cards.length <= FIRST_FRAME_CARDS || (drewWhole.get(place) ?? 0) >= FIRST_FRAME_CARDS) return cards;
    return cards.slice(0, FIRST_FRAME_CARDS);
  }
  /**
   * A view that shows every ticket opens on fifty a column, and a column of fifty fills to the screen's foot, about 15
   * cards where any other view draws eight: twice the work of the frame its board lands, a 194 ms task at CPU 6x on
   * Björn's PC against 105 with the setting off (ux18). That frame draws the eight a column any view draws, and the
   * rest of the screen's cards the frame after, below them: nothing drawn moves. The column keeps every card's room
   * meanwhile (`TicketStack`'s `atMost`), so it stands as tall and its near-end mark stays out of reach.
   *
   * A key does the same for a column that windows (the ux18 Windows pass, Fable's ruling): its first key takes each
   * column from eight to fifty, and at 5120 px wide the screen shows about twelve cards of each of six 520 px columns,
   * 69 cards, which took the frame the answer lands from 32 to 45 ms at CPU 4x. In that frame a column that showed eight
   * or fewer draws eight, 48 cards, and the rest of its screen the frame after, below them. A column that showed more
   * draws its screen at once, so no card drawn goes from under the pointer. Under 1500 px the screen shows eight a
   * column or fewer, so nothing changes there.
   */
  function openShare(place: string, cards: MiniBoardCard[]): number | null {
    if (!firstFrame || cards.length <= COLUMN_CAP) return null;
    const drew = drewWhole.get(place) ?? 0;
    if (reader.all && drew === 0) return COLUMN_CAP;
    return cards.length >= STACK_WINDOW_FROM && drew <= COLUMN_CAP ? COLUMN_CAP : null;
  }
  /**
   * What a column's window reckons an undrawn card at: never taller than it draws, so a key draws the screen's cards. Its
   * summary's lines counted by the column's width, which goes by the window (`tickets.css`), and Interface's size.
   */
  const cardFloorOf = (card: MiniBoardCard) =>
    cardFloor(card, view?.card_fields ?? null, columnWidth(innerWidth.current ?? 1440), fonts.size("ui") / 100);
  const listHold = new HeldOrder<{ card: MiniBoardCard }>(({ card }) => card.entity_id);
  const heldList = $derived(
    showing.layout === "list" && reader.board !== null ? listHold.place(rows, heldBasis) : { rows: [] as { card: MiniBoardCard }[], waiting: 0, stale: false },
  );
  const held = $derived(showing.layout === "board" ? heldBoard : heldList);
  const heldBar = $derived({
    waiting: held.waiting,
    stale: held.stale || (showing.layout === "board" && heldColumns.stale),
    show: () => {
      if (showing.layout === "board") {
        boardHold.release();
        columnHold.release();
      } else listHold.release();
    },
  });
  const listRows = $derived(heldList.rows);

  // ---- a ticket's move (ADR-0054, ticket-edit decisions sections 3 to 13) ----
  /** Where each moved card stood on the board, for the way back of a move refused, held or withdrawn. */
  const movedFrom = new Map<string, { tag: string; rank: number }>();
  // The refresh after a move, or a run of its source's end, reads the board again, so its echo settles. Not under a held
  // card: the read waits for its drop (`afterDrag`), so the columns under the copy are the ones it was picked up from.
  const refreshBoard = () => reader.refresh();
  /** The board read again after the person's *Sync now*: its answer sorts the cards again (`refresh(true)`). */
  const sortBoard = () => reader.refresh(true);
  $effect(() => untrack(() => watchEchoes("tickets", () => drag.afterDrag(refreshBoard), { listen: ports.listen })));
  // The notes of refused and held moves, from the queue; and a move the queue still holds, drawn after a restart.
  $effect(() => untrack(() => watchTicketNotes(ports.moves ?? {})));
  // Each read: the statuses each source uses, and a move's echo gone once the read shows its status. The read as the
  // source said it, before the overlay, or the echo would settle on itself.
  $effect(() => {
    const read = reader.read;
    const rowsRead = reader.readRows;
    if (read !== null) untrack(() => noteObserved(read.sources));
    const cards = [...(read?.columns.flatMap((column) => column.cards) ?? []), ...(rowsRead ?? [])];
    untrack(() => {
      for (const card of cards) {
        const echo = echoOn(card.entity_id, "status");
        if (echo === null || !sameStatus(card.status, echo.text)) continue;
        settleEchoes(card.entity_id, (one) => one.kind === "status" && sameStatus(card.status, one.text));
        movedFrom.delete(card.entity_id);
      }
      cardFields.settle(cards);
    });
  });

  /** A card by its entity id, among what the board or the list read. */
  function cardById(entityId: string): MiniBoardCard | null {
    for (const column of reader.board?.columns ?? []) for (const card of column.cards) if (card.entity_id === entityId) return card;
    return reader.rows?.find((card) => card.entity_id === entityId) ?? null;
  }

  /** The lane key `card` stands in on the board, `""` without lanes. */
  function laneOf(card: MiniBoardCard): string {
    const entry = heldBoard.rows.find((one) => one.card.entity_id === card.entity_id);
    return entry === undefined ? "" : (boardHold.tagOf(entry).split("\u0001")[0] ?? "");
  }

  /** Why `card` cannot be moved from here, or `null`. */
  function moveBlocked(card: MiniBoardCard): string | null {
    const source = sourceRoles.nameOf(card.source_id);
    if (!sourceRoles.writes(card.source_id, "transition")) return `${source} tickets cannot be moved from knobas: its adapter offers no moves.`;
    return moveRefusal(card.entity_id) ?? null;
  }

  /** The board column a move to `status` lands `card` in, in its own lane, and the card it lands above: the column's top. */
  function landing(card: MiniBoardCard, status: string): { place: string; before: string | null } | null {
    const entry = heldBoard.rows.find((one) => one.card.entity_id === card.entity_id);
    if (entry === undefined) return null;
    const lane = boardHold.tagOf(entry).split("\u0001")[0] ?? "";
    for (const { lane: band, drawn } of reader.bands) {
      if ((band.key ?? "") !== lane) continue;
      const column = drawn.find(({ column: one, listed }) => (listed.length > 0 ? listed.some((named) => sameStatus(named, status)) : sameStatus(one.status, status)));
      if (column === undefined) return null;
      const place = placeAt(band, column.column);
      const first = (heldCards.get(place) ?? []).find((one) => one.entity_id !== card.entity_id);
      return { place, before: first?.entity_id ?? null };
    }
    return null;
  }

  /**
   * The card back where it stood: its move was refused, held or withdrawn. The one move on the board nobody's hand made,
   * so it is shown (Björn, 2026-11-22: "it must be obvious what happened"): the card glides from where it stood into its
   * old place, and its note says why. Under `prefers-reduced-motion` it is simply there.
   */
  function putBack(entityId: string) {
    const was = movedFrom.get(entityId);
    movedFrom.delete(entityId);
    if (was === undefined) return;
    const find = () => document.querySelector<HTMLElement>(`.tf-tickets .tf-tcard[data-entity="${CSS.escape(entityId)}"]`);
    const from = find()?.getBoundingClientRect() ?? null;
    boardHold.restore(entityId, was);
    if (from === null || typeof matchMedia === "undefined" || matchMedia("(prefers-reduced-motion: reduce)").matches) return;
    void tick().then(() => {
      const card = find();
      const to = card?.getBoundingClientRect();
      if (!card || !to || typeof card.animate !== "function") return;
      const dx = from.left - to.left;
      const dy = from.top - to.top;
      if (Math.abs(dx) < 1 && Math.abs(dy) < 1) return;
      card.animate([{ transform: `translate(${dx}px, ${dy}px)` }, { transform: "none" }], { duration: RETURN_MS, easing: "cubic-bezier(0.2, 0, 0, 1)" });
    });
  }
  /** How long a refused card takes to glide back to its column. */
  const RETURN_MS = 260;

  /**
   * Move `card` to `status`: on the board it stands at once where it was dropped (`at`, section 8), or at the top of its
   * new column for a move by menu.
   */
  function moveCard(card: MiniBoardCard, status: string, at: { place: string; before: string | null } | null = null) {
    if (moveBlocked(card) !== null) return;
    const id = card.entity_id;
    if (showing.layout === "board") {
      const lands = at ?? landing(card, status);
      const was = lands === null ? null : boardHold.move(id, lands.place, lands.before);
      if (was !== null && !movedFrom.has(id)) movedFrom.set(id, was);
    }
    void moveTicket(card, status, { ...(ports.moves ? { ports: ports.moves } : {}), onstop: () => putBack(id), onwithdrawn: () => putBack(id) });
  }

  /** The chip a moved card draws on its head line, in a column holding `listed` statuses (several: it names the one picked). */
  function writeOf(card: MiniBoardCard, listed: readonly string[]): { words: string; other: string; tone: "info" | "muted"; title?: string } | null {
    const echo = echoOn(card.entity_id, "status");
    // No move: the newest field edit's chip, where it has one (section 26).
    if (echo === null) return cardFields.chip(card.entity_id);
    const sent = echo.state === "sent";
    const several = listed.length > 1;
    const queued = several ? `${echo.text}, queued` : "Queued";
    const done = several ? `${echo.text}, sent` : "Sent";
    const title = echo.write === null ? undefined : moveWaits[echo.write];
    return { words: sent ? done : queued, other: sent ? queued : done, tone: sent ? "muted" : "info", ...(title === undefined ? {} : { title }) };
  }

  /** What *Move to ▸* in a ticket's right-click menu offers (section 10). */
  function offerFor(entityId: string): MoveOffer | null {
    const card = cardById(entityId);
    if (card === null) return null;
    const source = sourceRoles.nameOf(card.source_id);
    const blocked = moveBlocked(card);
    if (blocked !== null) return { statuses: [], asking: null, unsure: null, disabled: blocked };
    const reach = reachOf(card);
    if (reach === null) return { statuses: [], asking: reachAsking(card) ? `Asking ${source}…` : null, unsure: reachAsking(card) ? null : `${source} did not answer; it decides when the move is sent.`, disabled: null };
    const statuses = reach.statuses.filter((status) => !sameStatus(status, card.status));
    const unsure = reach.standing !== "exact" && reach.failed !== undefined ? `${source} did not answer; it decides when the move is sent.` : null;
    if (statuses.length === 0 && unsure === null) return { statuses: [], asking: null, unsure: null, disabled: `${source} offers no move from ${card.status ?? "no status"}.` };
    return { statuses, asking: null, unsure, disabled: null };
  }

  // While shown, the right-click menu's *Move to* on a ticket is this page's.
  $effect(() => {
    if (!pageShown()) return;
    return untrack(() =>
      offerMoves({
        offer: offerFor,
        ask: (id) => {
          const card = cardById(id);
          if (card !== null) void askReach(card, "press", ports.moves?.reachableTransitions);
        },
        move: (id, status) => {
          const card = cardById(id);
          if (card !== null) moveCard(card, status);
        },
      }),
    );
  });

  // While shown, *Assign to…* and *Priority ▸* on a ticket are this page's (section 26).
  $effect(() => {
    if (!pageShown()) return;
    return untrack(() =>
      offerFields({
        offer: (id) => cardFields.offer(id),
        ask: (id) => cardFields.ask(id),
        assign: (id) => {
          const card = cardById(id);
          if (card !== null) openPicker(card);
        },
        priority: (id, value, label) => {
          const card = cardById(id);
          if (card !== null) cardFields.priority(card, value, label);
        },
      }),
    );
  });

  // -- the drag (sections 7 to 10) --
  /** What the live region says: each step of a carry, a drop, a put back. */
  let said = $state("");
  /** The status menu at a drop point, and what its pick answers. */
  let dropMenu = $state<{ card: MiniBoardCard; statuses: string[]; at: { x: number; y: number }; answer: (status: string | null) => void } | null>(null);
  let dropMenuOpen = $state(false);
  /** The column a card stands in, as the drag reads it: the statuses it gathers, by name. */
  function dropColumnOf(column: MiniBoardColumn, listed: readonly string[]) {
    const statuses = listed.length > 0 ? [...listed] : column.status === null ? [] : [column.status];
    return { name: columnHead(column.status, view?.columns ?? []), statuses };
  }
  const drag = createCardDrag({
    cardOf: (entityId) => {
      const entry = heldBoard.rows.find((one) => one.card.entity_id === entityId);
      if (entry === undefined) return null;
      const place = boardHold.tagOf(entry);
      return { card: entry.card, place, lane: place.split("\u0001")[0] ?? "" };
    },
    columns: () => {
      const byPlace = new Map<string, { lane: string; column: { name: string; statuses: string[] } }>();
      for (const { lane } of reader.bands) for (const { column, listed } of columnsOf(lane)) byPlace.set(placeAt(lane, column), { lane: lane.key ?? "", column: dropColumnOf(column, listed) });
      return [...document.querySelectorAll<HTMLElement>(".tf-tickets .tf-col[data-place]")]
        .filter((node) => node.closest(".tf-kept[hidden]") === null)
        .flatMap((node) => {
          const known = byPlace.get(node.dataset.place!);
          return known === undefined ? [] : [{ place: node.dataset.place!, lane: known.lane, column: known.column, node }];
        });
    },
    facts: (card) => ({
      source: sourceRoles.nameOf(card.source_id),
      moves: sourceRoles.writes(card.source_id, "transition"),
      refused: moveRefusal(card.entity_id) ?? null,
      lane: (key) => cardFields.lane(card, lanedBy(), key),
    }),
    drop: (card, to) => {
      const moving = !sameStatus(to.status, card.status);
      const from = laneOf(card);
      if (moving) moveCard(card, to.status, { place: to.place, before: to.before });
      if (to.lane === from) return;
      // Across lanes (section 26): the move first, then the edit of the field the board is laned by.
      if (!moving) {
        const was = boardHold.move(card.entity_id, to.place, to.before);
        if (was !== null && !movedFrom.has(card.entity_id)) movedFrom.set(card.entity_id, was);
      }
      return cardFields.relane(card, lanedBy(), to.lane) ?? undefined;
    },
    choose: (card, statuses, at) =>
      new Promise((answer) => {
        dropMenu = { card, statuses, at, answer };
        dropMenuOpen = true;
      }),
    ask: (card, when) => {
      // Laned by a field a drop edits: whether the lanes take it is asked at pickup too (section 26).
      const by = lanedBy();
      if (when === "pickup" && (by === "assignee" || by === "priority")) cardFields.ask(card.entity_id, "ahead");
      return askReach(card, when, ports.moves?.reachableTransitions);
    },
    scroller: () => {
      const wrap = document.querySelector<HTMLElement>(".tf-tickets .tf-board-wrap");
      const cols = wrap?.querySelectorAll<HTMLElement>(".tf-cols") ?? [];
      return cols.length === 1 ? cols[0]! : (wrap ?? document.body);
    },
    say: (text) => (said = text),
  });
  // The layout or the view changes, or the page hides: a held card goes back.
  $effect(() => {
    void showing.layout;
    void viewId;
    if (!pageShown()) untrack(() => drag.putBack());
    return () => untrack(() => drag.putBack());
  });

  // -- the list's Status cell (section 10) --
  /** The ticket whose status menu stands, and the cell that opened it. */
  let statusMenu = $state<{ card: MiniBoardCard; opener: HTMLElement } | null>(null);
  let statusMenuOpen = $state(false);
  function openStatusMenu(card: MiniBoardCard, opener: HTMLElement) {
    if (moveBlocked(card) !== null) return;
    void askReach(card, "press", ports.moves?.reachableTransitions);
    statusMenu = { card, opener };
    statusMenuOpen = true;
  }

  /** How many lanes the board draws: the scroller's `tf-lane-one` or `tf-lanes` (`tickets.css`). */
  let lanesDrawn = $state(0);
  /** On each lane drawn: counts it while it is there. Untracked, or the attachment would run again on its own count. */
  function countLane(): () => void {
    untrack(() => (lanesDrawn += 1));
    return () => untrack(() => (lanesDrawn -= 1));
  }
  /** Each board column's window, by its place, while it is drawn (`TicketStack`): `j`/`k` draws a card before focusing it. */
  const stackWins = new Map<string, Windowed>();
  const holdWin = (place: string) => (stacked: Windowed | null) => {
    if (stacked === null) stackWins.delete(place);
    else stackWins.set(place, stacked);
  };
  /**
   * The list after *Show N more* draws only the rows near the viewport (#2209); a row is 51 to 53 px, 70 when its summary wraps.
   * From 40 rows, not the default 300: a typed word narrows the list into that range, and each key drew every match whole,
   * its worst key 145 ms at CPU 4x in knobas's window on Björn's PC (the ux13 Windows pass, 2026-11-15), as Inbox windows from 40.
   */
  const win = new Windowed({ count: () => listRows.length, key: (i) => listRows[i]!.card.entity_id, estimate: 52, from: 40 });
  /** The keys two sources' cards hold (#1476): those cards name their source and open by it. */
  const shared = $derived(sharedKeys(reader.rows !== null ? [{ status: null, cards: reader.rows, total: reader.rows.length }] : boardColumns));
  /**
   * Whether each drawn card named its source, held until the person's next read (`reader.asked`): a read that only adds
   * cards (the whole read after a key's fifty a column, a sync's) can bring the twin of a card drawn above, and naming
   * the source then grew that card by a line under the pointer (the ux13-overs pass, 2026-11-15). Plain, not state: it
   * is worked out as each card draws.
   */
  let named = new Map<string, boolean>();
  let namedAt = -1;
  /** A card's source name when its key is shared, else `null`. */
  const sourceOf = (card: MiniBoardCard) => {
    if (reader.asked !== namedAt) {
      named = new Map();
      namedAt = reader.asked;
    }
    let names = named.get(card.entity_id);
    if (names === undefined) {
      names = shared.has(card.key);
      named.set(card.entity_id, names);
    }
    return names ? sourceRoles.nameOf(card.source_id) : null;
  };
  /** The list's columns: the view's, in the list's order (R-3.16). */
  const listColumns = $derived(
    LIST_COLUMN_ORDER.filter(([column]) => listColumnsOf(view).includes(column)),
  );
  /** How many characters of a key `.tf-c-key` holds in the code face. */
  const KEY_ROOM = 12;
  const SORTABLE: readonly SortColumn[] = ["key", "summary", "status", "priority", "assignee", "updated"];
  const sortable = (column: string): column is SortColumn => (SORTABLE as readonly string[]).includes(column);
  const listCut = $derived(boardColumns.some((column) => column.cards.length < column.total));
  const listRest = $derived(
    boardColumns.reduce((all, column) => all + column.total - column.cards.length, 0),
  );
  /** What is on screen, in reading order: the first is selected when the selection is not. A card says its column and place in it. */
  const visible: { card: MiniBoardCard; place?: string; at?: number }[] = $derived(
    showing.layout === "board"
      ? reader.bands.flatMap(({ lane }) =>
          columnsOf(lane).flatMap(({ column }) => {
            const place = placeAt(lane, column);
            return (heldCards.get(place) ?? []).map((card, at) => ({ card, place, at }));
          }),
        )
      : listRows,
  );
  const current = $derived(visible.find((entry) => entry.card.entity_id === session.selected) ?? visible[0] ?? null);

  /** A boolean of its own, so a views read that answers the same row again reads no board again. */
  const showsAll = $derived(view?.show_all ?? false);
  // The board, again for each view and once its text is known. Untracked,
  // so a read's own answer does not ask for the read again.
  $effect(() => {
    if (!ready) return;
    const scope = { key: sessionKey, base, without: subtasksOf };
    // Saved with *Show every ticket when the view opens* turned on or off, the view is read again as it now opens.
    void showsAll;
    untrack(() => reader.open({ ...scope, savedView: savedViewId }, filterNow()));
  });
  $effect(() => () => reader.close());

  // *Sync now*'s runs ended (#2106), or a run that changed something: the board is read again. The person's own *Sync
  // now* sorts it again too (`syncAsked`, the ux30 end gate): a ticket it brought that sorts above the drawn cards draws
  // there, where a run nobody here pressed leaves it behind *Show N new*.
  let boardAt = untrack(() => syncRevision);
  let askedAt = untrack(() => syncAsked);
  $effect(() => {
    const at = syncRevision;
    const asked = syncAsked;
    if (!ready || (at === boardAt && asked === askedAt)) return;
    const sorts = asked !== askedAt;
    boardAt = at;
    askedAt = asked;
    untrack(() => drag.afterDrag(sorts ? sortBoard : refreshBoard));
  });

  /**
   * Whether the filter bar has its chips: they wrap the bar onto a second line, so cards drawn before them moved down
   * 32 px when they landed (the still-rows pass, 2026-11-11). The board waits for them, or {@link FRAME_WAIT_MS} at most.
   */
  let framed = $state(false);
  const FRAME_WAIT_MS = 1500;

  // Once the database answers: the views and the suggestions.
  $effect(() => {
    if (!ready) return;
    const most = setTimeout(() => (framed = true), FRAME_WAIT_MS);
    untrack(() => {
      loadViews();
      ports
        .suggestions()
        .then((answer) => (suggested = answer))
        .catch(() => {})
        .finally(() => {
          clearTimeout(most);
          framed = true;
        });
    });
    return () => clearTimeout(most);
  });

  /** A long key gives up its middle, so `payout-service#143` and `#138` stay told apart: what the Key column's width holds, with the number whole. */
  function keepsItsEnd(key: string): string {
    if (key.length <= KEY_ROOM) return key;
    const number = key.lastIndexOf("#");
    const end = key.slice(number < 0 ? -5 : number);
    return `${key.slice(0, KEY_ROOM - 1 - end.length)}…${end}`;
  }

  /**
   * A long column's *See all N in the list*: the list, which draws only the rows on screen, filtered to the column's
   * statuses by words the box shows, so they read as any other filter and go as one.
   */
  function listColumn(column: MiniBoardColumn, listed: readonly string[]) {
    const words = columnWords(column, listed);
    filter.text = [filter.text.trim(), words].filter((part) => part !== "").join(" ");
    switchLayout("list");
    reader.setFilter(filterNow());
    reader.refresh();
  }

  /** The other layout, on the same ticket: the one the preview shows, even where nothing was picked yet. */
  function switchLayout(next: "board" | "list") {
    if (current !== null) session.selected = current.card.entity_id;
    reader.chooseLayout(next);
  }

  /** A click, a double click or `Enter`: the ticket page (spec #2227), the card selected for the way back. There is no preview to fill. */
  function openTicket(card: MiniBoardCard) {
    session.selected = card.entity_id;
    location.hash = ticketAddress(card);
  }

  /** `j`/`k` and the arrows (R-3.10): the next or previous card on screen, selected and focused. */
  /** With nothing selected and the first ticket scrolled away, a walk starts from what is in view, so its first key keeps the place (F-075). */
  function walkStart(by: 1 | -1): number | null {
    if (session.selected !== null) return null;
    const cards = [...document.querySelectorAll<HTMLElement>(".tf-tickets [data-entity]")];
    const first = cards.find((card) => card.dataset.entity === current?.card.entity_id);
    if (first === undefined || rowInView([first], by) !== null) return null;
    const seen = rowInView(cards, by) as HTMLElement | null;
    const found = visible.findIndex((entry) => entry.card.entity_id === seen?.dataset.entity);
    return found < 0 ? null : found;
  }

  function step(by: 1 | -1) {
    if (visible.length === 0) return;
    const at = current === null ? 0 : visible.findIndex((entry) => entry.card.entity_id === current.card.entity_id);
    const seen = walkStart(by);
    const to = seen ?? Math.min(visible.length - 1, Math.max(0, at + by));
    const next = visible[to]!;
    session.selected = next.card.entity_id;
    // A selected card is asked where it can go, behind, so a status menu or a pickup lights from memory (section 5).
    void askReach(next.card, "select", ports.moves?.reachableTransitions);
    // A row or card past the drawn ones is drawn first, in its column's window on the board; a row in view is drawn.
    if (seen === null) {
      if (showing.layout !== "board") win.reveal(to);
      else if (next.place !== undefined && next.at !== undefined) stackWins.get(next.place)?.reveal(next.at);
    }
    flushSync();
    const element = document.querySelector<HTMLElement>(`.tf-tickets [data-entity="${CSS.escape(next.card.entity_id)}"]`);
    element?.focus();
    element?.scrollIntoView?.({ block: "nearest" });
  }

  // `Enter` opens the selected or focused card's ticket page (spec #2227),
  // from anywhere but a field that types or a control of its own; a card is
  // not one.
  const pageShown = pageVisible();
  onMount(() => {
    const onkey = (event: KeyboardEvent) => {
      // A page keep-alive holds hidden (#2247) takes no keys.
      if (!pageShown()) return;
      // `⌘Z` / `Ctrl+Z` undoes the last move while its toast stands (ADR-0054), outside a field and with nothing open.
      if (event.key.toLowerCase() === "z" && (event.metaKey || event.ctrlKey) && !event.shiftKey && !event.altKey && !event.defaultPrevented) {
        if (layers.open.length > 0 || inTextField(document.activeElement)) return;
        if (undoLastMove()) event.preventDefault();
        return;
      }
      if (event.metaKey || event.ctrlKey || event.altKey || event.defaultPrevented) return;
      const at = document.activeElement;
      if (at instanceof HTMLElement && inTextField(at)) return;
      // The move key on the board picks up the focused card, else the selected one (the carry, section 10); on the list it
      // opens the row's status menu under its cell. The focused one first, as `Enter` reads it: a drop leaves focus on the
      // card it moved while the selection stays where it was.
      if (event.key === MOVE_KEY && current !== null && layers.open.length === 0) {
        const focused = at instanceof HTMLElement ? at.closest<HTMLElement>("[data-entity]") : null;
        const entry = (focused === null ? null : visible.find((one) => one.card.entity_id === focused.dataset.entity)) ?? current;
        if (showing.layout === "board") {
          if (drag.carry(entry.card.entity_id)) event.preventDefault();
          return;
        }
        const cell = document.querySelector<HTMLElement>(`.tf-tickets [data-entity="${CSS.escape(entry.card.entity_id)}"] .tf-statecell`);
        if (cell !== null) {
          event.preventDefault();
          openStatusMenu(entry.card, cell);
        }
        return;
      }
      // `a` on the selected card or row opens the people picker at it (section 26), as `a` does on the ticket's page.
      if (event.key === "a" && current !== null && layers.open.length === 0 && sourceRoles.writes(current.card.source_id, "edit_ticket")) {
        event.preventDefault();
        openPicker(current.card);
        return;
      }
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
      // A control of its own inside a row (its Status cell) takes its own Enter.
      if (at instanceof HTMLElement && card !== null && at !== card && at.closest("a, button, select") !== null && card.contains(at)) return;
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

  /** Below 900 px, whether the views rail is unfolded under its bar. */
  let railOpen = $state(false);
</script>

<div class="tf-tickets" class:tf-rail-shut={!railOpen} {@attach sideWidth("tickets-views")}>
  <TfSideEdge side="tickets-views" />
  <!-- Narrowed to a ticket's sub-tasks the page is a trip, named by its title with *Back to <key>* above it: stacked, a bar saying the same again only pushes the title down. -->
  {#if subtasksOf === null}
    <TfRailBar label="View" value={view?.name ?? "All tickets"} open={railOpen} controls="tf-views-rail" onpress={() => (railOpen = !railOpen)} onclose={() => (railOpen = false)} />
  {/if}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
  <nav
    id="tf-views-rail"
    class="tf-views tf-rail"
    aria-label="Ticket views"
    onclick={(event) => {
      if ((event.target as Element).closest("a[href]")) railOpen = false;
    }}
  >
    <div class="tf-vfind"><input type="search" bind:value={find} placeholder="Find a view" aria-label="Find a view" {@attach filterKeys({ aside: true })} /></div>
    <div class="tf-vlist">
      <div class="tf-vw-acts">
        <button type="button" class="tf-btn tf-sm" onclick={() => newView(draftNew())}>New view…</button>
      </div>
      {#if find.trim() === ""}
        <a href="#/tickets" aria-current={viewId === null && subtasksOf === null ? "true" : undefined} onclick={() => subtasksOf !== null && scopedTicket.clear()}
          >All tickets {#if viewId === null && subtasksOf === null && reader.total !== null}<span class="tf-count">{reader.total}</span>{/if}</a
        >
      {/if}
      {#each groups as group (group.name)}
        <h2>
          {group.name}{#if group.system !== null}{" "}<span class="tf-vw-sync">from the {group.system} source</span>{/if}
        </h2>
        {#each group.views as row (row.id)}
          <div class="tf-vw-row">
            <a href="#/tickets/{row.id}" aria-current={row.id === viewId ? "true" : undefined}
              ><span class="tf-vw-name" title={row.name}><Marked text={row.name} typed={find} /></span
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
        {savedViewsFoot(systems)} <a href={savedViewSettingsHref}>Choose which</a>
      </p>
    </div>
  </nav>

  <TfScroller class="tf-board-wrap{lanesDrawn > 1 ? ' tf-lanes' : lanesDrawn === 1 ? ' tf-lane-one' : ''}" {@attach fbarHeight} {@attach drag.attach}>
    {#if subtasksOf !== null || back !== null}
      <!-- A card's *Sub-tasks* row lands here narrowed, under the same bar the card's other rows land under. A launcher jump has a way back to say too (F-047). -->
      <TfCrumbs parts={[{ name: "Tickets" }]} back={back ?? { name: "All tickets" }} />
    {/if}
    <header class="tf-fhead tf-board-head">
      <div class="tf-board-title">
        <h1 tabindex="-1">
          {#if subtasksOf !== null}Sub-tasks of <TfKey value={subtasksOf} />{:else}{view?.name ?? (viewId === null ? "All tickets" : "This view")}{/if}
        </h1>
        {#if view && view.saved_view_source === null}<span class="tf-q"
            >{view.query.trim() === "" ? "every synced ticket" : view.query}</span
          >{/if}
      </div>
      <div class="tf-acts">
        {#if view !== undefined && (view === null || view.unreadable === null)}
          <div class="tf-seg" role="group" aria-label="Layout">
            <button type="button" aria-pressed={showing.layout === "board"} onclick={() => switchLayout("board")}
              >Board</button
            >
            <button type="button" aria-pressed={showing.layout === "list"} onclick={() => switchLayout("list")}
              >List</button
            >
          </div>
        {/if}
        <!-- With nothing to save it is not offered. On a saved view it is a copy, so a source that translates no query offers none (#1208). -->
        {#if isFiltering(filter) && (!view?.saved_view_source || offersCopy(view))}
          <button type="button" class="tf-btn tf-ghost" onclick={() => void saveAsView()}>Save as a view…</button>
        {/if}
        {#if view}
          <button type="button" class="tf-btn" onclick={() => editView(view)}
            >{view.saved_view_source === null ? "Edit view…" : "View settings…"}</button
          >
          <ViewMenu
            name="this view"
            savedViewKind={view.saved_view_source?.kind ?? null}
            copies={offersCopy(view)}
            className="tf-btn tf-ghost"
            onedit={() => editView(view)}
            onduplicate={() => copyView(view)}
            ondelete={() => remove(view)}
            onhide={() => void setHidden(view, true)}
          />
        {/if}
        {@render sync?.()}
      </div>
    </header>
    {#if view?.saved_view_source && view.saved_view}
      <SavedViewSourceLine
        savedViewSource={view.saved_view_source}
        name={view.name}
        sourceId={savedViewSourceId(view.saved_view)}
        openExternal={ports.openExternal ?? openExternal}
      />
    {/if}
    {#if base !== null}
      {#snippet lanesSwitch()}
        <!-- prototype-6 offers lanes on the ticket board only. The list keeps the select's room, unseen and out of the Tab order, as a
             header keeps an action a tab does not offer: the bar wraps, and taking the select out reflowed it under the reader
             (the nothing-moves pass, 2026-11-17). -->
        {@const off = showing.layout !== "board"}
          <label class="tf-fsel" class:tf-hidden={off} inert={off} aria-hidden={off ? "true" : undefined}>
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
      {/snippet}
      <FilterBar
        {config}
        {filter}
        shown={reader.shown}
        total={reader.total}
        onchange={() => reader.setFilter(filterNow())}
        extra={lanesSwitch}
        held={heldBar}
      />
    {/if}

    {#if viewId !== null && views !== null && view === undefined}
      <div class="tf-empty">
        <p>This view does not exist. It may have been deleted.</p>
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
    {:else if reader.board === null || !framed || reader.rowsWaiting}
      <div class="tf-empty" aria-busy="true"></div>
    {:else if reader.shown === 0 && isFiltering(filter)}
      <div class="tf-empty">
        <p>No tickets match.</p>
        <button type="button" class="tf-btn tf-sm" onclick={clearFilter}>Clear the filter</button>
      </div>
    {:else if reader.shown === 0}
      {#if view}
        <!-- A view that matches nothing is not an empty mirror. -->
        <div class="tf-empty">
          <p>No tickets match this view.</p>
          {#if view.saved_view_source === null}
            <button type="button" class="tf-btn tf-sm" onclick={() => editView(view)}>Edit view…</button>
          {:else}
            <a class="tf-btn tf-sm" href="#/tickets">All tickets</a>
          {/if}
        </div>
      {:else if noSources}
        <div class="tf-empty tf-nosources">
          <p>No sources yet. Add one and its tickets, pull requests, builds and pages come into knobas.</p>
          <div class="tf-empty-acts">
            <a class="tf-btn tf-sm tf-primary" href={ADD_SOURCE_HASH}>Add a source…</a>
            {#if demo}<TfLoadDemo small {health} />{/if}
          </div>
          <p class="tf-empty-note">
            Have a backup or a share export? Restore it before you add a source: knobas restores only into a database that holds no data yet.
            <a href={settingsHash("backup")}>Open Settings › Backup and export</a>
          </p>
        </div>
      {:else}
        <div class="tf-empty">
          <p>No tickets synced yet. Add a ticket system in Settings and they appear here.</p>
          <a class="tf-btn tf-sm" href="#/settings/sources">Open Settings › Sources</a>
        </div>
      {/if}
    {:else if showing.layout === "board"}
      {#each reader.bands as { lane } (lane.key ?? "")}
        <div class="tf-lane" class:tf-laned={reader.laned} {@attach countLane}>
          {#if lane.key !== null}
            <h2><TfKeyed text={lane.title} lead /> <span class="tf-count">{lane.count}</span></h2>
          {/if}
          <div class="tf-cols">
            {#each columnsOf(lane) as { column, cards, done, listed } (columnKey(column))}
              <!-- A view that shows every ticket offers no more: its whole read fills each column behind the first. -->
              {@const more = reader.all ? null : moreLine(column, cards.length, done)}
              {@const pointer = reader.all ? null : listPointer(column, cards.length)}
              {@const limit = columnLimit(view?.columns ?? [], column.status)}
              {@const place = placeAt(lane, column)}
              <!-- Its own derived: a stack handed the array it already draws hears nothing (`keepColumnArrays`). -->
              {@const stackCards = heldCards.get(place) ?? NO_CARDS}
              <div class="tf-col" data-place={place}>
                <h3>
                  {columnHead(column.status, view?.columns ?? [])}
                  <span
                    class="tf-count"
                    class:tf-err={limit !== null && column.total > limit}
                    title={limit !== null ? `At most ${limit} on this column` : undefined}
                    >{column.total}{#if limit !== null}{" "}of {limit}{/if}</span
                  >
                  <!-- While a card is held, a column that would ask says which statuses, in room its head's line has (section 7). -->
                  <span class="tf-colsts" aria-hidden="true"></span>
                </h3>
                <div class="tf-stack">
                  <TicketStack cards={stackCards} atMost={openShare(place, stackCards)} cut={(all) => drawnNow(place, all)} onwin={holdWin(place)} floor={cardFloorOf}>
                    {#snippet card(card, row)}
                      <TicketCard
                        {card}
                        {row}
                        fields={view?.card_fields ?? null}
                        source={sourceOf(card)}
                        {needles}
                        selected={current?.card.entity_id === card.entity_id}
                        chosen={chosenJob.id === card.entity_id}
                        onselect={() => openTicket(card)}
                        onopen={() => openTicket(card)}
                        write={writeOf(card, listed)}
                        note={ticketNotes[card.entity_id] ?? null}
                        ondismiss={() => void dismissNote(card.entity_id)}
                      />
                    {/snippet}
                  </TicketStack>
                  {#if more}
                    <button type="button" class="tf-more" onclick={() => reader.expand(lane, column)}>{more}</button>
                  {/if}
                  {#if reader.all && cards.length < column.total}
                    <!-- A view that shows every ticket reads this column's next page as its foot nears (ux18). -->
                    <div class="tf-near" aria-hidden="true" {@attach nearEnd(() => reader.more(lane, column, listed), () => cards.length)}></div>
                  {/if}
                  {#if pointer}
                    <button type="button" class="tf-more tf-more-list" onclick={() => listColumn(column, listed)}>{pointer}</button>
                  {/if}
                </div>
              </div>
            {/each}
          </div>
        </div>
      {/each}
      {@const moreLanesText = reader.laned && !reader.all && reader.board !== null ? moreLanesLine(reader.board.more_lanes) : null}
      {#if moreLanesText}
        <button type="button" class="tf-btn tf-sm tf-lanes-more" onclick={reader.moreLanes}>{moreLanesText}</button>
      {:else if reader.all && reader.laned && reader.board !== null && reader.board.more_lanes > 0}
        <div class="tf-near" aria-hidden="true" {@attach nearEnd(() => reader.moreLanes(), () => reader.board?.lanes.length ?? 0)}></div>
      {/if}
    {:else}
      <div class="tf-list">
        <table class="tf-tbl" class:tf-has-epic={listColumns.some(([column]) => column === "epic")} class:tf-has-labels={listColumns.some(([column]) => column === "labels")}>
          <thead>
            <tr>
              {#each listColumns as [column, label] (column)}
                {#if sortable(column)}
                  <th class="tf-c-{column}" aria-sort={showing.sort.column === column ? showing.sort.direction : undefined}
                    ><button type="button" onclick={() => sortBy(column)}>{label}</button></th
                  >
                {:else}
                  <th class="tf-c-{column}">{label}</th>
                {/if}
              {/each}
            </tr>
          </thead>
          <tbody {@attach win.list}>
            {#if win.before > 0}
              <tr class="tf-wgap" aria-hidden="true"><td colspan={listColumns.length} {@attach win.room("before")}></td></tr>
            {/if}
            {#each win.slice(listRows) as { card } (card.entity_id)}
              <tr
                {@attach win.row(card.entity_id)}
                class="tf-row"
                class:tf-card={chosenJob.id === card.entity_id}
                class:tf-sel={current?.card.entity_id === card.entity_id}
                aria-current={current?.card.entity_id === card.entity_id ? "true" : undefined}
                data-entity={card.entity_id}
                tabindex="0"
                {@attach menuThing(() => ({ kind: "ticket", id: card.entity_id, hash: ticketHash(card.entity_id) }))}
                onclick={() => openTicket(card)}
                ondblclick={() => openTicket(card)}
              >
                {#each listColumns as [column] (column)}
                  {#if column === "key"}
                    {@const source = sourceOf(card)}
                    <td class="tf-mono tf-nowrap tf-keycell" title={source ? `${card.key}, ${source}` : card.key}
                      ><Marked text={keepsItsEnd(shortKey(card.key))} {needles} />{#if source}<span class="tf-sub">{source}</span>{/if}</td
                    >
                  {:else if column === "summary"}
                    <td class="tf-c-summary"
                      ><Marked text={card.title} {needles} />{#if card.path}<span class="tf-sub"
                          ><TfKeyed text={card.path} /></span
                        >{/if}</td
                    >
                  {:else if column === "status"}
                    {@const blocked = moveBlocked(card)}
                    {@const write = writeOf(card, [])}
                    <td class="tf-nowrap tf-c-status"
                      ><button
                        type="button"
                        class="tf-statecell"
                        aria-haspopup={blocked === null ? "menu" : undefined}
                        aria-expanded={blocked === null ? statusMenuOpen && statusMenu?.card.entity_id === card.entity_id : undefined}
                        aria-disabled={blocked === null ? undefined : "true"}
                        title={blocked ?? `${columnName(card.status)}. Change the status (S)`}
                        onclick={(event) => {
                          event.stopPropagation();
                          openStatusMenu(card, event.currentTarget);
                        }}
                        ondblclick={(event) => event.stopPropagation()}
                        ><span class="tf-statename">{columnName(card.status)}</span><span class="tf-chev" aria-hidden="true"></span></button
                      >{#if write}{" "}<span class="tf-chip tf-twrite" class:tf-info={write.tone === "info"} title={write.title}
                          ><span class="tf-wwords"><span class="tf-won">{write.words}</span><span aria-hidden="true">{write.other}</span></span></span
                        >{/if}</td
                    >
                  {:else if column === "priority"}
                    <td class="tf-c-priority" title={sentenceCase(card.priority) ?? undefined}>{sentenceCase(card.priority) ?? ""}</td>
                  {:else if column === "signals"}
                    <td class="tf-c-signals"><span class="tf-sigs"><Signals signals={card.signals} /></span></td>
                  {:else if column === "assignee"}
                    <td class="tf-c-assignee" title={assigneeName(card) ?? undefined}>{listName(card) ?? "Unassigned"}</td>
                  {:else if column === "updated"}
                    <td class="tf-dim tf-nowrap">{clockLabel(card.updated_at)}</td>
                  {:else if column === "epic"}
                    <td class="tf-c-epic tf-mono tf-nowrap" title={card.path ?? undefined}>{card.path ?? ""}</td>
                  {:else}
                    <td class="tf-c-labels" title={card.labels.join(", ") || undefined}>{card.labels.join(", ")}</td>
                  {/if}
                {/each}
              </tr>
            {/each}
            {#if win.after > 0}
              <tr class="tf-wgap" aria-hidden="true"><td colspan={listColumns.length} {@attach win.room("after")}></td></tr>
            {/if}
          </tbody>
        </table>
        {#if listCut && !reader.typing && !reader.all}
          <button type="button" class="tf-more tf-list-more" onclick={reader.expandAll}>Show {listRest} more</button>
        {:else if reader.rows !== null && reader.rows.length < (reader.shown ?? 0)}
          <div class="tf-near" aria-hidden="true" {@attach nearEnd(() => reader.moreRows(), () => reader.rows?.length ?? 0)}></div>
        {/if}
      </div>
    {/if}
  </TfScroller>

  <p class="tf-sr" role="status" aria-live="polite">{said}</p>
  {#if dropMenu !== null}
    {@const asked = dropMenu}
    <StatusMenu
      bind:open={dropMenuOpen}
      opener={null}
      at={asked.at}
      heading
      ticketKey={asked.card.key}
      status={asked.card.status}
      source={sourceRoles.nameOf(asked.card.source_id)}
      reach={null}
      only={asked.statuses}
      onpick={(status) => {
        dropMenu = null;
        asked.answer(status);
      }}
      onclose={() => {
        dropMenu = null;
        asked.answer(null);
      }}
    />
  {/if}
  {#if statusMenu !== null}
    {@const menuCard = statusMenu.card}
    <StatusMenu
      bind:open={statusMenuOpen}
      opener={statusMenu.opener}
      ticketKey={menuCard.key}
      status={movedStatus(menuCard.entity_id) ?? menuCard.status}
      source={sourceRoles.nameOf(menuCard.source_id)}
      reach={reachOf(menuCard)}
      asking={reachAsking(menuCard)}
      onpick={(status) => moveCard(menuCard, status)}
      onretry={() => void askReach(menuCard, "press", ports.moves?.reachableTransitions)}
    />
  {/if}

  {#if picking !== null}
    {@const pick = picking}
    {@const field = cardFields.assignee(pick.card)}
    {@const held = field === null ? null : cardFields.options(pick.card, field)}
    {@const source = sourceRoles.nameOf(pick.card.source_id)}
    <TfPeoplePick
      bind:open={pickOpen}
      opener={pick.opener}
      label="Assignee"
      current={pick.card.assignee ? { value: pick.card.assignee, shown: pick.card.assignee_name ?? pick.card.assignee } : null}
      person={field?.kind.type === "search" ? field.kind.person : true}
      required={field?.required ?? false}
      options={field?.kind.type === "choice" ? field.kind.options : (held?.options ?? [])}
      complete={field?.kind.type === "choice" ? true : (held?.complete ?? false)}
      asking={field === null || (held?.asking ?? false)}
      failed={held?.failed ?? null}
      sourceName={source}
      scope={field?.kind.type === "search" ? field.kind.scope : source}
      onpick={(value, shown) => {
        pickOpen = false;
        cardFields.assign(pick.card, value, shown);
      }}
      onsearch={(typed) => {
        if (field !== null) cardFields.search(pick.card, field, typed);
      }}
      {layers}
    />
  {/if}

  {#if drafting !== null}
    {@const editing = drafting.editing}
    <!-- Keyed: *Make an editable copy* in a saved view's settings opens a new editor on the copy. -->
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

</div>
