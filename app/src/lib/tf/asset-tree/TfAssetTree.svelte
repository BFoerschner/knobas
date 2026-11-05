<!--
  The *Asset tree* in the new shell (#871): today's Miller columns
  (`assets/AssetsView.svelte`), unchanged, at `#/assets`, `#/asset/<id>` and
  `#/route/<id>`. It has no top-bar entry (D13): a row, an alert or a search
  opens it at an asset, and that page's breadcrumb leads to `#/assets`.

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
