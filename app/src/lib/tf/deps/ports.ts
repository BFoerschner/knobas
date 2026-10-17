/**
 * What Dependencies' dialogs and panels call (#873, #875): the commands, as
 * ports a test can stand in for.
 */
import type {
  applyEstateImport,
  assetTree,
  previewEstateImport,
  produceEstateFile,
  AssetDetail,
  AssetRow,
  AssetType,
  DepLayer,
  DepView,
  DepViews,
  DepViewSettings,
  RouteDetail,
  RouteEdit,
  RouteRow,
  Visibility,
} from "../../ipc/assets";
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

/** *Add ▾*'s writes and the route dialog's (#876). `createLink` is {@link SavePorts}'. */
export interface AddPorts extends SavePorts {
  assetTypes: () => Promise<AssetType[]>;
  createAsset: (typeId: string, name: string, parentId?: string | null) => Promise<AssetRow>;
  getRoute: (routeId: string) => Promise<RouteDetail>;
  createRoute: (assetId: string, name: string, url: string, targetId?: string | null, visibility?: Visibility) => Promise<RouteRow>;
  editRoute: (routeId: string, edits: RouteEdit[]) => Promise<RouteRow>;
  deleteRoute: (routeId: string) => Promise<void>;
}

/** *Import…*'s commands (#1066), the rooms UI `ImportDialog`'s. */
export interface ImportPorts {
  previewEstateImport: typeof previewEstateImport;
  applyEstateImport: typeof applyEstateImport;
  produceEstateFile: typeof produceEstateFile;
  /** One level of the estate per read: the *land under* picker's walk. */
  assetTree: typeof assetTree;
}

export interface EditorPorts extends SavePorts {
  getAsset: (assetId: string) => Promise<AssetDetail>;
  assetTypes: () => Promise<AssetType[]>;
  deleteAsset: (assetId: string) => Promise<void>;
}
