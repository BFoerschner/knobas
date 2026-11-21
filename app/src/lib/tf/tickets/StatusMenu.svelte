<!--
  A ticket's status menu (ticket-edit decisions, sections 7, 10 and 13; ADR-0054): the statuses it can reach from where
  it stands, in the source's order, the current one left out, each with its number, which picks it without a second
  aim. The one menu the list's Status cell, the ticket page's status and a drop on a column of several statuses open.

  - While the first read is out: *Asking ⟨source⟩…*, focus on the menu itself, its items taken when they land.
  - Nowhere to go: one disabled line, *⟨Source⟩ offers no move from ⟨status⟩.*
  - A failed read: the statuses the source was seen to use, under *⟨Source⟩ did not answer; it decides when the move
    is sent.*; with none, that line and *Try again*.
  - `only`: the statuses a drop's column holds that the ticket can reach, which the menu offers as they are.
-->
<script lang="ts">
  import TfMenu from "../TfMenu.svelte";
  import type { Reach } from "./reach.svelte";
  import { sameStatus } from "./drop";

  let {
    open = $bindable(false),
    opener,
    ticketKey,
    status,
    source,
    reach,
    asking = false,
    only = null,
    at = null,
    heading = false,
    onpick,
    onretry,
    onclose,
  }: {
    open?: boolean;
    opener: HTMLElement | null;
    /** The ticket's key, for the menu's name: *Move PAY-231 to*. */
    ticketKey: string;
    /** The status it stands in (or is drawn in, while a move of it stands). */
    status: string | null;
    /** The source's display name. */
    source: string;
    reach: Reach | null;
    /** Whether a read of it is out. */
    asking?: boolean;
    /** The statuses to offer as they are (a drop's column), in place of the reach's. */
    only?: readonly string[] | null;
    /** A point to stand at: a drop's landing place. */
    at?: { x: number; y: number } | null;
    /** Draw *Move PAY-231 to* as the menu's first line (a drop's menu, which no button names). */
    heading?: boolean;
    onpick: (status: string) => void;
    onretry?: () => void;
    /** It closed without a pick. */
    onclose?: () => void;
  } = $props();

  const items = $derived(only !== null ? [...only] : (reach?.statuses ?? []).filter((one) => !sameStatus(one, status)));
  const unsure = $derived(only === null && reach !== null && reach.standing !== "exact" && reach.failed !== undefined);
  const waiting = $derived(only === null && reach === null && asking);

  let picked = false;
  let wasOpen = false;
  $effect(() => {
    if (open) {
      wasOpen = true;
      picked = false;
    } else if (wasOpen) {
      wasOpen = false;
      if (!picked) onclose?.();
    }
  });

  function pick(next: string) {
    picked = true;
    open = false;
    onpick(next);
  }

  /** `1` to `9` pick by place. */
  function digits(event: KeyboardEvent) {
    const place = Number(event.key);
    if (!Number.isInteger(place) || place < 1 || place > 9 || event.metaKey || event.ctrlKey || event.altKey) return;
    const item = items[place - 1];
    if (item === undefined) return;
    event.preventDefault();
    event.stopPropagation();
    pick(item);
  }
</script>

<TfMenu bind:open {opener} {at} label="Move {ticketKey} to" className="tf-move-m" align="start" ready={!waiting}>
  {#if heading}<p class="tf-menu-h">Move {ticketKey} to</p>{/if}
  {#if waiting}
    <p class="tf-menu-line">Asking {source}…</p>
  {:else if only === null && (reach === null || (unsure && reach.statuses.length === 0))}
    <p class="tf-menu-line">{source} did not answer; it decides when the move is sent.</p>
    {#if onretry}<button type="button" role="menuitem" onclick={onretry}>Try again</button>{/if}
  {:else}
    {#if unsure}<p class="tf-menu-line">{source} did not answer; it decides when the move is sent.</p>{/if}
    {#each items as item, place (item)}
      <button type="button" role="menuitem" onclick={() => pick(item)} onkeydown={digits}>{item}{#if place < 9}<kbd>{place + 1}</kbd>{/if}</button>
    {:else}
      <button type="button" role="menuitem" aria-disabled="true">{source} offers no move from {status ?? "no status"}.</button>
    {/each}
  {/if}
</TfMenu>
