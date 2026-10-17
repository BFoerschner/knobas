<!--
  The *Asset tree* in the new shell (#871): today's Miller columns
  (`assets/AssetsView.svelte`), unchanged, at `#/assets`, `#/asset/<id>` and
  `#/route/<id>`. Dependencies replaced *Assets* in the top bar (D13); the tree
  stays reachable from its *Asset tree* button (R-11.24, story 152).

  The view navigates through the rooms router's two members it reads, `route`
  and `go`. This page hands it the address the shell already parsed, read the
  rooms way, and lets every other member lead back to Dependencies. The tree
  keeps its own look until a ticket ports it.
-->
<script lang="ts">
  import AssetsView from "../../assets/AssetsView.svelte";
  import { parseHash, type Router } from "../../shell/router.svelte";

  let { head, rest }: { head: string; rest: string[] } = $props();

  const address = $derived(`#/${[head, ...rest].map(encodeURIComponent).join("/")}`);

  const router: Router = {
    get route() {
      return parseHash(address);
    },
    go: (hash) => {
      location.hash = hash;
    },
    replace: () => {
      location.replace("#/deps");
    },
    canGoBack: false,
    canGoForward: false,
    goBack: () => history.back(),
    goForward: () => history.forward(),
    start: () => () => {},
  };
</script>

<div class="tf-asset-tree">
  <AssetsView {router} />
</div>
