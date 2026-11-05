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
  import type { EscTarget, EscTargetOf } from "../escape";
  import { parentPage } from "../page-name.svelte";
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
    back?: (Pick<EscTarget, "name"> & Partial<EscTargetOf>) | null;
  } = $props();

  /** The asset the view has read, which the breadcrumb's last part names. */
  let named = $state<string | null>(null);
  /** The asset holding it, where `Esc` steps up to; `null` at the estate's top, whose parent the address says. */
  let holder = $state<{ id: string; name: string } | null>(null);
  parentPage(() => (head === "asset" ? (holder === null ? { hash: "#/assets", name: "Asset tree" } : { hash: `#/asset/${encodeURIComponent(holder.id)}`, name: holder.name }) : null));

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
  <!-- Always drawn, so the first click in a column does not move the tree under the pointer. -->
  {#if head === "assets"}
    <TfCrumbs parts={[{ name: "Asset tree" }]} {back} />
  {:else}
    <TfCrumbs parts={[{ name: "Asset tree", href: "#/assets" }, { name: named ?? HEAD_NAME[head] ?? "Asset" }]} {back} />
  {/if}
  <div class="tf-asset-tree">
    <AssetsView {nav} onnamed={(name) => (named = name)} onholder={(held) => (holder = held)} />
  </div>
</div>
