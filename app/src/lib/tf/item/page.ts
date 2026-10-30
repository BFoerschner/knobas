/**
 * An item's page (#1201): `#/item/<entity id>`, the block view of an item whose
 * kind no surface draws, such as a custom role's (ADR-0029). It lights no nav
 * entry and draws its own header, under the shell's breadcrumb named by the
 * item's kind (#1408), which only the page's own read knows: so the page
 * draws `TfCrumbs` itself and does not set `crumbs`.
 */
import { definePage } from "../page-entry";
import type { TfRoute } from "../route";
import TfItem from "./TfItem.svelte";

type ItemRoute = Extract<TfRoute, { view: "item" }>;

export const page = definePage({
  matches: (route): route is ItemRoute => route.view === "item",
  component: TfItem,
  address: (route) => route.id,
  props: ({ route, ready, job, jobAt }) => ({ id: route.id, ready, job, jobAt }),
});
