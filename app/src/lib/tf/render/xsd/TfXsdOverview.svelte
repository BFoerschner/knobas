<!--
  The page a schema opens on (the XSD design, section 13): what a document starts with, which files the schema
  reaches and how each was found, what was seen while reading, and the file's first comment, which for BMEcat and
  openTRANS is all the prose they have. A file still being read holds its row and says *reading…*; a file not found
  here is a row with the warn edge, its reason in one sentence, and what to do.
-->
<script lang="ts">
  import type { SetIndex } from "../../../render/xsd-answer";
  import { occText } from "../../../render/xsd-view";
  import TfXsdRuns from "./TfXsdRuns.svelte";
  import type { XsdState } from "./xsd.svelte";
  import { candidatesElsewhere, type Candidate } from "./xsd-point";

  let {
    st,
    index,
    ongo,
    onfile,
    onpoint = null,
    onpick = null,
    ondownload = null,
  }: {
    st: XsdState;
    index: SetIndex;
    ongo: (key: string, label: string) => void;
    onfile: (path: string) => void;
    onpoint?: ((namespace: string | null, location: string | null) => void) | null;
    /** One of the files two repositories declare, picked: kept as a pointed file (section 16). */
    onpick?: ((namespace: string | null, location: string | null, c: Candidate) => void) | null;
    /** *Download it*: the address downloaded on the press (section 17); never by itself. */
    ondownload?: ((namespace: string | null, location: string | null, address: string) => Promise<void>) | null;
  } = $props();
  /** The address being downloaded, while its press waits. */
  let downloading = $state<string | null>(null);
  async function download(namespace: string | null, location: string | null, address: string) {
    downloading = address;
    try {
      await ondownload?.(namespace, location, address);
    } finally {
      downloading = null;
    }
  }

  /** How many roots, files and lines seen the overview draws: the set's own bounds, never a whole repository. */
  const ROOTS_SHOWN = 12;
  const FILES_SHOWN = 800;
  const SEEN_SHOWN = 40;
  /** The files offered where two repositories declare a namespace: the index answers at most 50. */
  const TWO_SHOWN = 6;
  const base = (p: string) => p.slice(p.lastIndexOf("/") + 1).replace(/^@(repo\/[^/]+|download|knobas)\//, "");
  const plural = (n: number, one: string, many: string) => `${n.toLocaleString("en")} ${n === 1 ? one : many}`;
  const head = $derived((index.head ?? "").replace(/^\s*\n/, "").replace(/\s+$/, ""));
  const others = $derived(index.files.length - 1);
  const rootName = (key: string) => index.decls.find((d) => d.key === key);
  /** The files and the references not found here, in the order they were found: a landing round only appends. */
  const fileRows = $derived(
    index.listing.map((at) => (at.kind === "file" ? { kind: "file" as const, f: index.files[at.at]! } : { kind: "lost" as const, l: index.lost[at.at]! })),
  );
  /** What stands under *Files* waits for the whole set, so a row landing above it never moves it. */
  const whole = $derived(index.reading === 0);
</script>

<h2 class="tf-oas-title">{base(index.entry)}</h2>
<div class="tf-oas-meta">
  <span class="tf-oas-code">{index.home === "" ? "no namespace" : index.home}</span>
  {#if index.version !== null}<span class="tf-lab">version {index.version}</span>{/if}
  <span>XML Schema {index.v11 ? "1.1" : "1.0"}</span>
</div>
<p class="tf-oas-sum">
  {plural(index.ownElements, "element", "elements")} and {plural(index.ownTypes, "type", "types")} in this file{#if others > 0}; with the {plural(others, "file", "files")} it reaches, {plural(index.total, "declaration", "declarations")}{/if}.
</p>
{#if st.lost !== null}<p class="tf-oas-gone" role="status">This link named <code class="tf-xsd-code">{st.lost}</code>, which this version of the file does not have.</p>{/if}

{#if index.roots.length > 0}
  <section class="tf-oas-sec">
    <h3 class="tf-oas-lab"><span>A document starts with</span></h3>
    <div class="tf-xsd-roots">
      {#each index.roots.slice(0, ROOTS_SHOWN) as r (r.key)}
        {@const d = rootName(r.key)}
        <button type="button" class="tf-oas-tag" onclick={() => ongo(r.key, `${d?.prefix ?? ""}${d?.local ?? ""}`)}><span class="tf-oas-code">{d?.prefix}{d?.local}</span><span class="tf-oas-n">{r.holds > 0 ? `holds ${r.holds}` : ""}</span></button>
      {/each}
    </div>
    {#if index.under.length > 0}
      <p class="tf-oas-sub">which holds {index.underOneOf ? "one of" : ""}</p>
      <div class="tf-xsd-roots">
        {#each index.under as u, i (i)}
          <button type="button" class="tf-oas-tag" onclick={() => u.name.key !== null && ongo(u.name.key, `${u.name.prefix}${u.name.local}`)}><span class="tf-oas-code">{u.name.prefix}{u.name.local}</span><span class="tf-oas-n">{occText(u.occ)}</span></button>
        {/each}
      </div>
    {/if}
  </section>
{/if}

<section class="tf-oas-sec">
  <h3 class="tf-oas-lab"><span>Files {index.files.length}{#if index.lost.length > 0}, and {index.lost.length} not found here{/if}</span></h3>
  <div class="tf-xsd-files">
    {#each fileRows.slice(0, FILES_SHOWN) as row, i (i)}
      {#if row.kind === "file"}
        {@const f = row.f}
        <div class="tf-xsd-frow" data-d={Math.min(f.depth, 3)}>
          <span>
            <!-- A file kept outside this repository (with knobas, downloaded, another repository's) opens nowhere from here. -->
            {#if f.path.startsWith("@")}<span class="tf-oas-code" title={f.url ?? undefined}>{f.url !== null ? base(f.url) : base(f.path)}</span>{:else}<button type="button" class="tf-oas-ref" onclick={() => onfile(f.path)}>{base(f.path)}</button>{/if}
            {#if f.from !== null} <span class="tf-xsd-from">in {f.from.name}, at {f.from.branch}</span>{/if}
            {#if f.adopted} <span class="tf-xsd-from">has no namespace of its own; included, it takes this one</span>{/if}
          </span>
          <span class="tf-xsd-ns">{f.ns === "" ? "no namespace" : f.ns}{#if f.prefix !== null} <span class="tf-lab">{f.prefix}:</span>{/if}</span>
          <span class="tf-xsd-how">{#if f.reading}reading…{:else if f.bad !== null}{f.bad}{:else if f.depth === 0}{f.how}{:else}{f.how} · {f.count.toLocaleString("en")}{/if}</span>
        </div>
      {:else}
        {@const l = row.l}
        <div class="tf-xsd-frow tf-xsd-nf" data-d={Math.min(l.depth, 3)}>
          <span class="tf-oas-code">{l.location ?? "(no file named)"}</span>
          <span class="tf-xsd-ns">{l.namespace ?? ""}</span>
          <span class="tf-xsd-how">not found here</span>
          <div class="tf-xsd-why">
            <span>{l.why}</span>
            {#if l.two !== null && onpick !== null}
              <!-- Asked once: the pick is kept as a pointed file, and no repository is guessed. -->
              <span class="tf-xsd-two" role="group" aria-label="Pick the one to use">
                {#each candidatesElsewhere(l.namespace ?? "", l.two).slice(0, TWO_SHOWN) as c (`${c.repo}\n${c.path}`)}
                  <button type="button" class="tf-btn tf-sm" title="Use {c.repoName}: {c.path}, at {c.branch}" onclick={() => onpick?.(l.namespace, l.location, c)}>Use {c.repoName}: {c.path}</button>
                {/each}
              </span>
            {/if}
            {#if onpoint !== null}<button type="button" class="tf-btn tf-sm" onclick={() => onpoint?.(l.namespace, l.location)}>Point to a file…</button>{/if}
            {#if l.address !== null && ondownload !== null}
              {@const address = l.address}
              <button type="button" class="tf-btn tf-sm" title="One request to {address}, kept on this computer" disabled={downloading !== null} onclick={() => void download(l.namespace, l.location, address)}>{downloading === address ? "Downloading…" : "Download it"}</button>
            {/if}
            {#if l.address !== null}<button type="button" class="tf-btn tf-sm tf-ghost" onclick={() => void navigator.clipboard?.writeText(l.address!)}>Copy its address</button>{/if}
          </div>
        </div>
      {/if}
    {/each}
  </div>
</section>

{#if whole && index.seen.length > 0}
  <section class="tf-oas-sec">
    <h3 class="tf-oas-lab"><span>Seen while reading</span></h3>
    <ul class="tf-xsd-seen">{#each index.seen.slice(0, SEEN_SHOWN) as line, i (i)}<li><TfXsdRuns runs={line} /></li>{/each}</ul>
  </section>
{/if}

{#if whole && head !== ""}
  {@const all = st.more.has("head")}
  <section class="tf-oas-sec">
    <h3 class="tf-oas-lab"><span>From the file's first comment</span></h3>
    <pre class="tf-xsd-head" class:tf-xsd-all={all}>{head}</pre>
    {#if !all}<p class="tf-oas-sub"><button type="button" class="tf-oas-more" onclick={() => st.more.add("head")}>Show all of it</button></p>{/if}
  </section>
{/if}
