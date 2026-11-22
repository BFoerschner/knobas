<!--
  An operation's page in the API explorer (ux31, section 7), in reading order:

  1. **The head**: the method and the path, `{parameters}` dim; the summary; one quiet line of *Deprecated*, the
     operationId, the tags, what it needs to sign in (each a link to the overview's *Signing in*), *Copy link*.
  2. **The description**, Markdown, held to about 700 characters with *Show all*.
  3. **Parameters**, in the order path, query, header, cookie, each a field row.
  4. **Request body**, its media types a `.tf-seg`, its schema as fields, its example.
  5. **Responses**: the statuses a `.tf-seg` at the label's end, the first shown; the picked one's words, headers,
     schema, example and *Then* (its links, each a row to the operation it names).
  6. **Callbacks**, by name.

  Nothing has a tone but *Deprecated*: a status in the reference is an outcome the spec lists, not one that happened.
  What is open or picked is kept in `OasState` by the operation's place, so a page visited again is as it was left.
-->
<script lang="ts">
  import type { Snippet } from "svelte";

  import type { Operation, Param } from "../../render/openapi";
  import { pathParts } from "../../render/openapi-view";
  import TfOasField from "./TfOasField.svelte";
  import TfOasMedia from "./TfOasMedia.svelte";
  import TfOasWords from "./TfOasWords.svelte";
  import { OVERVIEW, type OasState, type Place } from "./oas.svelte";

  let {
    st,
    op,
    k,
    prose = null,
    onfollow,
    ongo,
    oncopylink = null,
    onuse = null,
  }: {
    st: OasState;
    op: Operation;
    /** What its open parts are kept by: its place. */
    k: string;
    prose?: Snippet<[string]> | null;
    /** Goes to a named schema's page. */
    onfollow: (name: string) => void;
    /** Follows a link to another place of the spec (*Then*, *Signing in*). */
    ongo: (place: Place, label: string) => void;
    oncopylink?: (() => void) | null;
    onuse?: ((type: string, value: unknown) => void) | null;
  } = $props();

  const WHERE = ["path", "query", "header", "cookie"];
  const params = $derived(
    [...op.parameters].filter((p) => p.ref === null).sort((a, b) => WHERE.indexOf(a.in) - WHERE.indexOf(b.in)),
  );
  const unread = $derived(op.parameters.filter((p) => p.ref !== null));
  const needs = $derived(op.security.filter((r) => r.length > 0));
  const anon = $derived(op.security.some((r) => r.length === 0) && needs.length > 0);
  const status = $derived(op.responses.find((r) => r.status === st.picked.get(`${k}/r`)) ?? op.responses[0]);
  const rk = $derived(status === undefined ? "" : `${k}/r:${status.status}`);

  /** The operation a response's link names, by its operationId. */
  function linked(id: string) {
    return id === "" ? undefined : st.index?.ops.find((o) => o.operationId === id);
  }

  const paramWhere = (p: Param) => p.in;
</script>

<h2 class="tf-oas-h">
  <span class="tf-oas-m">{op.method.toUpperCase()}</span>
  <span class="tf-oas-path" class:tf-oas-dep={op.deprecated}>{#each pathParts(op.path) as part, n (n)}{#if part.param}<span class="tf-oas-pp">{part.text}</span>{:else}{part.text}{/if}{/each}</span>
</h2>
{#if op.summary !== ""}<p class="tf-oas-sum">{op.summary}</p>{/if}
<div class="tf-oas-meta">
  {#if op.deprecated}<span class="tf-oas-warn">Deprecated</span>{/if}
  {#if op.webhook}<span>Webhook: the API sends this to you</span>{/if}
  {#if op.operationId !== null}<code>{op.operationId}</code>{/if}
  {#if (st.opAt(k)?.tags.length ?? 0) > 0}<span>{st.opAt(k)?.tags.join(", ")}</span>{/if}
  {#if needs.length > 0}
    <span
      >Needs {#each needs as req, n (n)}{#if n > 0}{", or "}{/if}<button type="button" class="tf-oas-more" onclick={() => ongo(OVERVIEW, "Signing in")}>{req.join(" and ")}</button>{/each}{#if anon}{", or nothing"}{/if}</span
    >
  {/if}
  {#if op.externalDocs !== null}<a class="tf-oas-link" href={op.externalDocs.url} target="_blank" rel="noreferrer">{op.externalDocs.description || "More"}</a>{/if}
  {#if oncopylink !== null}<span class="tf-trail"><button type="button" class="tf-btn tf-sm tf-ghost" onclick={() => oncopylink?.()}>Copy link</button></span>{/if}
</div>
{#if op.description !== "" && op.description !== op.summary}
  <div class="tf-oas-desc"><TfOasWords text={op.description} {st} k="{k}#desc" max={700} {prose} block /></div>
{/if}

{#if params.length > 0 || unread.length > 0}
  <section class="tf-oas-sec">
    <h3 class="tf-oas-lab">Parameters <span class="tf-oas-n">{params.length + unread.length}</span></h3>
    {#each params as p (`${p.in}:${p.name}`)}
      <TfOasField
        {st}
        name={p.name}
        node={p.schema}
        base={op.base}
        k="{k}/p:{p.in}:{p.name}"
        required={p.required}
        where={paramWhere(p)}
        words={p.description}
        example={p.example}
        deprecated={p.deprecated}
        {prose}
        {onfollow}
      />
    {/each}
    {#each unread as p (p.ref)}
      <div class="tf-oas-f"><span></span><div class="tf-oas-fl"><span class="tf-oas-fname">{p.ref}</span><span class="tf-oas-fwhere">kept in another file</span></div></div>
    {/each}
  </section>
{/if}

{#if op.body !== null}
  <section class="tf-oas-sec">
    <h3 class="tf-oas-lab">Request body{#if op.body.required}<span class="tf-lab">required</span>{/if}</h3>
    {#if op.body.ref !== null}
      <p class="tf-oas-fd">Kept in <code>{op.body.ref}</code>.</p>
    {:else}
      {#if op.body.description !== ""}<div class="tf-oas-fd"><TfOasWords text={op.body.description} {st} k="{k}/b#desc" {prose} /></div>{/if}
      <TfOasMedia {st} list={op.body.content} k="{k}/b" base={op.base} {prose} {onfollow} onuse={op.webhook ? null : onuse} />
    {/if}
  </section>
{/if}

{#if status !== undefined}
  <section class="tf-oas-sec">
    <h3 class="tf-oas-lab">
      Responses
      <span class="tf-trail">
        <span class="tf-seg" role="group" aria-label="Status">
          {#each op.responses as r (r.status)}<button type="button" class="tf-oas-code" aria-pressed={r === status} onclick={() => st.picked.set(`${k}/r`, r.status)}>{r.status}</button>{/each}
        </span>
      </span>
    </h3>
    {#if status.ref !== null}
      <p class="tf-oas-fd">Kept in <code>{status.ref}</code>.</p>
    {:else}
      {#if status.description !== ""}<div class="tf-oas-fd"><TfOasWords text={status.description} {st} k="{rk}#desc" {prose} /></div>{/if}
      {#if status.headers.length > 0}
        <div class="tf-oas-sub">Headers</div>
        {#each status.headers as h (h.name)}
          <TfOasField {st} name={h.name} node={h.schema} base={op.base} k="{rk}/h:{h.name}" words={h.description} {prose} {onfollow} />
        {/each}
      {/if}
      {#key rk}<TfOasMedia {st} list={status.content} k={rk} base={op.base} {prose} {onfollow} />{/key}
      {#if status.links.length > 0}
        <div class="tf-oas-sub">Then</div>
        <div class="tf-oas-used">
          {#each status.links as l (l.name)}
            {@const to = linked(l.operationId)}
            {#if to !== undefined}
              <button type="button" class="tf-oas-usedrow" onclick={() => ongo(to.anchor, `${to.method.toUpperCase()} ${to.path}`)}>
                <span class="tf-oas-m">{to.method.toUpperCase()}</span>
                <span class="tf-oas-rpath">{#each pathParts(to.path) as part, n (n)}{#if part.param}<span class="tf-oas-pp">{part.text}</span>{:else}{part.text}{/if}{/each}</span>
                <span class="tf-oas-rsum">{l.description || l.name}</span>
              </button>
            {:else}
              <div class="tf-oas-usedrow"><span></span><span class="tf-oas-rpath">{l.operationId || l.operationRef || l.name}</span><span class="tf-oas-rsum">{l.description}</span></div>
            {/if}
          {/each}
        </div>
      {/if}
    {/if}
  </section>
{/if}

{#if op.callbacks.length > 0}
  <section class="tf-oas-sec">
    <h3 class="tf-oas-lab">Callbacks</h3>
    <div class="tf-oas-fd">{#each op.callbacks as c, n (c)}{#if n > 0}{" "}{/if}<code>{c}</code>{/each}</div>
  </section>
{/if}
