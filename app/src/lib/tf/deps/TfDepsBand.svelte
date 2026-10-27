<!--
  One band of Dependencies (#871, #872): the layer's name, summary, what is
  wrong and its health bar, then its open groups (problems first, then a few,
  then "N more"), and the healthy groups folded into chips. Folded, the band is
  its header alone.

  Every element that stands for items carries `data-el` (`draw.ts`' `where`),
  so the page can draw a line to it and dim or light it. A tile is a link to
  the item's view with a ⌖ beside it; the page handles the clicks and keys.
-->
<script lang="ts">
  import type { DepItem, DepKind } from "../../ipc/assets";
  import Marked from "../filter/Marked.svelte";
  import { hrefOf, isEstate, kindOf, lineOf, nameIsCode, stateOf } from "./deps";
  import { moreLine, tileEl, type DrawnBand, type Mix } from "./draw";

  let {
    band,
    kinds,
    query,
    hit,
    lit,
    hover,
    onfold,
    onopengroup,
    onshutgroup,
    onmore,
    onopen,
    ontrace,
    flash = null,
    onadd = null,
    onlink = null,
  }: {
    band: DrawnBand;
    kinds: readonly DepKind[];
    query: string;
    hit: (item: DepItem) => boolean;
    /** The elements what is pointed at touches, or `null` when nothing is. */
    lit: ReadonlySet<string> | null;
    hover: string | null;
    onfold: (layer: string, folded: boolean) => void;
    onopengroup: (key: string) => void;
    onshutgroup: (key: string) => void;
    onmore: (key: string) => void;
    onopen: (item: DepItem, event: MouseEvent) => void;
    ontrace: (item: DepItem) => void;
    /** The tile just added or saved, which flashes once (#876). */
    flash?: string | null;
    /** *+ Add* on a layer that holds assets or routes (#876, R-11.20). */
    onadd?: ((layer: string) => void) | null;
    /** *+ Link* on a layer of work, from a ticket. */
    onlink?: (() => void) | null;
  } = $props();

  const WORK = ["ticket", "pr", "page", "repo", "pipeline"];
  const holdsEstate = $derived(band.kinds.some(isEstate));

  const dim = (el: string) => lit !== null && !lit.has(el);
  const glow = (el: string) => lit !== null && lit.has(el) && (hover === null || el !== tileEl(hover));
  const total = (mix: readonly Mix[]) => mix.reduce((sum, part) => sum + part.count, 0);
</script>

{#snippet bar(mix: readonly Mix[], className: string)}
  {#if mix.length > 0}
    <svg class={className} viewBox="0 0 {total(mix)} 1" preserveAspectRatio="none" aria-hidden="true">
      {#each mix as part, index (part.tone)}
        {@const x = mix.slice(0, index).reduce((sum, p) => sum + p.count, 0)}
        <rect class="tf-dp-mix-{part.tone}" {x} y="0" width={part.count} height="1" />
      {/each}
    </svg>
  {/if}
{/snippet}

<section
  class="tf-dp-band"
  class:tf-dp-folded={band.folded}
  class:tf-dp-dim={band.folded && dim(band.el)}
  class:tf-dp-lit={band.folded && glow(band.el)}
  data-layer={band.id}
  data-el={band.folded ? band.el : undefined}
  aria-labelledby="tf-dp-h-{band.id}"
>
  <header>
    <button
      type="button"
      class="tf-dp-fold"
      aria-expanded={!band.folded}
      aria-label="{band.folded ? 'Unfold' : 'Fold'} {band.name}"
      onclick={() => onfold(band.id, !band.folded)}>{band.folded ? "▸" : "▾"}</button
    >
    <div>
      <h3 id="tf-dp-h-{band.id}">{band.name}</h3>
      <p>
        {band.count > 0 ? band.summary : "Nothing here in this view"}
        {#if band.wrong}<b class="tf-dp-wrong">{band.wrong}</b>{:else if band.count > 0}<span class="tf-dp-fine">all fine</span>{/if}
      </p>
      {@render bar(band.mix, "tf-dp-mini")}
    </div>
    {#if holdsEstate && onadd !== null}
      <button type="button" class="tf-btn tf-sm tf-ghost tf-dp-add" title="Add to {band.name}" aria-label="Add to {band.name}" onclick={() => onadd(band.id)}>+ Add</button>
    {:else if onlink !== null && band.kinds.some((kind) => WORK.includes(kind))}
      <button type="button" class="tf-btn tf-sm tf-ghost tf-dp-add" title="Link something to this ticket" onclick={() => onlink()}>+ Link</button>
    {/if}
  </header>
  {#if !band.folded}
    <div class="tf-dp-body">
      {#each band.open as group (group.key)}
        <div class="tf-dp-g">
          {#if group.name}
            <div class="tf-dp-gh">
              <button type="button" aria-label="Fold {group.name}" onclick={() => onshutgroup(group.key)}>▾</button>
              <b>{group.name}</b>
              <span>{group.count.toLocaleString("en")}{#if group.wrong}, <span class="tf-dp-wrong-i">{group.wrong}</span>{/if}</span>
            </div>
          {/if}
          <div class="tf-dp-tiles" class:tf-dp-c={band.compact}>
            {#each group.tiles as tile (tile.id)}
              {@const kind = kindOf(kinds, tile.kind)}
              {@const state = stateOf(tile)}
              {@const line = lineOf(tile, band.group === "environment")}
              {@const el = tileEl(tile.id)}
              {@const matched = hit(tile)}
              <div
                class="tf-dp-t tf-dp-{state.tone}"
                class:tf-dp-c={band.compact}
                class:tf-dp-hit={matched}
                class:tf-dp-miss={query.trim() !== "" && !matched}
                class:tf-dp-dim={dim(el)}
                class:tf-dp-lit={glow(el)}
                class:tf-dp-flash={flash === tile.id}
                data-el={el}
                data-id={tile.id}
              >
                <a
                  class="tf-dp-open"
                  data-testid="dp-{tile.name}"
                  href={hrefOf(tile)}
                  title="{kind.label}: {tile.name}{line ? `. ${line}` : ''}"
                  onclick={(event) => onopen(tile, event)}
                >
                  <span class="tf-dp-mono" aria-hidden="true">{kind.monogram}</span>
                  <span class="tf-dp-nm" class:tf-dp-code={nameIsCode(tile.kind)}><Marked text={tile.name} typed={query} /></span>
                  {#if band.compact}
                    <i class="tf-dp-dot tf-dp-dot-{state.tone}" role="img" aria-label={state.word || kind.label}></i>
                  {:else}
                    <span class="tf-dp-w">{line}</span>
                  {/if}
                </a>
                <button
                  type="button"
                  class="tf-dp-pin"
                  aria-label="Show only what {tile.name} touches"
                  title="Show only what this touches (T)"
                  onclick={() => ontrace(tile)}>⌖</button
                >
              </div>
            {/each}
            {#if group.more}
              <button
                type="button"
                class="tf-dp-more"
                class:tf-dp-hit={group.more.hits > 0}
                class:tf-dp-dim={dim(group.more.el)}
                class:tf-dp-lit={glow(group.more.el)}
                data-el={group.more.el}
                onclick={() => onmore(group.key)}
              >
                {moreLine(group.more.count, group.more.wrong, group.more.hits)}
              </button>
            {/if}
          </div>
        </div>
      {/each}
      {#if band.shut.length > 0}
        <div class="tf-dp-shut">
          <span class="tf-dp-shut-l">{band.shutLabel}</span>
          {#each band.shut as group (group.key)}
            <button
              type="button"
              class="tf-dp-chip"
              class:tf-dp-bad={group.wrong !== ""}
              class:tf-dp-dim={dim(group.el)}
              class:tf-dp-lit={glow(group.el)}
              data-el={group.el}
              onclick={() => onopengroup(group.key)}
            >
              <b>{group.name || band.name}</b>
              {group.count.toLocaleString("en")}{@render bar(group.mix, "tf-dp-mini tf-dp-chipbar")}
            </button>
          {/each}
        </div>
      {/if}
      {#if band.open.length === 0 && band.shut.length === 0}
        <p class="tf-dp-none">Nothing in this layer matches the view.</p>
      {/if}
    </div>
  {/if}
</section>
