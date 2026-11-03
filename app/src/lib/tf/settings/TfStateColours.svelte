<!--
  Settings › Appearance › State colours (#2466, spec #2463): one section per
  entity kind that declares a state, each a board of six columns (the five
  tones, then *No colour*) holding one chip per state word, over
  `state_colours`, `set_tone_override` and `clear_tone_overrides`.

  A chip moves by drag, or by ← / → on its grip, one column per press, as a
  file pipeline's step does (`TfFilePipeline.svelte`). A move sets the
  word's tone at once; there is no Save. The board draws only what the last
  answer stored: a write that fails raises a toast and the chip stays where
  that answer put it. One write at a time; a press while one is in flight is
  dropped.

  Every surface that draws a state reads the merged map, so a move changes
  more than the colour: the line under each section says what (`drives`).
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
  import { toneClass } from "../tone";
  import { COLUMNS, columnName, columnOf, drives, ordered, originName, stepped } from "./state-colours";

  let { ports }: { ports: StateColourPorts } = $props();

  /** The last answer the backend gave; `null` until the read answers. */
  let stored = $state<KindStateColours[] | null>(null);
  let writing = false;
  let dragged: { kind: string; word: string } | null = null;
  let board: HTMLElement | undefined = $state();

  $effect(() => {
    void ports
      .stateColours()
      .then((read) => (stored = read))
      .catch((cause) => push({ text: `Could not read the state colours: ${ipcErrorMessage(cause)}`, tone: "err" }));
  });

  /** Run one write, keep its answer, or toast and keep the last one. */
  async function write(send: () => Promise<KindStateColours[]>, failed: string): Promise<boolean> {
    if (writing) return false;
    writing = true;
    try {
      stored = await send();
      return true;
    } catch (cause) {
      push({ text: `${failed}: ${ipcErrorMessage(cause)}`, tone: "err" });
      return false;
    } finally {
      writing = false;
    }
  }

  /** Put a word in a column: what a press and a drop both do. */
  async function place(kind: string, word: string, column: ToneChoice, refocus: boolean) {
    const now = stored?.find((k) => k.kind === kind)?.words.find((w) => w.word === word);
    if (now === undefined || columnOf(now) === column) return;
    const done = await write(() => ports.setToneOverride(kind, word, column), `Could not set the colour of ${word}`);
    if (done && refocus) {
      await tick();
      board?.querySelector<HTMLElement>(`section[data-kind="${CSS.escape(kind)}"] button[aria-label="Move ${CSS.escape(word)}"]`)?.focus();
    }
  }

  function gripKeys(event: KeyboardEvent, kind: string, word: KindStateColours["words"][number]) {
    if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") return;
    event.preventDefault();
    const to = stepped(word, event.key === "ArrowLeft" ? -1 : 1);
    if (to !== null) void place(kind, word.word, to, true);
  }

  function dragStart(event: DragEvent, kind: string, word: string) {
    dragged = { kind, word };
    event.dataTransfer?.setData("text/plain", word);
  }

  function drop(event: DragEvent, kind: string, column: ToneChoice) {
    event.preventDefault();
    const held = dragged;
    dragged = null;
    if (held !== null && held.kind === kind) void place(kind, held.word, column, false);
  }

  /** A line with its tone names in backticks, as text and code runs. */
  function runs(line: string): { code: boolean; text: string }[] {
    return line.split("`").map((text, i) => ({ code: i % 2 === 1, text }));
  }
</script>

{#if stored !== null}
  <div class="tf-sc" bind:this={board}>
    <h2 class="tf-sc-title">State colours</h2>
    <p class="tf-stg-p">
      Where a state word draws, per kind of item. Drag a word to another column, or focus it and press ← or →. It saves at once.
    </p>
    {#each stored as kind (kind.kind)}
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
            onclick={() => void write(() => ports.clearToneOverrides(kind.kind, null), `Could not reset ${name}`)}>Reset all</button
          >
        </div>
        <div class="tf-sc-board">
          {#each COLUMNS as column (column)}
            <div
              class="tf-sc-col"
              role="group"
              aria-label={columnName(column)}
              ondragover={(event) => event.preventDefault()}
              ondrop={(event) => drop(event, kind.kind, column)}
            >
              <div class="tf-sc-colh">
                {#if column !== "none"}<span class="tf-dot tf-{column}" aria-hidden="true"></span>{/if}{columnName(column)}
              </div>
              {#each words.filter((w) => columnOf(w) === column) as word (word.word)}
                <span
                  class="tf-chip tf-sc-chip {toneClass(word)}"
                  role="group"
                  aria-label={word.word}
                  draggable="true"
                  ondragstart={(event) => dragStart(event, kind.kind, word.word)}
                  ondragend={() => (dragged = null)}
                  ><button
                    type="button"
                    class="tf-chipbtn tf-sc-word"
                    aria-label="Move {word.word}"
                    title="Drag, or press ← or →"
                    onkeydown={(event) => gripKeys(event, kind.kind, word)}>{word.word}</button
                  ><span class="tf-sc-from">{originName(word.origin)}</span>{#if word.yours !== null}<button
                      type="button"
                      class="tf-chipbtn tf-sc-reset"
                      aria-label="Reset {word.word}"
                      title="Back to the default"
                      onclick={() => void write(() => ports.clearToneOverrides(kind.kind, word.word), `Could not reset ${word.word}`)}
                      >Reset</button
                    >{/if}</span
                >
              {/each}
            </div>
          {/each}
        </div>
        {#if line !== null}
          <p class="tf-stg-p tf-sc-line">
            {#each runs(line) as run, i (i)}{#if run.code}<code>{run.text}</code>{:else}{run.text}{/if}{/each}
          </p>
        {/if}
      </section>
    {/each}
  </div>
{/if}
