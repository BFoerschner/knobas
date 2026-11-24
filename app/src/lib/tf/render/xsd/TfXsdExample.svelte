<!--
  An example made from the element shown (the XSD design, 2026-11-23, sections 19 to 23): beside the reference from
  1,480 px, in the page's place under it.

  - **The controls** are one grid, a label and its control a row, so nothing moves when a value changes: *Parts*
    (*Required only*, *Everything*), *Repeats* (1, 2, 3, of what may repeat), *Values* (the language, the seed,
    *Another*, which `A` presses too). A choice is picked on its own line of the reference.
  - **Check** (libxml2, in the schema's worker), *Copy* and *Save to Downloads* stand under them with the line count,
    then the verdict's line, holding its room, then the notes, then the document, which takes the rest and scrolls by
    itself.
  - **The document** is drawn as Source draws a file, windowed; a line pressed goes to its element's declaration.
  - **Labelled three times**: *Example*, *Made from the schema*; its second line's comment; the saved file's name.
-->
<script lang="ts">
  import TfLines from "../../code/TfLines.svelte";
  import TfXsdRuns from "./TfXsdRuns.svelte";
  import { xsdSaveExample } from "../../../ipc/api";
  import { ipcErrorMessage } from "../../../ipc";
  import { push } from "../../../shell/toasts.svelte";
  import type { RenderPort } from "../../../render/port";
  import type { XsdState } from "./xsd.svelte";

  let {
    st,
    render,
    ongo,
  }: {
    st: XsdState;
    render: RenderPort;
    /** A line pressed: its element's declaration. */
    ongo: (key: string, label: string) => void;
  } = $props();

  /** The element the page shows, where an example can be made from it: a global element's page. */
  const key = $derived(st.page !== null && st.page.key.startsWith("el ") ? st.page.key : null);
  // Made again when the element or a setting changes; the document keeps its box meanwhile.
  $effect(() => {
    const k = key;
    void st.ex;
    if (k !== null) void st.makeExample(k);
  });
  const made = $derived(st.made !== null && st.made.key === key ? st.made : null);
  const lineCount = $derived(made === null ? 0 : made.example.lines.length);
  /** The notes drawn: their room is held for two, the rest in the second's title. */
  const NOTES_SHOWN = 2;

  const labelOf = (k: string) => {
    const d = st.index?.decls.find((x) => x.key === k);
    return d === undefined ? k.slice(k.indexOf("}") + 1) : `${d.prefix}${d.local}`;
  };
  function pressed(event: MouseEvent) {
    const row = event.target instanceof Element ? event.target.closest<HTMLElement>(".tf-l[data-n]") : null;
    const n = Number(row?.dataset["n"] ?? 0);
    const to = made?.example.lines[n - 1]?.key ?? null;
    if (to !== null && to !== key) ongo(to, labelOf(to));
  }
  async function copy() {
    if (made === null) return;
    try {
      await navigator.clipboard.writeText(made.example.xml);
      push({ text: "Copied the example", subject: "copy-example" });
    } catch (cause) {
      push({ text: `Could not copy it: ${cause instanceof Error ? cause.message : String(cause)}`, tone: "err" });
    }
  }
  async function save() {
    if (made === null) return;
    try {
      const path = await xsdSaveExample(made.example.fileName, made.example.xml);
      push({ text: `Saved ${path.slice(path.lastIndexOf("/") + 1)} to Downloads` });
    } catch (cause) {
      push({ text: `Could not save it: ${ipcErrorMessage(cause)}`, tone: "err" });
    }
  }
  const seg = <T,>(list: readonly (readonly [T, string])[]) => list;
  const PARTS = seg<"required" | "all">([
    ["required", "Required only"],
    ["all", "Everything"],
  ]);
  const REPEATS = seg<1 | 2 | 3>([
    [1, "1"],
    [2, "2"],
    [3, "3"],
  ]);
  function seedTyped(event: Event) {
    const v = Number((event.currentTarget as HTMLInputElement).value);
    if (Number.isInteger(v) && v > 0 && v !== st.ex.seed) st.setExample({ seed: v });
  }
</script>

<div class="tf-xsd-ex">
  <div class="tf-xsd-exhead"><b>Example</b> <span class="tf-oas-exfrom">Made from the schema</span></div>
  {#if key === null}
    <p class="tf-oas-fnote">Pick an element: knobas makes an XML document from it, valid and with values a person would have typed.</p>
  {:else}
    <div class="tf-xsd-ctl">
      <span>Parts</span>
      <span>
        <span class="tf-seg" role="group" aria-label="Parts">
          {#each PARTS as [v, word] (v)}<button type="button" aria-pressed={st.ex.optional === v} onclick={() => st.setExample({ optional: v })}>{word}</button>{/each}
        </span>
      </span>
      <span>Repeats</span>
      <span>
        <span class="tf-seg" role="group" aria-label="Repeats">
          {#each REPEATS as [v, word] (v)}<button type="button" aria-pressed={st.ex.repeats === v} onclick={() => st.setExample({ repeats: v })}>{word}</button>{/each}
        </span>
        <span>of what may repeat</span>
      </span>
      <span>Values</span>
      <span>
        <select aria-label="Language of the values" value={st.ex.lang} onchange={(e) => st.setExample({ lang: (e.currentTarget as HTMLSelectElement).value === "de" ? "de" : "en" })}>
          <option value="de">Deutsch</option>
          <option value="en">English</option>
        </select>
        <span>seed</span>
        <input class="tf-xsd-seed" inputmode="numeric" aria-label="Seed" value={st.ex.seed} onchange={seedTyped} />
        <button type="button" class="tf-btn tf-sm" title="Another seed (A)" onclick={() => st.another()}>Another</button>
      </span>
    </div>
    <div class="tf-xsd-acts">
      <button type="button" class="tf-btn tf-sm tf-primary" disabled={made === null || st.verdict === "checking"} onclick={() => void st.checkExample()}>Check</button>
      <button type="button" class="tf-btn tf-sm" disabled={made === null} onclick={() => void copy()}>Copy</button>
      <button type="button" class="tf-btn tf-sm" disabled={made === null} onclick={() => void save()}>Save to Downloads</button>
      <span class="tf-dim tf-xsd-excount">{made === null ? "Making it…" : `${lineCount.toLocaleString("en")} lines`}</span>
    </div>
    {@const v = st.verdict}
    <div class="tf-xsd-verdict" class:tf-ok={v !== null && v !== "checking" && v.state === "checked" && v.findings.length === 0} class:tf-err={v !== null && v !== "checking" && v.state === "checked" && v.findings.length > 0} class:tf-warn={v !== null && v !== "checking" && v.state !== "checked"} aria-live="polite">
      {#if v === null}<span class="tf-dim">Not checked yet.</span>
      {:else if v === "checking"}<span class="tf-dim">Checking…</span>
      {:else if v.state !== "checked"}<b>Not checked</b> <span>{v.detail[0] ?? v.why}</span>
      {:else if v.findings.length === 0}<b>Valid</b> <span>against the schema, says libxml2.</span> <span class="tf-dim">{(v.ms + v.compileMs).toFixed(1)} ms</span>
      {:else}<b>{v.findings.length === 1 ? "1 departure" : `${v.findings.length.toLocaleString("en")} departures`}</b> <span><TfXsdRuns runs={v.findings[0]!.words.text} /></span>
      {/if}
    </div>
    <ul class="tf-xsd-notes">
      {#each (made?.example.notes ?? []).slice(0, NOTES_SHOWN) as note, i (i)}<li title={i === NOTES_SHOWN - 1 ? (made?.example.notes.slice(i).join("\n") ?? note) : note}>{note}</li>{/each}
    </ul>
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <div class="tf-xsd-xml" role="presentation" onclick={pressed}>
      {#if made !== null}
        <TfLines text={made.example.xml} path="example.xml" {render} find="overlay" scrolls label="The example" wholeUpTo={64} />
      {/if}
    </div>
  {/if}
</div>
