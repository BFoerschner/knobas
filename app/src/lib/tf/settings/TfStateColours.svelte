<!--
  Settings › Appearance › State colours (#2466, spec #2463): one section per
  entity kind that declares a state, each a board of six columns (the five
  tones, then *No colour*) holding one chip per state word, over
  `state_colours`, `set_tone_override` and `clear_tone_overrides`.

  A chip moves three ways (the state-colours bug, 2026-11-17, Fable's ruling):
  a press on its word opens a menu of the six columns (Enter and Space too), a
  pick lands it there; a drag by pointer (`pointer-drag.ts`, since HTML5 drag
  never drops on Windows, where Tauri holds the web view's drop target); and
  ← / → on the focused word, one column per press, as a file pipeline's step
  does (`TfFilePipeline.svelte`).

  **A move lands at once.** The board draws the stored answer with every move
  not yet answered applied (`applied`), and sends the moves one at a time; a
  newer move of a word replaces its older one still waiting (`queuedWith`), so
  ← pressed three times sends at most two writes, and only the newest target
  of a word. Each answer is the stored state after its move, the moves still
  waiting drawn over it, so a chip never steps back. A write that fails raises
  a toast and its chip goes back to where the last answer put it. There is no
  Save.

  Every surface that draws a state reads the merged map, so a move changes
  more than the colour: the line under each section says what (`drives`).

  A chip says its word as every other chip does (`stateWord`, F-634), so *Passed* and not *SUCCESS*; the source's spelling is
  in its title. Only a word you moved is marked, by its *Reset*: where the others' tones come from is in the title too.
-->
<script lang="ts" module>
  import type { SectionPorts } from "./ports";

  /** The board's read, set and clear: the settings ports' own (`ports.ts`). */
  export type StateColourPorts = Pick<SectionPorts, "stateColours" | "setToneOverride" | "clearToneOverrides">;
</script>

<script lang="ts">
  import { tick } from "svelte";

  import { ipcErrorMessage } from "../../ipc";
  import type { KindStateColours, ToneChoice } from "../../ipc/sources";
  import { kindLabel } from "../../shell/kinds";
  import { push } from "../../shell/toasts.svelte";
  import { pointerDrag } from "../pointer-drag";
  import { stateWord } from "../state-word";
  import { toneClass } from "../tone";
  import TfMenu from "../TfMenu.svelte";
  import { COLUMNS, applied, columnName, columnOf, drives, ordered, originName, queuedWith, stepped, type ColourMove } from "./state-colours";

  let { ports }: { ports: StateColourPorts } = $props();

  /** The last answer the backend gave; `null` until the read answers. */
  let stored = $state<KindStateColours[] | null>(null);
  /** The moves not answered yet, oldest first; the first is out while `sending`. */
  let waiting = $state<ColourMove[]>([]);
  let sending = false;
  /** What the board draws: the stored answer with every waiting move applied. */
  const drawn = $derived(stored === null ? null : waiting.reduce<KindStateColours[]>(applied, stored));
  let board: HTMLElement | undefined = $state();
  /** The column a dragged chip is over, for its highlight. */
  let over = $state<{ kind: string; column: ToneChoice } | null>(null);
  /** The word whose menu is open, and the button that opened it. */
  let picking = $state<{ kind: string; word: string } | null>(null);
  let menuOpen = $state(false);
  let opener = $state<HTMLElement | null>(null);

  $effect(() => {
    void ports
      .stateColours()
      .then((read) => (stored = read))
      .catch((cause) => push({ text: `Could not read the state colours: ${ipcErrorMessage(cause)}`, tone: "err" }));
  });

  /** What a failed write says. */
  function failure(move: ColourMove): string {
    if (move.word === null) return `Could not reset ${kindLabel(move.kind)}`;
    return move.to === null ? `Could not reset ${stateWord(move.word)}` : `Could not set the colour of ${stateWord(move.word)}`;
  }

  /** Draw `move` at once and send it after the moves before it. */
  function enqueue(move: ColourMove) {
    waiting = queuedWith(waiting, sending, move);
    void pump();
  }

  /** Send the waiting moves one at a time, each answer replacing the stored one as its move leaves the queue. */
  async function pump() {
    if (sending) return;
    sending = true;
    try {
      while (waiting.length > 0) {
        const move = waiting[0]!;
        try {
          const answer =
            move.to === null ? await ports.clearToneOverrides(move.kind, move.word) : await ports.setToneOverride(move.kind, move.word!, move.to);
          stored = answer;
        } catch (cause) {
          push({ text: `${failure(move)}: ${ipcErrorMessage(cause)}`, tone: "err" });
        }
        waiting = waiting.slice(1);
      }
    } finally {
      sending = false;
    }
  }

  /** Put a word in a column: what a press, a pick and a drop all do. */
  function place(kind: string, word: string, column: ToneChoice, refocus: boolean) {
    const now = drawn?.find((k) => k.kind === kind)?.words.find((w) => w.word === word);
    if (now === undefined || columnOf(now) === column) return;
    enqueue({ kind, word, to: column });
    if (refocus) void focusWord(kind, word);
  }

  /** A word's button, wherever its chip now stands, so a press by key carries on from it. */
  async function focusWord(kind: string, word: string) {
    await tick();
    board?.querySelector<HTMLElement>(`section[data-kind="${CSS.escape(kind)}"] button[data-word="${CSS.escape(word)}"]`)?.focus();
  }

  /** *Reset*: the button goes with the mark, so focus stays with the word. */
  function resetWord(kind: string, word: string) {
    enqueue({ kind, word, to: null });
    void focusWord(kind, word);
  }

  /** ← / →: one column from where the word is drawn now, which presses faster than the board redraws have already moved. */
  function wordKeys(event: KeyboardEvent, kind: string, word: string) {
    if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") return;
    event.preventDefault();
    const now = drawn?.find((k) => k.kind === kind)?.words.find((w) => w.word === word);
    const to = now === undefined ? null : stepped(now, event.key === "ArrowLeft" ? -1 : 1);
    if (to !== null) place(kind, word, to, true);
  }

  function openMenu(event: MouseEvent, kind: string, word: string) {
    opener = event.currentTarget as HTMLElement;
    picking = { kind, word };
    menuOpen = true;
  }

  /** A pick in the menu: the chip's node is drawn anew in its column, so focus goes to it there. */
  function pick(column: ToneChoice) {
    const at = picking;
    menuOpen = false;
    if (at === null) return;
    place(at.kind, at.word, column, false);
    void focusWord(at.kind, at.word);
  }

  /** The word the menu is open on, as drawn now. */
  const picked = $derived(picking === null ? null : (drawn?.find((k) => k.kind === picking!.kind)?.words.find((w) => w.word === picking!.word) ?? null));

  /** A chip's drag: the column under the pointer, of the chip's own kind. */
  function chipDrag(kind: string, word: string) {
    return pointerDrag<ToneChoice>({
      target: (at) => {
        const col = at.closest<HTMLElement>(".tf-sc-col[data-column]");
        return col?.closest<HTMLElement>("section[data-kind]")?.dataset.kind === kind ? (col.dataset.column as ToneChoice) : null;
      },
      over: (column) => (over = column === null ? null : { kind, column }),
      drop: (column) => place(kind, word, column, true),
      ignore: (at) => at.closest(".tf-sc-reset") !== null,
    });
  }

  /** A line with its columns' names in backticks, as text and name runs. */
  function runs(line: string): { name: boolean; text: string }[] {
    return line.split("`").map((text, i) => ({ name: i % 2 === 1, text }));
  }
</script>

{#if drawn !== null}
  <div class="tf-sc" bind:this={board}>
    <h2 class="tf-sc-title">State colours</h2>
    <p class="tf-stg-p">
      The colour each state word draws in, per kind of item. Click a word to pick its colour, drag it to another column, or focus it
      and press ← or →. It saves at once.
    </p>
    {#each drawn as kind (kind.kind)}
      {@const name = kindLabel(kind.kind)}
      {@const line = drives(kind.kind)}
      {@const words = ordered(kind.words)}
      <section class="tf-stg-sec tf-sc-kind" aria-label={name} data-kind={kind.kind}>
        <div class="tf-ext-h">
          <h3>{name}</h3>
          <button
            type="button"
            class="tf-btn tf-sm tf-ghost"
            disabled={!kind.words.some((w) => w.yours !== null)}
            onclick={() => enqueue({ kind: kind.kind, word: null, to: null })}>Reset all</button
          >
        </div>
        <div class="tf-sc-board">
          {#each COLUMNS as column (column)}
            <div
              class="tf-sc-col"
              class:tf-sc-over={over?.kind === kind.kind && over.column === column}
              role="group"
              aria-label={columnName(column)}
              data-column={column}
            >
              <div class="tf-sc-colh">
                {#if column !== "none"}<span class="tf-dot tf-{column}" aria-hidden="true"></span>{/if}{columnName(column)}
              </div>
              {#each words.filter((w) => columnOf(w) === column) as word (word.word)}
                {@const said = stateWord(word.word)}
                <span class="tf-chip tf-sc-chip {toneClass(word)}" role="group" aria-label={said} {@attach chipDrag(kind.kind, word.word)}
                  ><button
                    type="button"
                    class="tf-chipbtn tf-sc-word"
                    aria-label="Colour of {said}"
                    aria-haspopup="menu"
                    aria-expanded={menuOpen && picking?.kind === kind.kind && picking.word === word.word}
                    data-word={word.word}
                    title="{word.word}. {originName(word.origin)}. Click to pick its colour, drag it, or press ← or →"
                    onclick={(event) => openMenu(event, kind.kind, word.word)}
                    onkeydown={(event) => wordKeys(event, kind.kind, word.word)}>{said}</button
                  >{#if word.yours !== null}<button
                      type="button"
                      class="tf-chipbtn tf-sc-reset"
                      aria-label="Reset {said}"
                      title="Back to the default"
                      onclick={() => resetWord(kind.kind, word.word)}>Reset</button
                    >{/if}</span
                >
              {/each}
            </div>
          {/each}
        </div>
        {#if line !== null}
          <p class="tf-stg-p tf-sc-line">
            {#each runs(line) as run, i (i)}{#if run.name}<i>{run.text}</i>{:else}{run.text}{/if}{/each}
          </p>
        {/if}
      </section>
    {/each}
  </div>
  <TfMenu bind:open={menuOpen} {opener} label={picked === null ? "Colour" : `Colour of ${stateWord(picked.word)}`} className="tf-sc-m" focusOn="[aria-checked=true]">
    {#each COLUMNS as column (column)}
      {@const on = picked !== null && columnOf(picked) === column}
      <button type="button" role="menuitemradio" aria-checked={on} class:tf-on={on} onclick={() => pick(column)}
        ><span class="tf-dot tf-sc-mdot {column === 'none' ? 'tf-sc-mnone' : `tf-${column}`}" aria-hidden="true"></span>{columnName(column)}<span
          class="tf-sc-mtick"
          aria-hidden="true">{on ? "✓" : ""}</span
        ></button
      >
    {/each}
  </TfMenu>
{/if}
