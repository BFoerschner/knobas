/**
 * What Dependencies' dialogs and panels call (#873, #875): the commands, as
 * ports a test can stand in for.
 */
import type { AssetDetail, AssetType, DepLayer, DepView, DepViews, DepViewSettings } from "../../ipc/assets";
import type { SavePorts } from "./asset-edit";

export interface ViewPorts {
  saveDepView: (viewId: string | null, name: string, settings: DepViewSettings) => Promise<DepView>;
  deleteDepView: (viewId: string) => Promise<DepViews>;
  setDepViewDefaults: (fromTicket: string, fromTop: string) => Promise<DepViews>;
}

export interface LayerPorts {
  createDepLayer: (name: string) => Promise<DepLayer[]>;
  renameDepLayer: (layerId: string, name: string) => Promise<DepLayer[]>;
  deleteDepLayer: (layerId: string) => Promise<DepLayer[]>;
  orderDepLayers: (layerIds: string[]) => Promise<DepLayer[]>;
  placeDepKind: (kind: string, layerId: string | null) => Promise<DepLayer[]>;
}

export interface EditorPorts extends SavePorts {
  getAsset: (assetId: string) => Promise<AssetDetail>;
  assetTypes: () => Promise<AssetType[]>;
  deleteAsset: (assetId: string) => Promise<void>;
}
