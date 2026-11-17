<!--
  A schema in the OpenAPI view (ux18): its one line (the type in words, a ref as a link), then, where it has more,
  its properties, one a row: the name in the code face, *required*, the type, the description, its enum. A property
  with more inside opens under a caret; a ref opens to the schema it names, inline, and its name follows to that
  schema in the Schemas section (with *Back to* in the band). A ref to one of the schemas it sits inside does not
  open again: it says so and follows. A ref to another file opens on *Reading ⟨file⟩…* and is read from the
  repository at the spec's commit; a ref to a URL is named and never read.

  `top` draws the schema's own line first and opens its ref at once (a body's or a response's schema); the
  operation resolved that ref before it drew, so nothing grows after the first frame.
-->
<script lang="ts">
  import Self from "./TfOasSchema.svelte";
  import { refTarget, isUrlRef } from "../../render/openapi";
  import { exampleText, hasInside, refName, summary } from "../../render/openapi-view";
  import { innerRef, type OasState } from "./oas.svelte";

  type Obj = Record<string, unknown>;
  const isObj = (v: unknown): v is Obj => typeof v === "object" && v !== null && !Array.isArray(v);

  interface Props {
    node: unknown;
    /** The file the node came from: where its refs are read. */
    base: string;
    st: OasState;
    /** This node's key: what its open parts are kept by. */
    k: string;
    /** The schemas this one sits inside, as `file#pointer`: a ref to one of them does not open again. */
    trail?: readonly string[];
    /** The row it is drawn in, for *Back to*. */
    from: { k: string; label: string };
    top?: boolean;
  }

  let { node, base, st, k, trail = [], from, top = false }: Props = $props();

  const obj = $derived(isObj(node) ? node : null);
  const fields = $derived(obj !== null && isObj(obj.properties) ? Object.entries(obj.properties) : []);
  const required = $derived(new Set(Array.isArray(obj?.required) ? (obj.required as unknown[]).map(String) : []));
  const comps = $derived(
    obj === null
      ? []
      : (["allOf", "oneOf", "anyOf"] as const)
          .filter((c) => Array.isArray(obj[c]))
          .map((c) => ({ c, words: c === "allOf" ? "All of" : c === "oneOf" ? "One of" : "Any of", list: obj[c] as unknown[] })),
  );
  /** An array's items drawn inside it where they are a schema of their own (not a ref: that is the line's link). */
  const items = $derived(obj !== null && isObj(obj.items) && typeof obj.items.$ref !== "string" && hasInside(obj.items) ? obj.items : null);
  const extra = $derived(obj !== null && isObj(obj.additionalProperties) ? obj.additionalProperties : null);

  /** A ref's target as `file#pointer`, the trail's currency. */
  function idOf(from: string, ref: string): string | null {
    const t = refTarget(from, ref);
    return t.ok ? `${t.file}#${t.pointer}` : null;
  }

  const topRef = $derived(top ? innerRef(node) : null);
</script>

