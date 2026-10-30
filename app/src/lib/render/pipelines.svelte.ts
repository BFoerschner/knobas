/**
 * The app's copies of what the person saves for pipelines (#2265): their named pipelines and their command steps.
 * Read from the backend once the database answers (`TfShell`), replaced by what the backend stored after every
 * save, from the file viewer (*Save as default for …*) and from Settings › Renderers alike. A default per file
 * pattern names a pipeline by id (`defaults.ts`'s `defaultPipeline`), so a viewer reading `namedPipelines.list`
 * follows every edit.
 */
import {
  commandSteps as readCommandSteps,
  namedPipelines as readNamedPipelines,
  setCommandSteps,
  setNamedPipelines,
  type CommandStep,
  type CommandStepDraft,
  type NamedPipeline,
  type NamedPipelineDraft,
} from "../ipc/app";

export interface SavedPort<T, D> {
  read(): Promise<T[]>;
  save(list: D[]): Promise<T[]>;
}

export interface SavedList<T, D> {
  readonly list: readonly T[];
  /** Reads the stored list; a failed read leaves the one held. */
  load(): Promise<void>;
  /** Holds `list` as read elsewhere. */
  hold(list: T[]): void;
  /** Stores `list` whole and holds what the backend answers; a refusal rejects and changes nothing. */
  save(list: D[]): Promise<T[]>;
  reset(): void;
}

function createSaved<T, D>(port: SavedPort<T, D>): SavedList<T, D> {
  let list = $state<T[]>([]);
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
      return list;
    },
    reset() {
      list = [];
    },
  };
}

export type NamedPipelines = SavedList<NamedPipeline, NamedPipelineDraft>;
export type CommandSteps = SavedList<CommandStep, CommandStepDraft>;

export const ipcPipelines: SavedPort<NamedPipeline, NamedPipelineDraft> = { read: () => readNamedPipelines(), save: (list) => setNamedPipelines(list) };
export const ipcCommandSteps: SavedPort<CommandStep, CommandStepDraft> = { read: () => readCommandSteps(), save: (list) => setCommandSteps(list) };

export function createNamedPipelines(port: SavedPort<NamedPipeline, NamedPipelineDraft> = ipcPipelines): NamedPipelines {
  return createSaved(port);
}

export function createCommandSteps(port: SavedPort<CommandStep, CommandStepDraft> = ipcCommandSteps): CommandSteps {
  return createSaved(port);
}

/** The app's one copy of the named pipelines. */
export const namedPipelines = createNamedPipelines();
/** The app's one copy of the command steps. */
export const commandSteps = createCommandSteps();
