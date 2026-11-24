<!--
  The schema explorer's page column (the XSD design, sections 9 to 13): the overview, or a declaration's page as the
  worker answered it, swapped whole with the column at its top. One page shape for six kinds: its name and what it is,
  where it lives, the line it derives along, its documentation, its value, what it holds, what may stand in for it or
  derives from it, and who uses it. A followed name puts *Back to ⟨where you were⟩* at its top.
-->
<script lang="ts">
  import type { Snippet } from "svelte";

  import type { Docu } from "../../../render/xsd";
  import type { UseRow } from "../../../render/xsd-answer";
  import TfXsdName from "./TfXsdName.svelte";
  import TfXsdOverview from "./TfXsdOverview.svelte";
  import TfXsdRows from "./TfXsdRows.svelte";
  import TfXsdRuns from "./TfXsdRuns.svelte";
  import TfXsdType from "./TfXsdType.svelte";
  import type { XsdState } from "./xsd.svelte";
  import type { Candidate } from "./xsd-point";

  let {
    st,
    ongo,
    onback,
    onfile,
    onsource = null,
    oncopylink = null,
    onpoint = null,
    onpick = null,
    ondownload = null,
    head = null,
    body = null,
    scroller = $bindable(null),
  }: {
    st: XsdState;
    ongo: (key: string, label: string) => void;
    onback: () => void;
    /** Opens a file of the set as a file of its own. */
    onfile: (path: string) => void;
    /** Shows a file as Source at a line. */
    onsource?: ((file: string, line: number) => void) | null;
    oncopylink?: ((place: string, label: string) => void) | null;
    /** *Point to a file…* on a reference not found here. */
    onpoint?: ((namespace: string | null, location: string | null) => void) | null;
    /** One of the files two repositories declare, picked for a reference (section 16). */
    onpick?: ((namespace: string | null, location: string | null, c: Candidate) => void) | null;
    /** *Download it* on a reference that names an address (section 17). */
    ondownload?: ((namespace: string | null, location: string | null, address: string) => Promise<void>) | null;
    /** What stands at the head's end under 1,480 px (S4's *Reference* or *Example*). */
    head?: Snippet | null;
    /** What stands under the head in place of the reference: the example, under 1,480 px. */
    body?: Snippet | null;
    scroller?: HTMLElement | null;
  } = $props();

  const page = $derived(st.page);
  const base = (p: string) => p.slice(p.lastIndexOf("/") + 1);
  let allUses = $state<UseRow[] | null>(null);
  $effect(() => {
    void page;
    allUses = null;
  });
  const usesShown = $derived(page === null ? [] : (allUses ?? page.uses));

  /**
   * A page draws a screenful of its rows first and the rest 12 a frame (section 9, as the API explorer's schema page
   * does), what stands under them once they are all in, so nothing drawn moves. A page gone back to at a scroll draws
   * whole, so its scroll is there to go back to.
   */
  const FIRST = 18;
  /** The rest of a type's values, of those who use a declaration, and of who may stand in for it: at most this many. */
  const VALUES_ALL = 5000;
  const USES_ALL = 5000;
  const NAMES_SHOWN = 24;
  let limit = $state(FIRST);
  $effect(() => {
    const at = page;
    if (at === null) return;
    limit = st.drawWhole ? Infinity : FIRST;
    let frame = 0;
    const more = () => {
      if (limit >= at.rows.length) return;
      limit += 12;
      frame = requestAnimationFrame(more);
    };
    frame = requestAnimationFrame(more);
    return () => cancelAnimationFrame(frame);
  });
  const allIn = $derived(page === null || limit >= page.rows.length);

  const paragraphs = (d: Docu) => d.text.split(/\n\n/).filter((p) => p.trim() !== "");
  const factName = (k: string) => k.replace(/([a-z])([A-Z])/g, "$1 $2").toLowerCase();
</script>

