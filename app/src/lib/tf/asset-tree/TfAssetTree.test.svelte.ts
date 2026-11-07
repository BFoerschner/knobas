/**
 * The page's navigation (#1667): what it hands the Assets view is the address
 * the shell parsed, and `go` moves `location.hash`. The view itself is stubbed;
 * its own tests run over `test-nav`, so this is the one place the page's `go`
 * is witnessed.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, expect, test, vi } from "vitest";

vi.mock("../../assets/AssetsView.svelte", async () => await import("./nav-stub.svelte"));

import type { AssetRow } from "../../ipc/assets";
import { pageContext } from "../kept-pages.svelte";

const { default: TfAssetTree } = await import("./TfAssetTree.svelte");
const { seen } = await import("./nav-stub.svelte");

let mounted: ReturnType<typeof mount> | null = null;
afterEach(() => {
  if (mounted) unmount(mounted);
  mounted = null;
  seen.nav = null;
  seen.onroots = null;
  location.hash = "";
});

test("the tree draws its crumb bar at both addresses; an asset's names the tree and the asset", () => {
  mounted = mount(TfAssetTree, { target: document.body, props: { head: "assets", rest: [] } });
  flushSync();
  expect(document.querySelector(".tf-crumbs")?.textContent).toContain("Asset tree");
  void unmount(mounted);
  mounted = mount(TfAssetTree, { target: document.body, props: { head: "asset", rest: ["gitea:acme/web"], back: { name: "Asset tree" } } });
  flushSync();
  const crumbs = document.querySelector(".tf-crumbs")!;
  expect([...crumbs.querySelectorAll("a.tf-crumb")].map((a) => a.getAttribute("href"))).toEqual(["#/assets"]);
  expect(crumbs.querySelector("strong")?.textContent).toBe("Asset");
  expect(crumbs.textContent).toContain("Back to Asset tree");
});

test("the page reads its address with parseHash and go sets location.hash", () => {
  mounted = mount(TfAssetTree, {
    target: document.body,
    props: { head: "asset", rest: ["gitea:acme/web"] },
  });
  flushSync();
  expect(seen.nav?.route).toEqual({ view: "assets", tab: "tree", assetId: "gitea:acme/web" });

  seen.nav?.go("#/asset/other");
  expect(location.hash).toBe("#/asset/other");
});

const ROOT = { id: "asset:estate", name: "estate" } as AssetRow;
const OTHER = { id: "asset:other", name: "other" } as AssetRow;

test("a single root opens on arrival, in place of the address, and only on arrival (F-054)", () => {
  location.hash = "#/assets";
  const entries = history.length;
  const props = $state({ head: "assets", rest: [] as string[] });
  mounted = mount(TfAssetTree, { target: document.body, props });
  flushSync();
  expect(location.hash, "nothing read yet").toBe("#/assets");

  seen.onroots?.([ROOT]);
  flushSync();
  expect(location.hash).toBe("#/asset/asset:estate");
  expect(history.length, "no history entry").toBe(entries);

  // `Esc` and the crumb come up from that root to the bare tree, and it stays there.
  location.hash = "#/assets";
  props.head = "assets";
  seen.onroots?.([ROOT]);
  flushSync();
  expect(location.hash).toBe("#/assets");
});

test("hidden and shown again, the page arrives again", () => {
  location.hash = "#/assets";
  let shown = $state(true);
  mounted = mount(TfAssetTree, { target: document.body, props: { head: "assets", rest: [] }, context: pageContext(() => shown) });
  flushSync();
  seen.onroots?.([ROOT]);
  flushSync();
  location.hash = "#/assets";
  flushSync();
  expect(location.hash).toBe("#/assets");

  shown = false;
  flushSync();
  shown = true;
  flushSync();
  expect(location.hash).toBe("#/asset/asset:estate");
});

test("an estate with two roots, or with none, stays on the bare tree", () => {
  location.hash = "#/assets";
  mounted = mount(TfAssetTree, { target: document.body, props: { head: "assets", rest: [] } });
  flushSync();
  seen.onroots?.([ROOT, OTHER]);
  flushSync();
  expect(location.hash).toBe("#/assets");
  void unmount(mounted);
  mounted = mount(TfAssetTree, { target: document.body, props: { head: "assets", rest: [] } });
  flushSync();
  seen.onroots?.([]);
  flushSync();
  // The first asset made here is no arrival.
  seen.onroots?.([ROOT]);
  flushSync();
  expect(location.hash).toBe("#/assets");
});
