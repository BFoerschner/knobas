/**
 * A typed value in Settings with no *Save* (F-641): it saves on `Enter` or on leaving, says *Saved* beside itself for
 * about two seconds, and `Esc` puts the saved value back and keeps focus. Without this `Esc` blurred the field, the blur
 * fired `change`, and the typo was stored.
 *
 * A pick (a select, a switch) saves at once and needs none of this. An untouched field's `Esc` is not this field's: it
 * goes on to the shell's ladder like any other.
 */
import type { Attachment } from "svelte/attachments";

const SAID_MS = 2000;

/** The word beside a field: drawn by the section, said by {@link savedField}. */
export class SavedWord {
  shown = $state(false);
  #timer: ReturnType<typeof setTimeout> | null = null;

  say(): void {
    this.shown = true;
    if (this.#timer !== null) clearTimeout(this.#timer);
    this.#timer = setTimeout(() => {
      this.shown = false;
      this.#timer = null;
    }, SAID_MS);
  }

  clear(): void {
    if (this.#timer !== null) clearTimeout(this.#timer);
    this.#timer = null;
    this.shown = false;
  }
}

export interface SavedFieldOptions {
  /** What the field holds once nothing is typed: the stored value, as the field draws it. */
  saved: () => string;
  /** Store what the field holds. Answers whether a new value was stored, which is what *Saved* is said for. */
  save: (input: HTMLInputElement) => Promise<boolean> | boolean;
  word: SavedWord;
  /** `Esc` put the saved value back: a note about the last value typed is out of date. */
  restored?: () => void;
}

/** The attachment for one field. The section's `save` keeps its own bounds and its own words for a refusal. */
export function savedField(options: SavedFieldOptions): Attachment<HTMLInputElement> {
  return (input) => {
    /** One write at a time: `Enter` and then a quick `Tab` must not send the same value twice. */
    let saving = false;
    const dirty = () => input.value !== options.saved();

    async function commit() {
      if (saving) return;
      saving = true;
      try {
        if (await options.save(input)) options.word.say();
      } finally {
        saving = false;
      }
    }

    function onkeydown(event: KeyboardEvent) {
      if (event.key === "Enter") {
        // Stops the webview's own `change` on `Enter`, so one press is one write.
        event.preventDefault();
        void commit();
      } else if (event.key === "Escape" && dirty()) {
        // Answered here, so the shell's ladder sees it handled and focus stays.
        event.preventDefault();
        input.value = options.saved();
        options.restored?.();
      }
    }

    function onchange() {
      void commit();
    }

    input.addEventListener("keydown", onkeydown);
    input.addEventListener("change", onchange);
    return () => {
      input.removeEventListener("keydown", onkeydown);
      input.removeEventListener("change", onchange);
      // A key that leaves the page while the field holds focus (⌘[, a chord) fires no `change`, and leaving saves.
      if (dirty()) void commit();
      options.word.clear();
    };
  };
}
