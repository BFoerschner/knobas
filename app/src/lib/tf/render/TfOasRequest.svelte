<!--
  An operation's request in the API explorer (ux31, sections 14 to 18, ADR-0057), beside its reference.

  - **Server**: the operation's servers from the spec (a `{variable}` a field, a select where the spec lists
    choices), then *Yours*, the servers saved in Settings › API servers.
  - **Sign in with**: the schemes the operation accepts, and *Nothing*. A secret is typed here and handed to the
    app once, on the first send (Björn: "I'd just type it into the openapi viewer page itself. not using stored
    credentials"): the app holds it for this origin until knobas quits, and the field then says so and holds nothing.
  - **In the path**, **In the query**, **Headers**: a row a parameter, a query or header row sent when ticked; typing
    ticks it; a required one is always sent.
  - **Body**: the media type and the text, with *Start again from the spec*.
  - **The line**: the method and the whole address as it will go out, a secret as dots; it follows every key.
  - **Send** (⌘Enter) and **Copy as curl**. A request that changes something on a host that is not this computer turns
    the row into a question in place, in the warn tone, once a host until knobas quits.
-->
<script lang="ts">
  import { secondsLabel } from "../../time/format";
  import { untrack } from "svelte";

  import type { Operation, SpecIndex } from "../../render/openapi";
  import {
    DEFAULT_MANUAL_API_KEY,
    MANUAL_API_KEY,
    MANUAL_BASIC,
    MANUAL_BEARER,
    authPlace,
    bodyText,
    curl,
    firstSchemeOf,
    hasTypedSecret,
    outgoing,
    requestLine,
    resolveScheme,
    serverUrl,
    shownHeaders,
    startDraft,
    type DraftParam,
    type RequestDraft,
  } from "../../render/openapi-request";
  import { schemeWords } from "../../render/openapi-view";
  import { apiCancel, apiForgetHeld, apiHeld, apiHoldSecret, apiSend, type ApiSend, type SavedServer } from "../../ipc/api";
  import { ipcErrorMessage } from "../../ipc";
  import { push } from "../../shell/toasts.svelte";
  import { confirmedHosts, type OasState, type Place } from "./oas.svelte";

  let {
    st,
    op,
    place,
    index,
    file,
    saved = [],
    bodies = {},
    onsent = () => {},
    sendKey = $bindable(null),
  }: {
    st: OasState;
    op: Operation;
    place: Place;
    index: SpecIndex;
    /** The spec's file, `repository\npath`, for *Sent since knobas started*. */
    file: string;
    /** The servers saved in Settings › API servers. */
    saved?: readonly SavedServer[];
    /** Each media type's starting body: the spec's example, else the one made from the schema. */
    bodies?: Record<string, unknown>;
    /** After a send, so the list of sends can read again. */
    onsent?: () => void;
    /** What ⌘Enter calls: this request's *Send*. */
    sendKey?: (() => void) | null;
  } = $props();

  let nextId = Date.now() % 1_000_000_000;

  // ---- The draft: started from the spec once, then what the person typed, kept for the operation ----
  const draft = $derived.by((): RequestDraft => {
    const kept = st.drafts.get(place);
    if (kept !== undefined) return kept;
    const made = startDraft(op, bodies, firstSchemeOf(op, index.security));
    // A value typed for a parameter of the same name, on this server, is offered here too.
    for (const p of made.params) {
      const was = st.sticky.get(p.name);
      if (p.value === "" && was !== undefined) {
        p.value = was;
        p.on = true;
      }
    }
    const last = saved.length > 0 && op.servers.length === 0 ? saved[0] : undefined;
    if (last !== undefined) made.server = { kind: "saved", id: last.id };
    untrack(() => st.drafts.set(place, made));
    return made;
  });
  /** Writes a change to the draft: a new object, so what reads it draws again. */
  function edit(change: (d: RequestDraft) => void) {
    const next: RequestDraft = { ...draft, params: draft.params.map((p) => ({ ...p })), server: { ...draft.server } as RequestDraft["server"] };
    change(next);
    st.drafts.set(place, next);
  }

  const specServer = $derived(draft.server.kind === "spec" ? op.servers[draft.server.index] : undefined);
  const savedServer = $derived(draft.server.kind === "saved" ? saved.find((s) => s.id === (draft.server as { id: string }).id) : undefined);
  const base = $derived(specServer !== undefined ? serverUrl(specServer, draft.server.kind === "spec" ? draft.server.variables : {}) : (savedServer?.url ?? ""));
  const manualKey = $derived(draft.manualKey ?? DEFAULT_MANUAL_API_KEY);
  const scheme = $derived(resolveScheme(draft.scheme, index.security, manualKey));
  const accepted = $derived(index.security.filter((s) => op.security.some((r) => r.includes(s.name))));
  const out = $derived(outgoing(op, draft, base, scheme));
  const line = $derived(requestLine(out));
  const origin = $derived.by(() => {
    try {
      return new URL(out.url).origin;
    } catch {
      return "";
    }
  });

  // ---- Secrets: typed here, handed to the app on the first send, never kept by the page ----
  let secret = $state("");
  let user = $state("");
  let editing = $state(false);
  let held = $state<string[]>([]);
  $effect(() => {
    const at = origin;
    if (at === "") {
      held = [];
      return;
    }
    void apiHeld(at).then((names) => {
      if (at === origin) held = names;
    }, () => {});
  });
  const isHeld = $derived(scheme !== null && held.includes(scheme.name));

  async function forgetHeld() {
    if (origin === "") return;
    try {
      await apiForgetHeld(origin);
      held = [];
      secret = "";
      user = "";
      editing = false;
      push({ text: `Forgot the secret held for ${out.host || origin}`, subject: "api-forget" });
    } catch (cause) {
      push({ text: `Could not forget: ${ipcErrorMessage(cause)}`, tone: "err" });
    }
  }

  // ---- Sending ----
  const sending = $derived(st.answers.get(place));
  const running = $derived(sending?.state === "running");
  let asking = $state<{ host: string } | null>(null);
  let dontAsk = $state(false);
  let timeoutS = $state(60);

  const why = $derived(out.blocked ?? (out.missing.length > 0 ? `Fill in ${out.missing.join(", ")}.` : scheme !== null && !isHeld && secret === "" ? null : null));

  /** A send was asked for with a path value missing: the empty fields are marked until filled (the mock's `missing`). */
  let tried = $state(false);

  async function sendNow(confirmed: string | null) {
    if (out.missing.length > 0) tried = true;
    if (running || out.blocked !== null || out.missing.length > 0 || op.webhook) return;
    if (!out.safe && !out.local && confirmed === null && !confirmedHosts.has(out.host)) {
      asking = { host: out.host };
      return;
    }
    asking = null;
    if (confirmed !== null && dontAsk) confirmedHosts.add(confirmed);
    const id = ++nextId;
    st.answers.set(place, { state: "running", id, line });
    try {
      if (scheme !== null && hasTypedSecret(scheme, secret, user)) {
        await apiHoldSecret(origin, scheme.name, secret, scheme.type === "http" && scheme.scheme === "basic" ? user : null);
        secret = "";
        user = "";
        editing = false;
        held = [...new Set([...held, scheme.name])];
      } else if (editing) {
        editing = false;
      }
      const auth = scheme !== null && (isHeld || held.includes(scheme.name)) ? { origin, scheme: scheme.name, ...authPlace(scheme) } : null;
      const send: ApiSend = {
        id,
        method: out.method,
        url: out.url,
        headers: out.headers.flatMap(([n, v]) => (typeof v === "string" ? [[n, v] as [string, string]] : [])),
        body: out.body,
        auth,
        timeout_s: timeoutS,
        confirmed_host: !out.safe && !out.local ? (confirmed ?? (confirmedHosts.has(out.host) ? out.host : null)) : null,
        server: savedServer?.id ?? null,
        file,
        operation: place,
      };
      const answer = await apiSend(send);
      st.answers.set(place, { state: "done", id, line, answer });
    } catch (cause) {
      st.answers.set(place, { state: "refused", line, words: ipcErrorMessage(cause) });
    }
    onsent();
  }

  function stop() {
    if (sending?.state === "running") void apiCancel(sending.id);
  }

  $effect(() => {
    sendKey = () => void sendNow(null);
    return () => (sendKey = null);
  });

  async function copyCurl() {
    try {
      await navigator.clipboard.writeText(curl(out));
      push({ text: "Copied as curl", subject: "copy-curl" });
    } catch (cause) {
      push({ text: `Could not copy: ${ipcErrorMessage(cause)}`, tone: "err" });
    }
  }

  function setParam(p: DraftParam, value: string) {
    edit((d) => {
      const at = d.params.find((x) => x.name === p.name && x.in === p.in)!;
      at.value = value;
      if (value !== "") at.on = true;
    });
    if (value !== "") st.sticky.set(p.name, value);
  }

  const groups = $derived(
    (["path", "query", "header"] as const)
      .map((where) => [where, draft.params.filter((p) => p.in === where)] as const)
      .filter(([, list]) => list.length > 0),
  );
  const WHERE = { path: "In the path", query: "In the query", header: "Headers" } as const;
</script>

<div class="tf-oas-req" aria-label="Request">
  <h3>Request</h3>
  {#if op.webhook}
    <p class="tf-oas-fnote">A webhook is what the API sends to you; knobas has nothing to send.</p>
  {:else}
    <div class="tf-oas-grid">
      <div class="tf-oas-prow">
      <label for="oas-server-{place}">Server</label>
      <select
        id="oas-server-{place}"
        value={draft.server.kind === "spec" ? `spec:${draft.server.index}` : `saved:${(draft.server as { id: string }).id}`}
        onchange={(e) => {
          const v = (e.currentTarget as HTMLSelectElement).value;
          edit((d) => (d.server = v.startsWith("spec:") ? { kind: "spec", index: Number(v.slice(5)), variables: {} } : { kind: "saved", id: v.slice(6) }));
        }}
      >
        {#if op.servers.length > 0}
          <optgroup label="From the spec">
            {#each op.servers as server, n (n)}<option value="spec:{n}">{server.url}{server.description !== "" ? ` (${server.description})` : ""}</option>{/each}
          </optgroup>
        {/if}
        {#if saved.length > 0}
          <optgroup label="Yours">
            {#each saved as server (server.id)}<option value="saved:{server.id}">{server.name}, {server.url}</option>{/each}
          </optgroup>
        {/if}
      </select>
      </div>
      {#if specServer !== undefined && draft.server.kind === "spec"}
        {@const vars = draft.server.variables}
        {#each Object.entries(specServer.variables) as [name, v] (name)}
          <div class="tf-oas-prow">
          <label class="tf-oas-code" for="oas-var-{place}-{name}">{`{${name}}`}</label>
          {#if v.enum.length > 0}
            <select id="oas-var-{place}-{name}" value={vars[name] ?? v.default} onchange={(e) => edit((d) => { if (d.server.kind === "spec") d.server.variables = { ...d.server.variables, [name]: (e.currentTarget as HTMLSelectElement).value }; })}>
              {#each v.enum as choice (choice)}<option value={choice}>{choice}</option>{/each}
            </select>
          {:else}
            <input class="tf-oas-inp" id="oas-var-{place}-{name}" value={vars[name] ?? v.default} oninput={(e) => edit((d) => { if (d.server.kind === "spec") d.server.variables = { ...d.server.variables, [name]: (e.currentTarget as HTMLInputElement).value }; })} />
          {/if}
          </div>
        {/each}
      {/if}
      {#if op.servers.length === 0 && saved.length === 0}
        <p class="tf-oas-hint">The spec names no server. Save one of yours in Settings › API servers.</p>
      {/if}

      <div class="tf-oas-prow">
        <label for="oas-scheme-{place}">Sign in with</label>
        <select
          id="oas-scheme-{place}"
          value={draft.scheme ?? ""}
          onchange={(e) => {
            const next = (e.currentTarget as HTMLSelectElement).value || null;
            edit((d) => (d.scheme = next));
            editing = false;
            secret = "";
            user = "";
          }}
        >
          {#if accepted.length > 0}
            <optgroup label="From the spec">
              {#each accepted as s (s.name)}<option value={s.name}>{schemeWords(s)} ({s.name})</option>{/each}
            </optgroup>
          {/if}
          <option value="">Nothing</option>
          {#if accepted.length > 0}
            <optgroup label="Manual">
              <option value={MANUAL_BASIC}>Basic</option>
              <option value={MANUAL_BEARER}>Bearer</option>
              <option value={MANUAL_API_KEY}>API key</option>
            </optgroup>
          {:else}
            <option value={MANUAL_BASIC}>Basic</option>
            <option value={MANUAL_BEARER}>Bearer</option>
            <option value={MANUAL_API_KEY}>API key</option>
          {/if}
        </select>
      </div>
      {#if draft.scheme === MANUAL_API_KEY}
        <div class="tf-oas-prow">
          <span class="tf-oas-k">Send in</span>
          <div class="tf-oas-row2">
            <span class="tf-seg" role="group" aria-label="API key placement">
              <button type="button" aria-pressed={manualKey.in === "header"} onclick={() => edit((d) => (d.manualKey = { ...(d.manualKey ?? DEFAULT_MANUAL_API_KEY), in: "header" }))}>Header</button>
              <button type="button" aria-pressed={manualKey.in === "query"} onclick={() => edit((d) => (d.manualKey = { ...(d.manualKey ?? DEFAULT_MANUAL_API_KEY), in: "query" }))}>Query</button>
            </span>
          </div>
        </div>
        <div class="tf-oas-prow">
          <label for="oas-keyname-{place}">{manualKey.in === "query" ? "Query name" : "Header name"}</label>
          <input
            class="tf-oas-inp"
            id="oas-keyname-{place}"
            autocomplete="off"
            spellcheck="false"
            value={manualKey.name}
            placeholder="X-API-Key"
            oninput={(e) => edit((d) => (d.manualKey = { ...(d.manualKey ?? DEFAULT_MANUAL_API_KEY), name: (e.currentTarget as HTMLInputElement).value }))}
          />
        </div>
      {/if}
      {#if scheme !== null}
        {#if isHeld && !editing}
          <div class="tf-oas-prow">
            <span class="tf-oas-k">Secret</span>
            <div class="tf-oas-row2">
              <span class="tf-oas-inp tf-oas-dots" aria-label="Held">••••••••••••</span>
              <button type="button" class="tf-btn tf-sm tf-ghost" onclick={() => (editing = true)}>Change</button>
              <button type="button" class="tf-btn tf-sm tf-ghost" onclick={() => void forgetHeld()}>Forget</button>
            </div>
          </div>
          <p class="tf-oas-hint">Held for {out.host || origin} until knobas quits, and sent nowhere else.</p>
        {:else}
          {#if scheme.type === "http" && scheme.scheme === "basic"}
            <div class="tf-oas-prow">
              <label for="oas-user-{place}">User name</label>
              <input class="tf-oas-inp" id="oas-user-{place}" autocomplete="off" bind:value={user} />
            </div>
          {/if}
          <div class="tf-oas-prow">
            <label for="oas-secret-{place}">{scheme.type === "http" && scheme.scheme === "basic" ? "Password" : scheme.type === "apiKey" ? (scheme.keyName || "API key") : "Token"}</label>
            <input class="tf-oas-inp" id="oas-secret-{place}" type="password" placeholder={scheme.type === "http" && scheme.scheme === "basic" ? "optional" : ""} autocomplete="off" spellcheck="false" bind:value={secret} />
          </div>
          {#if editing && isHeld}
            <p class="tf-oas-hint tf-oas-edithint">
              <span>Type a new secret and press Send to replace it.</span>
              <button type="button" class="tf-btn tf-sm tf-ghost" onclick={() => { editing = false; secret = ""; user = ""; }}>Cancel</button>
            </p>
          {:else}
            <p class="tf-oas-hint">Kept in memory until knobas quits, for {out.host || "this server"} only. Never saved.</p>
          {/if}
        {/if}
      {/if}

      {#each groups as [where, list] (where)}
        <span class="tf-oas-k tf-oas-sect">{WHERE[where]}</span>
        {#each list as p (`${p.in}:${p.name}`)}
          <!-- One row a parameter, on the grid's two columns: its tick and name, then its value (the mock's `reqHtml`). -->
          <!-- A row not sent draws its value in second ink: the spec's value shows what one looks like, the tick says it goes (Fable, 2026-11-22). -->
          <div class="tf-oas-prow" class:tf-oas-off={!(p.on || p.required)}>
            <label class="tf-oas-k tf-oas-code" for="oas-p-{place}-{p.in}-{p.name}" title={p.name}>
              {#if where !== "path"}
                <input type="checkbox" aria-label="Send {p.name}" checked={p.on || p.required} disabled={p.required} onchange={(e) => edit((d) => (d.params.find((x) => x.name === p.name && x.in === p.in)!.on = (e.currentTarget as HTMLInputElement).checked))} />
              {/if}
              <span class="tf-oas-pname">{p.name}</span>
            </label>
            <input
              class="tf-oas-inp"
              class:tf-oas-missing={tried && where === "path" && p.value === ""}
              id="oas-p-{place}-{p.in}-{p.name}"
              value={p.value}
              placeholder={p.required ? "required" : ""}
              aria-required={p.required}
              aria-invalid={tried && where === "path" && p.value === ""}
              spellcheck="false"
              oninput={(e) => setParam(p, (e.currentTarget as HTMLInputElement).value)}
            />
          </div>
        {/each}
      {/each}
    </div>

    {#if op.body !== null && op.body.content.length > 0}
      <div class="tf-oas-bodyhead">
        <span class="tf-oas-k">Body</span>
        {#if op.body.content.length > 1}
          <select aria-label="Media type" value={draft.media ?? ""} onchange={(e) => {
            const media = (e.currentTarget as HTMLSelectElement).value;
            edit((d) => { d.media = media; d.body = bodyText(media, bodies[media]); });
          }}>
            {#each op.body.content as m (m.type)}<option value={m.type}>{m.type}</option>{/each}
          </select>
        {:else}<span class="tf-oas-code tf-oas-fwhere">{draft.media}</span>{/if}
        <span class="tf-trail"><button type="button" class="tf-btn tf-sm tf-ghost" onclick={() => edit((d) => (d.body = d.media === null ? "" : bodyText(d.media, bodies[d.media])))}>Start again from the spec</button></span>
      </div>
      {#if draft.media !== null && /multipart\/form-data/.test(draft.media)}<p class="tf-oas-hint">knobas does not send files yet: a file field is left out.</p>{/if}
      <textarea class="tf-oas-body" spellcheck="false" aria-label="Body" value={draft.body} oninput={(e) => edit((d) => (d.body = (e.currentTarget as HTMLTextAreaElement).value))}></textarea>
    {/if}

    <div class="tf-oas-line" title={line}><span class="tf-oas-m">{out.method}</span><span>{line.slice(out.method.length + 1)}</span></div>
    {#if shownHeaders(out).length > 0}
      <details class="tf-oas-hdrs"><summary>{shownHeaders(out).length} {shownHeaders(out).length === 1 ? "header" : "headers"} knobas sends</summary>
        <dl class="tf-oas-kv">{#each shownHeaders(out) as [n, v], i (i)}<dt class="tf-oas-code">{n}</dt><dd class="tf-oas-code">{v}</dd>{/each}</dl>
      </details>
    {/if}

    {#if asking !== null}
      <div class="tf-oas-ask" role="alertdialog" aria-label="Send {out.method} to {asking.host}?">
        <p><code>{out.method}</code> changes something on {asking.host}. knobas sends it once and does not undo it.</p>
        <label><input type="checkbox" bind:checked={dontAsk} /> Don't ask again for {asking.host} until knobas quits</label>
        <div>
          <button type="button" class="tf-btn tf-sm tf-primary" onclick={() => void sendNow(asking!.host)}>Send {out.method}</button>
          <button type="button" class="tf-btn tf-sm tf-ghost" onclick={() => (asking = null)}>Cancel</button>
        </div>
      </div>
    {:else}
      <div class="tf-oas-send">
        {#if running}
          <button type="button" class="tf-btn tf-sm" onclick={stop}>Stop</button>
          <span class="tf-dim" role="status">Sending…</span>
        {:else}
          <button type="button" class="tf-btn tf-sm tf-primary" disabled={out.blocked !== null || out.missing.length > 0} title="Send (⌘Enter)" onclick={() => void sendNow(null)}>Send <kbd>⌘↵</kbd></button>
        {/if}
        <button type="button" class="tf-btn tf-sm tf-ghost" onclick={() => void copyCurl()}>Copy as curl</button>
        <label class="tf-oas-time"><span class="tf-dim">Give up after</span>
          <select aria-label="Give up after" bind:value={timeoutS}>
            {#each [5, 15, 30, 60, 120, 300, 600] as s (s)}<option value={s}>{secondsLabel(s)}</option>{/each}
          </select>
        </label>
        {#if why !== null}<span class="tf-dim">{why}</span>{/if}
      </div>
    {/if}
  {/if}
</div>
