<!--
  One page the page host (`TfPage.svelte`) draws, shown or kept hidden by
  keep-alive (#2247, `page-entry.ts`).

  It hands the page the shell's context with the page's own address and its
  `visible` flag, so a hidden page keeps the props of the address it was left
  at, and tells the components inside it through `pageVisible()`. It draws the
  role frame (#2106) for a page that names a `role`: *Sync now* into its header
  as `sync`, and `syncRevision`, which also moves each time the page is shown
  again, so a role view re-reads on its return as it does after a run. It
  keeps the page's scroll across a hide (`keepScroll`, #2248).
-->
<script lang="ts">
  import { untrack } from "svelte";
  import { providePageVisible } from "./kept-pages.svelte";
  import { keepScroll } from "./while-shown.svelte";
  import type { RoleFrameProps, TfPageContext, TfPageEntry } from "./page-entry";
  import { SECTION_NAME, type TfRoute } from "./route";
  import TfCrumbs from "./TfCrumbs.svelte";
  import TfSyncNow from "./TfSyncNow.svelte";

  let {
    entry,
    route,
    visible,
    shows,
    ctx,
  }: {
    /** The page; fixed for this instance, since the host keys it with the entry. */
    entry: TfPageEntry;
    /** The address the page is at, or was left at while hidden. */
    route: TfRoute;
    visible: boolean;
    /** How many times the page was shown again after being hidden. */
    shows: number;
    ctx: Omit<TfPageContext, "route" | "visible">;
  } = $props();

  providePageVisible(() => visible);
  // The shell's one scroller is every page's: each page shows again where it was left (#2248).
  let wrapper = $state<HTMLElement>();
  keepScroll(() => wrapper, () => visible);

  const pageCtx = $derived<TfPageContext>({ ...ctx, route, visible });

  /**
   * The page's props, each its own signal (#1987). A spread of `entry.props(ctx)`
   * is one object, new on every change to `ctx`, and every prop read hangs on
   * it: a tab switch would re-run a page's read keyed on a prop that stayed,
   * as the job's `segment`. Here a prop tells its readers only when its own
   * value changed (`===`: an array such as `rest`, new per route, still tells
   * them). The keys are read once; `props` returns a fixed set.
   */
  function propSignals(read: () => Record<string, unknown>): Record<string, unknown> {
    const out: Record<string, unknown> = {};
    for (const key of Object.keys(untrack(read))) {
      const value = $derived(read()[key]);
      Object.defineProperty(out, key, { get: () => value, enumerable: true });
    }
    return out;
  }
  const page = untrack(() => entry);
  const pageProps = propSignals(() => page.props(pageCtx));

  /** The role this page is a view of, and its *Sync now* (#2106). */
  const roleSync = $derived(page.role === undefined ? null : ctx.roleSyncs.of(page.role));
  /** What the frame hands a role view beside its own props: nothing for any other page. */
  const frameProps = $derived<RoleFrameProps>(
    roleSync === null ? {} : { sync: syncNow, syncRevision: roleSync.revision + shows },
  );
  const Page = page.component;
</script>

{#snippet syncNow()}
  {#if roleSync !== null}<TfSyncNow sync={roleSync} />{/if}
{/snippet}

<div class="tf-kept" hidden={!visible} bind:this={wrapper}>
  {#if page.crumbs && route.view === "full"}
    <div class="tf-full">
      <TfCrumbs job={ctx.job} jobAt={ctx.jobAt} name={SECTION_NAME[route.section]} />
      <Page {...pageProps} {...frameProps} />
    </div>
  {:else}
    <Page {...pageProps} {...frameProps} />
  {/if}
</div>