<div class="tf-oas-col tf-oas-page" bind:this={scroller} tabindex="-1" aria-label="Reference">
  <div class="tf-oas-measure">
    {#if st.backs.length > 0}
      <div class="tf-oas-back"><button type="button" class="tf-btn tf-sm" onclick={onback}>Back to {st.backs.at(-1)?.label}</button></div>
    {/if}
    {#if page === null}
      {#if st.index !== null}<TfXsdOverview {st} index={st.index} {ongo} {onfile} {onpoint} {onpick} {ondownload} />{/if}
    {:else}
      {@const p = page}
      <h2 class="tf-xsd-h">
        <span class="tf-xsd-hname">{#if p.name.prefix !== ""}<span class="tf-xsd-pre">{p.name.prefix}</span>{/if}{p.name.local}</span>
        <span class="tf-xsd-what">{p.what}</span>
        {#if head !== null}{@render head()}{/if}
      </h2>
      <div class="tf-oas-meta">
        <span>in <span class="tf-oas-code">{base(p.file)}</span>{#if p.reached}, which this file reaches{/if}</span>
        <span>line {p.line.toLocaleString("en")}</span>
        {#if p.over !== null}<span class="tf-oas-warn">{p.over}</span>{/if}
        {#if oncopylink !== null}<button type="button" class="tf-oas-more" onclick={() => oncopylink?.(p.place, `${p.name.prefix}${p.name.local}`)}>Copy link</button>{/if}
        {#if onsource !== null}<button type="button" class="tf-oas-more" onclick={() => onsource?.(p.file, p.line)}>Show in Source</button>{/if}
      </div>
      {#if body !== null}
        {@render body()}
      {:else}
      {#if p.type !== null || p.chain.length > 0}
        <div class="tf-xsd-chain">
          {#if p.type !== null}<span>{#if p.type.name === null && p.type.missing === null}A type of its own{:else}Type <TfXsdType type={p.type} {ongo} />{/if}</span>{/if}
          {#each p.chain as step, i (i)}<span class="tf-xsd-step">{step.word}</span>{#if step.name !== null}<TfXsdName name={step.name} {ongo} />{:else}<span class="tf-oas-code">{step.text}</span>{/if}{/each}
        </div>
      {/if}
      {#each p.docu as d, i (i)}
        {#if d.text !== ""}<div class="tf-xsd-words">{#if d.lang !== null}<span class="tf-lab tf-xsd-lang">{d.lang}</span>{/if}{#each paragraphs(d) as para, n (n)}<p>{para}</p>{/each}</div>{/if}
        {#if d.facts.length > 0}<dl class="tf-xsd-facts2">{#each d.facts as [k, v], n (n)}<dt>{factName(k)}</dt><dd>{v}</dd>{/each}</dl>{/if}
      {/each}
      {#if p.value !== null}
        {@const v = p.value}
        <section class="tf-oas-sec">
          <h3 class="tf-oas-lab"><span>Its value</span></h3>
          <dl class="tf-xsd-kv">
            {#if v.held.length > 0}<dt>Held to</dt><dd><span class="tf-oas-facts">{#each v.held as fact, i (i)}<span><TfXsdRuns runs={fact} /></span>{/each}</span></dd>{/if}
            {#if v.always !== null}<dt>Always</dt><dd><code class="tf-xsd-code">{v.always}</code></dd>{/if}
            {#if v.fallback !== null}<dt>Default</dt><dd><code class="tf-xsd-code">{v.fallback}</code></dd>{/if}
            {#if v.valueCount > 0}
              {@const all = st.more.has(`vals:${p.key}`)}
              <dt>One of {v.valueCount.toLocaleString("en")}</dt>
              <dd>
                <div class="tf-xsd-vals">
                  {#each v.values as e (e.v)}<code title={e.title}>{e.v}</code>{/each}
                  {#if all}{#await st.port.values(p.key, v.values.length) then rest}{#each rest.slice(0, VALUES_ALL) as e (e.v)}<code title={e.title}>{e.v}</code>{/each}{/await}{/if}
                  {#if v.valueCount > v.values.length && !all}<button type="button" class="tf-oas-more" onclick={() => st.more.add(`vals:${p.key}`)}>Show the other {(v.valueCount - v.values.length).toLocaleString("en")}</button>{/if}
                </div>
                {#if v.valuesFrom !== null}<span class="tf-xsd-from">from <TfXsdName name={v.valuesFrom} {ongo} /></span>{/if}
              </dd>
            {/if}
            {#each v.patterns as pat, i (i)}
              <dt>Pattern</dt>
              <dd><code class="tf-xsd-pat">{pat.text}</code>{#if pat.unread !== null}<span class="tf-xsd-lost">knobas does not read this pattern: {pat.unread}</span>{/if}</dd>
            {/each}
            {#if v.union.length > 0}<dt>One of the types</dt><dd>{#each v.union as m, i (i)}{#if i > 0}, {/if}<TfXsdName name={m} {ongo} />{/each}</dd>{/if}
            {#if v.list !== null}<dt>A list of</dt><dd><TfXsdName name={v.list} {ongo} /></dd>{/if}
            {#if v.held.length === 0 && v.always === null && v.fallback === null && v.valueCount === 0 && v.patterns.length === 0 && v.union.length === 0 && v.list === null}
              <dt>Held to</dt><dd>nothing more than <span class="tf-oas-code">{v.builtin ?? "its type"}</span></dd>
            {/if}
          </dl>
        </section>
      {/if}
      {#if p.rows.length > 0}
        <section class="tf-oas-sec">
          <h3 class="tf-oas-lab"><span>{p.holdsLabel}</span>{#if p.expandable}<span class="tf-trail"><button type="button" class="tf-btn tf-sm tf-ghost" onclick={() => void st.openAll(p.rows)}>Expand all</button></span>{/if}</h3>
          <div class="tf-oas-fs"><TfXsdRows {st} at={p.key} rows={p.rows.length > limit ? p.rows.slice(0, limit) : p.rows} {ongo} /></div>
        </section>
      {:else if p.empty}
        <section class="tf-oas-sec"><h3 class="tf-oas-lab"><span>Holds</span></h3><p class="tf-oas-fnote">Nothing: it stands empty.</p></section>
      {/if}
      {#if allIn && p.standIns.length > 0}
        <section class="tf-oas-sec">
          <h3 class="tf-oas-lab"><span>May be stood in for by</span></h3>
          <div class="tf-oas-used">{#each p.standIns.slice(0, NAMES_SHOWN) as m, i (i)}<button type="button" class="tf-oas-usedrow" onclick={() => m.key !== null && ongo(m.key, `${m.prefix}${m.local}`)}><span></span><span class="tf-oas-code"><span class="tf-xsd-pre">{m.prefix}</span>{m.local}</span><span></span></button>{/each}</div>
        </section>
      {/if}
      {#if allIn && p.rules.length > 0}
        <section class="tf-oas-sec"><h3 class="tf-oas-lab"><span>Rules across its content</span></h3><p class="tf-oas-fnote">{p.rules.join(", ")}</p></section>
      {/if}
      {#if allIn && p.derivedCount > 0}
        <section class="tf-oas-sec">
          <h3 class="tf-oas-lab"><span>Narrowed or extended by {p.derivedCount.toLocaleString("en")}</span></h3>
          <div class="tf-oas-used">{#each p.derived as m, i (i)}<button type="button" class="tf-oas-usedrow" onclick={() => m.key !== null && ongo(m.key, `${m.prefix}${m.local}`)}><span></span><span class="tf-oas-code"><span class="tf-xsd-pre">{m.prefix}</span>{m.local}</span><span></span></button>{/each}</div>
        </section>
      {/if}
      {#if allIn && p.useCount > 0}
        <section class="tf-oas-sec">
          <h3 class="tf-oas-lab"><span>Used by {p.useCount.toLocaleString("en")}</span></h3>
          <div class="tf-oas-used">
            {#each usesShown.slice(0, USES_ALL) as u, i (i)}
              <button type="button" class="tf-oas-usedrow" onclick={() => u.name.key !== null && ongo(u.name.key, `${u.name.prefix}${u.name.local}`)}
                ><span class="tf-oas-fwhere">{u.kind}</span><span class="tf-oas-code"><span class="tf-xsd-pre">{u.name.prefix}</span>{u.name.local}</span><span class="tf-oas-fwhere">{u.how}{#if u.file !== null}, in {u.file}{/if}</span></button
              >
            {/each}
          </div>
          {#if p.useCount > usesShown.length}<p class="tf-oas-sub"><button type="button" class="tf-oas-more" onclick={() => void st.port.uses(p.key).then((all) => (allUses = all))}>Show the other {(p.useCount - usesShown.length).toLocaleString("en")}</button></p>{/if}
        </section>
      {:else if allIn && !p.key.startsWith("el ")}
        <section class="tf-oas-sec"><h3 class="tf-oas-lab"><span>Used by</span></h3><p class="tf-oas-fnote">Nothing in these files names it.</p></section>
      {/if}
      {/if}
    {/if}
  </div>
</div>
