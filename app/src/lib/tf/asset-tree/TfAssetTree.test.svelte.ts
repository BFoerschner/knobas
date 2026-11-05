/**
 * The page's navigation (#1667): what it hands the Assets view is the address
 * the shell parsed, and `go` moves `location.hash`. The view itself is stubbed;
 * its own tests run over `test-nav`, so this is the one place the page's `go`
 * is witnessed.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, expect, test, vi } from "vitest";

vi.mock("../../assets/AssetsView.svelte", async () => await import("./nav-stub.svelte"));

const { default: TfAssetTree } = await import("./TfAssetTree.svelte");
const { seen } = await import("./nav-stub.svelte");

let mounted: ReturnType<typeof mount> | null = null;
afterEach(() => {
  if (mounted) unmount(mounted);
  mounted = null;
  seen.nav = null;
  location.hash = "";
});

test("the bare tree draws no breadcrumb of its own name; an asset's names the tree and the asset", () => {
  mounted = mount(TfAssetTree, { target: document.body, props: { head: "assets", rest: [] } });
  flushSync();
  expect(document.querySelector(".tf-crumbs")).toBeNull();
  void unmount(mounted);
  mounted = mount(TfAssetTree, { target: document.body, props: { head: "asset", rest: ["gitea:acme/web"], back: { name: "Asset tree" } } });
  flushSync();
  const crumbs = document.querySelector(".tf-crumbs")!;
  expect(crumbs.querySelector("a.tf-crumb")?.getAttribute("href")).toBe("#/assets");
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
