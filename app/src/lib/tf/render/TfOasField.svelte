<!--
  One field row of the API explorer (ux31, section 8): a parameter, a header or a property. Its name, its type
  (a ref's name pressed goes to the schema's page), where it goes and whether it is required; under it its words,
  its line of facts, and *read only*, *write only* or *Deprecated*. The caret opens what it holds in place, one
  indent in under a hairline guide (`TfOasShape`), once the worker answered its shape, so it opens whole. A ref to
  a schema the row sits inside does not open again: it says so, and its name follows.
-->
<script lang="ts">
  import type { Snippet } from "svelte";

  import { factsOf, hasInside, refName, summary } from "../../render/openapi-view";
  import TfOasShape from "./TfOasShape.svelte";
  import TfOasWords from "./TfOasWords.svelte";
  import { innerRef, type OasState } from "./oas.svelte";

  interface Props {
    st: OasState;
    name: string;
    node: unknown;
    base: string;
    /** What its caret and *Show all* are kept by. */
    k: string;
    required?: boolean;
    where?: string;
    words?: string;
    example?: unknown;
    deprecated?: boolean;
    /** The refs the row sits inside: one of them opening again would never end. */
    trail?: readonly string[];
    prose?: Snippet<[string]> | null;
    /** Goes to a named schema's page. */
    onfollow: (name: string) => void;
  }

  let {
    st,
    name,
    node,
    base,
    k,
    required = false,
    where = "",
    words = "",
    example = undefined,
    deprecated = false,
    trail = [],
    prose = null,
    onfollow,
  }: Props = $props();

  type Obj = Record<string, unknown>;
  const own = $derived((typeof node === "object" && node !== null && !Array.isArray(node) ? node : {}) as Obj);
  const line = $derived(summary(node));
  const inner = $derived(innerRef(node));
  const cycle = $derived(inner !== null && trail.includes(`${base}#${inner}`));
  const can = $derived(!cycle && hasInside(node));
  const open = $derived(can && st.expand.has(k));
  const shape = $derived(open ? st.shapes.get(k) : undefined);
  // A ref's facts are its schema's, drawn when it opens; a plain node's are its own.
  const facts = $derived(factsOf(inner === null ? node : null, example));
  const named = $derived(line.ref === null ? null : st.named(base, line.ref));
  const away = $derived(line.ref === null || named !== null ? null : st.hrefOf(base, line.ref));
  const label = $derived(line.ref === null ? "" : refName(line.ref));
</script>

<div class="tf-oas-f">
  {#if can}
    <button type="button" class="tf-oas-tog" aria-expanded={open} aria-label="{open ? 'Close' : 'Open'} {name}" onclick={() => st.toggleShape(k, base, node)}>
      <svg viewBox="0 0 10 10" width="10" height="10" fill="none" stroke="currentColor" stroke-width="1.6" aria-hidden="true"><path d="M3 2l4 3-4 3" /></svg>
    </button>
  {:else}<span></span>{/if}
  <div class="tf-oas-fl">
    <span class="tf-oas-fname" class:tf-oas-dep={deprecated || own.deprecated === true}>{name}</span>
    <span class="tf-oas-ftype"
      >{line.type}{#if line.ref !== null}{line.type !== "" ? " " : ""}{#if named !== null}<button type="button" class="tf-oas-ref" title="Open {named}" onclick={() => onfollow(named)}>{label}</button
        >{:else if away !== null}<a class="tf-oas-ref" href={away}>{label}</a>{:else}<span class="tf-oas-fname">{label}</span>{/if}{/if}{#if line.nullable}, or null{/if}</span
    >
    {#if where !== ""}<span class="tf-oas-fwhere">{where}</span>{/if}
    {#if required}<span class="tf-lab">required</span>{/if}
    {#if own.readOnly === true}<span class="tf-oas-fwhere">read only</span>{/if}
    {#if own.writeOnly === true}<span class="tf-oas-fwhere">write only</span>{/if}
    {#if deprecated || own.deprecated === true}<span class="tf-oas-fwhere tf-oas-warn">Deprecated</span>{/if}
  </div>
  {#if words !== ""}<div class="tf-oas-fd"><TfOasWords text={words} {st} {k} max={420} {prose} /></div>{/if}
  {#if facts.length > 0}
    <div class="tf-oas-fd">{#each facts as fact, n (n)}{#if n > 0}. {/if}{#each fact as run, m (m)}{#if run.code}<code>{run.text}</code>{:else}{run.text}{/if}{/each}{/each}</div>
  {/if}
  {#if cycle && inner !== null}
    <div class="tf-oas-fd">It holds itself: follow {#if named !== null}<button type="button" class="tf-oas-ref" onclick={() => onfollow(named)}>{label}</button>{:else}<b>{refName(inner)}</b>{/if} to read it.</div>
  {/if}
  {#if open}
    <div class="tf-oas-in">
      {#if shape !== undefined}
        <TfOasShape {st} {shape} {k} trail={inner === null ? trail : [...trail, `${base}#${inner}`]} {prose} {onfollow} />
      {:else}
        {@const got = inner === null ? undefined : st.ref(base, inner)}
        {#if got !== undefined && "reading" in got}<p class="tf-oas-fd" role="status">Reading {got.reading}…</p>
        {:else if got !== undefined && "error" in got}<p class="tf-oas-fd">{got.error}</p>{/if}
      {/if}
    </div>
  {/if}
</div>
