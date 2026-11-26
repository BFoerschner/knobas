<!--
  A schema's own page in the API explorer (ux31, section 8): its name, *Schema* and its type, its words, its
  properties merged, *An example, made from the schema*, then *Used by* (the operations that name it, each a row
  that goes there) and *Part of* (the schemas that hold it). Direct uses only: a transitive list would name most
  of the API for a schema like `simple-user`.

  **The first frame draws a screenful** (the ux30 end gate: TeamCity's `build`, 86 properties, took 82 ms at 4x to
  draw whole on a followed ref): its head and the properties that fill the column (`firstRows`), the rest a step a
  frame after it, below what is drawn, the few that could widen a column standing unseen until then (`TfOasShape`'s
  `upTo`). The example, *Used by* and *Part of* come with the last property. A page gone back to at a scroll draws
  whole at once (`OasState.drawWhole`), so its scroll is where it was. The inline form draws it whole (`screenful`).
-->
<script lang="ts">
  import { untrack, type Snippet } from "svelte";

  import type { Shape, Uses } from "../../render/openapi";
  import { exampleText, factsOf, hasInside, pathParts, summary } from "../../render/openapi-view";
  import TfOasShape from "./TfOasShape.svelte";
  import TfOasWords from "./TfOasWords.svelte";
  import { firstRows, HEAD_PX, SHAPE_PROPS, STEP_ROWS, type OasState, type Place } from "./oas.svelte";

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
    screenful = false,
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
    /**
     * Draws a screenful first and the rest after it: the explorer's page column, whose page ends at the column's
     * foot. Not the inline form, where rows stand under the page and would move as it grew.
     */
    screenful?: boolean;
  } = $props();

  /** How many *Used by* rows draw before *Show the other*, and *Part of* chips draw before *Show all*. */
  const USED = 12;
  const PART = 12;

  const k = $derived(`schemas/${name}`);
  const type = $derived(summary(node).type || "object");
  const ops = $derived(uses.ops.map((i) => st.index?.ops[i]).filter((o) => o !== undefined));
  const used = $derived(st.expand.has(`${k}#uses`) ? ops : ops.slice(0, USED));
  const partAll = $derived(st.expand.has(`${k}#part`));
  const parts = $derived(partAll ? uses.schemas : uses.schemas.slice(0, PART));
  function togglePart() {
    if (partAll) st.expand.delete(`${k}#part`);
    else st.expand.add(`${k}#part`);
  }
  const facts = $derived(factsOf(shape.facts));
  const trail = $derived([`${shape.base}#${st.index?.schemaBase ?? ""}${name}`]);
  const opens = $derived(shape.props.some((p) => hasInside(p.node)) || shape.variants !== null || shape.items !== undefined);
  const anyOpen = $derived([...st.expand].some((x) => x.startsWith(`${k}/`) || x.startsWith(`${k}|`)));
  function flipAll() {
    if (anyOpen) st.closeAll(k);
    else void st.expandShape(k, shape, trail);
  }
  const hasBody = $derived(shape.props.length > 0 || shape.variants !== null || shape.items !== undefined || shape.extra !== undefined);

  // ---- A screenful first, the rest a step a frame (the ux30 end gate) ----
  // Once every property is drawn the page stays whole: *Show the other* then draws the rest in its own press.
  const wholeFrom = (n: number) => (n >= Math.min(shape.props.length, SHAPE_PROPS) ? Infinity : n);
  /** The properties drawn so far; the page is keyed by its place, so this starts again on each. */
  let upTo = $state(untrack(() => (!screenful || st.drawWhole ? Infinity : wholeFrom(firstRows(shape.props, (st.paneTall || window.innerHeight) - HEAD_PX)))));
  const total = $derived(Math.min(shape.props.length, st.expand.has(`${k}#props`) ? Infinity : SHAPE_PROPS));
  const whole = $derived(upTo >= total);
  $effect(() => {
    const drawn = upTo;
    if (drawn >= total) return;
    // The frame after the first draws the next step, and each after it: a frame drawn, a step taken.
    let id = requestAnimationFrame(() => (id = requestAnimationFrame(() => (upTo = wholeFrom(drawn + STEP_ROWS)))));
    return () => cancelAnimationFrame(id);
  });
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
    <h3 class="tf-oas-lab">
      Properties{#if shape.props.length > 0}<span class="tf-oas-n">{shape.props.length}</span>{/if}
      {#if opens}<span class="tf-trail"><button type="button" class="tf-btn tf-sm tf-ghost" onclick={flipAll}>{anyOpen ? "Collapse all" : "Expand all"}</button></span>{/if}
    </h3>
    <TfOasShape {st} {shape} {k} {trail} {prose} {onfollow} {upTo} />
  </section>
{/if}
<!-- The example, *Used by* and *Part of* come with the last property, so nothing drawn above them moves. -->
{#if whole}
{#if sample !== null && typeof sample === "object"}
  <section class="tf-oas-sec">
    <h3 class="tf-oas-lab">Example <span class="tf-oas-n">{given ? "From the spec" : "Made from the schema"}</span></h3>
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
    <div class="tf-oas-chips">
      {#each parts as holder (holder)}<span class="tf-chip tf-oas-part"><button type="button" class="tf-chipbtn" onclick={() => onfollow(holder)}>{holder}</button></span>{/each}
    </div>
    {#if uses.schemas.length > PART}
      <p class="tf-oas-fd"><button type="button" class="tf-oas-more" onclick={togglePart}>{partAll ? "Show fewer" : "Show all"}</button></p>
    {/if}
  </section>
{/if}
{#if ops.length === 0 && uses.schemas.length === 0}
  <section class="tf-oas-sec">
    <h3 class="tf-oas-lab">Used by</h3>
    <div class="tf-oas-fd">Nothing in this file names it.</div>
  </section>
{/if}
{/if}
