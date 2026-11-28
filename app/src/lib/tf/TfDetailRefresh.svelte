<!--
  An item's detail's *Refresh* or *Sync now* (#2107), drawn in the detail's
  header toolbar or in a monitor's roster row (#2138)
  (`tf/refresh.svelte.ts`). *Refresh* reads the one item and
  says *Refreshing…* meanwhile; *Sync now* runs the item's source, shared with
  every detail of it, and the detail is read again once that run ends. Neither
  is drawn for a turned-off source.
  A page that asks (`looking`) says *Looking for changes…* before it while its
  item is read again after a write (#2615, #2595), in room it always holds.
-->
<script lang="ts">
  import { untrack } from "svelte";
  import type { DetailRefresh, RefreshSubject } from "./refresh.svelte";
  import { push } from "../shell/toasts.svelte";
  import { pageRefresh } from "./page-refresh.svelte";
  import { pageVisible } from "./kept-pages.svelte";
  import { itemLooked } from "./builds/looking.svelte";

  let {
    detail,
    refresh,
    reread,
    page = false,
    small = false,
    ghost = false,
    button = true,
    noun,
    name,
    looking = false,
  }: {
    detail: RefreshSubject;
    refresh: DetailRefresh;
    /** Read the detail again, over what it drew. */
    reread: () => Promise<void> | void;
    /**
     * Whether this is the page's own button, which ⌘R presses while it is
     * drawn (#2156): a detail page's header says so. A monitor's
     * roster row does not; that page's is the header's *Sync now*.
     */
    page?: boolean;
    /** Drawn at the small size: in a page body, as a monitor's roster row draws it (#2294). */
    small?: boolean;
    /** Drawn as a ghost, beside the other ghost actions of a monitor's row (#2138). */
    ghost?: boolean;
    /**
     * Whether the button is drawn. `false` keeps ⌘R and the source's run
     * ending reading the detail again, with nothing on screen: the ticket
     * page, whose Refresh is the keybind's alone (#2401).
     */
    button?: boolean;
    /** The one thing *Refresh* reads, where the view's *Sync now* stands beside it. */
    noun?: string | undefined;
    /**
     * What a press reads, in its toast: *Refreshing payout-service…*, then *Refreshed payout-service from …* (ux25).
     * The item's title unless the page names it shorter (a repository's name without its owner).
     */
    name?: string | undefined;
    /**
     * Whether the page says *Looking for changes…* while its item is read again after a write (#2615): the ticket, a
     * Docs page, a pull request and an item's page. Its room is held while it says nothing, so nothing moves.
     */
    looking?: boolean;
  } = $props();

  // What knobas looks at again (#2595), followed for the window from the app's start (`App.svelte`).
  const looked = $derived(looking && itemLooked(detail.row.entity_id));
  /** The words the toast names. */
  const named = $derived(name ?? (detail.row as { title?: string }).title);
  /** The button's word at rest. */
  const word = $derived(noun === undefined ? "Refresh" : `Refresh ${noun}`);
  /** The button's classes: the Controls button at its size and look, and `tf-refresh`, which tests find it by. */
  const classes = $derived(["tf-btn", small && "tf-sm", ghost && "tf-ghost", "tf-refresh"].filter(Boolean).join(" "));

  const offer = $derived(refresh.offer(detail));
  const sync = $derived(offer === "sync" ? refresh.syncOf(detail) : null);

  /** The run endings this button has seen: one more is a run that ended while it was drawn. */
  let seen: number | null = null;
  // The source's run ended: the detail is read again, once per ending.
  $effect(() => {
    const ended = sync?.revision ?? 0;
    if (seen !== null && ended > seen) untrack(() => void reread());
    seen = ended;
  });

  async function syncNow(run: NonNullable<typeof sync>) {
    await run.start(detail.source.display_name);
    if (run.failed !== null) push({ text: `Could not sync ${detail.source.display_name}: ${run.failed}`, tone: "err" });
  }

  /** *Refreshing…* or *Syncing…*: the button is disabled, and ⌘R presses nothing. */
  const busy = $derived(offer === "refresh" ? refresh.reading : sync?.running === true);

  /** What a click runs, and ⌘R. */
  function press() {
    if (busy) return;
    if (offer === "refresh") void refresh.refresh(detail, reread, named);
    else if (sync !== null) void syncNow(sync);
  }

  const drawn = $derived(offer === "refresh" || sync !== null);
  // While drawn, ⌘R presses it.
  // Only on the shown page: ⌘R never presses a page keep-alive holds hidden (#2247).
  const shown = pageVisible();
  $effect(() => {
    if (page && drawn && shown()) return untrack(() => pageRefresh.add({ kind: "detail", press }));
  });
</script>

{#if looking}
  <!-- Drawn empty while nothing is looked at: a live region inserted with its words is often not announced (#2585). -->
  <span class="tf-looking tf-keepw" data-keep="Looking for changes…" role="status"><span>{looked ? "Looking for changes…" : ""}</span></span>
{/if}
{#if !button}
  <!-- ⌘R's alone (#2401): registered above, nothing drawn. -->
{:else if offer === "refresh"}
  <button
    type="button"
    class={classes}
    disabled={busy}
    title={named === undefined ? `Read this item from ${detail.source.display_name} again` : `Read ${named} from ${detail.source.display_name} again`}
    onclick={press}
    ><!-- As wide as its widest word, so *Refreshing…* moves nothing beside it (ux25). --><span
      class="tf-keepw"
      data-keep={word}
      data-keep2="Refreshing…"><span>{refresh.reading ? "Refreshing…" : word}</span></span
    ></button
  >
{:else if sync !== null}
  <button
    type="button"
    class={classes}
    disabled={busy}
    title="Sync {detail.source.display_name}, which this item is from"
    onclick={press}
    ><span class="tf-keepw" data-keep="Sync now" data-keep2="Syncing…"><span>{sync.running ? "Syncing…" : "Sync now"}</span></span></button
  >
{/if}
