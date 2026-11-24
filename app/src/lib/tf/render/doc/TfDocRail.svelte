<!--
  The XML view's rail (its design, 2026-11-23, sections 5, 11 and 22): the head, *Findings* and *Results*, their list,
  and the keys.

  **The head holds its room from the first frame**: a verdict line and two lines under it, so the list under it never
  moves when the outline lands or the check answers (section 11). It says what the check found: *Checking…*, *Valid*
  in `--ok`, *5 departures* in `--err`, *Not checked* in `--warn` and why, *Schema not found here*, a schema at an
  address, *No schema*, or that checking is off; *Not well formed* in the error ink where the file breaks. A tone is
  for what happened: a document with no schema is not wrong, so its head has none. *Schema ▾* holds *Stop checking
  this file*, *Open the schema* and *Point to another file…*.
-->
<script lang="ts">
  import type { Snippet } from "svelte";

  import TfMenu from "../../TfMenu.svelte";
  import TfXsdRuns from "../xsd/TfXsdRuns.svelte";
  import { coded } from "./coded";
  import type { DocCheck } from "./doc-check.svelte";
  import type { DocState } from "./doc.svelte";

  let {
    st,
    mode = $bindable("results"),
    ongo,
    results = null,
    copy = null,
    check = null,
    onstop = null,
    oncheck = null,
    onschema = null,
    onpoint = null,
  }: {
    st: DocState;
    /** What the list shows. */
    mode?: "findings" | "results";
    /** Goes to a line and picks it. */
    ongo: (line: number) => void;
    /** The query's results, drawn by the query's own part (X4). */
    results?: Snippet | null;
    /** The results' *Copy ▾*, where a query answered. */
    copy?: Snippet | null;
    /** The schema check (X2); `null` where the view checks nothing. */
    check?: DocCheck | null;
    /** *Stop checking this file*, *Check it*, *Open the schema*, *Point to a file…*. */
    onstop?: (() => void) | null;
    oncheck?: (() => void) | null;
    onschema?: (() => void) | null;
    onpoint?: (() => void) | null;
  } = $props();
  const head = $derived(check?.head ?? null);
  const departures = $derived(check?.found ?? []);
  let schemaMenu = $state(false);
  let schemaButton = $state<HTMLButtonElement | null>(null);
  const NUM = new Intl.NumberFormat("en");
  /** The departures listed: the check's own bound (`FINDINGS_CAP`), the rest counted. */
  const FINDINGS_SHOWN = 500;
  const ms = (n: number) => `${NUM.format(Math.max(1, Math.round(n)))} ms`;

  const opened = $derived(st.opened);
  const broken = $derived(opened?.error ?? null);
  const root = $derived(opened?.root ?? null);
  /** The rail opens on *Findings* for a document that is not well formed: its break is the one row there. */
  let shownFor: unknown = null;
  $effect(() => {
    const o = opened;
    if (o === null || o === shownFor) return;
    shownFor = o;
    mode = o.error !== null ? "findings" : "results";
  });
  /** A check that finds departures opens the rail on them, once. */
  let findingsFor: unknown = null;
  $effect(() => {
    const h = head;
    if (h === null || h.k !== "checked" || h.count === 0 || findingsFor === check) return;
    findingsFor = check;
    mode = "findings";
  });
</script>

