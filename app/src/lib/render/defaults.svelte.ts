/**
 * The app's copy of the default renderer per file pattern (#2263): read from
 * the backend once the database answers (`TfShell`), replaced by what the
 * backend stored after every save, from the file viewer's switcher and from
 * Settings › Renderers alike. What it means is `defaults.ts`. It is a saved
 * list like the pipelines' (`pipelines.svelte.ts`).
 */
import { rendererDefaults as readDefaults, setRendererDefaults, type RendererDefault } from "../ipc/app";
import { createSaved, type SavedList, type SavedPort } from "./pipelines.svelte";

export type DefaultsPort = SavedPort<RendererDefault, RendererDefault>;

export const ipcDefaults: DefaultsPort = { read: () => readDefaults(), save: (list) => setRendererDefaults(list) };

export type RendererDefaults = SavedList<RendererDefault, RendererDefault>;

export function createRendererDefaults(port: DefaultsPort = ipcDefaults): RendererDefaults {
  return createSaved(port);
}

/** The app's one copy. */
export const rendererDefaults = createRendererDefaults();
