<!--
  One board column's cards (the Windows pass, 2026-11-14): from
  {@link STACK_WINDOW_FROM} cards it draws only those near the viewport
  (`shell/windowed.svelte.ts`), with the room of the rest above and below, so
  a key that changes the board draws again what is on screen, not every card
  each pressed *Show more* brought. Under it a column draws whole, as it always
  did, and `cut` gives the first frame's share of a column that grows
  (`TicketsPage`'s `drawnNow`).

  The column's *Show more* and its way to the list stay below this box, in the
  column's own stack, so the undrawn cards' room is above them.
-->
<script lang="ts" module>
  /**
   * A column of this many cards or more windows: three of the eight-card pages, so a board as it first draws (eight a
   * column, three for done) is the DOM it always was, and a column a filter's words take to fifty, or a *Show more*
   * to fifty-eight, draws the screen's cards. Expanded columns of 1,243 cards between them made each filter key draw
   * every one again: 248 to 732 ms a key in WebView2 at 4x (the Windows pass, 2026-11-14).
   */
  export const STACK_WINDOW_FROM = 24;
</script>

<script lang="ts">
  import { untrack, type Snippet } from "svelte";
  import type { Attachment } from "svelte/attachments";
  import type { MiniBoardCard } from "../../ipc/entity";
  import { Windowed } from "../../shell/windowed.svelte";

  let {
    cards,
    cut,
    card,
    onwin,
    floor = null,
  }: {
    /** Every card the column draws, in order. */
    cards: MiniBoardCard[];
    /** The first frame's share of `cards` while the column does not window. */
    cut: (cards: MiniBoardCard[]) => MiniBoardCard[];
    /** One card, with what goes on its element so the window measures it. */
    card: Snippet<[MiniBoardCard, Attachment<HTMLElement>]>;
    /** Hands the page this column's window, so `j`/`k` can draw a card before it focuses it; `null` once it goes. */
    onwin: (win: Windowed | null) => void;
    /**
     * The least a card stands before it is drawn (`cardFloor`): given, a key's first frame draws the cards on screen
     * and one more, not a quarter of a screen more (the ux14 pass, 2026-11-16).
     */
    floor?: ((card: MiniBoardCard) => number) | null;
  } = $props();

  const byKey = $derived(floor === null ? null : new Map(cards.map((one) => [one.entity_id, one])));
  const floorOf = (key: string) => {
    const one = byKey?.get(key);
    return one && floor ? floor(one) : 130;
  };

  /** A card is 86 to 211 px on the scale estate (a line of summary or four, an epic line or not), most about 130, 6 apart. */
  const win = new Windowed({
    count: () => cards.length,
    key: (i) => cards[i]!.entity_id,
    estimate: 130,
    estimateOf: untrack(() => floor) === null ? null : floorOf,
    onScreen: untrack(() => floor) !== null,
    // Reckoned at its floor, a card stands at about two thirds of the 136 px it stood at, so 400 px below the view
    // holds as many cards as 600 did: the key's later grow builds no more than the overscan always held.
    overscan: untrack(() => floor) === null ? 600 : 400,
    gap: 6,
    from: STACK_WINDOW_FROM,
  });

  /** The page holds this window while the column's box is drawn. */
  const held = () => {
    onwin(win);
    return () => onwin(null);
  };
</script>

<div class="tf-cards" {@attach win.list} {@attach held}>
  {#each win.slice(win.on ? cards : cut(cards)) as one (one.entity_id)}
    {@render card(one, win.row(one.entity_id))}
  {:else}
    <span class="tf-dim tf-nothing">Nothing here</span>
  {/each}
</div>
