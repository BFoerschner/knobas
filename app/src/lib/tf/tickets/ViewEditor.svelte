<!--
  The view editor (#814, R-3.12 to R-3.16): one dialog behind *+ New view*,
  *Edit view*, *Save as a view* and *Duplicate*. The form on the left (name,
  group, *Which tickets* as conditions and words, then how they show), a live
  preview on the right.

  The stored text is the truth (R-3.3): the editor reads the draft's text
  into conditions when it opens (`editor.ts`) and writes text from them on
  save. A text whose conditions the reader left as they were is saved as it
  was typed, not rewritten. Under the conditions the same view shows as
  search text, tokens the grammar does not read underlined.

  The preview reads `mini_board` with the text the view would store, a
  moment after each change of it, and draws the count, the board with its
  columns (an over-limit count in err tone, tickets in a status on no column
  called out) or the list. The statuses the columns offer are the board's
  own, read once unfiltered.

  *Create view* stays off until there is a name. A save that fails keeps the
  dialog open and says why.
-->
<script lang="ts">
  import { clockLabel } from "../../time/format";
  import { untrack } from "svelte";

  import type { MiniBoard } from "../../ipc/entity";
  import type {
    CardField,
    FieldSuggestion,
    ListColumn,
    ParentSuggestion,
    QueryTranslation,
    SavedViewSource,
    TicketViewDraft,
    ViewLanes,
  } from "../../ipc/search";
  import TfDialog from "../settings/TfDialog.svelte";
  import {
    CONDITION_FIELDS,
    conditionsText,
    editColumns,
    fieldKey,
    looseStatuses,
    moveColumn,
    onePerStatus,
    parseConditions,
    previewBoard,
    searchParts,
    storedColumns,
    toggleStatus,
    type Condition,
    type ConditionField,
    type EditColumn,
  } from "./editor";
  import { columnName, LANE_FIELDS, splitLanes } from "./tickets";
  import { counted } from "../counted";
  import { copyKeeps, drawnSavedViewColumns, LIST_COLUMN_ORDER, savedViewKindWords, systemsWords, type CopiedFrom } from "./views";

  let {
    draft,
    editing = false,
    unreadable = null,
    groups,
    fields,
    parents,
    board,
    onsave,
    ondelete,
    onclose,
    savedViewSource = null,
    copiedFrom = null,
    oncopy,
    copy = null,
    systems = [],
  }: {
    /** The ticket systems the saved views come from (#2032): what a view of your own is never sent to. */
    systems?: readonly string[];
    /**
     * The saved view's query as its source reads it (#1208), for *A copy
     * keeps …*; `null` until it answers, and for a source that does not.
     */
    copy?: QueryTranslation | null;
    /** A copy of a Jira view (#880): where it came from and the JQL parts it left out. */
    copiedFrom?: CopiedFrom | null;
    /** *Make an editable copy*, offered in a Jira view's settings (#880). */
    oncopy?: (() => void) | undefined;
    /**
     * *View settings* on a Jira view (#877, R-3.24): where it comes from. Jira
     * decides its name, which tickets and its columns, so those are shown and
     * not edited; layout, lanes, card fields and list columns are the reader's.
     */
    savedViewSource?: SavedViewSource | null;
    /** Where the view starts: a saved one (*Edit view*) or a new one. */
    draft: TicketViewDraft;
    /** *Edit view*: the title names the view, the button says *Save view*, *Delete view* shows. */
    editing?: boolean;
    /** The part of a saved text the grammar could not read, or `null`. */
    unreadable?: string | null;
    /** The groups there are, offered under *Group*. */
    groups: string[];
    /** The values the ticket keys take (#811): what a condition's values are. */
    fields: FieldSuggestion[];
    /** The parents `under:` can name: Epic's values are the tickets among them. */
    parents: ParentSuggestion[];
    /** `mini_board` over every source, capped per column or not, narrowed by a query or not. */
    board: (perColumn: number | null, query: string | null) => Promise<MiniBoard>;
    /** Save it; a rejection's message is shown in the dialog. */
    onsave: (draft: TicketViewDraft) => Promise<void>;
    ondelete?: (() => void) | undefined;
    onclose: () => void;
  } = $props();

  // The dialog edits its own copy: the draft it was opened with stays as it was.
  const start = untrack(() => draft);
  const first = parseConditions(start.query);
  const firstText = conditionsText(first);

  let name = $state(start.name);
  let group = $state(start.group);
  let conditions = $state<Condition[]>(first.conditions);
  let words = $state(first.words);
  let layout = $state(start.layout);
  let lanes = $state<ViewLanes>(start.lanes);
  let cardFields = $state<CardField[]>([...start.card_fields]);
  let listColumns = $state<ListColumn[]>([...start.list_columns]);
  /** `null` until the board's statuses are read, for a view with no columns of its own. */
  let columns = $state<EditColumn[] | null>(start.columns.length > 0 ? editColumns(start.columns, []) : null);
  let statuses = $state<string[]>([]);

  let saving = $state(false);
  let failed = $state<string | null>(null);
  const named = $derived(name.trim() !== "");
  const saveLabel = $derived(editing ? "Save view" : "Create view");
  /** Fixed for the dialog's life: a saved view stays one. */
  const fixedSource = untrack(() => savedViewSource);
  const title = $derived(fixedSource ? `Settings for ${start.name}` : editing ? `Edit ${start.name}` : "New view");
  /** *Which tickets* on a Jira view: where Jira's answer comes from, in prototype-6's words. */
  const decidesLine = $derived.by(() => {
    if (fixedSource === null) return "";
    return `${fixedSource.system} decides, from the ${savedViewKindWords(fixedSource)}`;
  });
  /** How much of a Jira view's query a copy would keep (#880). */
  const keepsLine = $derived.by(() => {
    if (fixedSource === null || copy === null) return "";
    return copyKeeps(copy);
  });
  const uid = $props.id();

  const read = $derived({ conditions, words });
  const parts = $derived(searchParts(read));
  /** The text the view would store: as typed while its conditions are as they were. */
  const query = $derived.by(() => {
    const text = conditionsText(read);
    return text === firstText ? start.query : text;
  });

  // ---- the values a condition offers ----
  const SIGNAL_WORDS: Record<string, string> = {
    "failing-build": "build failed",
    blocker: "blocked",
    "open-pr": "PR open",
    "review-for-me": "review asked of you",
  };
  const FIXED: Partial<Record<ConditionField, [string, string][]>> = {
    sprint: [["current", "The active sprint"]],
    updated: [
      ["today", "today"],
      ["7d", "in the last 7 days"],
      ["30d", "in the last 30 days"],
    ],
    assigned: [
      ["today", "today"],
      ["week", "this week"],
    ],
  };
  const same = (a: string, b: string) => {
    const norm = (v: string) => (v.toLowerCase() === "me" ? "@me" : v.toLowerCase());
    return norm(a) === norm(b);
  };

  function options(condition: Condition): [string, string][] {
    const field = condition.field;
    let offered: [string, string][];
    if (FIXED[field]) offered = FIXED[field]!;
    else if (field === "epic")
      offered = parents.filter((parent) => parent.kind === "ticket").map((parent) => [parent.name, parent.name]);
    else {
      const key = fieldKey(field);
      offered = fields
        .filter((suggestion) => suggestion.key === key)
        .map((suggestion): [string, string] => {
          if (field === "assignee" && suggestion.value === "@me") return ["@me", "Me"];
          if (field === "signal") return [suggestion.value, SIGNAL_WORDS[suggestion.value] ?? suggestion.value];
          if (field === "project" && suggestion.detail)
            return [suggestion.value, `${suggestion.detail} (${suggestion.value})`];
          return [suggestion.value, suggestion.value];
        });
    }
    // A value the text names that no suggestion offers is still the condition's.
    const extra = condition.values
      .filter((value) => !offered.some(([offer]) => same(offer, value)))
      .map((value): [string, string] => [value, value]);
    return [...offered, ...extra];
  }

  /** Signal appears once the signals answer (#815); absent, not broken, before. */
  const hasSignals = $derived(fields.some((suggestion) => suggestion.key === "has"));
  function fieldChoices(condition: Condition) {
    return CONDITION_FIELDS.filter(
      (entry) => entry.field !== "signal" || hasSignals || condition.field === "signal",
    );
  }

  function addCondition() {
    const used = conditions.map((condition) => condition.field);
    const free = fieldChoices({ field: "status", negated: false, values: [] }).find(
      (entry) => !used.includes(entry.field),
    );
    conditions = [...conditions, { field: free?.field ?? "status", negated: false, values: [] }];
  }

  function setCondition(at: number, change: Partial<Condition>) {
    conditions = conditions.map((condition, index) => (index === at ? { ...condition, ...change } : condition));
  }

  function toggleValue(at: number, value: string) {
    const condition = conditions[at]!;
    const held = condition.values.find((v) => same(v, value));
    const values = held === undefined ? [...condition.values, value] : condition.values.filter((v) => v !== held);
    setCondition(at, { values });
  }

  // ---- the columns ----
  function resetColumns() {
    columns = onePerStatus(statuses);
  }
  function setColumn(at: number, change: Partial<EditColumn>) {
    columns = (columns ?? []).map((column, index) => (index === at ? { ...column, ...change } : column));
  }
  const loose = $derived(looseStatuses(columns ?? [], statuses));
  const looseLine = $derived(
    `${loose.join(", ")} ${loose.length > 1 ? "are" : "is"} on no column, so those tickets do not show on the board.`,
  );

  const CARD_FIELDS: [CardField, string][] = [
    ["type", "Issue type and priority"],
    ["signals", "Signals"],
    ["comments", "Comment count"],
    ["assignee", "Assignee"],
    ["epic", "Epic"],
    ["labels", "Labels"],
  ];
  function toggleCard(field: CardField, on: boolean) {
    cardFields = CARD_FIELDS.map(([f]) => f).filter((f) => (f === field ? on : cardFields.includes(f)));
  }
  function toggleListColumn(column: ListColumn, on: boolean) {
    listColumns = LIST_COLUMN_ORDER.map(([c]) => c).filter((c) => (c === column ? on : listColumns.includes(c)));
  }

  // ---- the preview ----
  let answer = $state<MiniBoard | null>(null);
  let previewFailed = $state<string | null>(null);
  /** A laned board counts its lanes over every ticket; the rest draws a few per column. */
  const laned = $derived(layout === "board" && lanes !== "none");
  let reading = 0;

  // The board's statuses, once, unfiltered: what a column can hold.
  $effect(() => {
    untrack(() => {
      // A Jira view's statuses are the ones on Jira's columns, as Jira holds them.
      if (fixedSource !== null) {
        statuses = [...new Set((columns ?? []).flatMap((column) => column.statuses))];
        return;
      }
      board(1, null)
        .then((whole) => {
          const own = (columns ?? []).flatMap((column) => column.statuses);
          const seen = whole.columns.flatMap((column) => (column.status === null ? [] : [column.status]));
          statuses = [...seen, ...own.filter((status) => !seen.includes(status))];
          if (columns === null) columns = onePerStatus(statuses);
        })
        .catch(() => {
          if (columns === null) columns = [];
        });
    });
  });

  $effect(() => {
    const text = query;
    const perColumn = laned ? null : 9;
    const mine = ++reading;
    const timer = setTimeout(() => {
      board(perColumn, text === "" ? null : text)
        .then((got) => {
          if (mine !== reading) return;
          answer = got;
          previewFailed = null;
        })
        .catch((error: unknown) => {
          if (mine === reading) previewFailed = messageOf(error);
        });
    }, 150);
    return () => clearTimeout(timer);
  });

  // A Jira view's board comes back under Jira's columns already, each by its name.
  const shown = $derived(
    answer === null
      ? null
      : previewBoard(answer.columns, fixedSource !== null ? drawnSavedViewColumns(start.columns) : (columns ?? [])),
  );
  const laneCount = $derived(
    answer === null || !laned
      ? 0
      : splitLanes(
          answer.columns.filter((column) => column.total > 0),
          lanes,
        ).length,
  );
  const listRows = $derived(
    (answer?.columns ?? [])
      .flatMap((column) => column.cards.map((card) => ({ card })))
      .sort((a, b) => (b.card.updated_at ?? "").localeCompare(a.card.updated_at ?? ""))
      .slice(0, 9),
  );
  const drawnList = $derived(LIST_COLUMN_ORDER.filter(([column]) => listColumns.includes(column)));

  function messageOf(error: unknown): string {
    if (error && typeof error === "object" && "message" in error) return String(error.message);
    return String(error);
  }

  async function save() {
    if (!named || saving) return;
    saving = true;
    failed = null;
    try {
      await onsave({
        ...start,
        name: name.trim(),
        group: group.trim() || "Mine",
        query,
        layout,
        columns: fixedSource !== null ? start.columns : storedColumns(columns ?? [], statuses),
        lanes,
        card_fields: cardFields,
        list_columns: listColumns,
      });
    } catch (error) {
      failed = messageOf(error);
    } finally {
      saving = false;
    }
  }
</script>

<TfDialog {title} className="tf-vw-dialog" {onclose}>
  {#snippet body()}
    <div class="tf-vw-body">
      <form
        class="tf-vw-edit"
        id="tf-vw-form-{uid}"
        onsubmit={(event) => {
          event.preventDefault();
          void save();
        }}
      >
        {#if fixedSource !== null}
          <section class="tf-vw-sec">
            <h3>Which tickets</h3>
            <p>
              {decidesLine} <b>{start.name}</b>. knobas runs its query at every sync and keeps {fixedSource.system}'s order.{#if fixedSource.sprint}
                Like the board in {fixedSource.system}, it shows the active sprint, now {fixedSource.sprint.name}, and moves on to the next
                sprint when this one closes.{/if}
            </p>
            <pre class="tf-vw-source-query">{fixedSource.source_query}{#if fixedSource.sub_query}{"\n"}{fixedSource.sub_query}{/if}</pre>
            <!-- A source that does not translate its query (#1208) offers no copy, so the line does not either. -->
            <p class="tf-dim">
              {#if oncopy}To change which tickets it holds, change it in {fixedSource.system}, or make your own copy. {keepsLine}{:else}To
                change which tickets it holds, change it in {fixedSource.system}.{/if}
            </p>
            {#if oncopy}
              <div class="tf-vw-btns">
                <button type="button" class="tf-btn tf-sm" onclick={oncopy}>Make an editable copy</button>
              </div>
            {/if}
          </section>
        {:else}
        <section class="tf-vw-sec">
          <div class="tf-vw-two">
            <label>
              <span>Name</span>
              <input
                type="text"
                class="tf-vw-name-in"
                aria-label="Name"
                bind:value={name}
                placeholder="For example, Payments bugs this sprint"
              />
            </label>
            <label>
              <span>Group</span>
              <input type="text" aria-label="Group" bind:value={group} list="tf-vw-groups-{uid}" />
              <datalist id="tf-vw-groups-{uid}">
                {#each groups as option (option)}<option value={option}></option>{/each}
              </datalist>
              <small>Pick one, or type a new group</small>
            </label>
          </div>
        </section>

        <section class="tf-vw-sec">
          <h3>Which tickets</h3>
          {#if copiedFrom !== null}
            <p class="tf-vw-from">
              Copied from the {copiedFrom.system} {copiedFrom.kind} <b>{copiedFrom.name}</b>. It no longer follows {copiedFrom.system}.{#if copiedFrom.left.length > 0}{" "}Left out, because knobas cannot test {copiedFrom.left.length > 1 ? "them" : "it"}:
                {#each copiedFrom.left as part, i (i)}{i > 0 ? ", " : ""}<code class="tf-mono">{part.clause}</code>
                  ({part.reason}){/each}.{/if}
            </p>
          {/if}
          {#if unreadable !== null}
            <p class="tf-vw-warn">The search cannot read <code class="tf-mono">{unreadable}</code> in this view.</p>
          {/if}
          <div class="tf-vw-rules">
            {#if conditions.length === 0}
              <p class="tf-dim">No conditions yet, so it holds every synced ticket.</p>
            {/if}
            {#each conditions as condition, at (at)}
              <div class="tf-vw-rule">
                <select
                  aria-label="Field"
                  value={condition.field}
                  onchange={(event) =>
                    setCondition(at, { field: event.currentTarget.value as ConditionField, negated: false, values: [] })}
                >
                  {#each fieldChoices(condition) as entry (entry.field)}
                    <option value={entry.field}>{entry.label}</option>
                  {/each}
                </select>
                <div class="tf-seg" role="group" aria-label="Match">
                  <button
                    type="button"
                    aria-pressed={!condition.negated}
                    onclick={() => setCondition(at, { negated: false })}
                    >is</button
                  >
                  <button
                    type="button"
                    aria-pressed={condition.negated}
                    onclick={() => setCondition(at, { negated: true })}
                    >is not</button
                  >
                </div>
                <button
                  type="button"
                  class="tf-vw-x"
                  aria-label="Remove this condition"
                  onclick={() => (conditions = conditions.filter((_, i) => i !== at))}>✕</button
                >
                <div class="tf-vw-vals">
                  {#each options(condition) as [value, label] (value)}
                    <button
                      type="button"
                      class="tf-vw-val"
                      aria-pressed={condition.values.some((v) => same(v, value))}
                      onclick={() => toggleValue(at, value)}>{label}</button
                    >
                  {/each}
                </div>
              </div>
            {/each}
          </div>
          <div class="tf-vw-btns">
            <button type="button" class="tf-btn tf-sm" onclick={addCondition}>+ Add a condition</button>
            <span class="tf-dim">Every condition has to hold. Values in one condition mean any of them.</span>
          </div>
          <label class="tf-vw-words">
            <span>Words in the key or summary</span>
            <input type="text" aria-label="Words in the key or summary" bind:value={words} placeholder="Optional" />
          </label>
          <p class="tf-vw-q">
            As a search: <code class="tf-mono"
              >{#if parts.length === 0}every synced ticket{:else}{#each parts as part, i (i)}{i > 0 ? " " : ""}{#if part.parses}{part.text}{:else}<span
                      class="tf-vw-newq"
                      title="Not in the search grammar yet">{part.text}</span
                    >{/if}{/each}{/if}</code
            >
          </p>
        </section>
        {/if}

        <section class="tf-vw-sec">
          <h3>How they show</h3>
          <div class="tf-seg" role="group" aria-label="Layout">
            <button type="button" aria-pressed={layout === "board"} onclick={() => (layout = "board")}>Board</button>
            <button type="button" aria-pressed={layout === "list"} onclick={() => (layout = "list")}>List</button>
          </div>
          {#if layout === "board"}
            <div class="tf-vw-sub">
              <h4>Columns</h4>
              {#if fixedSource !== null}
                <p class="tf-dim">The columns come from the board in {fixedSource.system} and change when someone changes them there.</p>
              {:else}
                <p class="tf-dim">
                  Each status sits on one column. A column can hold several. First in this list is leftmost on the board.
                </p>
              {/if}
              <div class="tf-vw-cols">
                {#each columns ?? [] as column, at (at)}
                  <div class="tf-vw-col">
                    <input
                      type="text"
                      aria-label="Column name"
                      value={column.name}
                      disabled={fixedSource !== null}
                      oninput={(event) => setColumn(at, { name: event.currentTarget.value })}
                    />
                    <div class="tf-vw-vals">
                      {#each statuses as status (status)}
                        <button
                          type="button"
                          class="tf-vw-val"
                          aria-pressed={column.statuses.includes(status)}
                          disabled={fixedSource !== null}
                          onclick={() => (columns = toggleStatus(columns ?? [], at, status))}>{status}</button
                        >
                      {/each}
                    </div>
                    <label class="tf-vw-max"
                      >At most <input
                        type="number"
                        min="0"
                        aria-label="At most"
                        value={column.limit ?? ""}
                        disabled={fixedSource !== null}
                        placeholder="any"
                        oninput={(event) => {
                          const n = Number.parseInt(event.currentTarget.value, 10);
                          setColumn(at, { limit: Number.isFinite(n) && n > 0 ? n : null });
                        }}
                      /></label
                    >
                    {#if fixedSource === null}
                    <span class="tf-vw-move">
                      <button
                        type="button"
                        aria-label="Earlier"
                        disabled={at === 0}
                        onclick={() => (columns = moveColumn(columns ?? [], at, -1))}>↑</button
                      ><button
                        type="button"
                        aria-label="Later"
                        disabled={at === (columns?.length ?? 0) - 1}
                        onclick={() => (columns = moveColumn(columns ?? [], at, 1))}>↓</button
                      ><button
                        type="button"
                        aria-label="Remove this column"
                        onclick={() => (columns = (columns ?? []).filter((_, i) => i !== at))}>✕</button
                      >
                    </span>
                    {/if}
                  </div>
                {/each}
              </div>
              {#if loose.length > 0}
                <p class="tf-vw-warn">{looseLine}</p>
              {/if}
              {#if fixedSource === null}
              <div class="tf-vw-btns">
                <button
                  type="button"
                  class="tf-btn tf-sm"
                  onclick={() => (columns = [...(columns ?? []), { name: "New column", statuses: [], limit: null }])}
                  >+ Add a column</button
                >
                <button type="button" class="tf-btn tf-sm tf-ghost" onclick={resetColumns}>One column per status</button>
              </div>
              {/if}
            </div>
            <div class="tf-vw-sub">
              <label class="tf-vw-inline"
                ><span>Lanes</span>
                <select aria-label="Lanes" bind:value={lanes}>
                  {#each LANE_FIELDS as [value, label] (value)}<option {value}>{label}</option>{/each}
                </select></label
              >
            </div>
            <div class="tf-vw-sub">
              <h4>Cards show</h4>
              <div class="tf-vw-checks">
                {#each CARD_FIELDS as [field, label] (field)}
                  <label
                    ><input
                      type="checkbox"
                      checked={cardFields.includes(field)}
                      onchange={(event) => toggleCard(field, event.currentTarget.checked)}
                    />
                    {label}</label
                  >
                {/each}
              </div>
            </div>
          {:else}
            <div class="tf-vw-sub">
              <h4>Columns</h4>
              <div class="tf-vw-checks">
                {#each LIST_COLUMN_ORDER as [column, label] (column)}
                  <label
                    ><input
                      type="checkbox"
                      checked={listColumns.includes(column)}
                      disabled={column === "key"}
                      onchange={(event) => toggleListColumn(column, event.currentTarget.checked)}
                    />
                    {label}</label
                  >
                {/each}
              </div>
            </div>
          {/if}
        </section>
        {#if failed !== null}<p class="tf-fail" role="alert">The view was not saved: {failed}</p>{/if}
      </form>

      <aside class="tf-vw-prev" aria-live="polite" aria-label="Preview">
        {#if previewFailed !== null}
          <p class="tf-vw-empty">The tickets could not be read: {previewFailed}</p>
        {:else if shown === null || answer === null}
          <div aria-busy="true"></div>
        {:else}
          <div class="tf-vw-n"><b>{shown.count}</b><span>{shown.count === 1 ? "ticket" : "tickets"} now</span></div>
          {#if shown.count === 0}
            <p class="tf-vw-empty">No synced ticket matches. Remove a condition or a value to see more.</p>
          {:else if layout === "board"}
            <div class="tf-vw-mini">
              {#each shown.columns as column, at (at)}
                <div class="tf-vw-mcol" class:tf-vw-over={column.over}>
                  <div class="tf-vw-mh">
                    <span>{column.name || "Unnamed"}</span><b
                      >{column.total}{#if column.limit !== null}{" "}of {column.limit}{/if}</b
                    >
                  </div>
                  {#each column.cards as card (card.entity_id)}
                    <div class="tf-vw-stub"><span class="tf-mono">{card.key}</span>{card.title}</div>
                  {/each}
                  {#if column.total > column.cards.length}
                    <div class="tf-vw-stub tf-vw-stub-more">{column.total - column.cards.length} more</div>
                  {/if}
                </div>
              {/each}
            </div>
            {#if shown.hidden > 0}
              <p class="tf-vw-warn">{shown.hidden} of them sit in a status on no column.</p>
            {/if}
            {#if laned}
              <p class="tf-dim tf-vw-pnote">
                In {counted(laneCount, "lane", "lanes")} by {LANE_FIELDS.find(([value]) => value === lanes)![1].toLowerCase()}.
              </p>
            {/if}
          {:else}
            <table class="tf-vw-mlist">
              <thead>
                <tr>{#each drawnList as [column, label] (column)}<th>{label}</th>{/each}</tr>
              </thead>
              <tbody>
                {#each listRows as { card } (card.entity_id)}
                  <tr>
                    {#each drawnList as [column] (column)}
                      {#if column === "key"}<td class="tf-mono">{card.key}</td>
                      {:else if column === "summary"}<td>{card.title}</td>
                      {:else if column === "status"}<td>{columnName(card.status)}</td>
                      {:else if column === "priority"}<td>{card.priority ?? ""}</td>
                      {:else if column === "signals"}<td>{card.signals.length}</td>
                      {:else if column === "assignee"}<td>{card.assignee ?? "Unassigned"}</td>
                      {:else if column === "updated"}<td>{clockLabel(card.updated_at)}</td>
                      {:else if column === "epic"}<td>{card.path ?? ""}</td>
                      {:else}<td>{card.labels.join(", ")}</td>{/if}
                    {/each}
                  </tr>
                {/each}
              </tbody>
            </table>
            {#if shown.count > listRows.length}
              <p class="tf-dim tf-vw-pnote">and {shown.count - listRows.length} more</p>
            {/if}
          {/if}
        {/if}
      </aside>
    </div>
  {/snippet}
  {#snippet footer()}
    {#if editing && ondelete && fixedSource === null}
      <button type="button" class="tf-btn tf-ghost tf-vw-del" onclick={ondelete}>Delete view</button>
    {/if}
    <span class="tf-first tf-dim tf-vw-store"
      >{fixedSource !== null
        ? `Only how it looks here. ${fixedSource.system} still decides which tickets.`
        : systems.length === 0
          ? "Kept in knobas. No source sees it."
          : `Kept in knobas. ${systemsWords(systems)} ${systems.length > 1 ? "never see" : "never sees"} it.`}</span
    >
    <button type="button" class="tf-btn tf-ghost" onclick={onclose}>Cancel</button>
    <button type="submit" class="tf-btn tf-primary" form="tf-vw-form-{uid}" disabled={!named || saving}>{saveLabel}</button>
  {/snippet}
</TfDialog>
