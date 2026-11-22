<!--
  An answer in the API explorer (ux31, sections 19 and 20). Here a status is something that happened, so it takes a
  tone: `--ok` under 300, `--warn` under 400, `--err` from 400.

  - One line first: the status, its words, the time, the size, the content type.
  - *Body* / *Raw* / *Headers N*. JSON is indented by Rust and drawn as numbered lines, windowed, its keys, strings and
    numbers told apart by the syntax tokens; other text as written. HTML is never rendered. A body over 8 MiB is not
    drawn: it says its size and offers *Save to Downloads*.
  - A failure is a block with the error edge: what failed and what to check.
  - While it runs the box holds its room, so nothing below moves when it fills.
  - *Sent since knobas started*: the sends of this spec, in the app's memory only.
-->
<script lang="ts">
  import { Windowed } from "../../shell/windowed.svelte";
  import { apiAnswerBody, apiSaveAnswer, type ApiAnswer, type SentLine } from "../../ipc/api";
  import { ipcErrorMessage } from "../../ipc";
  import { push } from "../../shell/toasts.svelte";
  import { jsonTokens, statusTone, sizeWords } from "./oas-answer";
  import type { Sending } from "./oas.svelte";

  let {
    sending,
    sent = [],
    onopen = () => {},
  }: {
    sending: Sending | undefined;
    /** What was sent from this spec since knobas started, the newest first. */
    sent?: readonly SentLine[];
    /** Opens a send of the list: its operation, its request and its answer. */
    onopen?: (line: SentLine) => void;
  } = $props();

  /** The most of a body that is drawn. */
  const DRAWN = 8 * 1024 * 1024;

  const answer = $derived(sending?.state === "done" ? (sending.answer as ApiAnswer) : null);
  const ok = $derived(answer !== null && answer.kind === "answered" ? answer : null);
  let tab = $state<"body" | "raw" | "headers">("body");
  let body = $state<{ id: number; text: string; pretty: boolean } | null>(null);
  let gone = $state(false);

  const isJson = $derived(ok !== null && /json/i.test(ok.content_type));
  const drawable = $derived(ok !== null && ok.bytes <= DRAWN && !/^(image|audio|video)\/|octet-stream|zip|pdf/i.test(ok.content_type));

  $effect(() => {
    const a = ok;
    const view = tab;
    if (a === null || !drawable || view === "headers") return;
    const pretty = view === "body" && isJson;
    if (body !== null && body.id === a.id && body.pretty === pretty) return;
    gone = false;
    void apiAnswerBody(a.id, pretty).then(
      (bytes) => {
        if (ok?.id === a.id) body = { id: a.id, text: new TextDecoder().decode(bytes), pretty };
      },
      () => (gone = true),
    );
  });

  const lines = $derived(body === null ? [] : body.text.split("\n"));
  const win = new Windowed<number>({ count: () => lines.length, key: (i) => i, estimate: 19, scrolls: true });

  async function copy() {
    if (body === null) return;
    try {
      await navigator.clipboard.writeText(body.text);
      push({ text: "Copied the answer", subject: "copy-answer" });
    } catch (cause) {
      push({ text: `Could not copy: ${ipcErrorMessage(cause)}`, tone: "err" });
    }
  }

  async function save() {
    if (ok === null) return;
    const ext = isJson ? "json" : /xml/.test(ok.content_type) ? "xml" : /html/.test(ok.content_type) ? "html" : /text/.test(ok.content_type) ? "txt" : "bin";
    try {
      const path = await apiSaveAnswer(ok.id, `answer-${ok.status}.${ext}`);
      push({ text: `Saved to ${path}`, subject: "save-answer" });
    } catch (cause) {
      push({ text: `Could not save: ${ipcErrorMessage(cause)}`, tone: "err" });
    }
  }
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
      <span class="tf-dim">{sizeWords(ok.bytes)}</span>
      {#if ok.content_type !== ""}<span class="tf-dim tf-oas-code">{ok.content_type}</span>{/if}
      <span class="tf-trail">
        <span class="tf-seg" role="group" aria-label="Show">
          <button type="button" aria-pressed={tab === "body"} onclick={() => (tab = "body")}>Body</button>
          <button type="button" aria-pressed={tab === "raw"} onclick={() => (tab = "raw")}>Raw</button>
          <button type="button" aria-pressed={tab === "headers"} onclick={() => (tab = "headers")}>Headers {ok.headers.length}</button>
        </span>
      </span>
    </div>
    {#if ok.hops.length > 0}<p class="tf-oas-hint">Followed {ok.hops.length === 1 ? "one redirect" : `${ok.hops.length} redirects`} on the same origin.</p>{/if}
    {#if ok.follow !== null}<p class="tf-oas-hint">It points to <code>{ok.follow}</code>. knobas does not follow a redirect there by itself.</p>{/if}
    {#if ok.cut}<p class="tf-oas-hint">The answer is longer than 64 MiB. knobas kept its first 64 MiB.</p>{/if}
    {#if tab === "headers"}
      <dl class="tf-oas-kv tf-oas-anshdrs">{#each ok.headers as [n, v], i (i)}<dt class="tf-oas-code">{n}</dt><dd class="tf-oas-code">{v}</dd>{/each}</dl>
    {:else if !drawable}
      <p class="tf-oas-fnote">{ok.content_type || "A body"} of {sizeWords(ok.bytes)}, not drawn here.</p>
    {:else if gone}
      <p class="tf-oas-fnote">The answer is no longer kept.</p>
    {:else if body !== null}
      {#if tab === "raw" || !isJson}
        <pre class="tf-oas-ansbody tf-oas-wrap">{body.text}</pre>
      {:else}
        <div class="tf-oas-ansbody tf-oas-anslines" {@attach win.list}>
          {#if win.before > 0}<div class="tf-wgap" aria-hidden="true" {@attach win.room("before")}></div>{/if}
          {#each win.slice(lines) as l, n (win.start + n)}
            <div class="tf-oas-al" {@attach win.row(win.start + n)}><span class="tf-oas-ln">{win.start + n + 1}</span><span>{#each jsonTokens(l) as t, m (m)}{#if t.k === ""}{t.t}{:else}<span class="tf-oas-t{t.k}">{t.t}</span>{/if}{/each}</span></div>
          {/each}
          {#if win.after > 0}<div class="tf-wgap" aria-hidden="true" {@attach win.room("after")}></div>{/if}
        </div>
      {/if}
    {:else}
      <div class="tf-oas-ansbody"></div>
    {/if}
    <div class="tf-oas-send">
      {#if body !== null}<button type="button" class="tf-btn tf-sm tf-ghost" onclick={() => void copy()}>Copy</button>{/if}
      <button type="button" class="tf-btn tf-sm tf-ghost" onclick={() => void save()}>Save to Downloads</button>
    </div>
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
