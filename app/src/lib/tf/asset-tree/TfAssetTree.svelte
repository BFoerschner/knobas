<!--
  The *Asset tree* in the new shell (#871): today's Miller columns
  (`assets/AssetsView.svelte`), unchanged, at `#/assets`, `#/asset/<id>` and
  `#/route/<id>`. It has no top-bar entry (D13): a row, an alert or a search
  opens it at an asset, and that page's breadcrumb leads to `#/assets`.

  The view takes the two things it reads off the shell's navigation, `route`
  and `go`. This page hands it the address the shell already parsed, with the
  shared `parseHash`, and `go` writes `location.hash` as the rest of the shell
  does. The page draws its own breadcrumb, which names the asset it is on.

  A ticket card's *Assets* row (`job/scoped.svelte.ts`) lands here at the ticket's
  first asset: the line under the bar names all it reaches, each a way to it, and
  the columns mark them and what holds them, while the reader stays in the tree.
-->
<script lang="ts">
  import AssetsView from "../../assets/AssetsView.svelte";
  import { scopedTicket } from "../job/scoped.svelte";
  import { ticketAssets } from "./ticket-assets.svelte";
  import type { AssetsNav } from "../../assets/tree";
  import { parseHash } from "../../shell/router";
  import type { EscTarget, EscTargetOf } from "../escape";
  import { goAcross } from "../opener";
  import { parentPage } from "../page-name.svelte";
  import { HEAD_NAME } from "../route";
  import TfCrumbs from "../TfCrumbs.svelte";
  import TfKey from "../TfKey.svelte";

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

  /** What a card's *Assets* row is showing. */
  const reached = ticketAssets(() => scopedTicket.assetsOf);

  const nav: AssetsNav = {
    get route() {
      return parseHash(address);
    },
    go: (hash) => {
      location.hash = hash;
    },
    jump: goAcross,
  };
</script>

<div class="tf-full">
  <!-- Always drawn, so the first click in a column does not move the tree under the pointer. -->
  {#if head === "assets"}
    <TfCrumbs parts={[{ name: "Asset tree" }]} {back} />
  {:else}
    <TfCrumbs parts={[{ name: "Asset tree", href: "#/assets" }, { name: named ?? HEAD_NAME[head] ?? "Asset" }]} {back} />
  {/if}
  {#if reached.marks !== undefined && reached.list.length > 0}
    <p class="tf-at-reach">
      <TfKey value={reached.marks.key} /> reaches {reached.list.length} {reached.list.length === 1 ? "asset" : "assets"}:
      {#each reached.list as asset, at (asset.id)}{at > 0 ? ", " : ""}<a href="#/asset/{encodeURIComponent(asset.id)}">{asset.name}</a>{/each}
    </p>
  {/if}
  <div class="tf-asset-tree">
    <AssetsView {nav} marks={reached.marks} onnamed={(name) => (named = name)} onholder={(held) => (holder = held)} />
  </div>
</div>
