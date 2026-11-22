<!--
  One entry (ADR-0056), as the clickdummy draws it, top to bottom: its path as links and its title; the offline line;
  the access gate; Username; Password; One-time code; Address; Expires; Notes; custom fields (a field the mirror holds
  no value for is a secret row); the foot line. It draws once its record has answered, in one step: the page asks it
  ahead when a row takes the selection or the pointer rests on it.
-->
<script lang="ts">
  import type { PwEntry, PwSource } from "../../ipc/passwords";
  import TfPwCode from "./TfPwCode.svelte";
  import TfPwSecret from "./TfPwSecret.svelte";
  import TfPwFile from "./TfPwFile.svelte";
  import type { Layers } from "../layers.svelte";
  import { attachmentKind } from "../job/attachments";
  import { ipcErrorMessage } from "../../ipc";
  import { push } from "../../shell/toasts.svelte";
  import { savedText } from "../builds/run-reads";
  import { formatBytes } from "../../sources/diagnostics";
  import { chainOf, commentFolder, expiry, isWeb, type Model } from "./model";
  import { PASSWORD, type PasswordPorts } from "./ports";
  import { stampLabel } from "../../time/format";

  let {
    entry,
    source,
    m,
    word,
    timedTicket,
    visible = true,
    ports,
    onretry,
    retrying = false,
    copied,
    onchange,
    onedit,
    layers,
  }: {
    entry: PwEntry;
    source: PwSource;
    m: Model;
    word: string;
    timedTicket: string | null;
    visible?: boolean;
    ports: PasswordPorts;
    onretry: () => void;
    retrying?: boolean;
    /** A plain copy landed (a username, an address): the page's toast. */
    copied: (what: string) => void;
    /** *Change password…* and *Edit*, drawn where the source performs them and the account may change the entry. */
    onchange?: () => void;
    onedit?: () => void;
    layers?: Layers | undefined;
  } = $props();
  /** The attachments the listing named (step 9a): each read live when opened or saved, never kept. */
  const files = $derived(source.offers.attachments && entry.attachments.state === "listed" ? entry.attachments.files : []);
  let viewing = $state<{ id: string; name: string; size: number | null } | null>(null);
  let saving = $state<string | null>(null);
  async function save(file: { id: string; name: string }): Promise<void> {
    saving = file.id;
    try {
      push({ text: savedText(await ports.saveAttachment(entry.id, file.id)) });
    } catch (cause) {
      push({ text: `Could not save ${file.name}: ${ipcErrorMessage(cause)}`, tone: "err" });
    } finally {
      saving = null;
    }
  }
  const canChange = $derived(entry.can_modify && source.offers.writes.includes("set_password"));
  const canEdit = $derived(entry.can_modify && source.offers.writes.includes("edit_entry"));

  let password = $state<TfPwSecret | null>(null);
  let code = $state<TfPwCode | null>(null);

  const folder = $derived(entry.folder !== null ? m.nodes.get(entry.folder) ?? null : null);
  const chain = $derived(chainOf(m, folder));
  const asks = $derived(commentFolder(folder));
  const scope = $derived(asks ? chainOf(m, asks).map((n) => n.name) : []);
  const down = $derived(source.down_at !== null);
  const blocked = $derived(entry.approval.state === "required");
  const x = $derived(source.offers.expiry ? expiry(entry.expires) : null);
  const Word = $derived(word.charAt(0).toUpperCase() + word.slice(1));
  const when = (iso: string | null) => (iso ? stampLabel(iso, new Date(), true) : "");

  /** The page's keys. */
  export function copyPassword(): void {
    void password?.press("copy");
  }
  export function togglePassword(): void {
    void password?.toggle();
  }
  export function copyCode(): void {
    void code?.copy();
  }
  export async function copyUsername(): Promise<void> {
    if (!entry.username) return;
    await ports.copyPlain(entry.username);
    copied("username");
  }
  export async function address(): Promise<void> {
    if (!entry.address) return;
    if (isWeb(entry.address)) await ports.openExternal(entry.address);
    else {
      await ports.copyPlain(entry.address);
      copied("address");
    }
  }
</script>

