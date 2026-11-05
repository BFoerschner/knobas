<!--
  The *Asset tree* in the new shell (#871): today's Miller columns
  (`assets/AssetsView.svelte`), unchanged, at `#/assets`, `#/asset/<id>` and
  `#/route/<id>`. It has no top-bar entry (D13): a row, an alert or a search
  opens it at an asset, and that page's breadcrumb leads to `#/assets`.

  The view takes the two things it reads off the shell's navigation, `route`
  and `go`. This page hands it the address the shell already parsed, with the
  shared `parseHash`, and `go` writes `location.hash` as the rest of the shell
  does. The page draws its own breadcrumb, which names the asset it is on.
-->
<script lang="ts">
  import AssetsView from "../../assets/AssetsView.svelte";
  import type { AssetsNav } from "../../assets/tree";
  import { parseHash } from "../../shell/router";
  import type { EscTarget } from "../escape";
  import { HEAD_NAME } from "../route";
  import TfCrumbs from "../TfCrumbs.svelte";

  let {
    head,
    rest,
    back = null,
  }: {
    head: string;
    rest: string[];
    /** Where `Esc` goes, which the breadcrumb's hint names; `null` where it does nothing. */
    back?: Pick<EscTarget, "name"> | null;
  } = $props();

  /** The asset the view has read, which the breadcrumb's last part names. */
  let named = $state<string | null>(null);

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

<div class="tf-full">
  {#if head === "assets"}
    <TfCrumbs parts={[{ name: "Asset tree" }]} {back} sectionTop />
  {:else}
    <TfCrumbs parts={[{ name: "Asset tree", href: "#/assets" }, { name: named ?? HEAD_NAME[head] ?? "Asset" }]} {back} />
  {/if}
  <div class="tf-asset-tree">
    <AssetsView {nav} onnamed={(name) => (named = name)} />
  </div>
</div>
