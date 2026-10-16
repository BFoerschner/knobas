<!--
  Dependencies (#871, R-11.10 to R-11.14): `#/deps`, the top-bar opening.
  Everything knobas knows, in the shared layers from the work down to the
  hardware, over one read (`dependencies`).

  Left, the rail: every layer with a checkbox, its count and a red count of
  its problems; a name scrolls to its band. Right, a band per shown layer: its
  name, a one-line summary, what is wrong and a health bar, then its tiles
  (monogram, name, one state word; solid red when broken, amber on warning,
  outlined blue when running or open, hatched when nothing watches it; one
  line each above 30). Under the bands, the layers this page hides and the
  items whose kind sits in no layer, never dropped silently.

  The tracer: which layers show is the page's own for the session, until
  #873's saved views keep it. Lines, *Find*, folding and the filters are
  #872's; the layers panel and the view menu #873's; the editors #875's and
  #876's. A tile opens the thing's own address.
-->
<script lang="ts">
  import { ipcErrorMessage } from "../../ipc";
  import { dependencies, type Dependencies } from "../../ipc/assets";
  import { bandsOf, hrefOf, kindOf, lineOf, railOf, stateOf, unplacedOf } from "./deps";
  import { hiddenLayers, type HiddenLayers } from "./hidden.svelte";

  let {
    ready,
    ports = { dependencies },
    hidden = hiddenLayers,
  }: {
    /** Whether the database answers yet. */
    ready: boolean;
    ports?: { dependencies: () => Promise<Dependencies> };
    /** Which layers are hidden: the session's set unless a test hands its own. */
    hidden?: HiddenLayers;
  } = $props();

  let read = $state.raw<Dependencies | null>(null);
  let failure = $state<string | null>(null);

  async function load() {
    try {
      read = await ports.dependencies();
      failure = null;
    } catch (error) {
      failure = ipcErrorMessage(error);
    }
  }

  // Read once, when the database answers.
  let started = false;
  $effect(() => {
    if (ready && !started) {
      started = true;
      void load();
    }
  });

  const bands = $derived(read === null ? [] : bandsOf(read, hidden.ids));
  const rail = $derived(read === null ? [] : railOf(read, hidden.ids));
  const unplaced = $derived(read === null ? { count: 0, kinds: [] } : unplacedOf(read));
  const hiddenRows = $derived(rail.filter((row) => !row.shown));

  let bandsEl = $state<HTMLElement | null>(null);

  function scrollTo(layer: string) {
    bandsEl?.querySelector(`[data-layer="${CSS.escape(layer)}"]`)?.scrollIntoView({ block: "start", behavior: "smooth" });
  }
</script>

<header class="tf-fhead">
  <div>
    <h1>Dependencies</h1>
    <div class="tf-sub">
      Everything knobas knows, in layers from the work down to the hardware. Point at anything to see what it touches;
      click to open it.
    </div>
  </div>
  <div class="tf-acts">
    <a class="tf-btn tf-ghost" href="#/assets" title="The asset tree in columns">Asset tree</a>
    <a class="tf-btn tf-ghost" href="#/monitors">Monitors</a>
  </div>
</header>

<div class="tf-dp">
  {#if failure !== null}
    <p class="tf-dp-fail" role="alert">The estate could not be read: {failure}. Open Dependencies again to retry.</p>
  {:else if read !== null}
    {@const kinds = read.kinds}
    <div class="tf-dp-grid">
      <nav class="tf-dp-rail" aria-label="Layers">
        <div class="tf-dp-rh">Layers</div>
        {#each rail as row (row.id)}
          <div class="tf-dp-rl" class:tf-dp-off={!row.shown}>
            <input
              type="checkbox"
              checked={row.shown}
              aria-label="Show {row.name}"
              onchange={(event) => hidden.set(row.id, !event.currentTarget.checked)}
            />
            <button type="button" disabled={!row.shown} onclick={() => scrollTo(row.id)}>{row.name}</button>
            <span class="tf-dp-rc">{row.count.toLocaleString("en")}</span>
            {#if row.problems > 0}<b class="tf-dp-rb" aria-label="{row.problems} problems">{row.problems}</b>{/if}
          </div>
        {/each}
      </nav>

      <div class="tf-dp-bands" bind:this={bandsEl}>
        {#each bands as band (band.id)}
          <section class="tf-dp-band" data-layer={band.id} aria-labelledby="tf-dp-h-{band.id}">
            <header>
              <div>
                <h3 id="tf-dp-h-{band.id}">{band.name}</h3>
                <p>
                  {band.items.length > 0 ? band.summary : "Nothing here in this view"}
                  {#if band.wrong}<b class="tf-dp-wrong">{band.wrong}</b>{:else if band.items.length > 0}<span class="tf-dp-fine">all fine</span>{/if}
                </p>
                {#if band.items.length > 0}
                  {@const total = band.items.length}
                  <svg class="tf-dp-mini" viewBox="0 0 {total} 1" preserveAspectRatio="none" aria-hidden="true">
                    {#each band.mix as part, index (part.tone)}
                      {@const x = band.mix.slice(0, index).reduce((sum, p) => sum + p.count, 0)}
                      <rect class="tf-dp-mix-{part.tone}" {x} y="0" width={part.count} height="1" />
                    {/each}
                  </svg>
                {/if}
              </div>
            </header>
            <div class="tf-dp-body">
              {#if band.items.length === 0}
                <p class="tf-dp-none">Nothing in this layer matches the view.</p>
              {:else}
                <div class="tf-dp-tiles" class:tf-dp-c={band.compact}>
                  {#each band.items as tile (tile.id)}
                    {@const kind = kindOf(kinds, tile.kind)}
                    {@const state = stateOf(tile)}
                    {@const line = lineOf(tile)}
                    <a
                      class="tf-dp-t tf-dp-{state.tone}"
                      class:tf-dp-c={band.compact}
                      href={hrefOf(tile)}
                      title="{kind.label}: {tile.name}{line ? `. ${line}` : ''}"
                    >
                      <span class="tf-dp-mono" aria-hidden="true">{kind.monogram}</span>
                      <span class="tf-dp-nm" class:tf-dp-code={tile.kind !== "ticket" && tile.kind !== "pr"}>{tile.name}</span>
                      {#if band.compact}
                        <i class="tf-dp-dot tf-dp-dot-{state.tone}" role="img" aria-label={state.word || kind.label}></i>
                      {:else}
                        <span class="tf-dp-w">{line}</span>
                      {/if}
                    </a>
                  {/each}
                </div>
              {/if}
            </div>
          </section>
        {/each}
        {#if hiddenRows.length > 0 || unplaced.count > 0}
          <p class="tf-dp-foot">
            {#if hiddenRows.length > 0}
              Hidden in this view:
              {#each hiddenRows as row, index (row.id)}
                <button type="button" class="tf-linkish" onclick={() => hidden.set(row.id, false)}>{row.name} ({row.count.toLocaleString("en")})</button>{index < hiddenRows.length - 1 ? ", " : ""}
              {/each}.
            {/if}
            {#if unplaced.count > 0}
              {unplaced.count.toLocaleString("en")} items are in no layer: {unplaced.kinds.join(", ")}. Place them in Layers…
            {/if}
          </p>
        {/if}
      </div>
    </div>
  {/if}
</div>