<article class="tf-panel tf-pw-entry" aria-label={entry.title}>
  <div class="tf-pw-ehead">
    <div class="tf-pw-etitle">
      <div class="tf-pw-epath">
        {#if chain.length === 0}Not in a folder{/if}
        {#each chain as node, i (node.id)}{#if i > 0}<span class="tf-pw-sl">/</span>{/if}<a href="#/passwords/{node.top ? `source/${encodeURIComponent(node.id.slice(4))}` : `in/${encodeURIComponent(node.id)}`}">{node.name}</a>{/each}
      </div>
      <h2>{entry.title}</h2>
    </div>
    {#if canChange || canEdit}
      <div class="tf-acts">
        {#if canChange}<button type="button" class="tf-btn" disabled={down || blocked} title={down ? `${source.name} does not answer` : undefined} onclick={onchange}>Change password…</button>{/if}
        {#if canEdit}<button type="button" class="tf-btn" aria-label="Edit" aria-keyshortcuts="E" disabled={down} title={down ? `${source.name} does not answer` : undefined} onclick={onedit}>Edit<kbd>E</kbd></button>{/if}
      </div>
    {/if}
  </div>
  {#if down}
    <div class="tf-stale tf-pw-enote" role="status">
      <span>{Word} did not answer at {when(source.down_at)}. This entry is as listed {source.synced_at ? `at ${when(source.synced_at)}` : "before"}, and its password can't be read until it answers.</span>
      <button type="button" class="tf-btn tf-sm" disabled={retrying} onclick={onretry}>{retrying ? "Trying…" : "Try again"}</button>
    </div>
  {/if}
  {#if blocked}
    <div class="tf-pw-gate" role="status">
      <span>
        {entry.approval.state === "required" && entry.approval.approver ? entry.approval.approver : "Someone"} approves each use of this entry. Its password{entry.otp ? " and its one-time code stay" : " stays"} hidden until then.
        {#if !source.offers.access_requests}Ask for it in {source.name}'s own client, then try again.{/if}
      </span>
    </div>
  {/if}
  {#if entry.username}
    <div class="tf-pw-f">
      <div class="tf-pw-fl">Username</div>
      <div class="tf-pw-fv tf-mono">{entry.username}</div>
      <div class="tf-pw-fa"><button type="button" class="tf-btn tf-sm" aria-label="Copy the username" aria-keyshortcuts="U" onclick={() => void copyUsername()}>Copy<kbd>U</kbd></button></div>
    </div>
  {/if}
  <TfPwSecret bind:this={password} {entry} which={PASSWORD} label="Password" main {source} {word} {scope} {timedTicket} {blocked} {ports} />
  {#if entry.otp && source.offers.one_time_codes}
    <TfPwCode bind:this={code} {entry} {word} {ports} {visible} />
  {/if}
  {#if entry.address}
    <div class="tf-pw-f">
      <div class="tf-pw-fl">Address</div>
      <div class="tf-pw-fv tf-mono">{entry.address}</div>
      <div class="tf-pw-fa">
        <button type="button" class="tf-btn tf-sm" aria-label={isWeb(entry.address) ? "Open the address" : "Copy the address"} aria-keyshortcuts="A" title={isWeb(entry.address) ? "Opens in your browser" : undefined} onclick={() => void address()}
          >{isWeb(entry.address) ? "Open" : "Copy"}<kbd>A</kbd></button
        >
      </div>
    </div>
  {/if}
  {#if x}
    <div class="tf-pw-f">
      <div class="tf-pw-fl">Expires</div>
      <div class="tf-pw-fv">{entry.expires} <span class="tf-chip{x.tone ? ` tf-${x.tone}` : ''}">{x.text.replace("Expires ", "")}</span></div>
      <div class="tf-pw-fa"></div>
    </div>
  {/if}
  {#if entry.notes}
    <div class="tf-pw-f tf-pw-top">
      <div class="tf-pw-fl">Notes</div>
      <div class="tf-pw-fv tf-pw-notes tf-pw-wide">{entry.notes}</div>
    </div>
  {/if}
  {#each entry.fields as field (field.name)}
    {#if field.value === null}
      <TfPwSecret {entry} which={{ kind: "field", name: field.name }} label={field.name} {source} {word} {scope} {timedTicket} {blocked} {ports} />
    {:else}
      <div class="tf-pw-f">
        <div class="tf-pw-fl">{field.name}</div>
        <div class="tf-pw-fv tf-mono">{field.value}</div>
        <div class="tf-pw-fa">
          <button type="button" class="tf-btn tf-sm" onclick={() => void ports.copyPlain(field.value ?? "").then(() => copied(field.name))}>Copy</button>
        </div>
      </div>
    {/if}
  {/each}
  {#if files.length > 0}
    <div class="tf-pw-f tf-pw-top">
      <div class="tf-pw-fl">Attachments</div>
      <div class="tf-pw-fv tf-pw-wide">
        <div class="tf-pw-files">
          {#each files as file (file.id)}
            <div class="tf-pw-file">
              <span class="tf-tkkind" aria-hidden="true">{attachmentKind({ name: file.name, mime: null })}</span>
              <span class="tf-pw-filename"><span class="tf-t">{file.name}</span>{#if file.size !== null}<span class="tf-rsub">{formatBytes(file.size)}</span>{/if}</span>
              <span class="tf-pw-fa">
                <button type="button" class="tf-btn tf-sm" disabled={down || blocked} title={down ? `${source.name} does not answer` : undefined} onclick={() => (viewing = file)}>Open</button>
                <button type="button" class="tf-btn tf-sm" disabled={down || blocked || saving === file.id} title={down ? `${source.name} does not answer` : undefined} onclick={() => void save(file)}>{saving === file.id ? "Saving…" : "Save"}</button>
              </span>
            </div>
          {/each}
        </div>
      </div>
    </div>
  {/if}
  <footer class="tf-pw-efoot">
    {#if entry.changed}Changed {when(entry.changed)}{entry.changed_by ? ` by ${entry.changed_by}` : ""}.{/if}
    Listed at the last sync{source.synced_at ? `, ${source.by_hand ? "by hand " : ""}at ${when(source.synced_at)}` : ""}.
    {#if !entry.can_modify}You can read this entry but not change it.{/if}
  </footer>
</article>

{#if viewing}
  <TfPwFile entityId={entry.id} file={viewing} {word} entryTitle={entry.title} {ports} {layers} onsave={() => viewing && void save(viewing)} onclose={() => (viewing = null)} />
{/if}
