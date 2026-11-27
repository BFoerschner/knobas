<!--
  An answer in the API explorer (ux31, sections 19 and 20). Here a status is something that happened, so it takes a
  tone: `--ok` under 300, `--warn` under 400, `--err` from 400.

  - One line first: the status, its words, the time, the size, the content type.
  - *Body* / *Headers N*. The body is drawn by knobas's file renderers and runs through its file pipelines
    (`TfOasBody`, #2533): *Render as*, *Pipeline*, the compact JSON and XML views with their query, *Expand*. HTML is
    never rendered. A body over 8 MiB is not drawn until a pipeline makes text of it; *Save to Downloads* saves it.
  - A failure is a block with the error edge: what failed and what to check.
  - While it runs the box holds its room, so nothing below moves when it fills.
  - *Sent since knobas started*: the sends of this spec, in the app's memory only.
-->
<script lang="ts">
  import type { ApiAnswer, SentLine } from "../../ipc/api";
  import TfOasBody from "./TfOasBody.svelte";
  import { sized } from "../../render/pipeline";
  import { statusTone, type BodyView } from "./oas-answer";
  import type { Sending } from "./oas.svelte";

  let {
    sending,
    sent = [],
    onopen = () => {},
    onexpand = null,
  }: {
    sending: Sending | undefined;
    /** What was sent from this spec since knobas started, the newest first. */
    sent?: readonly SentLine[];
    /** Opens a send of the list: its operation, its request and its answer. */
    onopen?: (line: SentLine) => void;
    /** *Expand*: the body over the explorer's pane, with what the person set on it. */
    onexpand?: ((what: { answer: Extract<ApiAnswer, { kind: "answered" }>; view: BodyView; line: string }) => void) | null;
  } = $props();

  const answer = $derived(sending?.state === "done" ? (sending.answer as ApiAnswer) : null);
  const ok = $derived(answer !== null && answer.kind === "answered" ? answer : null);
  let tab = $state<"body" | "headers">("body");
  /** The body's steps and pick: kept while the column stands, so a send again draws as the last did. */
  const view = $state<BodyView>({ steps: null, as: null });
</script>

<div class="tf-oas-ans" aria-label="Answer">
  {#if sending === undefined}
    <p class="tf-oas-fnote">Nothing sent yet. knobas sends only when you press Send.</p>
  {:else if sending.state === "running"}
    <p class="tf-oas-wait" role="status">Sending…</p>
  {:else if sending.state === "refused"}
    <div class="tf-oas-fail" role="alert"><p><b>Not sent.</b></p><p>{sending.words}</p></div>
  {:else if answer !== null && answer.kind === "failed"}
    <div class="tf-oas-fail" role="alert">
      <p><b>{answer.stage === "stopped" ? "Stopped" : answer.stage === "timeout" ? "No answer in time" : "No answer"}</b></p>
      <p>{answer.words}</p>
    </div>
  {:else if ok !== null}
    <div class="tf-oas-status">
      <span class="tf-oas-code3 tf-{statusTone(ok.status)}">{ok.status}</span>
      <span>{ok.reason}</span>
      <span class="tf-dim">{ok.total_ms.toLocaleString("en")} ms</span>
      <span class="tf-dim">{sized(ok.bytes)}</span>
      {#if ok.content_type !== ""}<span class="tf-dim tf-oas-code">{ok.content_type}</span>{/if}
      <span class="tf-trail">
        <span class="tf-seg" role="group" aria-label="Show">
          <button type="button" aria-pressed={tab === "body"} onclick={() => (tab = "body")}>Body</button>
          <button type="button" aria-pressed={tab === "headers"} onclick={() => (tab = "headers")}>Headers {ok.headers.length}</button>
        </span>
      </span>
    </div>
    {#if ok.hops.length > 0}<p class="tf-oas-hint">Followed {ok.hops.length === 1 ? "one redirect" : `${ok.hops.length} redirects`} on the same origin.</p>{/if}
    {#if ok.follow !== null}<p class="tf-oas-hint">It points to <code>{ok.follow}</code>. knobas does not follow a redirect there by itself.</p>{/if}
    {#if ok.cut}<p class="tf-oas-hint">The answer is longer than 64 MiB. knobas kept its first 64 MiB.</p>{/if}
    {#if tab === "headers"}
      {#if ok.headers.length === 0}
        <p class="tf-oas-fnote">No headers.</p>
      {:else}
        <dl class="tf-oas-anshdrs">
          {#each ok.headers as [n, v], i (i)}
            <div class="tf-oas-anshdr"><dt class="tf-oas-code">{n}</dt><dd class="tf-oas-code">{v}</dd></div>
          {/each}
        </dl>
      {/if}
    {:else}
      {@const shownOk = ok}
      <TfOasBody
        answer={shownOk}
        {view}
        onexpand={onexpand === null || sending?.state !== "done" ? null : () => onexpand?.({ answer: shownOk, view, line: sending?.state === "done" ? sending.line : "" })}
      />
    {/if}
  {/if}

  {#if sent.length > 0}
    <h3 class="tf-oas-sentlab">Sent since knobas started <span class="tf-oas-n">{sent.length}</span></h3>
    <div class="tf-oas-sent">
      {#each sent as s (s.id)}
        <button type="button" class="tf-oas-sentrow" onclick={() => onopen(s)} title={s.send.url}>
          <span class="tf-oas-m">{s.send.method}</span>
          <span class="tf-oas-rpath">{s.send.operation.replace(/^\S+ /, "")}</span>
          {#if s.answer.kind === "answered"}<span class="tf-{statusTone(s.answer.status)}">{s.answer.status}</span><span>{s.answer.total_ms} ms</span>
          {:else}<span class="tf-err">{s.answer.stage}</span><span></span>{/if}
        </button>
      {/each}
    </div>
  {/if}
</div>
