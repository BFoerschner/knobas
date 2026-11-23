<!--
  What a schema holds, under its line (ux31, section 8): its properties merged (`allOf`), the choice it offers as
  a `.tf-seg` of the choices by name with the picked one drawn under it, what each item of an array is, and any
  other key. A schema of more than 80 properties draws 80 and *Show the other N*; a choice draws 12 and counts
  the rest.

  **`upTo`** (a schema's page, the ux30 end gate): only the first `upTo` properties are drawn whole. Of the ones after,
  each that can widen a column (`columnHolders`: the longest name, each set of notes, a *required*) stands as a row of
  no height, unseen, holding only its name, its notes and *required*, the cells that size the list's columns (the type
  and the words take what is left, so they size none). So the columns are as wide in the first frame as once every
  row is drawn, and the rows drawn never move as the rest come in below them. What stands after the properties (the
  choice, the items, *Show the other*) waits for the last of them.
-->
<script lang="ts">
  import type { Snippet } from "svelte";

  import type { Shape } from "../../render/openapi";
  import { nameParts, refName, summary } from "../../render/openapi-view";
  import TfOasField from "./TfOasField.svelte";
  import TfOasShape from "./TfOasShape.svelte";
  import { SHAPE_PROPS, columnHolders, type OasState } from "./oas.svelte";

  interface Props {
    st: OasState;
    shape: Shape;
    k: string;
    trail?: readonly string[];
    prose?: Snippet<[string]> | null;
    onfollow: (name: string) => void;
    /** How many properties are drawn whole; the rest stand as unseen rows that hold the columns' widths. */
    upTo?: number;
  }

  let {
    st,
    shape,
    k,
    trail = [],
    prose = null,
    onfollow,
    upTo = Infinity,
  }: Props = $props();

  /** How many properties draw before *Show the other*, and how many choices before the rest are counted. */
  const PROPS = SHAPE_PROPS;
  const CHOICES = 12;

  const all = $derived(st.expand.has(`${k}#props`));
  const drawn = $derived(all ? shape.props : shape.props.slice(0, PROPS));
  const now = $derived(upTo >= drawn.length ? drawn : drawn.slice(0, upTo));
  const later = $derived(upTo >= drawn.length ? [] : drawn.slice(upTo));
  /** Of those, the ones that can widen a column: they stand, unseen, until drawn (`columnHolders`). */
  const holders = $derived(columnHolders(later));
  /** Every property is drawn: what stands after them may be drawn too. */
  const whole = $derived(later.length === 0);
  const pick = $derived(shape.variants === null ? 0 : Math.min(Number(st.picked.get(`${k}|`) ?? 0), shape.variants.list.length - 1));
  const choiceKey = $derived(`${k}|${pick}`);
  const choice = $derived(shape.variants === null ? undefined : st.shapes.get(choiceKey));
  const eachKey = $derived(`${k}/[]`);
  const itemsOnly = $derived(shape.props.length === 0 && shape.variants === null && shape.items !== undefined);
  const each = $derived(itemsOnly ? st.shapes.get(eachKey) : undefined);

  // What the choice picked and an array's items hold is asked once, as a caret asks.
  $effect(() => {
    if (shape.variants !== null && choice === undefined) void st.openShape(choiceKey, shape.base, shape.variants.list[pick]);
  });
  $effect(() => {
    if (itemsOnly && each === undefined) void st.openShape(eachKey, shape.base, shape.items);
  });

  type Obj = Record<string, unknown>;
  function choiceName(m: unknown, i: number): string {
    const o = (typeof m === "object" && m !== null ? m : {}) as Obj;
    if (typeof o.$ref === "string") return refName(o.$ref);
    if (typeof o.title === "string") return o.title;
    return summary(m).type || `choice ${i + 1}`;
  }
</script>

{#if drawn.length > 0 || shape.extra !== undefined}
  <div class="tf-oas-fs">
    {#each now as p (p.name)}
      <TfOasField {st} name={p.name} node={p.node} base={shape.base} k="{k}/{p.name}" required={p.required} words={p.words} {trail} {prose} {onfollow} />
    {/each}
    {#each holders as p (p.name)}
      {@render held(p.name, p.node, p.required)}
    {/each}
    {#if whole && shape.props.length > drawn.length}
      <div class="tf-oas-fnote"><button type="button" class="tf-oas-more" onclick={() => st.expand.add(`${k}#props`)}>Show the other {shape.props.length - drawn.length} properties</button></div>
    {/if}
    {#if shape.extra !== undefined}
      {#if whole}
        <TfOasField {st} name="any other key" node={shape.extra} base={shape.base} k="{k}/{'{}'}" {trail} {prose} {onfollow} />
      {:else}
        {@render held("any other key", shape.extra, false)}
      {/if}
    {/if}
  </div>
{/if}
{#if !whole}
  <!-- The choice and the items wait for the last property, so nothing drawn above them moves. -->
{:else if shape.variants !== null}
  {@const v = shape.variants}
  <div class="tf-oas-pick">
    <span>{v.key === "oneOf" ? "One of" : "Any of"}</span>
    <span class="tf-seg" role="group" aria-label="Which one">
      {#each v.list.slice(0, CHOICES) as m, i (i)}
        <button type="button" aria-pressed={i === pick} onclick={() => st.picked.set(`${k}|`, String(i))}>{choiceName(m, i)}</button>
      {/each}
    </span>
    {#if v.list.length > CHOICES}<span>and {v.list.length - CHOICES} more</span>{/if}
    {#if v.discriminator !== ""}<span>told apart by <code>{v.discriminator}</code></span>{/if}
  </div>
  {#if choice !== undefined}
    {#if choice.props.length > 0 || choice.variants !== null || choice.items !== undefined || choice.extra !== undefined}
      <TfOasShape {st} shape={choice} k={choiceKey} {trail} {prose} {onfollow} />
    {:else}
      <div class="tf-oas-fnote">{summary(v.list[pick]).type || "any"}</div>
    {/if}
  {/if}
{/if}
{#if whole && itemsOnly && each !== undefined && (each.props.length > 0 || each.variants !== null || each.extra !== undefined)}
  <div class="tf-oas-pick">Each item</div>
  <TfOasShape {st} shape={each} k={eachKey} {trail} {prose} {onfollow} />
{/if}

{#snippet held(name: string, node: unknown, required: boolean)}
  <!-- A property not drawn yet: the cells of its head line that size the list's columns, as `TfOasField` draws them. -->
  {@const own = (typeof node === "object" && node !== null && !Array.isArray(node) ? node : {}) as Obj}
  <div class="tf-oas-fheld" aria-hidden="true">
    <div class="tf-oas-fr">
      <span class="tf-oas-tog"></span>
      <span class="tf-oas-fname" class:tf-oas-dep={own.deprecated === true}>{#each nameParts(name) as part, i (i)}{#if i > 0}<wbr />{/if}{part}{/each}</span>
      <span class="tf-oas-ftype"></span>
      {#if own.readOnly === true || own.writeOnly === true || own.deprecated === true}
        <span class="tf-oas-fnotes"
          >{#if own.readOnly === true}<span>read only</span>{/if}{#if own.writeOnly === true}<span>write only</span>{/if}{#if own.deprecated === true}<span class="tf-oas-warn">Deprecated</span>{/if}</span
        >
      {/if}
      {#if required}<span class="tf-oas-freq"><span class="tf-lab">required</span></span>{/if}
    </div>
  </div>
{/snippet}
