import { describe, expect, it, vi } from "vitest";
import type { Dependencies } from "../../ipc/assets";
import { assetPaths, placesOutside } from "./estate-paths";

const item = (id: string, kind: string) => ({
  id,
  kind,
  name: id.replace("asset:", ""),
  health: null,
  state: null,
  monitors: 0,
  environment: null,
  owner: null,
});

// A site holding a host, the host holding a VM, and a second host beside it.
const read: Dependencies = {
  layers: [],
  kinds: [],
  items: [item("asset:site", "site"), item("asset:hv-1", "hypervisor"), item("asset:vm-1", "vm"), item("asset:hv-2", "hypervisor")],
  relations: [
    { from: "asset:hv-1", to: "asset:site", relation: "inside" },
    { from: "asset:vm-1", to: "asset:hv-1", relation: "inside" },
    { from: "asset:hv-2", to: "asset:site", relation: "inside" },
  ],
};

describe("placesOutside", () => {
  it("offers every place but the asset and what it holds", () => {
    const paths = assetPaths(read);
    expect(placesOutside(paths, "asset:hv-1").map((place) => place.path)).toEqual(["site", "site › hv-2"]);
  });

  // #2200's class: the editor walked the asset's subtree once per place, so an
  // engine holding 5,000 containers took a second to open its editor.
  it("walks what the asset holds once, not once a place", () => {
    const paths = assetPaths(read);
    const under = vi.fn(paths.under);
    placesOutside({ places: paths.places, under }, "asset:site");
    expect(under).toHaveBeenCalledTimes(1);
  });
});
