/**
 * What the asset page calls: the commands, as ports a test can stand in for
 * (the shape the tree's `AssetPorts` has).
 */
import {
  assetTree,
  assetTypes,
  createAsset,
  createRoute,
  deleteAsset,
  deleteRoute,
  dependsOnThis,
  editAsset,
  editRoute,
  getAsset,
  getRoute,
  moveAsset,
} from "../../ipc/assets";
import { submitWrite, unlink } from "../../ipc/entity";
import { openExternal } from "../../shell/open-external";

export interface AssetPagePorts {
  getAsset: typeof getAsset;
  assetTypes: typeof assetTypes;
  editAsset: typeof editAsset;
  moveAsset: typeof moveAsset;
  deleteAsset: typeof deleteAsset;
  createAsset: typeof createAsset;
  /** One level of the estate per read: the move and route dialogs' pickers walk it. */
  assetTree: typeof assetTree;
  getRoute: typeof getRoute;
  createRoute: typeof createRoute;
  editRoute: typeof editRoute;
  deleteRoute: typeof deleteRoute;
  /** The OS browser, for a route's URL and a monitor's page in Kuma. */
  openExternal: typeof openExternal;
  /** Withdrawing a link from *Linked items*. Drawing one is `LinkDialog`'s own write. */
  unlink: typeof unlink;
  /** What breaks if the asset goes down (#505). */
  dependsOnThis: typeof dependsOnThis;
  /** Queueing *Create monitor* (#453). */
  submitWrite: typeof submitWrite;
}

export const REAL_PORTS: AssetPagePorts = {
  getAsset,
  assetTypes,
  editAsset,
  moveAsset,
  deleteAsset,
  createAsset,
  assetTree,
  getRoute,
  createRoute,
  editRoute,
  deleteRoute,
  openExternal,
  unlink,
  dependsOnThis,
  submitWrite,
};
