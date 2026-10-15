<!--
  Tickets, the ticket-first shell's home (#810, requirements §3), in three
  panes over today's reads, before ticket views exist (#812):

  - **left**, the views: *All tickets* with its count, and *Not a ticket*,
    the ad-hoc contexts (R-3.11);
  - **middle**, every synced ticket as a ticket board, one column per status
    in the source's words, eight cards then *Show N more*, *Done* three, and
    every match while the filter has words (R-3.4); or as a list (R-3.6);
  - **right**, the selected ticket's preview (R-3.9).

  The filter bar is `FilterBar` in its server-side mode (R-2.7): every
  change sends the base query, chips, dropdowns and words to `mini_board`,
  which runs the search grammar and draws only what it admits. The browser
  filters nothing; it only marks the words in what came back.

  The board is `mini_board`'s, the same read the rooms' Tickets tile makes,
  with its columns and their order (`knobas_core::mini_board`).
-->
<script lang="ts" module>
  import type {
    ContextRow,
    EntityDetail,
    EntityPage,
    MiniBoard,
  } from "../../ipc/entity";
  import type { FilterSuggestions } from "../../ipc/search";

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

  /** What the page keeps for the session: the layout, the selection, the expanded columns. */
  const session = $state({
    layout: "board" as "board" | "list",
    selected: null as string | null,
    /** Expanded columns by status (`""` for *No status*); `"*"` is every one. */
    expanded: [] as string[],
  });

  /** A new session: what each test is. */
  export function forgetTicketsSession(): void {
    session.layout = "board";
    session.selected = null;
    session.expanded = [];
  }
</script>

<script lang="ts">
  import { onMount, untrack } from "svelte";

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
  import TicketPreview from "./TicketPreview.svelte";
  import {
    aroundLines,
    COLUMN_CAP,
    columnName,
    drawnCards,
    moreLine,
    when,
    type AroundLine,
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
  const isExpanded = (column: MiniBoardColumn) =>
    session.expanded.includes("*") || session.expanded.includes(columnKey(column));

  /**
   * The per-column cap a read asks for: none while words filter (every match,
   * R-3.4), the largest expanded column's count once one is expanded, else
   * the eight a column draws.
   */
  function capFor(): number | null {
    if (typing) return null;
    if (session.expanded.length === 0 || board === null) return COLUMN_CAP;
    const wanted = board.columns.filter(isExpanded).map((column) => column.total);
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

  function expand(column: MiniBoardColumn) {
    session.expanded = [...session.expanded, columnKey(column)];
    read();
  }

  function expandAll() {
    session.expanded = ["*"];
    read();
  }

  function clearFilter() {
    filter.text = "";
    filter.chips = {};
    filter.dropdowns = {};
    read();
  }

  // ---- what is drawn, and the selection ----
  const drawn = $derived(
    (board?.columns ?? []).map((column) => ({ column, cards: drawnCards(column, isExpanded(column), typing) })),
  );
  /** The list's rows: every card read, newest first. */
  const rows = $derived(
    (board?.columns ?? [])
      .flatMap((column) => column.cards.map((card) => ({ card, status: column.status })))
      .sort((a, b) => (b.card.updated_at ?? "").localeCompare(a.card.updated_at ?? "")),
  );
  const listCut = $derived((board?.columns ?? []).some((column) => column.cards.length < column.total));
  const listRest = $derived(
    (board?.columns ?? []).reduce((all, column) => all + column.total - column.cards.length, 0),
  );
  /** What is on screen, in reading order: the first is selected when the selection is not. */
  const visible = $derived(
    session.layout === "board"
      ? drawn.flatMap(({ column, cards }) => cards.map((card) => ({ card, status: column.status })))
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

  // `Enter` presses the preview's button (R-3.9), from anywhere but a field
  // that types or a control of its own; a card is not one.
  onMount(() => {
    const onkey = (event: KeyboardEvent) => {
      if (event.key !== "Enter" || event.metaKey || event.ctrlKey || event.altKey || event.defaultPrevented) return;
      const at = document.activeElement;
      if (at instanceof HTMLElement && inTextField(at)) return;
      const onCard = at instanceof HTMLElement && at.closest("[data-entity]") !== null;
      if (at instanceof HTMLElement && at !== document.body && !onCard && at.closest("a, button, select") !== null)
        return;
      if (current === null) return;
      event.preventDefault();
      location.hash = jobHash(current.card.key);
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
        <button type="button" aria-pressed={session.layout === "board"} onclick={() => (session.layout = "board")}
          >Board</button
        >
        <button type="button" aria-pressed={session.layout === "list"} onclick={() => (session.layout = "list")}
          >List</button
        >
      </div>
    </div>
    <FilterBar {config} {filter} {shown} {total} onchange={read} />

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
      <div class="tf-lane">
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
                  />
                {:else}
                  <span class="tf-dim tf-nothing">Nothing here</span>
                {/each}
                {#if more}
                  <button type="button" class="tf-more" onclick={() => expand(column)}>{more}</button>
                {/if}
              </div>
            </div>
          {/each}
        </div>
      </div>
    {:else}
      <div class="tf-list">
        <table class="tf-tbl">
          <thead>
            <tr><th>Key</th><th>Summary</th><th>Status</th><th>Priority</th><th>Assignee</th><th>Updated</th></tr>
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
              >
                <td class="tf-mono tf-nowrap"><Marked text={card.key} typed={filter.text} /></td>
                <td
                  ><Marked text={card.title} typed={filter.text} />{#if card.path}<span class="tf-sub">{card.path}</span
                    >{/if}</td
                >
                <td class="tf-nowrap">{columnName(status)}</td>
                <td>{card.priority ?? ""}</td>
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
