/**
 * One opening of the key panel (#2429): what the keys will bind, the chord
 * pressed so far (`null` once cleared), the modifiers held now, and why the
 * last press was refused. ⌘B on a selected action, or *Assign key…* on a saved
 * list, makes one; `SheetMode` holds it on the open sheet, so it ends with the
 * sheet and nothing has to remember to reset it.
 *
 * The caller hands every keydown to {@link KeyPanelMode.press} and every keyup
 * to {@link KeyPanelMode.release}. `host.leave` is how the panel gives way; the
 * write lands after it has, and a refusal is said in a toast.
 */
import { ipcErrorMessage } from "../ipc";
import { push } from "../shell/toasts.svelte";
import { type Chord, chordOf, heldOf, kindInSentence, refusalOf } from "./keys";
import type { KeyStore } from "./keys.svelte";

export type KeyTarget = { kind: "list"; id: string; label: string } | { kind: "action"; on: string; actionId: string; label: string };

export interface KeyPanelHost {
  keys: KeyStore;
  /** Give the box its focus. */
  focus: () => void;
  /** Close this panel: back to the actions. */
  leave: () => void;
}

export class KeyPanelMode {
  readonly target: KeyTarget;
  chord = $state<Chord | null>(null);
  held = $state<string[]>([]);
  refused = $state<string | null>(null);

  readonly #host: KeyPanelHost;
  readonly #had: Chord | null;

  constructor(target: KeyTarget, host: KeyPanelHost) {
    this.target = target;
    this.#host = host;
    this.#had = target.kind === "list" ? host.keys.chordOfList(target.id) : host.keys.chordOfAction(target.on, target.actionId);
    this.chord = this.#had;
  }

  /**
   * A keydown. Every press is drawn: the modifiers while held, a full chord
   * once pressed. Nothing reaches the box, the dialog or the shell: a refused
   * ⌘D does not switch the scope, a refused ⌘T starts no timer.
   */
  press(event: KeyboardEvent): void {
    event.stopPropagation();
    this.held = heldOf(event);
    const plain = !event.metaKey && !event.ctrlKey && !event.altKey;
    if (event.key === "Escape") {
      event.preventDefault();
      return this.cancel();
    }
    if (event.key === "Enter" && plain) {
      event.preventDefault();
      return void this.keep();
    }
    event.preventDefault();
    if (event.key === "Backspace" && plain) {
      this.chord = null;
      this.refused = null;
      return;
    }
    const refusal = refusalOf(event);
    const chord = chordOf(event);
    const refused = refusal ?? (chord === null ? null : this.#clashOf(chord));
    if (refused !== null) {
      this.refused = refused;
      this.held = [];
      return;
    }
    if (chord === null) return;
    this.chord = chord;
    this.refused = null;
  }

  /** A keyup: the panel draws the modifiers as they are let go, too. */
  release(event: KeyboardEvent): void {
    this.held = heldOf(event);
  }

  /** The Clear button: no chord, and the box its focus back from the click. */
  clear(): void {
    this.chord = null;
    this.refused = null;
    this.#host.focus();
  }

  /** Esc or Cancel: back to the actions, nothing written. */
  cancel(): void {
    this.#host.leave();
    this.#host.focus();
  }

  /** Enter or OK: the chord in the panel, kept; no chord takes the key away. Back to the actions either way. */
  async keep(): Promise<void> {
    this.#host.leave();
    this.#host.focus();
    const { target, chord } = this;
    if (chord === this.#had) return;
    const { keys } = this.#host;
    try {
      if (target.kind === "list") {
        await keys.bindList(target.id, chord);
        push({ text: chord ? `${chord} opens ${target.label}` : `Key removed from ${target.label}` });
      } else {
        await keys.bindAction(target.on, target.actionId, chord);
        push({ text: chord ? `${chord} runs ${target.label} on any ${kindInSentence(target.on)}` : `Key removed from ${target.label}` });
      }
    } catch (cause) {
      push({ text: ipcErrorMessage(cause), tone: "err" });
    }
  }

  /** A chord the other table holds, said as the panel says it: a list's chord is no action's, on any kind. */
  #clashOf(chord: Chord): string | null {
    const { keys } = this.#host;
    if (this.target.kind === "list") {
      const action = keys.actions.find((key) => key.chord === chord);
      return action ? `${chord} runs an action on a ${kindInSentence(action.kind)}` : null;
    }
    const list = keys.listOf(chord);
    return list ? `${chord} opens ${list.label}` : null;
  }
}
