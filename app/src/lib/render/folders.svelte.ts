/**
 * The app's copy of the folder renderers (ADR-0044): read the first time a page
 * needs it, replaced by what the backend stored after every save in Settings ›
 * Renderers. A saved list like the pipelines' (`pipelines.svelte.ts`).
 */
import { folderRenderers as readFolderRenderers, setFolderRenderers, type FolderRenderer, type FolderRendererDraft } from "../ipc/app";
import { createSaved, type SavedList, type SavedPort } from "./pipelines.svelte";

export type FolderRenderersPort = SavedPort<FolderRenderer, FolderRendererDraft>;

export const ipcFolderRenderers: FolderRenderersPort = { read: () => readFolderRenderers(), save: (list) => setFolderRenderers(list) };

export interface FolderRenderers extends SavedList<FolderRenderer, FolderRendererDraft> {
  /** Reads the list once, the first time it is asked; later asks share that read. */
  ensure(): Promise<void>;
}

export function createFolderRenderers(port: FolderRenderersPort = ipcFolderRenderers): FolderRenderers {
  const saved = createSaved(port);
  let reading: Promise<void> | null = null;
  return {
    get list() {
      return saved.list;
    },
    load: () => saved.load(),
    hold: (list) => saved.hold(list),
    save: (list) => saved.save(list),
    reset() {
      reading = null;
      saved.reset();
    },
    ensure() {
      reading ??= saved.load().catch((cause: unknown) => {
        reading = null;
        throw cause;
      });
      return reading;
    },
  };
}

/** The app's one copy. */
export const folderRenderers = createFolderRenderers();
