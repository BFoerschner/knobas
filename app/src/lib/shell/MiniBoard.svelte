<!--
  The Tickets tile's body: the mini board (`signal-miller.html:2383-2388`).

  A group over a room's live tickets, one per status the source gave them,
  arranged either as columns side by side or stacked one under another.
  ADR-0009 names it the *mini board* — "board" never stands alone, and this is
  not the launcher's board or the assets one.

  **It sorts nothing and groups nothing.** Both consumers of the granted read
  (#177) get one board, so the grouping and the column order are the command's;
  a client that re-sorted here would be a second opinion about the same
  question, and the two would drift. What this file decides is what the reader
  *sees*: the words over the terminal group, and what a card shows.

  **Nor does it choose its layout** (#210). The room does, because the thing
  that decides is whether the room is bounded, which is a fact about the room
  and not about the cards that happen to be on the board today. One markup for
  both, one class apart: the groups, their order and their counts are the same
  either way, so a second template would be two places to keep telling the
  same truth.
-->
<script lang="ts">
  import type { EntityRow, MiniBoard } from "../ipc/entity";
  import type { MiniBoardLayout } from "./contexts";

  let {
    board,
    layout,
    onopen,
  }: {
    board: MiniBoard;
    /** How to arrange the groups — the room's answer, never this file's. */
    layout: MiniBoardLayout;
    onopen: (row: Pick<EntityRow, "kind" | "entity_id">) => void;
  } = $props();

  /**
   * What the terminal group is called on screen.
   *
   * The read puts a `null` status on that column rather than a word, so this
   * is the shell's copy and not a status any source said (§10.8, #177). A
   * ticket lands here because its mirrored record carries no status knobas
   * could read — a visible miss, which is the whole point of the group.
   */
  const NO_STATUS = "No status";

  /**
   * Every card on this board is a ticket.
   *
   * Not a guess: the granted read's statement is `where i.kind = 'ticket'`, so
   * the kind is a property of the query rather than of the row, and the DTO
   * rightly does not repeat it on every card. The address the tile opens needs
   * it, so it is spelled once, here.
   */
  const KIND = "ticket";
</script>

<div class="board {layout}">
  {#each board.columns as column (column.status ?? "")}
    <div class="col">
      <div class="col-h">
        <span>{column.status ?? NO_STATUS}</span>
        <span>{column.cards.length}</span>
      </div>
      <div class="tile-b">
        {#each column.cards as card (card.entity_id)}
          <!--
            Everything below is text: a key, a priority and a title are all
            whatever somebody typed into a ticket (roadmap §4 gotcha 7).
          -->
          <button class="card" onclick={() => onopen({ kind: KIND, entity_id: card.entity_id })}>
            <span class="k">
              <span class="mono">{card.key}</span>
              {#if card.priority}<span class="pr">{card.priority}</span>{/if}
            </span>
            <span class="s">{card.title}</span>
          </button>
        {/each}
      </div>
    </div>
  {/each}
</div>
