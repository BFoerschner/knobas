/**
 * The app's copy of the folder renderer picked per folder (ux26, `render.folder_picks`,
 * `docs/decisions/2026-11-20-a-comparison-is-drawn-rendered-and-may-end-at-the-working-tree.md` ruling 10): read once,
 * replaced by what the backend stored after each pick. A pick is made by choosing a folder's renderer anywhere (*Render
 * with* in a drawing's header or the made bar, the folder's right-click menu, the glyph's menu); there is no second
 * *make default* step. `render/folder.ts`'s `rendererFor` says which renderer a folder gets from it.
 */
import { folderRenderPicks, setFolderRenderPick, type FolderPick } from "../ipc/app";

export interface FolderPicksPort {
  read(): Promise<FolderPick[]>;
  save(entityId: string, folder: string, renderer: string | null): Promise<FolderPick[]>;
}

export const ipcFolderPicks: FolderPicksPort = { read: () => folderRenderPicks(), save: (id, folder, renderer) => setFolderRenderPick(id, folder, renderer) };

export interface FolderPicks {
  readonly list: readonly FolderPick[];
  /** Reads the picks once, the first time it is asked; later asks share that read. */
  ensure(): Promise<void>;
  /** The renderer picked for `folder` of the repository `repoId`, or `null`. */
  of(repoId: string, folder: string): string | null;
  /** Picks `renderer` for `folder` of the repository `entityId` names; held at once, then as the backend stored it. */
  set(entityId: string, repoId: string, folder: string, renderer: string | null): Promise<void>;
  /** Tests only: nothing read. */
  reset(): void;
}

const trimmed = (folder: string) => folder.replace(/^\/+|\/+$/g, "");

export function createFolderPicks(port: FolderPicksPort = ipcFolderPicks): FolderPicks {
  let list = $state.raw<FolderPick[]>([]);
  let reading: Promise<void> | null = null;
  return {
    get list() {
      return list;
    },
    ensure() {
      reading ??= port.read().then(
        (read) => void (list = read),
        (cause: unknown) => {
          reading = null;
          throw cause;
        },
      );
      return reading;
    },
    of(repoId, folder) {
      const at = trimmed(folder);
      // One pick a folder: `set` keeps it so.
      return list.find((pick) => pick.repo === repoId && pick.folder === at)?.renderer ?? null;
    },
    async set(entityId, repoId, folder, renderer) {
      const at = trimmed(folder);
      // Held at once: the glyph and the menus follow the choice in the press's frame.
      const rest = list.filter((pick) => !(pick.repo === repoId && pick.folder === at));
      list = renderer === null ? rest : [...rest, { repo: repoId, folder: at, renderer }];
      list = await port.save(entityId, at, renderer);
    },
    reset() {
      list = [];
      reading = null;
    },
  };
}

/** The app's one copy. */
export const folderPicks = createFolderPicks();
