<!--
  A body's or a response's content in the API explorer (ux31, section 7): its media types a `.tf-seg` where there
  are several, one drawn at a time; the schema's type line and what it holds (`TfOasShape`); then *Example*, a
  select where there are several, and, for a request body where a request column stands, *Use in the request*.
-->
<script lang="ts">
  import type { Snippet } from "svelte";

  import { refTarget, type Media, type Shape } from "../../render/openapi";
  import TfOasField from "./TfOasField.svelte";
  import { exampleText, hasInside, refName, summary } from "../../render/openapi-view";
  import TfOasShape from "./TfOasShape.svelte";
  import type { OasState } from "./oas.svelte";

  let {
    st,
    list,
    k,
    base,
    trail = [],
    prose = null,
    onfollow,
    onuse = null,
    direction = "response",
  }: {
    st: OasState;
    list: readonly Media[];
    /** The part's key (`get /pets/b`, `get /pets/r:200`): a media type's shape is kept at `<k>:<type>`. */
    k: string;
    base: string;
    trail?: readonly string[];
    prose?: Snippet<[string]> | null;
    onfollow: (name: string) => void;
    /** Fills the request's body with the example shown (a request body, where the request column stands). */
    onuse?: ((type: string, value: unknown) => void) | null;
    /** Which way the content goes, for an example made from its schema: a request leaves out read-only fields. */
    direction?: "request" | "response";
  } = $props();

  const shown = $derived(list.find((m) => m.type === st.picked.get(k)) ?? list[0]);
  const sk = $derived(shown === undefined ? "" : `${k}:${shown.type}`);
  const shape = $derived(shown === undefined ? undefined : st.shapes.get(sk));
  const line = $derived(shown === undefined || shown.schema === null ? null : summary(shown.schema));
  const named = $derived(line === null || line.ref === null ? null : st.named(base, line.ref));
  /** A ref into another file: a row of its own whose caret reads that file, only when pressed. */
  const away = $derived.by(() => {
    if (line === null || line.ref === null || named !== null) return false;
    const t = refTarget(base, line.ref);
    return t.ok && t.file !== base;
  });
  const exIndex = $derived(shown === undefined ? 0 : Math.min(Number(st.picked.get(`${sk}#ex`) ?? 0), Math.max(0, shown.examples.length - 1)));
  const given = $derived(shown?.examples[exIndex]);

  /** Whether any row of a shape opens: only then is there an *Expand all*. */
  const opens = (sh: Shape) => sh.props.some((p) => hasInside(p.node)) || sh.variants !== null || sh.items !== undefined || (sh.extra !== undefined && hasInside(sh.extra));
  /** Whether anything below `key` is open. */
  const anyOpen = (key: string) => [...st.expand].some((x) => x.startsWith(`${key}/`) || x.startsWith(`${key}|`));
  /** *Expand all* or *Collapse all*, one button that flips, as Builds' does. */
  function flipAll(key: string, sh: Shape) {
    if (anyOpen(key)) st.closeAll(key);
    else void st.expandShape(key, sh, trail);
  }
  /** The top choice the shape's switch picked: the made example follows it. */
  const choice = $derived(shape?.variants == null ? 0 : Number(st.picked.get(`${sk}|`) ?? 0));
  const made = $derived(shown === undefined || given !== undefined ? undefined : choice === 0 ? st.samples.get(sk) : st.samples.get(`${sk}|${choice}`));
  const example = $derived(given ?? (made === undefined ? undefined : { name: "", summary: "", value: made, external: null }));
  $effect(() => {
    if (shown !== undefined && given === undefined && shown.schema !== null && choice > 0) void st.sampleChoice(sk, base, shown.schema, direction, choice);
  });

  // A shape the page's one answer did not bring (a path item read from another file) is asked for once.
  $effect(() => {
    if (shown !== undefined && shown.schema !== null && shape === undefined && !away) {
      const at = sk;
      void st.openShape(at, base, shown.schema).then(() => st.expand.delete(at));
    }
  });
</script>

{#if shown !== undefined}
  {#if list.length > 1}
    <div class="tf-oas-exbar">
      <span class="tf-seg" role="group" aria-label="Media type">
        {#each list as m (m.type)}<button type="button" class="tf-oas-code" aria-pressed={m === shown} onclick={() => st.picked.set(k, m.type)}>{m.type}</button>{/each}
      </span>
    </div>
  {/if}
  {#if away && shown.schema !== null && line !== null && line.ref !== null}
    <div class="tf-oas-fs"><TfOasField {st} name={refName(line.ref)} node={shown.schema} {base} k="{sk}/@" where={list.length === 1 ? shown.type : ""} {trail} {prose} {onfollow} /></div>
  {:else if line !== null || list.length === 1}
    <!-- The schema's own line, the list's caption, with Expand all at its end where a row opens. -->
    <div class="tf-oas-what">
      {#if line !== null}
        <span class="tf-oas-ftype"
          >{line.type}{#if line.ref !== null}{line.type !== "" ? " " : ""}{#if named !== null}<button type="button" class="tf-oas-ref" onclick={() => onfollow(named)}>{refName(line.ref)}</button>{:else}<span class="tf-oas-code">{refName(line.ref)}</span>{/if}{/if}</span
        >
      {/if}
      {#if list.length === 1}<span class="tf-oas-fwhere tf-oas-code">{shown.type}</span>{/if}
      {#if shape !== undefined && opens(shape)}
        <span class="tf-trail"><button type="button" class="tf-btn tf-sm tf-ghost" onclick={() => flipAll(sk, shape)}>{anyOpen(sk) ? "Collapse all" : "Expand all"}</button></span>
      {/if}
    </div>
  {/if}
  {#if shape !== undefined && !away}<TfOasShape {st} {shape} k={sk} {trail} {prose} {onfollow} />{/if}
  {#if example !== undefined}
    <div class="tf-oas-exbar">
      <b>Example</b>
      <span class="tf-oas-exfrom">{given !== undefined ? "From the spec" : "Made from the schema"}</span>
      {#if given !== undefined && shown.examples.length > 1}
        <select aria-label="Example" value={String(exIndex)} onchange={(e) => st.picked.set(`${sk}#ex`, (e.currentTarget as HTMLSelectElement).value)}>
          {#each shown.examples as ex, i (i)}<option value={String(i)}>{ex.summary || ex.name || `Example ${i + 1}`}</option>{/each}
        </select>
      {/if}
      {#if onuse !== null}<span class="tf-trail"><button type="button" class="tf-btn tf-sm tf-ghost" onclick={() => onuse?.(shown.type, example.value)}>Use in the request</button></span>{/if}
    </div>
    {#if example.external !== null}<p class="tf-oas-fd">Kept outside the spec at <code>{example.external}</code>; knobas does not fetch it.</p>
    {:else}<pre class="tf-oas-ex">{exampleText(example.value)}</pre>{/if}
  {/if}
{/if}
