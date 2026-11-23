<!--
  The query's bar (the XML view's design, 2026-11-23, section 22), over the document under the way down and *Levels*:
  one line as wide as the document's column, so a query is read whole. The field says its language before the text and
  `Q` after it; beside it a slot holds its room for the answer, two lines tall for an error's sentence.

  Live: every key asks (in a document over 2 MiB, when typing rests). The field is never drawn again while it is typed
  in: an answer changes the slot and the rail only, so it keeps its focus and its caret.

  Keys in the field: `Enter` goes to the first match and then the next, `Shift Enter` to the one before, `↓` moves to
  the list, `↑` in the empty field takes the last query asked, `Esc` clears the field and then leaves it.
-->
<script lang="ts">
  import { filterKeys } from "../../filter-keys";
  import { askedBefore, type DocState } from "./doc.svelte";
  import { coded } from "./coded";

  let {
    st,
    size,
    field = $bindable(null),
    onstep,
    onlist,
    onleave,
  }: {
    st: DocState;
    /** The document's size in characters: past 2 MiB a query waits for typing to rest. */
    size: number;
    field?: HTMLInputElement | null;
    /** `Enter` and `Shift Enter`: the next match, or the one before. */
    onstep: (by: 1 | -1) => void;
    /** `↓`: focus moves to the results. */
    onlist: () => void;
    /** `Esc` in the empty field: focus goes back to the lines. */
    onleave: () => void;
  } = $props();

  const q = $derived(st.q);
  const plural = (n: number, one: string, many: string) => `${n.toLocaleString("en")} ${n === 1 ? one : many}`;
  const ms = (t: number) => (t < 10 ? t.toFixed(1) : Math.round(t).toString());

  function typed(event: Event) {
    st.queryTyped((event.currentTarget as HTMLInputElement).value, size);
  }

  function keys(event: KeyboardEvent) {
    const box = event.currentTarget as HTMLInputElement;
    if (event.key === "Enter") {
      event.preventDefault();
      onstep(event.shiftKey ? -1 : 1);
    } else if (event.key === "ArrowDown") {
      event.preventDefault();
      onlist();
    } else if (event.key === "Escape" && box.value === "") {
      event.preventDefault();
      event.stopPropagation();
      onleave();
    } else if (event.key === "ArrowUp" && box.value === "") {
      const last = askedBefore()[0];
      if (last === undefined) return;
      event.preventDefault();
      box.value = last;
      st.queryTyped(last, size);
    }
  }
</script>

<div class="tf-xv-qbar">
  <label class="tf-minif tf-xv-qfield">
    <span class="tf-xv-lang">XPath</span>
    <input
      bind:this={field}
      value={st.source}
      oninput={typed}
      onkeydown={keys}
      placeholder="//ORDER_ITEM[QUANTITY > 10]/LINE_ITEM_ID"
      aria-label="XPath query"
      autocomplete="off"
      spellcheck="false"
      data-xv-query=""
      {@attach filterKeys({ rail: true })}
    />
    <kbd>Q</kbd>
  </label>
  <div class="tf-xv-qsay" aria-live="polite">
    {#if q.state === "empty" || q.state === "asking"}
      {#if q.state === "empty" && st.opened?.defaultNs === true}
        <span class="tf-dim">A name finds what the document writes: no prefix needed for its own namespace.</span>
      {:else if q.state === "empty"}
        <span class="tf-dim">XPath 1.0. Results show as you type.</span>
      {/if}
    {:else if q.state === "error"}
      <span class="tf-xv-warn">{#each coded(q.message) as part, i (i)}{#if part.code}<code>{part.text}</code>{:else}{part.text}{/if}{/each}</span>
    {:else if q.state === "stopped"}
      <span class="tf-xv-warn">{q.message}</span>
    {:else if q.state === "nodes"}
      <b>{plural(q.count, "node", "nodes")}</b>
      {q.count === 0 ? "Nothing in the document answers." : `on ${plural(q.lines.length, "line", "lines")}.`}
      <span class="tf-dim">{ms(q.ms)} ms</span>
    {:else}
      <b>{q.text.length > 60 ? `${q.text.slice(0, 57)}…` : q.text}</b>
      <span class="tf-dim">{q.type === "number" ? "a number" : q.type === "boolean" ? "true or false" : "a string"}</span>
      <span class="tf-dim">{ms(q.ms)} ms</span>
    {/if}
  </div>
</div>
