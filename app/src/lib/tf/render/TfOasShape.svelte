<!--
  What a schema holds, under its line (ux31, section 8): its properties merged (`allOf`), the choice it offers as
  a `.tf-seg` of the choices by name with the picked one drawn under it, what each item of an array is, and any
  other key. A schema of more than 80 properties draws 80 and *Show the other N*; a choice draws 12 and counts
  the rest.
-->
<script lang="ts">
  import type { Snippet } from "svelte";

  import type { Shape } from "../../render/openapi";
  import { refName, summary } from "../../render/openapi-view";
  import TfOasField from "./TfOasField.svelte";
  import TfOasShape from "./TfOasShape.svelte";
  import type { OasState } from "./oas.svelte";

  interface Props {
    st: OasState;
    shape: Shape;
    k: string;
    trail?: readonly string[];
    prose?: Snippet<[string]> | null;
    onfollow: (name: string) => void;
  }

  let {
    st,
    shape,
    k,
    trail = [],
    prose = null,
    onfollow,
  }: Props = $props();

  /** How many properties draw before *Show the other*, and how many choices before the rest are counted. */
  const PROPS = 80;
  const CHOICES = 12;

  const all = $derived(st.expand.has(`${k}#props`));
  const drawn = $derived(all ? shape.props : shape.props.slice(0, PROPS));
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
    {#each drawn as p (p.name)}
      <TfOasField {st} name={p.name} node={p.node} base={shape.base} k="{k}/{p.name}" required={p.required} words={p.words} {trail} {prose} {onfollow} />
    {/each}
    {#if shape.props.length > drawn.length}
      <div class="tf-oas-fnote"><button type="button" class="tf-oas-more" onclick={() => st.expand.add(`${k}#props`)}>Show the other {shape.props.length - drawn.length} properties</button></div>
    {/if}
    {#if shape.extra !== undefined}
      <TfOasField {st} name="any other key" node={shape.extra} base={shape.base} k="{k}/{'{}'}" {trail} {prose} {onfollow} />
    {/if}
  </div>
{/if}
{#if shape.variants !== null}
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
{#if itemsOnly && each !== undefined && (each.props.length > 0 || each.variants !== null || each.extra !== undefined)}
  <div class="tf-oas-pick">Each item</div>
  <TfOasShape {st} shape={each} k={eachKey} {trail} {prose} {onfollow} />
{/if}
