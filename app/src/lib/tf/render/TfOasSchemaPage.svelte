<!--
  A schema's own page in the API explorer (ux31, section 8): its name, *Schema* and its type, its words, its
  properties merged, *An example, made from the schema*, then *Used by* (the operations that name it, each a row
  that goes there) and *Part of* (the schemas that hold it). Direct uses only: a transitive list would name most
  of the API for a schema like `simple-user`.
-->
<script lang="ts">
  import type { Snippet } from "svelte";

  import type { Shape, Uses } from "../../render/openapi";
  import { exampleText, factsOf, pathParts, summary } from "../../render/openapi-view";
  import TfOasShape from "./TfOasShape.svelte";
  import TfOasWords from "./TfOasWords.svelte";
  import type { OasState, Place } from "./oas.svelte";

  let {
    st,
    name,
    node,
    shape,
    uses,
    sample,
    given = false,
    prose = null,
    onfollow,
    ongo,
    oncopylink = null,
  }: {
    st: OasState;
    name: string;
    node: unknown;
    shape: Shape;
    uses: Uses;
    sample: unknown;
    /** The example is the schema's own, not made from it. */
    given?: boolean;
    prose?: Snippet<[string]> | null;
    onfollow: (name: string) => void;
    ongo: (place: Place, label: string) => void;
    oncopylink?: (() => void) | null;
  } = $props();

  /** How many *Used by* rows draw before *Show the other*, and *Part of* names before the rest are counted. */
  const USED = 12;
  const PART = 40;

  const k = $derived(`schemas/${name}`);
  const type = $derived(summary(node).type || "object");
  const ops = $derived(uses.ops.map((i) => st.index?.ops[i]).filter((o) => o !== undefined));
  const used = $derived(st.expand.has(`${k}#uses`) ? ops : ops.slice(0, USED));
  const facts = $derived(factsOf(shape.facts));
  const trail = $derived([`${shape.base}#${st.index?.schemaBase ?? ""}${name}`]);
  const hasBody = $derived(shape.props.length > 0 || shape.variants !== null || shape.items !== undefined || shape.extra !== undefined);
</script>

<h2 class="tf-oas-h"><span class="tf-oas-path">{name}</span></h2>
<div class="tf-oas-meta">
  <span>Schema</span>
  <span>{type}</span>
  {#if oncopylink !== null}<span class="tf-trail"><button type="button" class="tf-btn tf-sm tf-ghost" onclick={() => oncopylink?.()}>Copy link</button></span>{/if}
</div>
{#if shape.description !== ""}<div class="tf-oas-desc"><TfOasWords text={shape.description} {st} k="{k}#desc" max={700} {prose} block /></div>{/if}
{#if facts.length > 0}
  <div class="tf-oas-fd">{#each facts as fact, n (n)}{#if n > 0}. {/if}{#each fact as run, m (m)}{#if run.code}<code>{run.text}</code>{:else}{run.text}{/if}{/each}{/each}</div>
{/if}
{#if hasBody}
  <section class="tf-oas-sec">
    <h3 class="tf-oas-lab">Properties{#if shape.props.length > 0}<span class="tf-oas-n">{shape.props.length}</span>{/if}</h3>
    <TfOasShape {st} {shape} {k} {trail} {prose} {onfollow} />
  </section>
{/if}
{#if sample !== null && typeof sample === "object"}
  <section class="tf-oas-sec">
    <h3 class="tf-oas-lab">Example<span class="tf-oas-exfrom">{given ? "From the spec" : "Made from the schema"}</span></h3>
    <pre class="tf-oas-ex">{exampleText(sample)}</pre>
  </section>
{/if}
{#if ops.length > 0}
  <section class="tf-oas-sec">
    <h3 class="tf-oas-lab">Used by <span class="tf-oas-n">{ops.length}</span></h3>
    <div class="tf-oas-used">
      {#each used as op (op.i)}
        <button type="button" class="tf-oas-usedrow" onclick={() => ongo(op.anchor, `${op.method.toUpperCase()} ${op.path}`)}>
          <span class="tf-oas-m">{op.method.toUpperCase()}</span>
          <span class="tf-oas-rpath">{#each pathParts(op.path) as part, n (n)}{#if part.param}<span class="tf-oas-pp">{part.text}</span>{:else}{part.text}{/if}{/each}</span>
          <span class="tf-oas-rsum">{op.summary}</span>
        </button>
      {/each}
    </div>
    {#if ops.length > used.length}<p class="tf-oas-fd"><button type="button" class="tf-oas-more" onclick={() => st.expand.add(`${k}#uses`)}>Show the other {ops.length - used.length}</button></p>{/if}
  </section>
{/if}
{#if uses.schemas.length > 0}
  <section class="tf-oas-sec">
    <h3 class="tf-oas-lab">Part of <span class="tf-oas-n">{uses.schemas.length}</span></h3>
    <div class="tf-oas-fd">
      {#each uses.schemas.slice(0, PART) as holder, n (holder)}{#if n > 0}{" "}{/if}<button type="button" class="tf-oas-ref" onclick={() => onfollow(holder)}>{holder}</button>{/each}{#if uses.schemas.length > PART}{" "}and {uses.schemas.length - PART} more{/if}
    </div>
  </section>
{/if}
{#if ops.length === 0 && uses.schemas.length === 0}
  <section class="tf-oas-sec">
    <h3 class="tf-oas-lab">Used by</h3>
    <div class="tf-oas-fd">Nothing in this file names it.</div>
  </section>
{/if}
