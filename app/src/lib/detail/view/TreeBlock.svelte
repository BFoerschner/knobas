<!--
  One directory of a tree (#711): its trail, then one row per entry --
  directories first, as the source ordered them -- each opening the section
  it names in this tab. A submodule is another repository and is named, not
  opened. Rows are the diff index's: a hairline box of 24px mono lines.
-->
<script lang="ts">
  import type { Crumb, TreeEntry } from "../../ipc/entity";
  import { sizeOf } from "./bytes";
  import Trail from "./Trail.svelte";

  let {
    trail,
    entries,
    onopen,
    onback,
  }: {
    trail: Crumb[];
    entries: TreeEntry[];
    onopen: (section: string) => void;
    onback?: (() => void) | undefined;
  } = $props();
</script>

<div class="tb">
  <Trail {trail} {onopen} {onback} />
  {#if entries.length === 0}
    <p class="tb-empty">This directory is empty.</p>
  {:else}
    <ul class="tb-list">
      {#each entries as entry (entry.name)}
        <li class="tb-row" data-kind={entry.kind}>
          {#snippet line()}
            <svg class="tb-icon" viewBox="0 0 16 16" aria-hidden="true">
              {#if entry.kind === "dir" || entry.kind === "submodule"}
                <path d="M2 4.5h4l1.5 1.5H14v6.5H2z" />
              {:else}
                <path d="M4 2h5l3 3v9H4z" />
              {/if}
            </svg>
            <span class="tb-name">{entry.name}</span>
            {#if entry.kind === "symlink"}<span class="tb-tag">link</span>{/if}
            {#if entry.kind === "submodule"}<span class="tb-tag">submodule</span>{/if}
            <span class="tb-size">{entry.size === null ? "" : sizeOf(entry.size)}</span>
          {/snippet}
          {#if entry.section !== null}
            {@const section = entry.section}
            <button
              class="tb-line"
              aria-label={entry.kind === "dir" ? `${entry.name}, directory` : entry.name}
              onclick={() => onopen(section)}
            >
              {@render line()}
            </button>
          {:else}
            <div class="tb-line still">{@render line()}</div>
          {/if}
        </li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .tb {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .tb-empty {
    color: var(--muted);
    font-size: 12px;
  }

  .tb-list {
    list-style: none;
    border: 1px solid var(--hair);
    border-radius: 2px;
  }

  .tb-row + .tb-row {
    border-top: 1px solid var(--hair);
  }

  .tb-line {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    height: 24px;
    padding: 0 8px;
    text-align: left;
    font: 400 12px var(--mono);
  }

  button.tb-line:hover {
    background: var(--raised);
  }

  button.tb-line:focus-visible {
    outline-offset: -2px;
  }

  .tb-line.still {
    color: var(--muted);
  }

  .tb-icon {
    width: 14px;
    height: 14px;
    flex: none;
    fill: none;
    stroke: var(--faint);
    stroke-width: 1.2;
  }

  [data-kind="dir"] .tb-icon {
    stroke: var(--muted);
  }

  .tb-name {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  [data-kind="dir"] .tb-name {
    font-weight: 500;
  }

  .tb-tag {
    flex: none;
    font: 400 10px var(--mono);
    color: var(--faint);
    border: 1px solid var(--hair2);
    border-radius: 2px;
    padding: 0 4px;
  }

  .tb-size {
    margin-left: auto;
    flex: none;
    color: var(--faint);
    font-variant-numeric: tabular-nums;
  }
</style>
