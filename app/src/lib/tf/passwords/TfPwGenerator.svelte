<!--
  The password generator (ADR-0056, `generate.ts`): *Characters* (a length of 8 to 64, four sets, look-alikes left out)
  or *Words* (3 to 8 of the EFF's long list, a separator, capitals, a number), each drawn without bias on
  `crypto.getRandomValues`. A change of a setting makes a new value; *Generate another* does too. The value lives in the
  dialog's state and leaves it only in the write.
-->
<script lang="ts">
  import { DEFAULT_CHARS, DEFAULT_WORDS, characters, strength, wordList, words, type CharOptions, type WordOptions } from "./generate";

  let { onvalue }: { onvalue: (value: string) => void } = $props();

  let mode = $state<"chars" | "words">("chars");
  const chars = $state<CharOptions>({ ...DEFAULT_CHARS });
  const phrase = $state<WordOptions>({ ...DEFAULT_WORDS });
  let list = $state<string[] | null>(null);

  export async function make(): Promise<void> {
    if (mode === "words") {
      list ??= await wordList();
      onvalue(words(phrase, list));
    } else {
      onvalue(characters(chars));
    }
  }

  const bits = $derived(mode === "words" ? strength("words", phrase) : strength("chars", chars));
</script>

<div class="tf-pw-gen">
  <div class="tf-pw-gen-row">
    <label><input type="radio" name="tf-pw-mode" checked={mode === "chars"} onchange={() => ((mode = "chars"), void make())} /> Characters</label>
    <label><input type="radio" name="tf-pw-mode" checked={mode === "words"} onchange={() => ((mode = "words"), void make())} /> Words</label>
  </div>
  {#if mode === "chars"}
    <div class="tf-pw-gen-row">
      <label for="tf-pw-len">Length</label>
      <input id="tf-pw-len" type="range" min="8" max="64" bind:value={chars.length} oninput={() => void make()} />
      <output for="tf-pw-len">{chars.length}</output>
    </div>
    <div class="tf-pw-gen-row">
      <label><input type="checkbox" bind:checked={chars.lower} onchange={() => void make()} /> a–z</label>
      <label><input type="checkbox" bind:checked={chars.upper} onchange={() => void make()} /> A–Z</label>
      <label><input type="checkbox" bind:checked={chars.digits} onchange={() => void make()} /> 0–9</label>
      <label><input type="checkbox" bind:checked={chars.symbols} onchange={() => void make()} /> !#$%</label>
      <label><input type="checkbox" bind:checked={chars.plain} onchange={() => void make()} /> Leave out look-alikes</label>
    </div>
  {:else}
    <div class="tf-pw-gen-row">
      <label for="tf-pw-words">Words</label>
      <input id="tf-pw-words" type="range" min="3" max="8" bind:value={phrase.words} oninput={() => void make()} />
      <output for="tf-pw-words">{phrase.words}</output>
    </div>
    <div class="tf-pw-gen-row">
      <label for="tf-pw-sep">Separator</label>
      <select id="tf-pw-sep" bind:value={phrase.separator} onchange={() => void make()}>
        <option value="-">-</option>
        <option value=".">.</option>
        <option value="_">_</option>
        <option value=" ">space</option>
      </select>
      <label><input type="checkbox" bind:checked={phrase.capitals} onchange={() => void make()} /> Capitals</label>
      <label><input type="checkbox" bind:checked={phrase.number} onchange={() => void make()} /> A number</label>
    </div>
  {/if}
  <p class="tf-pw-strength">About {bits} bits{mode === "words" ? `, 12.9 a word` : ""}.</p>
</div>
