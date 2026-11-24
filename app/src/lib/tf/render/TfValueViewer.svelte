<!--
  **The value viewer** (the XML view's design, 2026-11-23, sections 15 to 19; ADR-0059): the file viewer's dialog
  opened on a value of a file (an element's text, an attribute), with the pipeline bar on top, `value → step → step →
  renderer`, and under it what came out, drawn by that renderer.

  - **A value pipeline is a file pipeline** whose first step, `Take`, cuts the value out of the file in the backend:
    the web view names a byte range, never the value. It runs where the steps run, under their caps, and is kept as
    they are, in memory.
  - **The chain is knobas's guess, a kept one, or the person's.** One line under the bar says what went in and what
    came out, in sizes, and why knobas guessed what it did. A change runs at once; the bar, the line and the stage each
    hold their room.
  - **The bytes decide** where the person names no renderer: a PNG is drawn as an image whatever was said beside it.
  - **Nothing decoded is trusted.** An image is an `<img>` from a blob, which runs no script, dropped when the dialog
    closes; a PDF is pdf.js with scripting off; text is lines. **Nothing is written** unless the person presses
    *Download*.
  - **Keep for this field…** keeps the chain as a named pipeline for every field whose names end the same way, in the
    files a pattern takes.

  It belongs to no renderer: the XML view opens it, and Source opens it on a selection.
-->
<script lang="ts" module>
  /** A value to look at: where it is, what it is called, and what is said of it. */
  export interface LookedAt {
    from: number;
    to: number;
    read: "bytes" | "xml_text" | "xml_attribute" | "json_string";
    /** Its name as the file writes it (`MIME_DATA`, `@href`), or what it is (*The selection*). */
    name: string;
    /** The names down to it, no positions; `null` where it is no field (a selection). */
    field: string | null;
    /** How long it is, in characters. */
    length: number;
    /** A name the document gives beside it, for *Download*. */
    fileName: string | null;
    /** Its line, for the title's line. */
    line: number | null;
  }
</script>

<script lang="ts">
  import "../css/xml.css";

  import TfDialog from "../settings/TfDialog.svelte";
  import TfFilePipeline from "./TfFilePipeline.svelte";
  import TfPdf from "./TfPdf.svelte";
  import TfLines from "../code/TfLines.svelte";
  import { layers as sharedLayers, type Layers } from "../layers.svelte";
  import { push } from "../../shell/toasts.svelte";
  import { ipcErrorMessage } from "../../ipc";
  import type { CommandStep, NamedPipeline } from "../../ipc/app";
  import type { PipeRun, PipeStep } from "../../ipc/entity";
  import { savedAs, sized, stepName } from "../../render/pipeline";
  import type { RenderPort } from "../../render/port";
  import { endFor, sizeWord, sniffBytes, type End, type Guess } from "../../render/value-guess";
  import { offeredField } from "../../render/value-kept";

  let {
    value,
    path,
    guess = null,
    kept = null,
    first = null,
    pipelines,
    commands = [],
    run,
    save,
    keep,
    render,
    layers = sharedLayers,
    onclose,
  }: {
    value: LookedAt;
    /** The file's path: the kept chain's pattern offers its ending. */
    path: string;
    /** What knobas took the value for, where it took it for something. */
    guess?: Guess | null;
    /** The chain kept for this field, where one is. */
    kept?: { pipeline: NamedPipeline; end: string } | null;
    /** A chain to start on that is neither: a right-click's *Send it through*. */
    first?: { steps: PipeStep[]; end: End | null } | null;
    /** The named pipelines: *Keep* adds to them. */
    pipelines: readonly NamedPipeline[];
    commands?: readonly CommandStep[];
    /** Runs `steps` over the file: the value pipeline, `Take` first. */
    run: (steps: PipeStep[]) => Promise<PipeRun>;
    /** *Download*: the output saved to Downloads as `name`; answers the path. */
    save: (steps: PipeStep[], name: string) => Promise<string>;
    /** Stores the named pipelines. */
    keep: (list: ReturnType<typeof savedAs>) => Promise<unknown>;
    render: RenderPort;
    layers?: Layers;
    onclose: () => void;
  } = $props();

  const RENDERERS: readonly { id: End | "auto"; label: string }[] = [
    { id: "auto", label: "As the bytes say" },
    { id: "image", label: "Image" },
    { id: "pdf", label: "PDF" },
    { id: "xml", label: "XML view" },
    { id: "json", label: "JSON view" },
    { id: "text", label: "Text" },
    { id: "hex", label: "Hex" },
  ];

  // svelte-ignore state_referenced_locally
  const start = kept !== null ? { steps: kept.pipeline.steps, end: kept.end as End } : first !== null ? first : guess !== null ? { steps: guess.steps, end: guess.end } : { steps: [] as PipeStep[], end: "text" as End };
  let steps = $state<PipeStep[]>([...start.steps]);
  /** The renderer the person picked, or the one the chain started with; `auto` lets the bytes decide. */
  let picked = $state<End | "auto">(start.end ?? "auto");
  /** Where the chain came from, as the bar says it. */
  // svelte-ignore state_referenced_locally
  let origin = $state<"kept" | "guess" | "given" | "edited" | "none">(kept !== null ? "kept" : first !== null ? "given" : guess !== null ? "guess" : "none");
  /** The name the chain is kept under: the field's kept chain, or the one kept from this dialog (the prop is as it opened). */
  // svelte-ignore state_referenced_locally
  let keptName = $state<string | null>(kept?.pipeline.name ?? null);

  const take = $derived<PipeStep>({ step: "take", from: value.from, to: value.to, read: value.read });
  let ran = $state.raw<PipeRun | null>(null);
  let running = $state(false);
  let refused = $state<string | null>(null);
  $effect(() => {
    const chain = [take, ...steps];
    let live = true;
    running = true;
    refused = null;
    run(chain).then(
      (answer) => {
        if (!live) return;
        running = false;
        ran = answer;
      },
      (cause) => {
        if (!live) return;
        running = false;
        ran = null;
        refused = ipcErrorMessage(cause);
      },
    );
    return () => {
      live = false;
    };
  });

  /** What came out, as bytes and as text where it is text. */
  const out = $derived.by(() => {
    if (ran === null || ran.outcome !== "done") return null;
    if (ran.output.kind === "text") {
      const text = ran.output.text;
      return { text, bytes: new TextEncoder().encode(text), size: new TextEncoder().encode(text).length };
    }
    const raw = atob(ran.output.base64);
    const bytes = new Uint8Array(raw.length);
    for (let i = 0; i < raw.length; i += 1) bytes[i] = raw.charCodeAt(i);
    return { text: null, bytes, size: ran.output.size };
  });
  const sniffed = $derived(out === null ? null : sniffBytes(out.bytes.subarray(0, 64)));
  /** The renderer drawing it: the one picked, else what the bytes are. */
  const end = $derived<End>(picked !== "auto" ? (picked === "image" && sniffed !== null && sniffed.kind !== "image" ? endFor(null, sniffed) : picked) : out === null ? "text" : sniffed !== null ? endFor(null, sniffed) : out.text !== null ? "text" : "hex");
  /** The bytes overrule a label: a value said to be an image that is not one is drawn as what it is. */
  const drawn = $derived<End>(end === "image" && sniffed?.kind !== "image" ? (out?.text !== null ? "text" : "hex") : end === "pdf" && sniffed?.kind !== "pdf" ? (out?.text !== null ? "text" : "hex") : end);

  // An image is a blob, dropped when the dialog closes or the output changes.
  let image = $state<string | null>(null);
  $effect(() => {
    const o = out;
    if (o === null || drawn !== "image") {
      image = null;
      return;
    }
    const url = URL.createObjectURL(new Blob([o.bytes as BlobPart], { type: sniffed?.mime ?? "application/octet-stream" }));
    image = url;
    return () => URL.revokeObjectURL(url);
  });

  /** The text drawn: laid out where it is JSON on one line, and a hex dump where it is bytes. */
  const shownText = $derived.by(() => {
    const o = out;
    if (o === null) return "";
    if (drawn === "hex") return hexDump(o.bytes.subarray(0, 64 * 1024)) + (o.size > 64 * 1024 ? `\n… ${sizeWord(o.size - 64 * 1024)} more` : "");
    const text = o.text ?? new TextDecoder().decode(o.bytes);
    if (drawn === "json" && !text.includes("\n") && text.length < 2 * 1024 * 1024) {
      try {
        return JSON.stringify(JSON.parse(text), null, 2);
      } catch {
        return text;
      }
    }
    return text;
  });

  function hexDump(bytes: Uint8Array): string {
    const lines: string[] = [];
    for (let at = 0; at < bytes.length; at += 16) {
      const row = bytes.subarray(at, at + 16);
      const hex = [...row].map((b, i) => (i === 8 ? " " : "") + b.toString(16).padStart(2, "0")).join(" ");
      const text = [...row].map((b) => (b >= 32 && b < 127 ? String.fromCharCode(b) : ".")).join("");
      lines.push(`${at.toString(16).padStart(8, "0")}  ${hex.padEnd(49)}  |${text}|`);
    }
    return lines.join("\n");
  }

  /** What went in and what came out, in sizes, and why the chain is what it is. */
  const flow = $derived.by(() => {
    const parts: string[] = [`${value.length.toLocaleString("en")} characters`];
    if (out !== null) parts[0] = `${parts[0]} → ${sizeWord(out.size)}.`;
    else parts[0] = `${parts[0]}.`;
    const said: string[] = [];
    if (sniffed !== null) said.push(`The result starts as ${/^[AEIOU]/.test(sniffed.word) ? "an" : "a"} ${sniffed.word}.`);
    if (origin === "guess" && guess !== null) said.push(`knobas takes the value for ${guess.why}.`);
    if (origin === "kept" && keptName !== null) said.push(`Kept as ${keptName}.`);
    return [...parts, ...said].join(" ");
  });

  const failure = $derived(ran !== null && ran.outcome === "failed" ? ran.failure : null);
  // The bar numbers steps after the value: the backend's index counts `Take` as step 1.
  const failedAt = $derived(failure === null ? null : failure.index - 1);

  const status = $derived(origin === "kept" && keptName !== null ? `Kept as ${keptName}` : origin === "guess" ? "knobas's guess" : origin === "none" ? null : "Not kept");

  function change(next: PipeStep[]) {
    steps = next;
    origin = "edited";
  }

  // ---- Keep for this field ----
  const ending = (() => {
    const name = path.slice(path.lastIndexOf("/") + 1);
    const dot = name.lastIndexOf(".");
    return dot > 0 ? `*${name.slice(dot).toLowerCase()}` : name;
  })();
  let keeping = $state<{ name: string; field: string; files: string } | null>(null);
  let storing = $state(false);
  function openKeep() {
    const field = value.field === null ? "" : offeredField(value.field);
    const word = RENDERERS.find((r) => r.id === drawn)?.label ?? "Text";
    keeping = { name: `${field.split("/").at(-1) ?? field} as ${word}`, field, files: ending };
  }
  async function doKeep() {
    const k = keeping;
    if (k === null || k.name.trim() === "" || k.field.trim() === "" || k.files.trim() === "") return;
    storing = true;
    try {
      const list = savedAs(pipelines, k.name, steps);
      const at = list.findIndex((p) => p.name.toLowerCase() === k.name.trim().toLowerCase());
      const mine = list[at]!;
      const fields = [...(mine.fields ?? []).filter((f) => !(f.field === k.field.trim() && f.files === k.files.trim())), { files: k.files.trim(), field: k.field.trim(), end: drawn }];
      list[at] = { ...mine, fields };
      await keep(list);
      push({ text: `Kept as ${k.name.trim()} for ${k.field.trim()} in ${k.files.trim()}.` });
      keeping = null;
      keptName = k.name.trim();
      origin = "kept";
    } catch (cause) {
      push({ text: `Could not keep it: ${ipcErrorMessage(cause)}`, tone: "err" });
    } finally {
      storing = false;
    }
  }

  // ---- Download ----
  function downloadName(): string {
    if (value.fileName !== null && value.fileName !== "") return value.fileName;
    const file = path.slice(path.lastIndexOf("/") + 1).replace(/\.[^.]*$/, "");
    const ext = sniffed?.ext ?? (drawn === "json" ? "json" : drawn === "xml" ? "xml" : out?.text !== null ? "txt" : "bin");
    return `${value.name.replace(/^@/, "")}-${file}.${ext}`;
  }
  async function download() {
    try {
      const at = await save([take, ...steps], downloadName());
      push({ text: `Saved to ${at}.` });
    } catch (cause) {
      push({ text: `Could not save it: ${ipcErrorMessage(cause)}`, tone: "err" });
    }
  }
</script>

{#snippet downloadButton()}
  <button type="button" class="tf-btn" disabled={out === null} title={out === null ? "Nothing came out yet" : `Save it to Downloads as ${downloadName()}`} onclick={() => void download()}>
    <svg class="tf-vglyph" viewBox="0 0 16 16" aria-hidden="true"><path d="M8 2.5v7.5M4.75 6.75 8 10l3.25-3.25M3 13.5h10" /></svg>
    Download
  </button>
{/snippet}

<TfDialog title={value.name} subtitle={value.line === null ? path : `${path}, line ${value.line.toLocaleString("en")}`} {onclose} {layers} className="tf-viewer tf-xv-dialog" closeButton>
  {#snippet actions()}{@render downloadButton()}{/snippet}
  {#snippet body()}
    <TfFilePipeline
      {steps}
      path={value.name}
      shownAs=""
      source="value"
      renderers={RENDERERS}
      renderer={picked}
      onrenderer={(id) => {
        picked = id as End | "auto";
        if (origin !== "kept") origin = "edited";
      }}
      {status}
      failedAt={failedAt}
      {running}
      onchange={change}
      onreset={() => {
        steps = [...start.steps];
        picked = start.end ?? "auto";
        origin = kept !== null ? "kept" : guess !== null ? "guess" : "none";
      }}
      edited={origin === "edited"}
      {commands}
      {layers}
    />
    <div class="tf-xv-flow" aria-live="polite">
      <span>{flow}</span>
      {#if value.field !== null && keeping === null && origin !== "kept"}
        <button type="button" class="tf-btn tf-sm tf-ghost" onclick={openKeep}>Keep for this field…</button>
      {/if}
    </div>
    {#if keeping !== null}
      <form
        class="tf-psave tf-xv-keep"
        onsubmit={(event) => {
          event.preventDefault();
          void doKeep();
        }}
      >
        <label>Name <input aria-label="Name" bind:value={keeping.name} spellcheck="false" /></label>
        <label>For the field <input class="tf-pmono" aria-label="For the field" bind:value={keeping.field} spellcheck="false" /></label>
        <label>in <input class="tf-pmono tf-xv-short" aria-label="In the files" bind:value={keeping.files} spellcheck="false" /></label>
        <button type="submit" class="tf-btn tf-sm tf-primary" disabled={storing || keeping.name.trim() === "" || keeping.field.trim() === ""}>Keep</button>
        <button type="button" class="tf-btn tf-sm tf-ghost" onclick={() => (keeping = null)}>Cancel</button>
      </form>
    {/if}
    <div class="tf-vstage" data-shows={failure !== null || refused !== null ? "missing" : drawn}>
      {#if refused !== null}
        <div class="tf-vempty"><p>{refused}</p></div>
      {:else if failure !== null}
        <div class="tf-stepfail" role="alert">
          <b>Step {failure.index}, {failure.name}, could not read what it was given.</b>
          <div class="tf-sfcols">
            <div><b>What it said</b><pre>{failure.error}</pre></div>
            <div><b>Its input, {sized(failure.input.size)}</b><pre>{failure.input.head}</pre></div>
          </div>
        </div>
      {:else if out === null}
        <p class="tf-vempty tf-dim">{running ? `Running ${steps.map((s) => stepName(s, commands)).join(", ") || "the value"}…` : ""}</p>
      {:else if drawn === "image" && image !== null}
        <img src={image} alt={value.name} />
      {:else if drawn === "pdf"}
        {@const bytes = out.bytes}
        <TfPdf read={() => Promise.resolve(bytes)} size={out.size} {render} />
      {:else}
        <div class="tf-xv-inline">
          <TfLines text={shownText} path={drawn === "xml" ? "value.xml" : drawn === "json" ? "value.json" : "value.txt"} {render} find="overlay" />
        </div>
      {/if}
    </div>
  {/snippet}
</TfDialog>
