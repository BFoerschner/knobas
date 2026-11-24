<!--
  An example made from the element shown (the XSD design, 2026-11-23, sections 19 to 23): beside the reference from
  1,480 px, in the page's place under it.

  - **The controls** are one grid, a label and its control a row, so nothing moves when a value changes: *Parts*
    (*Required only*, *Everything*), *Repeats* (1, 2, 3, of what may repeat), *Values* (the language, the seed,
    *Another*, which `A` presses too). A choice is picked on its own line of the reference.
  - **Check** (libxml2, in the schema's worker), *Copy*, *Save to Downloads* and *Open in XML view* stand under them
    with the line count,
    then the verdict's line, holding its room, then the notes, then the document, which takes the rest and scrolls by
    itself.
  - **The document** is a miniature XML view (step E1, Björn 2026-11-24): the XML view's own document without its
    rail, in the column's width. Folds and their keys, *Levels*, value chips and `Space`, links, and the check's
    departures at their lines' ends. A line pressed goes to its element's declaration.
  - **Labelled three times**: *Example*, *Made from the schema*; its second line's comment; the saved file's name.
-->
<script lang="ts">
  import TfDocView from "../doc/TfDocView.svelte";
  import TfXsdRuns from "./TfXsdRuns.svelte";
  import { textPiped, textPipedSave, xsdSaveExample } from "../../../ipc/api";
  import { ipcErrorMessage } from "../../../ipc";
  import { push } from "../../../shell/toasts.svelte";
  import type { RenderPort } from "../../../render/port";
  import type { XsdState } from "./xsd.svelte";

  let {
    st,
    render,
    ongo,
    onopen = null,
  }: {
    st: XsdState;
    render: RenderPort;
    /** A line pressed: its element's declaration. */
    ongo: (key: string, label: string) => void;
    /** *Open in XML view*: the example as a place of its own (E1). */
    onopen?: (() => void) | null;
  } = $props();

  /** The element the page shows, where an example can be made from it: a global element's page. */
  const pageKey = $derived(st.page !== null && st.page.key.startsWith("el ") ? st.page.key : null);
  /**
   * The element the example follows: the page's, once the page has painted. Mounted beside a
   * page's first rows, the controls shared their frame (UBL's Invoice page from the rail, 6x on 2560 x 1440 at 1.25).
   */
  // svelte-ignore state_referenced_locally
  let key = $state<string | null>(pageKey);
  $effect(() => {
    const k = pageKey;
    if (k === key) return;
    // After the next frame has painted: a frame callback alone ran in that frame, before its paint.
    let task = 0;
    const frame = requestAnimationFrame(() => {
      task = window.setTimeout(() => (key = k), 0);
    });
    return () => {
      cancelAnimationFrame(frame);
      window.clearTimeout(task);
    };
  });
  // Made again when the element or a setting changes; the document keeps its box meanwhile.
  $effect(() => {
    const k = key;
    void st.ex;
    if (k !== null) void st.makeExample(k);
  });
  const made = $derived(st.made !== null && st.made.key === key ? st.made : null);
  const lineCount = $derived(made === null ? 0 : made.example.lines.length);
  const v = $derived(st.verdict);
  /** The notes drawn: their room is held for two, the rest in the second's title. */
  const NOTES_SHOWN = 2;

  const labelOf = (k: string) => {
    const d = st.index?.decls.find((x) => x.key === k);
    return d === undefined ? k.slice(k.indexOf("}") + 1) : `${d.prefix}${d.local}`;
  };
  function pressed(event: MouseEvent) {
    // A caret, a fold's count, a value's chip or a link is the view's own.
    if (event.target instanceof Element && event.target.closest("button, a, [data-xv-link]") !== null) return;
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
      {#if onopen !== null}<button type="button" class="tf-btn tf-sm" disabled={made === null} title="The example as a whole XML view, checked against this schema" onclick={() => onopen?.()}>Open in XML view</button>{/if}
      <span class="tf-dim tf-xsd-excount">{made === null ? "Making it…" : `${lineCount.toLocaleString("en")} lines`}</span>
    </div>
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
        {@const doc = made.example}
        <TfDocView
          inline
          text={doc.xml}
          path={doc.fileName}
          {render}
          verdict={st.verdict}
          values={{ run: (steps) => textPiped(doc.xml, doc.fileName, steps), save: (steps, name) => textPipedSave(doc.xml, doc.fileName, steps, name) }}
        />
      {/if}
    </div>
  {/if}
</div>
