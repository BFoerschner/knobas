<!--
  The page a spec opens on (ux31, section 9): the title, the API's version and the spec's, one line of counts, the
  description held to about 900 characters; then where the API is (its servers, each `{variable}` with its default
  and choices), how to sign in (each security scheme in words), its groups with their counts (a press opens that
  fold in the rail and puts the keys there), and *About*. What the reading passed over stays a fold in the warn tone.
-->
<script lang="ts">
  import type { Snippet } from "svelte";

  import type { SpecIndex } from "../../render/openapi";
  import { counted, pathParts, schemeWords } from "../../render/openapi-view";
  import TfOasWords from "./TfOasWords.svelte";
  import { labelOf, type OasState } from "./oas.svelte";

  let {
    st,
    index,
    prose = null,
    onopengroup,
    oncopylink = null,
  }: {
    st: OasState;
    index: SpecIndex;
    prose?: Snippet<[string]> | null;
    /** Opens group `key`'s fold in the rail and puts the keys there. */
    onopengroup: (key: string) => void;
    oncopylink?: (() => void) | null;
  } = $props();

  const groups = $derived(st.groups);
  const contact = $derived(index.info.contact);
  const about = $derived(
    [
      contact === null ? null : (["Contact", [contact.name, contact.email, contact.url].filter((x) => x !== "").join(", ")] as const),
      index.info.license === null ? null : (["Licence", index.info.license.name] as const),
      index.info.terms === "" ? null : (["Terms", index.info.terms] as const),
      index.info.externalDocs === null ? null : (["More", index.info.externalDocs.url] as const),
    ].filter((x) => x !== null),
  );
</script>

<h2 class="tf-oas-title">{index.title}</h2>
<div class="tf-oas-meta">
  {#if index.apiVersion !== ""}<span class="tf-lab">{index.apiVersion}</span>{/if}
  <span>{index.key === "swagger" ? "Swagger" : "OpenAPI"} {index.version}</span>
  <span>{counted(index.ops.length, "endpoint", "endpoints")} in {counted(groups.length, "group", "groups")}, {counted(index.schemas.length, "schema", "schemas")}</span>
  {#if oncopylink !== null}<span class="tf-trail"><button type="button" class="tf-btn tf-sm tf-ghost" onclick={() => oncopylink?.()}>Copy link</button></span>{/if}
</div>
{#if st.gone !== null}
  <p class="tf-oas-gone" role="status">This link named <code>{labelOf(st.gone) === "Overview" ? st.gone : st.gone}</code>, which this version of the file does not have.</p>
{/if}
{#if index.description !== ""}<div class="tf-oas-desc"><TfOasWords text={index.description} {st} k="v#desc" max={900} {prose} block /></div>{/if}

<section class="tf-oas-sec">
  <h3 class="tf-oas-lab">{index.servers.length === 1 ? "Server" : "Servers"}</h3>
  {#if index.servers.length === 0}<div class="tf-oas-fd">The spec names none. A request needs one of yours.</div>{/if}
  {#each index.servers as server, n (n)}
    <div class="tf-oas-f">
      <span></span>
      <div class="tf-oas-fl">
        <span class="tf-oas-fname">{#each pathParts(server.url) as part, m (m)}{#if part.param}<span class="tf-oas-pp">{part.text}</span>{:else}{part.text}{/if}{/each}</span>
        {#if server.description !== ""}<span class="tf-oas-ftype">{server.description}</span>{/if}
        {#if !/^[a-z][a-z0-9+.-]*:\/\//i.test(server.url)}<span class="tf-oas-fwhere">relative: a request needs one of yours</span>{/if}
      </div>
      {#each Object.entries(server.variables) as [name, v] (name)}
        <div class="tf-oas-fd">
          <code>{`{${name}}`}</code>{#if v.description !== ""}{" "}{v.description}{/if}{#if v.default !== ""}. Default <code>{v.default}</code>{/if}{#if v.enum.length > 0}. One of {#each v.enum as e, m (m)}{#if m > 0}{" "}{/if}<code>{e}</code>{/each}{/if}
        </div>
      {/each}
    </div>
  {/each}
</section>

{#if index.security.length > 0}
  <section class="tf-oas-sec" id="tf-oas-signin">
    <h3 class="tf-oas-lab">Signing in</h3>
    {#each index.security as s (s.name)}
      <div class="tf-oas-f">
        <span></span>
        <div class="tf-oas-fl"><span class="tf-oas-fname">{s.name}</span><span class="tf-oas-ftype">{schemeWords(s)}</span></div>
        {#if s.description !== ""}<div class="tf-oas-fd"><TfOasWords text={s.description} {st} k="v/s:{s.name}" max={300} {prose} /></div>{/if}
      </div>
    {/each}
  </section>
{/if}

{#if groups.length > 0}
  <section class="tf-oas-sec">
    <h3 class="tf-oas-lab">Groups <span class="tf-oas-n">{groups.length}</span></h3>
    <div class="tf-oas-tags">
      {#each groups as g (g.key)}
        <button type="button" class="tf-oas-tag" title={g.description} onclick={() => onopengroup(g.key)}>
          <span class:tf-oas-code={g.code}>{g.name}</span><span class="tf-oas-n">{g.ops.length + g.refs.length}</span>
        </button>
      {/each}
    </div>
  </section>
{/if}

{#if about.length > 0}
  <section class="tf-oas-sec">
    <h3 class="tf-oas-lab">About</h3>
    <dl class="tf-oas-kv">
      {#each about as [label, value] (label)}
        <dt>{label}</dt>
        <dd>{#if /^https?:\/\//.test(value)}<a class="tf-oas-link" href={value} target="_blank" rel="noreferrer">{value}</a>{:else}{value}{/if}</dd>
      {/each}
    </dl>
  </section>
{/if}

{#if index.problems.length > 0}
  <details class="tf-oas-problems">
    <summary>{index.problems.length === 1 ? "One entry was left out" : `${index.problems.length} entries were left out`}</summary>
    <ul>{#each index.problems.slice(0, 200) as problem, n (n)}<li>{problem}</li>{/each}</ul>
  </details>
{/if}
