<!--
  An operation opened in the OpenAPI view (ux18): its description, its operationId and what security it asks for,
  *Copy link*, then its parameters (a table: name, where, type, what it is), its request body and its responses, each
  by status, with their media types (one picked where there are several), schemas and examples. Nothing here calls
  the API: there is no *Try it out*.
-->
<script lang="ts">
  import type { Snippet } from "svelte";

  import TfOasSchema from "./TfOasSchema.svelte";
  import type { Media, Operation } from "../../render/openapi";
  import { exampleText, refName } from "../../render/openapi-view";
  import type { OasState } from "./oas.svelte";

  let {
    op,
    st,
    k,
    prose = null,
    oncopylink = null,
  }: {
    op: Operation;
    st: OasState;
    /** The row's key: what its open parts are kept by. */
    k: string;
    /** Draws Markdown (the file viewer's Markdown component); plain text without it. */
    prose?: Snippet<[string]> | null;
    oncopylink?: ((place: string, label: string) => void) | null;
  } = $props();

  const label = $derived(`${op.method.toUpperCase()} ${op.path}`);
  const from = $derived({ k, label });
</script>

{#snippet text(words: string)}
  {#if prose !== null}{@render prose(words)}{:else}<p class="tf-oas-desc">{words}</p>{/if}
{/snippet}

{#snippet media(list: Media[], key: string)}
  {#if list.length > 0}
    {@const shown = list.find((m) => m.type === st.picked.get(key)) ?? list[0]!}
    <div class="tf-oas-media">
      {#if list.length > 1}
        <div class="tf-seg tf-oas-types" role="group" aria-label="Media type">
          {#each list as m (m.type)}
            <button type="button" aria-pressed={m === shown} onclick={() => st.picked.set(key, m.type)}>{m.type}</button>
          {/each}
        </div>
      {:else}
        <code class="tf-oas-mtype">{shown.type}</code>
      {/if}
      {#if shown.schema !== null && shown.schema !== undefined}
        <TfOasSchema node={shown.schema} base={op.base} {st} k="{key}:{shown.type}" {from} top />
      {/if}
      {#each shown.examples as ex, at (at)}
        <details class="tf-oas-ex">
          <summary>{ex.name === "" ? "Example" : `Example: ${ex.name}`}{#if ex.summary !== ""}<span class="tf-dim">, {ex.summary}</span>{/if}</summary>
          {#if ex.external !== null}
            <p class="tf-oas-note">Kept outside the spec at <code>{ex.external}</code>; knobas does not fetch it.</p>
          {:else}
            <pre>{exampleText(ex.value)}</pre>
          {/if}
        </details>
      {/each}
    </div>
  {/if}
{/snippet}

<div class="tf-oas-op">
  {#if op.description !== ""}{@render text(op.description)}{/if}
  <div class="tf-oas-meta">
    {#if op.operationId !== null}<code class="tf-oas-opid">{op.operationId}</code>{/if}
    {#if op.security.length > 0}<span class="tf-dim">Needs {op.security.join(", or ")}</span>{/if}
    {#if oncopylink !== null}
      <span class="tf-trail"><button type="button" class="tf-btn tf-sm tf-ghost" onclick={() => oncopylink?.(`${op.method} ${op.path}`, label)}>Copy link</button></span>
    {/if}
  </div>

  {#if op.parameters.length > 0}
    <section class="tf-oas-sec">
      <h4 class="tf-oas-glabel">Parameters</h4>
      <div class="tf-oas-params">
        {#each op.parameters as p (`${p.in}:${p.name}`)}
          {#if p.ref !== null}
            {@const href = st.hrefOf(op.base, p.ref)}
            <div class="tf-oas-param">
              <span class="tf-oas-pcell">{#if href !== null}<a class="tf-oas-ref" {href}>{refName(p.ref)}</a>{:else}<code>{refName(p.ref)}</code>{/if}</span>
              <span class="tf-dim tf-oas-pwhere">A parameter in another file</span>
            </div>
          {:else}
            <div class="tf-oas-param">
              <span class="tf-oas-pcell">
                <code class="tf-oas-pname" class:tf-oas-dep={p.deprecated}>{p.name}</code>
                {#if p.required}<span class="tf-lab tf-oas-req">required</span>{/if}
                {#if p.deprecated}<span class="tf-oas-deplab">Deprecated</span>{/if}
              </span>
              <span class="tf-oas-pwhere tf-dim">{p.in}</span>
              <div class="tf-oas-pbody">
                {#if p.schema !== null && p.schema !== undefined}<TfOasSchema node={p.schema} base={op.base} {st} k="{k}:p:{p.in}:{p.name}" {from} top />{/if}
                {#if p.description !== ""}<p class="tf-oas-pdesc">{p.description}</p>{/if}
                {#if p.example !== undefined}<p class="tf-oas-enum"><span class="tf-dim">Example</span> <code>{typeof p.example === "string" ? p.example : JSON.stringify(p.example)}</code></p>{/if}
              </div>
            </div>
          {/if}
        {/each}
      </div>
    </section>
  {/if}

  {#if op.body !== null}
    <section class="tf-oas-sec">
      <h4 class="tf-oas-glabel">Request body{#if op.body.required}<span class="tf-lab tf-oas-req">required</span>{/if}</h4>
      {#if op.body.ref !== null}
        {@const href = st.hrefOf(op.base, op.body.ref)}
        <p class="tf-oas-note">Defined in {#if href !== null}<a class="tf-oas-ref" {href}>{refName(op.body.ref)}</a>{:else}<code>{op.body.ref}</code>{/if}.</p>
      {/if}
      {#if op.body.description !== ""}<p class="tf-oas-pdesc">{op.body.description}</p>{/if}
      {@render media(op.body.content, `${k}:b`)}
    </section>
  {/if}

  {#if op.responses.length > 0}
    <section class="tf-oas-sec">
      <h4 class="tf-oas-glabel">Responses</h4>
      {#each op.responses as r (r.status)}
        <div class="tf-oas-resp">
          <span class="tf-oas-status">{r.status}</span>
          <div class="tf-oas-rbody">
            {#if r.ref !== null}
              {@const href = st.hrefOf(op.base, r.ref)}
              <p class="tf-oas-note">Defined in {#if href !== null}<a class="tf-oas-ref" {href}>{refName(r.ref)}</a>{:else}<code>{r.ref}</code>{/if}.</p>
            {/if}
            {#if r.description !== ""}<p class="tf-oas-rdesc">{r.description}</p>{/if}
            {@render media(r.content, `${k}:r:${r.status}`)}
            {#if r.headers.length > 0}
              <p class="tf-oas-glabel">Headers</p>
              {#each r.headers as h (h.name)}
                <div class="tf-oas-pline tf-oas-hline">
                  <code class="tf-oas-pname">{h.name}</code>
                  {#if h.schema !== null}<TfOasSchema node={h.schema} base={op.base} {st} k="{k}:r:{r.status}:h:{h.name}" {from} top />{/if}
                  {#if h.description !== ""}<span class="tf-oas-pdesc">{h.description}</span>{/if}
                </div>
              {/each}
            {/if}
          </div>
        </div>
      {/each}
    </section>
  {/if}
</div>
