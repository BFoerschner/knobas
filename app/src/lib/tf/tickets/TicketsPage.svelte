<!--
  Tickets, the ticket-first shell's home (#810, requirements §3), in three
  panes over today's reads, before ticket views exist (#812):

  - **left**, the views: *All tickets* with its count, and *Not a ticket*,
    the ad-hoc contexts (R-3.11);
  - **middle**, every synced ticket as a ticket board, one column per status
    in the source's words, eight cards then *Show N more*, *Done* three, and
    every match while the filter has words (R-3.4), split into lanes by a
    field or not (R-3.5, #813); or as a list sorted by any header (R-3.6);
  - **right**, the selected ticket's preview (R-3.9).

  The filter bar is `FilterBar` in its server-side mode (R-2.7): every
  change sends the base query, chips, dropdowns and words to `mini_board`,
  which runs the search grammar and draws only what it admits. The browser
  filters nothing; it only marks the words in what came back.

  The board is `mini_board`'s, the same read the rooms' Tickets tile makes,
  with its columns and their order (`knobas_core::mini_board`). A laned
  board reads every ticket, uncapped, and splits it here: a lane's columns
  and counts are its own, which a per-status cap cannot give.

  **Keys** (R-3.10): `j`/`k` and the arrows move the selection through the
  cards on screen in reading order (lane by lane, column by column, top to
  bottom; the list's rows as sorted), and focus the card. `Enter` and a
  double click open the job.

  **The *Board / List* and *Lanes* switches hold for the session** (R-3.3):
  module state, never written anywhere, so a restart forgets them.
-->
<script lang="ts" module>
  import type {
    ContextRow,
    EntityDetail,
    EntityPage,
    MiniBoard,
  } from "../../ipc/entity";
  import type { FilterSuggestions } from "../../ipc/search";
  import { DEFAULT_SORT, type LaneField, type ListSort } from "./tickets";

  /** The reads this page makes, injectable so a test needs no bridge. */
  export interface TicketsPorts {
    /** `mini_board` over every source: capped per column or not, narrowed by a query or not. */
    board: (perColumn: number | null, query: string | null) => Promise<MiniBoard>;
    /** The selected ticket's description. */
    detail: (entityId: string) => Promise<Pick<EntityDetail, "body_text">>;
    contexts: () => Promise<ContextRow[]>;
    /** A context's room, counted by kind and tone under `latest_per_parent`. */
    tally: (ctxId: string) => Promise<EntityPage["tally"]>;
    suggestions: () => Promise<Pick<FilterSuggestions, "fields" | "parents">>;
  }

  /** What the page keeps for the session: the layout, the lanes, the sort, the selection, the expanded columns. */
  const session = $state({
    layout: "board" as "board" | "list",
    /** The *Lanes* switch (#813): nothing, or the field the board is split by. */
    lanes: "none" as LaneField,
    /** The list's sort (#813), newest first until a header is pressed. */
    sort: DEFAULT_SORT as ListSort,
    selected: null as string | null,
    /** Expanded board columns by status (`""` for *No status*); in a lane, `<lane>|<status>`. */
    expanded: [] as string[],
    /** Whether the list's *Show N more* was pressed: the list reads every ticket, the board keeps its caps. */
    listAll: false,
  });

  /** A new session: what each test is. */
  export function forgetTicketsSession(): void {
    session.layout = "board";
    session.lanes = "none";
    session.sort = DEFAULT_SORT;
    session.selected = null;
    session.expanded = [];
    session.listAll = false;
  }
</script>

<script lang="ts">
  import { flushSync, onMount, untrack } from "svelte";

  import {
    getEntity,
    listContexts,
    listEntities,
    miniBoard,
    type MiniBoardCard,
    type MiniBoardColumn,
  } from "../../ipc/entity";
  import { filterSuggestions } from "../../ipc/search";
  import { inTextField } from "../../shell/keys";
  import { isFiltering, toQuery, type FilterConfig, type FilterOption } from "../filter/filter";
  import FilterBar from "../filter/FilterBar.svelte";
  import { sessionFilter } from "../filter/filter-state.svelte";
  import Marked from "../filter/Marked.svelte";
  import { jobHash } from "../route";
  import TicketCard from "./TicketCard.svelte";
  import Signals from "./Signals.svelte";
  import TicketPreview from "./TicketPreview.svelte";
  import {
    aroundLines,
    COLUMN_CAP,
    columnName,
    drawnCards,
    LANE_FIELDS,
    moreLine,
    nextSort,
    sortRows,
    splitLanes,
    when,
    type AroundLine,
    type Lane,
    type SortColumn,
  } from "./tickets";

  let {
    ready,
    timedTicket,
    ports = {
      board: (perColumn, query) => miniBoard({ sources: [], context: null, project: null }, perColumn, query),
      detail: getEntity,
      contexts: listContexts,
      tally: (ctxId) =>
        listEntities(
          {
            sources: [],
            kinds: [],
            updated_within_days: null,
            order: "latest_per_parent",
            context: ctxId,
            project: null,
            include_deleted: false,
          },
          1,
          0,
        ).then((page) => page.tally),
      suggestions: () => filterSuggestions(),
    },
  }: {
    /** Whether the database answers yet. */
    ready: boolean;
    /** The ticket key the timer runs on, or `null`. */
    timedTicket: string | null;
    ports?: TicketsPorts;
  } = $props();

  // ---- the filter bar ----
  const filter = sessionFilter("tickets-all");
  let suggested = $state<Pick<FilterSuggestions, "fields" | "parents"> | null>(null);

  /** At most this many chips a group; past it a field has too many values for chips. */
  const CHIPS_AT_MOST = 6;

  const config: FilterConfig = $derived.by(() => {
    const values = (key: string): FilterOption[] =>
      (suggested?.fields ?? [])
        .filter((field) => field.key === key)
        .map((field) => ({ value: field.value, label: field.value === "@me" ? "Me" : field.value }));
    return {
      id: "tickets-all",
      placeholder: "Filter by key, summary, epic or person",
      noun: "tickets",
      base: "",
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

  // ---- the board ----
  let board = $state<MiniBoard | null>(null);
  /** How many tickets there are unfiltered: the count's "of 75". */
  let total = $state<number | null>(null);
  let failed = $state<string | null>(null);
  let reading = 0;

  const typing = $derived(filter.text.trim() !== "");
  const sum = (b: MiniBoard) => b.columns.reduce((all, column) => all + column.total, 0);
  const columnKey = (column: Pick<MiniBoardColumn, "status">) => column.status ?? "";
  const isExpanded = (column: MiniBoardColumn) => session.expanded.includes(columnKey(column));
  /** Whether the board is split into lanes, which reads it whole. */
  const laned = $derived(session.layout === "board" && session.lanes !== "none");
  const expandKey = (lane: Lane, column: MiniBoardColumn) =>
    lane.key === null ? columnKey(column) : `${lane.key}|${columnKey(column)}`;

  /**
   * The per-column cap a read asks for: none while words filter (every match,
   * R-3.4), the largest expanded column's count once one is expanded (every
   * column's once the list is), else the eight a column draws.
   */
  function capFor(): number | null {
    if (typing || laned) return null;
    if ((session.expanded.length === 0 && !session.listAll) || board === null) return COLUMN_CAP;
    const wanted = board.columns
      .filter((column) => session.listAll || isExpanded(column))
      .map((column) => column.total);
    return Math.max(COLUMN_CAP, ...wanted);
  }

  function read() {
    const query = toQuery(config, filter);
    const mine = ++reading;
    ports
      .board(capFor(), query === "" ? null : query)
      .then((answer) => {
        if (mine !== reading) return;
        board = answer;
        failed = null;
        if (query === "") total = sum(answer);
      })
      .catch((error: unknown) => {
        if (mine === reading) failed = messageOf(error);
      });
    // A session that comes back to a filter has not counted the whole yet.
    if (query !== "" && total === null) {
      ports
        .board(1, null)
        .then((answer) => (total ??= sum(answer)))
        .catch(() => {});
    }
  }

  function messageOf(error: unknown): string {
    if (error && typeof error === "object" && "message" in error) return String(error.message);
    return String(error);
  }

  function expand(lane: Lane, column: MiniBoardColumn) {
    session.expanded = [...session.expanded, expandKey(lane, column)];
    // A lane's column is already read whole; the board's own is capped.
    if (lane.key === null) read();
  }

  function chooseLayout(layout: "board" | "list") {
    if (session.layout === layout) return;
    const wasLaned = laned;
    session.layout = layout;
    if (wasLaned !== laned) read();
  }

  function chooseLanes(lanes: LaneField) {
    const wasLaned = laned;
    session.lanes = lanes;
    if (wasLaned !== laned) read();
  }

  function sortBy(column: SortColumn) {
    session.sort = nextSort(session.sort, column);
  }

  function expandAll() {
    session.listAll = true;
    read();
  }

  function clearFilter() {
    filter.text = "";
    filter.chips = {};
    filter.dropdowns = {};
    read();
  }

  // ---- what is drawn, and the selection ----
  /** The board's bands (one, untitled, without lanes), each column with the cards it draws. */
  const bands = $derived(
    splitLanes(board?.columns ?? [], laned ? session.lanes : "none").map((lane) => ({
      lane,
      drawn: lane.columns.map((column) => ({
        column,
        cards: drawnCards(column, session.expanded.includes(expandKey(lane, column)), typing),
      })),
    })),
  );
  /** The list's rows: every card read, in the header's order. */
  const rows = $derived(
    sortRows(
      (board?.columns ?? []).flatMap((column) => column.cards.map((card) => ({ card, status: column.status }))),
      session.sort,
      (board?.columns ?? []).map((column) => column.status),
    ),
  );
  const listCut = $derived((board?.columns ?? []).some((column) => column.cards.length < column.total));
  const listRest = $derived(
    (board?.columns ?? []).reduce((all, column) => all + column.total - column.cards.length, 0),
  );
  /** What is on screen, in reading order: the first is selected when the selection is not. */
  const visible = $derived(
    session.layout === "board"
      ? bands.flatMap(({ drawn }) =>
          drawn.flatMap(({ column, cards }) => cards.map((card) => ({ card, status: column.status }))),
        )
      : rows,
  );
  const current = $derived(visible.find((entry) => entry.card.entity_id === session.selected) ?? visible[0] ?? null);
  const shown = $derived(board === null ? null : sum(board));

  // ---- the preview's reads ----
  let description = $state<string | null>(null);
  let around = $state<AroundLine[] | null>(null);
  let contexts = $state<ContextRow[] | null>(null);
  const adhoc = $derived((contexts ?? []).filter((context) => context.kind === "adhoc"));

  /** The selected ticket's id: the preview re-reads when it changes, not when the board answers. */
  const currentId = $derived(current?.card.entity_id ?? null);

  $effect(() => {
    const id = currentId;
    if (!ready || id === null) return;
    description = null;
    around = null;
    let dead = false;
    ports
      .detail(id)
      .then((detail) => {
        if (!dead) description = detail.body_text;
      })
      .catch(() => {});
    const anchored = contexts?.find((context) => context.anchor_id === id) ?? null;
    if (contexts === null) return () => (dead = true);
    if (anchored === null) around = [];
    else {
      ports
        .tally(anchored.id)
        .then((tally) => {
          if (!dead) around = aroundLines(tally);
        })
        .catch(() => {
          if (!dead) around = [];
        });
    }
    return () => (dead = true);
  });

  // Once the database answers: the board, the contexts and the suggestions.
  // Untracked, so a read's own answer does not ask for the read again.
  $effect(() => {
    if (!ready) return;
    untrack(() => {
      read();
      ports
        .contexts()
        .then((rows) => (contexts = rows))
        .catch(() => (contexts = []));
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
    location.hash = jobHash(card.key);
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
      location.hash = jobHash(entry.card.key);
    };
    window.addEventListener("keydown", onkey);
    return () => window.removeEventListener("keydown", onkey);
  });
</script>

<div class="tf-tickets">
  <nav class="tf-views" aria-label="Ticket views">
    <div class="tf-vlist">
      <a href="#/tickets" aria-current="true"
        >All tickets {#if total !== null}<span class="tf-count">{total}</span>{/if}</a
      >
      {#if adhoc.length > 0}
        <h2>Not a ticket</h2>
        {#each adhoc as context (context.id)}
          <a href={jobHash(context.id)}>{context.title}</a>
        {/each}
      {/if}
    </div>
  </nav>

  <section class="tf-board-wrap">
    <div class="tf-board-head">
      <h1>All tickets</h1>
      <div class="tf-seg" role="group" aria-label="Layout">
        <button type="button" aria-pressed={session.layout === "board"} onclick={() => chooseLayout("board")}
          >Board</button
        >
        <button type="button" aria-pressed={session.layout === "list"} onclick={() => chooseLayout("list")}
          >List</button
        >
      </div>
    </div>
    <FilterBar {config} {filter} {shown} {total} onchange={read} extra={lanesSwitch} />
    {#snippet lanesSwitch()}
      <!-- prototype-6 offers lanes on the ticket board only. -->
      {#if session.layout === "board"}
        <label class="tf-fsel">
          Lanes
          <select
            aria-label="Lanes"
            value={session.lanes}
            onchange={(event) => chooseLanes(event.currentTarget.value as LaneField)}
          >
            {#each LANE_FIELDS as [value, label] (value)}
              <option {value}>{label}</option>
            {/each}
          </select>
        </label>
      {/if}
    {/snippet}

    {#if failed !== null}
      <div class="tf-empty"><p>The tickets could not be read: {failed}</p></div>
    {:else if board === null}
      <div class="tf-empty" aria-busy="true"></div>
    {:else if shown === 0 && isFiltering(filter)}
      <div class="tf-empty">
        <p>No tickets match.</p>
        <button type="button" class="tf-btn tf-sm" onclick={clearFilter}>Clear the filter</button>
      </div>
    {:else if shown === 0}
      <div class="tf-empty"><p>No tickets synced yet. Add a Jira source in Settings and they appear here.</p></div>
    {:else if session.layout === "board"}
      {#each bands as { lane, drawn } (lane.key ?? "")}
        <div class="tf-lane">
          {#if lane.key !== null}
            <h2>{lane.title} <span class="tf-count">{lane.count}</span></h2>
          {/if}
          <div class="tf-cols">
            {#each drawn as { column, cards } (columnKey(column))}
              {@const more = moreLine(column, cards.length)}
              <div class="tf-col">
                <h3>{columnName(column.status)} <span class="tf-count">{column.total}</span></h3>
                <div class="tf-stack">
                  {#each cards as card (card.entity_id)}
                    <TicketCard
                      {card}
                      typed={filter.text}
                      selected={current?.card.entity_id === card.entity_id}
                      onselect={() => select(card)}
                      onopen={() => openJob(card)}
                    />
                  {:else}
                    <span class="tf-dim tf-nothing">Nothing here</span>
                  {/each}
                  {#if more}
                    <button type="button" class="tf-more" onclick={() => expand(lane, column)}>{more}</button>
                  {/if}
                </div>
              </div>
            {/each}
          </div>
        </div>
      {/each}
    {:else}
      <div class="tf-list">
        {#snippet sortable(column: SortColumn, label: string)}
          <th aria-sort={session.sort.column === column ? session.sort.direction : undefined}
            ><button type="button" onclick={() => sortBy(column)}>{label}</button></th
          >
        {/snippet}
        <table class="tf-tbl">
          <thead>
            <tr>
              {@render sortable("key", "Key")}
              {@render sortable("summary", "Summary")}
              {@render sortable("status", "Status")}
              {@render sortable("priority", "Priority")}
              <th>Signals</th>
              {@render sortable("assignee", "Assignee")}
              {@render sortable("updated", "Updated")}
            </tr>
          </thead>
          <tbody>
            {#each rows as { card, status } (card.entity_id)}
              <tr
                class="tf-row"
                class:tf-card={current?.card.entity_id === card.entity_id}
                aria-current={current?.card.entity_id === card.entity_id ? "true" : undefined}
                data-entity={card.entity_id}
                tabindex="0"
                onclick={() => select(card)}
                ondblclick={() => openJob(card)}
              >
                <td class="tf-mono tf-nowrap"><Marked text={card.key} typed={filter.text} /></td>
                <td
                  ><Marked text={card.title} typed={filter.text} />{#if card.path}<span class="tf-sub">{card.path}</span
                    >{/if}</td
                >
                <td class="tf-nowrap">{columnName(status)}</td>
                <td>{card.priority ?? ""}</td>
                <td><span class="tf-sigs"><Signals signals={card.signals} /></span></td>
                <td>{card.assignee ?? "Unassigned"}</td>
                <td class="tf-dim">{when(card.updated_at)}</td>
              </tr>
            {/each}
          </tbody>
        </table>
        {#if listCut && !typing}
          <button type="button" class="tf-more tf-list-more" onclick={expandAll}>Show {listRest} more</button>
        {/if}
      </div>
    {/if}
  </section>

  {#if current !== null}
    <TicketPreview
      card={current.card}
      status={current.status}
      {description}
      {around}
      timerOnIt={timedTicket === current.card.key}
    />
  {:else}
    <aside class="tf-preview" aria-label="Selected ticket"><p>Select a ticket to see what is around it.</p></aside>
  {/if}
</div>
