<!--
  What an import would do (#1066), the rooms UI `ImportDialog`'s groups in the
  new shell's look: *New*, then *Would change*, then monitors to attach and
  names Kuma does not hold, then *Already in the tree* as a count. A changed
  property draws which value wins: `set` the file's, `kept` the stored one,
  with the file's struck through.
-->
<script lang="ts">
  import type { ImportPreview } from "../../ipc/assets";
  import { changeCount, shown } from "./import";

  let { preview }: { preview: ImportPreview } = $props();
</script>

<p class="tf-note">
  <b>{preview.name}</b>
  {#if changeCount(preview) === 0}
    — everything in this file is already in the tree, and importing it again would change nothing.
  {/if}
</p>

<section class="tf-im-grp">
  <h3>New — {preview.new.length}</h3>
  {#if preview.new.length === 0}
    <p class="tf-dim tf-note">Nothing in this file is new.</p>
  {:else}
    <ul>
      {#each preview.new as entry (entry.id)}
        <li><span>{entry.name}</span> <span class="tf-dim">{entry.type_label ?? "Route"}</span> <span class="tf-mono tf-dim tf-im-id">{entry.id}</span></li>
      {/each}
    </ul>
  {/if}
</section>

<section class="tf-im-grp">
  <h3>Would change — {preview.changes.length}</h3>
  {#if preview.changes.length === 0}
    <p class="tf-dim tf-note">Nothing already in the tree would change.</p>
  {:else}
    <ul>
      {#each preview.changes as change (change.id)}
        <li class="tf-im-chg">
          <span>{change.name}</span>
          <ul>
            {#each change.properties as property (property.key)}
              <li>
                <span class="tf-dim tf-im-k">{property.label}</span>
                {#if property.plan === "set"}
                  <span class="tf-dim">{shown(property.from)}</span> <span class="tf-dim">→</span> <span>{shown(property.to)}</span>
                {:else}
                  <span>{shown(property.from)}</span> <span class="tf-dim">stays — you edited it here</span>
                  <s class="tf-dim">{shown(property.to)}</s>
                {/if}
              </li>
            {/each}
            {#each change.monitors as name (name)}
              <li><span class="tf-dim tf-im-k">monitors</span> <span>+ {name}</span></li>
            {/each}
          </ul>
        </li>
      {/each}
    </ul>
  {/if}
</section>

{#if preview.monitor_links.length > 0}
  <section class="tf-im-grp">
    <h3>Monitors to attach — {preview.monitor_links.length}</h3>
    <ul>
      {#each preview.monitor_links as link (link.asset_id + link.monitor_id)}
        <li><span>{link.asset_name}</span> <span class="tf-dim">monitored by</span> <span>{link.monitor_name}</span></li>
      {/each}
    </ul>
  </section>
{/if}

{#if preview.unresolved.length > 0}
  <section class="tf-im-grp">
    <h3>Named but not in Kuma — {preview.unresolved.length}</h3>
    <ul>
      {#each preview.unresolved as waiting (waiting.asset_id + waiting.monitor_name)}
        <li><span>{waiting.asset_name}</span> <span class="tf-dim">{waiting.monitor_name}</span></li>
      {/each}
    </ul>
  </section>
{/if}

<section class="tf-im-grp">
  <h3>Already in the tree — {preview.known.length}</h3>
</section>

<style>
  .tf-im-grp h3 {
    margin: 0 0 4px;
    font: 500 var(--t-2xs) var(--mono);
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--ink-2);
  }
  .tf-im-grp ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 3px;
    font-size: var(--t-sm);
  }
  .tf-im-grp li {
    display: flex;
    gap: 8px;
    align-items: baseline;
  }
  .tf-im-grp .tf-im-chg {
    display: grid;
    gap: 2px;
  }
  .tf-im-chg ul {
    padding-left: 12px;
    font-size: var(--t-xs);
  }
  .tf-im-k {
    min-width: 10ch;
  }
  .tf-im-id {
    margin-left: auto;
  }
</style>
