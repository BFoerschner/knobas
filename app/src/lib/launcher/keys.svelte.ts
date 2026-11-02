/**
 * The keys bound (#2429), held for the window: the shell reads the saved
 * lists' chords to open the launcher on a list with it closed, and the
 * launcher reads both tables to run a chord, draw it on its row and bind a
 * new one. One store, so a chord bound in the launcher's panel answers in the
 * shell the moment the write lands.
 *
 * Kept in `knobas.list_key` and `knobas.action_key` (migration 0070), read
 * through `keys` and written through `bind_list_key` and `bind_action_key`;
 * each write reads the tables back, so what is held is what the backend has.
 */
import { type ActionKey, type Keys, type ListKey, bindActionKey, bindListKey, keys } from "../ipc/search";
import type { Chord } from "./keys";

export interface KeyPorts {
  keys: () => Promise<Keys>;
  bindListKey: (listId: string, chord: Chord | null) => Promise<void>;
  bindActionKey: (kind: string, actionId: string, chord: Chord | null) => Promise<void>;
}

export interface KeyStore {
  readonly lists: readonly ListKey[];
  readonly actions: readonly ActionKey[];
  /** Read both tables; a refused read keeps what was held. */
  load(): Promise<void>;
  /** Give the list `chord`, or none; rejects with the backend's refusal. */
  bindList(listId: string, chord: Chord | null): Promise<void>;
  /** Give the action `chord` on `kind`, or none; rejects with the backend's refusal. */
  bindAction(kind: string, actionId: string, chord: Chord | null): Promise<void>;
  /** The list a chord opens. */
  listOf(chord: Chord): ListKey | undefined;
  /** A list's chord, or `null`. */
  chordOfList(listId: string): Chord | null;
  /** The action id a chord runs on `kind`, or `null`. */
  actionOf(kind: string, chord: Chord): string | null;
  /** An action's chord on `kind`, or `null`. */
  chordOfAction(kind: string, actionId: string): Chord | null;
  /** Tests only: nothing held. */
  reset(): void;
}

export function createKeys(ports: KeyPorts): KeyStore {
  let lists = $state<ListKey[]>([]);
  let actions = $state<ActionKey[]>([]);
  async function load() {
    try {
      const read = await ports.keys();
      lists = read.lists;
      actions = read.actions;
    } catch {
      // What was held stands: a key that worked a moment ago still works.
    }
  }
  return {
    get lists() {
      return lists;
    },
    get actions() {
      return actions;
    },
    load,
    async bindList(listId, chord) {
      await ports.bindListKey(listId, chord);
      await load();
    },
    async bindAction(kind, actionId, chord) {
      await ports.bindActionKey(kind, actionId, chord);
      await load();
    },
    listOf: (chord) => lists.find((key) => key.chord === chord),
    chordOfList: (listId) => lists.find((key) => key.list_id === listId)?.chord ?? null,
    actionOf: (kind, chord) => actions.find((key) => key.kind === kind && key.chord === chord)?.action_id ?? null,
    chordOfAction: (kind, actionId) => actions.find((key) => key.kind === kind && key.action_id === actionId)?.chord ?? null,
    reset() {
      lists = [];
      actions = [];
    },
  };
}

/**
 * The window's keys, over the real bridge. Each port reaches the barrel when
 * it is called, not when this module loads, so a suite that mocks `ipc/search`
 * without the keys still imports the shell.
 */
export const launcherKeys: KeyStore = createKeys({
  keys: () => keys(),
  bindListKey: (listId, chord) => bindListKey(listId, chord),
  bindActionKey: (kind, actionId, chord) => bindActionKey(kind, actionId, chord),
});
