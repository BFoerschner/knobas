<!--
  The *Asset tree* in the new shell (#871): today's Miller columns
  (`assets/AssetsView.svelte`), unchanged, at `#/assets`, `#/asset/<id>` and
  `#/route/<id>`. Dependencies replaced *Assets* in the top bar (D13); the tree
  stays reachable from its *Asset tree* button (R-11.24, story 152).

  The view takes the two things it reads off the shell's navigation, `route`
  and `go`. This page hands it the address the shell already parsed, with the
  shared `parseHash`, and `go` writes `location.hash` as the rest of the shell
  does. The tree keeps its own look until a ticket ports it.
-->
<script lang="ts">
  import AssetsView from "../../assets/AssetsView.svelte";
  import type { AssetsNav } from "../../assets/tree";
  import { parseHash } from "../../shell/router";

  let { head, rest }: { head: string; rest: string[] } = $props();

  const address = $derived(`#/${[head, ...rest].map(encodeURIComponent).join("/")}`);

  const nav: AssetsNav = {
    get route() {
      return parseHash(address);
    },
    go: (hash) => {
      location.hash = hash;
    },
  };
</script>

<div class="tf-asset-tree">
  <AssetsView {nav} />
</div>
