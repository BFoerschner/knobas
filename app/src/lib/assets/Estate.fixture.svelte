<!--
  The estate's two views behind one test navigation, as the shell draws them:
  the asset tree on `#/assets…`, an asset's page on `#/asset/…` and
  `#/route/…`. A test that walks from one to the other (a preview's *Open
  asset*, a delete that lands back in the tree) renders this once and follows
  the address.
-->
<script lang="ts">
  import TfAsset from "../tf/asset/TfAsset.svelte";
  import { alerts as sharedAlerts, type Alerts } from "./alerts.svelte";
  import AssetsView from "./AssetsView.svelte";
  import type { AssetsNav } from "./tree";

  let {
    nav,
    ports = {},
    now = () => new Date(),
    alerts = sharedAlerts,
    stripWidth,
  }: {
    nav: AssetsNav;
    /** Both views' ports: a test's fake estate answers every command either asks. */
    ports?: object;
    now?: () => Date;
    alerts?: Alerts;
    stripWidth?: () => number;
  } = $props();

  const route = $derived(nav.route);
</script>

{#if route.view === "assets" && route.tab === "asset"}
  <TfAsset
    assetId={route.assetId}
    routeId={route.routeId ?? null}
    ports={ports as never}
    {now}
    go={(hash) => nav.go(hash)}
  />
{:else}
  <AssetsView {nav} ports={ports as never} {now} {alerts} {...stripWidth === undefined ? {} : { stripWidth }} />
{/if}