{#snippet refLink(ref: string)}
  {@const named = st.named(base, ref)}
  {@const href = st.hrefOf(base, ref)}
  {#if named !== null}
    <button type="button" class="tf-oas-ref" title="Go to {named} in Schemas" onclick={() => st.follow(named, from)}>{named}</button>
  {:else if isUrlRef(ref)}
    <span class="tf-oas-ref tf-oas-far" title="{ref}: outside the repository, not fetched">{refName(ref)}</span>
  {:else if href !== null}
    <a class="tf-oas-ref" {href} title="Open {ref}">{refName(ref)}</a>
  {:else}
    <span class="tf-oas-ref tf-oas-plain" title={ref}>{refName(ref)}</span>
  {/if}
{/snippet}

{#snippet typeLine(schema: unknown)}
  {@const s = summary(schema)}
  <span class="tf-oas-type">{#if s.type !== ""}{s.type}{/if}{#if s.ref !== null}{#if s.type !== ""}{" "}{/if}{@render refLink(s.ref)}{/if}{#if s.nullable}<span class="tf-oas-null">, or null</span>{/if}</span>
{/snippet}

{#snippet opened(schema: unknown, key: string)}
  <!-- What a property or a composition's member shows once open: the ref's schema, or its own inside. -->
  {@const ref = innerRef(schema)}
  {#if ref === null}
    <div class="tf-oas-in"><Self node={schema} {base} {st} k={key} {trail} {from} /></div>
  {:else}
    {@const id = idOf(base, ref)}
    {@const state = st.ref(base, ref)}
    {#if state === undefined || "reading" in state}
      <p class="tf-oas-note" role="status">Reading {"reading" in (state ?? {}) ? (state as { reading: string }).reading : refName(ref)}…</p>
    {:else if "need" in state || !state.ok}
      <p class="tf-oas-note">{"error" in state ? state.error : `${ref} could not be read.`}</p>
    {:else}
      <div class="tf-oas-in"><Self node={state.node} base={state.base} {st} k={key} trail={id === null ? trail : [...trail, id]} {from} /></div>
    {/if}
  {/if}
{/snippet}

{#snippet caret(key: string, schema: unknown)}
  {@const ref = innerRef(schema)}
  {@const id = ref === null ? null : idOf(base, ref)}
  {#if ref !== null && id !== null && trail.includes(id)}
    <!-- A ref to a schema it sits inside: following it is the way there, opening it again would never end. -->
    <span class="tf-oas-caret tf-oas-nocaret" aria-hidden="true"></span>
  {:else if hasInside(schema) && !(ref !== null && isUrlRef(ref))}
    <button type="button" class="tf-oas-caret" aria-expanded={st.open.has(key)} aria-label={st.open.has(key) ? "Close" : "Open"} onclick={() => st.toggle(key, schema, base)}
      ><svg viewBox="0 0 10 10" aria-hidden="true"><path d="M3 2l4 3-4 3" /></svg></button
    >
  {:else}
    <span class="tf-oas-caret tf-oas-nocaret" aria-hidden="true"></span>
  {/if}
{/snippet}

{#snippet facts(schema: unknown)}
  {#if isObj(schema)}
    {#if typeof schema.description === "string" && schema.description !== ""}<p class="tf-oas-pdesc">{schema.description}</p>{/if}
    {#if Array.isArray(schema.enum)}
      <p class="tf-oas-enum">
        <span class="tf-dim">One of</span>
        {#each schema.enum.slice(0, 40) as value, at (at)}<code>{typeof value === "string" ? value : JSON.stringify(value)}</code>{" "}{/each}{#if schema.enum.length > 40}<span class="tf-dim">and {schema.enum.length - 40} more</span>{/if}
      </p>
    {/if}
    {#if schema.default !== undefined}<p class="tf-oas-enum"><span class="tf-dim">Default</span> <code>{JSON.stringify(schema.default)}</code></p>{/if}
  {/if}
{/snippet}

{#if top}
  <div class="tf-oas-top">{@render typeLine(node)}</div>
  {#if topRef !== null}
    {@const id = idOf(base, topRef)}
    {@const state = st.ref(base, topRef)}
    {@const far = id !== null && !id.startsWith(`${base}#`)}
    {#if far && !st.open.has(k)}
      <!-- Another file is read only when asked (Fable's ruling): its name is the link, the caret reads it here. -->
      <button type="button" class="tf-btn tf-sm tf-ghost tf-oas-read" onclick={() => void st.openNode(k, node, base)}>Show it here</button>
    {:else if state === undefined || "reading" in state}
      <p class="tf-oas-note" role="status">Reading {"reading" in (state ?? {}) ? (state as { reading: string }).reading : refName(topRef)}…</p>
    {:else if "need" in state || !state.ok}
      <p class="tf-oas-note">{"error" in state ? state.error : `${topRef} could not be read.`}</p>
    {:else}
      {@render facts(state.node)}
      <Self node={state.node} base={state.base} {st} {k} trail={id === null ? trail : [...trail, id]} {from} />
    {/if}
  {:else}
    {@render facts(node)}
    <Self {node} {base} {st} {k} {trail} {from} />
  {/if}
{:else}
  {#if fields.length > 0}
    <div class="tf-oas-props">
      {#each fields as [name, schema] (name)}
        {@const key = `${k}/${name}`}
        {@const dep = isObj(schema) && schema.deprecated === true}
        <div class="tf-oas-prop">
          <div class="tf-oas-pline">
            {@render caret(key, schema)}
            <code class="tf-oas-pname" class:tf-oas-dep={dep}>{name}</code>
            {#if required.has(name)}<span class="tf-lab tf-oas-req">required</span>{/if}
            {@render typeLine(schema)}
            {#if dep}<span class="tf-oas-deplab">Deprecated</span>{/if}
          </div>
          {@render facts(schema)}
          {#if st.open.has(key)}{@render opened(schema, key)}{/if}
        </div>
      {/each}
    </div>
  {/if}
  {#each comps as comp (comp.c)}
    <div class="tf-oas-props">
      <p class="tf-oas-glabel">{comp.words}</p>
      {#each comp.list as member, at (at)}
        {@const key = `${k}/${comp.c}${at}`}
        <div class="tf-oas-prop">
          <div class="tf-oas-pline">
            {@render caret(key, member)}
            <span class="tf-oas-pnum">{at + 1}</span>
            {@render typeLine(member)}
          </div>
          {@render facts(member)}
          {#if st.open.has(key)}{@render opened(member, key)}{/if}
        </div>
      {/each}
    </div>
  {/each}
  {#if items !== null}
    <div class="tf-oas-props">
      <p class="tf-oas-glabel">Each item</p>
      <Self node={items} {base} {st} k="{k}/[]" {trail} {from} />
    </div>
  {/if}
  {#if extra !== null}
    {@const key = `${k}/{}`}
    <div class="tf-oas-props">
      <div class="tf-oas-prop">
        <div class="tf-oas-pline">
          {@render caret(key, extra)}
          <span class="tf-oas-anykey">Any other key</span>
          {@render typeLine(extra)}
        </div>
        {#if st.open.has(key)}{@render opened(extra, key)}{/if}
      </div>
    </div>
  {/if}
  {#if isObj(node) && node.example !== undefined && fields.length > 0}
    <details class="tf-oas-ex">
      <summary>Example</summary>
      <pre>{exampleText(node.example)}</pre>
    </details>
  {/if}
{/if}
