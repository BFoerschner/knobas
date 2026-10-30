/**
 * The app's copy of the default renderer per file pattern (#2263): read from
 * the backend once the database answers (`TfShell`), replaced by what the
 * backend stored after every save, from the file viewer's switcher and from
 * Settings › Renderers alike. What it means is `defaults.ts`.
 */
import { rendererDefaults as readDefaults, setRendererDefaults, type RendererDefault } from "../ipc/app";

export interface DefaultsPort {
  read(): Promise<RendererDefault[]>;
  save(list: RendererDefault[]): Promise<RendererDefault[]>;
}

export const ipcDefaults: DefaultsPort = { read: () => readDefaults(), save: (list) => setRendererDefaults(list) };

export interface RendererDefaults {
  readonly list: readonly RendererDefault[];
  /** Reads the stored list; a failed read leaves the one held. */
  load(): Promise<void>;
  /** Holds `list` as read elsewhere (the shell's read once the database answers). */
  hold(list: RendererDefault[]): void;
  /** Stores `list` whole and holds what the backend answers; a refusal rejects and changes nothing. */
  save(list: RendererDefault[]): Promise<void>;
  reset(): void;
}

export function createRendererDefaults(port: DefaultsPort = ipcDefaults): RendererDefaults {
  let list = $state<RendererDefault[]>([]);
  return {
    get list() {
      return list;
    },
    async load() {
      list = await port.read();
    },
    hold(next) {
      list = next;
    },
    async save(next) {
      list = await port.save(next);
    },
    reset() {
      list = [];
    },
  };
}

/** The app's one copy. */
export const rendererDefaults = createRendererDefaults();