{#snippet said(text: string)}{#each coded(text) as part, i (i)}{#if part.code}<code class="tf-xv-code">{part.text}</code>{:else}{part.text}{/if}{/each}{/snippet}

{#snippet menu()}
  {#if onstop !== null || onschema !== null || onpoint !== null}
    <button type="button" class="tf-btn tf-sm tf-ghost tf-xv-schema" bind:this={schemaButton} aria-haspopup="menu" aria-expanded={schemaMenu} onclick={() => (schemaMenu = !schemaMenu)}>Schema ▾</button>
  {/if}
{/snippet}

<div class="tf-oas-rail tf-xv-rail">
  <div class="tf-xv-head" aria-live="polite">
    {#if opened === null}
      <div class="tf-xv-verdict"><span class="tf-dim">Reading…</span></div>
      <p>&nbsp;</p>
      <p>&nbsp;</p>
    {:else if broken !== null}
      <div class="tf-xv-verdict tf-xv-bad"><b>Not well formed</b></div>
      <p>
        <button type="button" class="tf-oas-ref" onclick={() => ongo(broken.line)}>At line {broken.line.toLocaleString("en")}</button>, {@render said(broken.message.charAt(0).toLowerCase() + broken.message.slice(1))}
      </p>
      <p>&nbsp;</p>
    {:else if head !== null && head.k !== "idle"}
      {#if head.k === "looking"}
        <div class="tf-xv-verdict"><span class="tf-dim">Checking…</span></div>
        <p>{#if head.found !== null}Against <code class="tf-xv-code">{head.found}</code>, found {head.how}.{:else}Looking for its schema.{/if}</p>
        <p>&nbsp;</p>
      {:else if head.k === "checked"}
        <div class="tf-xv-verdict" class:tf-xv-ok={head.count === 0} class:tf-xv-bad={head.count > 0}>
          <b>{head.count === 0 ? "Valid" : `${NUM.format(head.total)} ${head.total === 1 ? "departure" : "departures"}`}</b>
          {@render menu()}
        </div>
        <p>Against {#if onschema !== null}<button type="button" class="tf-oas-ref" title="Open the schema" onclick={() => onschema?.()}>{head.against}</button>{:else}<code class="tf-xv-code">{head.against}</code>{/if}, found {head.how}.</p>
        <p>Checked by libxml2 in {ms(head.ms)}; the schema's {head.files} {head.files === 1 ? "file" : "files"} read in {ms(head.readMs)}.</p>
      {:else if head.k === "not-checked"}
        <div class="tf-xv-verdict"><b class={head.warn ? "tf-xv-warn" : "tf-xv-quiet"}>Not checked</b>{@render menu()}</div>
        <p>{#if head.against !== null}Against <code class="tf-xv-code">{head.against}</code>: {/if}{@render said(head.lines[0] ?? "")}</p>
        <p>{#if head.lines.length > 1}{@render said(head.lines[1]!)}{:else}&nbsp;{/if}</p>
      {:else if head.k === "not-found"}
        <div class="tf-xv-verdict"><b class="tf-xv-warn">Schema not found here</b></div>
        <p>It names <code class="tf-xv-code">{head.location}</code>, and no file is there.</p>
        <p>{#if onpoint !== null}<button type="button" class="tf-oas-ref" onclick={() => onpoint?.()}>Point to a file…</button>{:else}&nbsp;{/if}</p>
      {:else if head.k === "address"}
        <div class="tf-xv-verdict"><b class="tf-xv-quiet">Not checked</b></div>
        <p>Its schema is at an address, <code class="tf-xv-code">{head.address}</code>. knobas fetches nothing by itself.</p>
        <p>{#if onpoint !== null}<button type="button" class="tf-oas-ref" onclick={() => onpoint?.()}>Point to a file…</button>{:else}&nbsp;{/if}</p>
      {:else if head.k === "two"}
        <div class="tf-xv-verdict"><b class="tf-xv-quiet">Not checked</b></div>
        <p>Two repositories declare <code class="tf-xv-code">{head.namespace}</code>.</p>
        <p>{#if onpoint !== null}<button type="button" class="tf-oas-ref" onclick={() => onpoint?.()}>Point to a file…</button>{:else}&nbsp;{/if}</p>
      {:else if head.k === "no-schema"}
        <div class="tf-xv-verdict"><b class="tf-xv-quiet">No schema</b></div>
        <p>{#if head.namespace !== null}Nothing in a synced repository declares <code class="tf-xv-code">{head.namespace}</code>.{:else}&nbsp;{/if}</p>
        <p>&nbsp;</p>
      {:else}
        <div class="tf-xv-verdict"><b class="tf-xv-quiet">Not checked</b></div>
        <p>Checking is off for this file.</p>
        <p>{#if oncheck !== null}<button type="button" class="tf-oas-ref" onclick={() => oncheck?.()}>Check it</button>{:else}&nbsp;{/if}</p>
      {/if}
    {:else if root !== null && root.location !== null}
      <div class="tf-xv-verdict"><b class="tf-xv-quiet">Not checked</b></div>
      <p>It names <code class="tf-xv-code">{root.location}</code>.</p>
      <p>{check === null ? "knobas does not check a document against a schema here." : " "}</p>
    {:else if root !== null && root.namespace !== ""}
      <div class="tf-xv-verdict"><b class="tf-xv-quiet">Not checked</b></div>
      <p>Its namespace is <code class="tf-xv-code">{root.namespace}</code>.</p>
      <p>{check === null ? "knobas does not check a document against a schema here." : " "}</p>
    {:else}
      <div class="tf-xv-verdict"><b class="tf-xv-quiet">No schema</b></div>
      <p>&nbsp;</p>
      <p>&nbsp;</p>
    {/if}
  </div>
  <div class="tf-oas-modes">
    <span class="tf-seg" role="group" aria-label="The list shows">
      <button type="button" aria-pressed={mode === "findings"} onclick={() => (mode = "findings")}>Findings{" "}<span class="tf-xv-fcount">{#if broken !== null}1{:else if departures.length > 0}{NUM.format(head?.k === "checked" ? head.total : departures.length)}{/if}</span></button>
      <button type="button" aria-pressed={mode === "results"} onclick={() => (mode = "results")}>Results</button>
    </span>
    {#if mode === "results" && copy !== null}{@render copy()}{/if}
  </div>
  <div class="tf-oas-list tf-xv-list" role="listbox" aria-label={mode === "findings" ? "Findings" : "Results"}>
    {#if mode === "findings"}
      {#if broken !== null}
        <button type="button" class="tf-oas-row tf-xv-frow" role="option" aria-selected={st.cursor === broken.line} onclick={() => ongo(broken.line)}>
          <span class="tf-xv-fline">{broken.line.toLocaleString("en")}</span>
          <span class="tf-xv-ftext"><span class="tf-xv-fname">Not well formed</span><span class="tf-xv-fsay">{@render said(broken.message)}</span></span>
        </button>
      {:else if departures.length > 0}
        {#each departures.slice(0, FINDINGS_SHOWN) as d, i (i)}
          <button type="button" class="tf-oas-row tf-xv-frow" role="option" aria-selected={st.cursor === d.line} title={d.raw} onclick={() => ongo(d.line)}>
            <span class="tf-xv-fline">{d.line.toLocaleString("en")}</span>
            <span class="tf-xv-ftext"><span class="tf-xv-fname">{d.words.attr !== null ? `@${d.words.attr}` : d.words.at}</span><span class="tf-xv-fsay"><TfXsdRuns runs={d.words.text} cls="tf-xv-code" /></span></span>
          </button>
        {/each}
        {#if head?.k === "checked" && head.total > departures.length}<p class="tf-oas-none">And {NUM.format(head.total - departures.length)} more, not drawn.</p>{/if}
      {:else if head?.k === "checked"}
        <p class="tf-oas-none">Nothing departs from the schema.</p>
      {:else if opened !== null}
        <p class="tf-oas-none">Nothing was checked.</p>
      {/if}
    {:else if results !== null}
      {@render results()}
    {/if}
  </div>
  <div class="tf-oas-keys">
    <span><kbd>Q</kbd> query</span><span><kbd>N</kbd><kbd>P</kbd> step</span><span><kbd>←</kbd><kbd>→</kbd> fold</span><span><kbd>Space</kbd> look</span><span><kbd>⌘F</kbd> find</span>
  </div>
</div>

{#if schemaMenu && schemaButton !== null}
  <TfMenu bind:open={schemaMenu} opener={schemaButton} label="The schema" className="tf-pmenu-m">
    {#if onstop !== null}<button type="button" role="menuitem" onclick={() => ((schemaMenu = false), onstop?.())}>Stop checking this file</button>{/if}
    {#if onschema !== null}<button type="button" role="menuitem" onclick={() => ((schemaMenu = false), onschema?.())}>Open the schema</button>{/if}
    {#if onpoint !== null}<button type="button" role="menuitem" onclick={() => ((schemaMenu = false), onpoint?.())}>Point to another file…</button>{/if}
  </TfMenu>
{/if}
