<!-- Settings › Keyboard (#831, R-13a.8): prototype-6's `STGV.keyboard`, read-only. -->
<script lang="ts">
  import { SHORTCUTS } from "./sections";
</script>

<header class="tf-stg-h"><h1>Keyboard</h1><p>The shortcuts. They are fixed in this version.</p></header>
<table class="tf-tbl tf-stg-keys">
  <tbody>
    {#each SHORTCUTS as [keys, where, does] (keys + where)}
      <tr>
        <td class="tf-stg-kk">
          {#each keys.split(", ") as chord, c (c)}{#if c > 0},
            {/if}{#each chord.split(" ") as key, k (k)}{#if k > 0}{" "}{/if}<kbd>{key}</kbd>{/each}{/each}
        </td>
        <td class="tf-dim">{where}</td>
        <td>{does}</td>
      </tr>
    {/each}
  </tbody>
</table>
