<!--
  One run of a page's text (#859, R-10.11): marks, links, ticket chips, page
  links and mentions. Every string is source text, interpolated, never markup.

  A web link is a button handed to the OS, as `detail/StorageBody.svelte`
  does: an anchor with an absolute `href` would navigate the webview.

  With `stems` (#860, R-10.5), every match in the words is a `<mark>`, as the
  backend marks a snippet.
-->
<script lang="ts">
  import type { DocInline } from "../../ipc/entity";
  import { jobHash } from "../route";
  import TfMarked from "./TfMarked.svelte";
  import { mentionText, pageHash, statusTone } from "./docs";

  let {
    text,
    onopen,
    stems = [],
  }: { text: DocInline[]; onopen: (url: string) => void; stems?: string[] } = $props();
</script>

{#snippet words(value: string)}<TfMarked text={value} {stems} />{/snippet}

{#each text as run, index (index)}{#if run.type === "text"}{#if run.code}<code>{@render words(run.text)}</code
      >{:else if run.strong && run.emphasis}<b><em>{@render words(run.text)}</em></b>{:else if run.strong}<b
        >{@render words(run.text)}</b
      >{:else if run.emphasis}<em>{@render words(run.text)}</em>{:else}{@render words(run.text)}{/if}{:else if run.type ===
    "link"}<button type="button" class="tf-doc-link" title={run.href} onclick={() => onopen(run.href)}
      >{@render words(run.text)}</button
    >{:else if run.type === "page_link"}{#if run.entity_id}<a class="tf-pgl" href={pageHash(run.entity_id)}
        >{@render words(run.text)}</a
      >{:else}{@render words(run.text)}{/if}{:else if run.type === "issue"}{#if run.entity_id}{@const tone = statusTone(
        run.status,
      )}<a class="tf-tchip" href={jobHash(run.entity_id)} title={run.title ?? undefined}
        ><span class="tf-k">{run.key}</span>{#if run.title}{run.title}{/if}{#if run.status}
          <span class="tf-chip" class:tf-ok={tone === "ok"}>{run.status}</span
          >{/if}</a
      >{:else}<span class="tf-tchip"><span class="tf-k">{run.key}</span></span>{/if}{:else if run.type ===
    "mention"}<span class="tf-ment" class:tf-me={run.me}>{mentionText(run)}</span>{:else if run.type === "break"}<br
    />{/if}{/each}
