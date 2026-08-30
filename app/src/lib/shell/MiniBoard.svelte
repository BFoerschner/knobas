<!--
  The Tickets tile's body: the mini board (`signal-miller.html:2383-2388`).

  Columns over a room's live tickets, one per status the source gave them.
  ADR-0009 names it the *mini board* — "board" never stands alone, and this is
  not the launcher's board or the assets one.

  **It sorts nothing and groups nothing.** Both consumers of the granted read
  (#177) get one board, so the grouping and the column order are the command's;
  a client that re-sorted here would be a second opinion about the same
  question, and the two would drift. What this file decides is what the reader
  *sees*: the words over the terminal group, and what a card shows.
-->
<script lang="ts">
  import type { EntityRow, MiniBoard } from "../ipc/entity";

  let {
    board,
    onopen,
  }: {
    board: MiniBoard;
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

<div class="board">
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
