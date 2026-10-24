<!--
  Sources › Jira › *Boards and filters* (#878, R-3.20, R-13a.2, D17):
  prototype-6's `#boards` section of `stgSource`, naming the adapter's ticket
  system (`system`, #2032) where prototype-6 says Jira.

  - Which boards come in, and whether the favourite filters do: two keys of
    the adapter's config (`boards.ts`), each saved as it changes through the
    page's own save, so a sync recorded field id is kept. Read at the next
    sync.
  - Every Jira view the source keeps: its ticket count, the tickets on it
    knobas does not mirror, and *Hide* / *Show* (`hide_saved_view`).
-->
<script lang="ts">
  import { ipcErrorMessage } from "../../ipc";
  import type { TicketViewRow } from "../../ipc/search";
  import type { SourceSummary } from "../../ipc/sources";
  import { push } from "../../shell/toasts.svelte";
  import {
    BOARD_SCOPES,
    boardScope,
    counted,
    favouriteFilters,
    savedViewsOf,
    unsyncedWords,
    viewType,
    type BoardScope,
    type SavedViewRow,
  } from "./boards";
  import type { SettingsPorts } from "./ports";

  let {
    source,
    system,
    ports,
    saving,
    finished,
    onsave,
  }: {
    source: SourceSummary;
    /** The ticket system its views come from (`savedViewSystem`, #2032): *Jira*. */
    system: string;
    ports: SettingsPorts;
    saving: boolean;
    /** Moves on every finished run, so the views are read again. */
    finished: number;
    /** Lay `changes` over the stored config and save it, saying `done`. */
    onsave: (changes: Record<string, unknown>, done: string) => Promise<unknown>;
  } = $props();

  const scope = $derived(boardScope(source.config));
  const filtersOn = $derived(favouriteFilters(source.config));
  const DONE = $derived(`Saved. The next ${system} sync reads the boards again.`);

  let all = $state<TicketViewRow[] | null>(null);
  const views = $derived(savedViewsOf(all ?? [], source.id));
  const boards = $derived(views.filter((view) => view.saved_view_source.kind === "board").length);
  const filters = $derived(views.filter((view) => view.saved_view_source.kind === "filter").length);

  function read() {
    ports
      .ticketViews()
      .then((rows) => (all = rows))
      .catch(() => (all ??= []));
  }

  $effect(() => {
    void finished;
    read();
  });

  async function choose(next: BoardScope, input: HTMLInputElement) {
    if (next === scope) return;
    if ((await onsave({ boards: next }, DONE)) !== null) return;
    // Refused: the stored choice is checked again.
    const stored = BOARD_SCOPES.findIndex(([key]) => key === scope);
    const inputs = input.closest("[role=radiogroup]")?.querySelectorAll<HTMLInputElement>("input") ?? [];
    if (inputs[stored]) inputs[stored].checked = true;
  }

  async function toggle(view: SavedViewRow) {
    try {
      await ports.hideSavedView(view.id, !view.saved_view_source.hidden);
      read();
    } catch (cause) {
      push({ text: `${view.name}: ${ipcErrorMessage(cause)}`, tone: "err" });
    }
  }
</script>

<section class="tf-stg-sec" id="boards">
  <h2>Boards and filters</h2>
  <p class="tf-stg-p">Each one is a ticket view in Tickets, kept at every sync. {system} decides which tickets it holds.</p>
  <div class="tf-radio" role="radiogroup" aria-label="Boards">
    {#each BOARD_SCOPES as [key, label] (key)}
      <label>
        <input
          type="radio"
          name="tf-src-boards"
          checked={scope === key}
          disabled={saving}
          onchange={(event) => void choose(key, event.currentTarget)}
        />
        <span
          >{label}{#if scope === key && key !== "none" && all !== null}<small>{counted(boards, "board", "boards")}</small
            >{/if}</span
        >
      </label>
    {/each}
  </div>
  <div class="tf-stg-row">
    <div class="tf-stg-l">
      My favourite filters{#if all !== null && filtersOn}<small>{counted(filters, "filter", "filters")}</small>{/if}
    </div>
    <div class="tf-stg-c">
      <button
        class="tf-stg-sw"
        role="switch"
        aria-checked={filtersOn}
        aria-label="Favourite filters"
        disabled={saving}
        onclick={() => void onsave({ favourite_filters: !filtersOn }, DONE)}><i></i></button
      >
    </div>
  </div>
  {#if views.length > 0}
    <table class="tf-tbl tf-stg-boards">
      <thead><tr><th>In Tickets</th><th>Type</th><th>Tickets</th><th><span class="tf-sr">Hide or show</span></th></tr></thead>
      <tbody>
        {#each views as view (view.id)}
          {@const outside = unsyncedWords(view.saved_view_source.unsynced)}
          <tr>
            <td
              ><a href="#/tickets/{view.id}">{view.name}</a>{#if view.saved_view_source.hidden}{" "}<span class="tf-chip">hidden</span
                >{/if}</td
            >
            <td class="tf-dim">{viewType(view.saved_view_source)}</td>
            <td>{view.count ?? 0}{#if outside}{" "}<span class="tf-dim">{outside}</span>{/if}</td>
            <td class="tf-stg-end">
              <button
                class="tf-btn tf-sm tf-ghost"
                aria-label="{view.saved_view_source.hidden ? 'Show' : 'Hide'} {view.name}"
                onclick={() => void toggle(view)}>{view.saved_view_source.hidden ? "Show" : "Hide"}</button
              >
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
  {/if}
</section>
