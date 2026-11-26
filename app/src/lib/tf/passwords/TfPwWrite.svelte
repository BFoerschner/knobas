<!--
  The Passwords page's three writes (ADR-0056), as the clickdummy's dialogs: *Change password…* (the new value, the
  generator, a usage comment where the source asks one for a change, and that saving does not change the password where
  it is used), *New entry* (in the folder the list stands in) and *Edit* (no password: Edit reads none).

  **A write is direct.** It is sent when *Save* is pressed and answered before the dialog closes; while it is out the
  button reads *Saving…*. If the source does not answer, nothing is saved, the dialog stays open with what was typed,
  and it says so. A value typed or generated lives in this dialog's state and leaves it only in the write.
-->
<script lang="ts">
  import type { Layers } from "../layers.svelte";
  import TfDialog from "../settings/TfDialog.svelte";
  import TfPwGenerator from "./TfPwGenerator.svelte";
  import type { PwEntry, PwSource, Write, WriteAnswer } from "../../ipc/passwords";
  import type { Model, Node } from "./model";
  import { chainOf } from "./model";
  import { rememberComment, startingComment } from "./ports";

  let {
    kind,
    source,
    entry = null,
    folder = null,
    m,
    timedTicket,
    layers,
    send,
    copySecret,
    onclose,
    ondone,
  }: {
    kind: "change" | "new" | "edit";
    source: PwSource;
    entry?: PwEntry | null;
    /** Where a new entry goes: the folder the list stands in. */
    folder?: Node | null;
    m: Model;
    timedTicket: string | null;
    layers?: Layers | undefined;
    send: (write: Write) => Promise<WriteAnswer>;
    copySecret: (text: string, what: string) => Promise<unknown>;
    onclose: () => void;
    ondone: (entityId: string, words: string) => void;
  } = $props();

  // What the dialog starts with, read once: the props are the entry as it was when the dialog opened.
  // svelte-ignore state_referenced_locally
  const start = entry;
  let title = $state(start?.title ?? "");
  let username = $state(start?.username ?? "");
  let address = $state(start?.address ?? "");
  let notes = $state(start?.notes ?? "");
  let expires = $state(start?.expires ?? "");
  // svelte-ignore state_referenced_locally
  let into = $state<string>(start?.folder ?? (folder && folder.id !== "" && !folder.top ? folder.id : ""));
  let password = $state("");
  let codeKey = $state("");
  // svelte-ignore state_referenced_locally
  let comment = $state(needsComment() ? startingComment(timedTicket) : "");
  let saving = $state(false);
  let failed = $state<string | null>(null);
  let generator = $state<TfPwGenerator | null>(null);

  function needsComment(): boolean {
    if (!source.offers.usage_comments) return false;
    if (start) return start.comment.modify;
    const at = into ? m.nodes.get(into) : null;
    return at ? folderAsks(at) : false;
  }
  function folderAsks(at: Node): boolean {
    for (let n: Node | null = at; n !== null; n = n.parent) if (n.comment) return true;
    return false;
  }
  const asks = $derived(needsComment());

  /** The source's folders, as paths, for the folder select: a move stays inside its source. */
  const folders = $derived(
    [...m.nodes.values()]
      .filter((n) => n.id !== "" && !n.top && n.src === m.held.sources.findIndex((s) => s.id === source.id))
      // A new entry, or a move, goes only where the account may add.
      .filter((n) => m.held.folders.some((f) => f.id === n.id && f.can_add) || n.id === start?.folder)
      .map((n) => ({ id: n.id, path: chainOf(m, n).filter((x) => !x.top).map((x) => x.name).join(" / ") }))
      .sort((a, b) => a.path.localeCompare(b.path)),
  );

  const host = $derived.by(() => {
    const at = start?.address ?? "";
    try {
      return at ? new URL(at.includes("://") ? at : `x://${at}`).hostname || at : "";
    } catch {
      return at;
    }
  });

  const heading = $derived(kind === "change" ? `Change the password of ${start?.title ?? ""}` : kind === "edit" ? `Edit ${start?.title ?? ""}` : "New entry");
  const ready = $derived(
    !saving && (kind === "change" ? password !== "" : kind === "new" ? title.trim() !== "" && password !== "" : title.trim() !== "") && (!asks || comment.trim() !== ""),
  );
  const why = $derived(
    saving ? null : kind !== "edit" && password === "" ? "Type or generate a password." : kind !== "change" && title.trim() === "" ? "Give it a title." : asks && comment.trim() === "" ? "The source asks for a usage comment." : null,
  );

  const orNull = (v: string) => (v.trim() === "" ? null : v.trim());

  function write(): Write {
    const note = asks ? comment.trim() : null;
    if (kind === "change") {
      return { write: "set_password", entity: start!.id, password, comment: note, listed_in: start!.folder };
    }
    if (kind === "new") {
      return {
        write: "create_entry", source: source.id, folder: into || null, title: title.trim(), username: orNull(username), address: orNull(address),
        notes: orNull(notes), expires: expires || null, password, otp_key: orNull(codeKey), comment: note,
      };
    }
    // Only what changed.
    const edit: Extract<Write, { write: "edit_entry" }> = { write: "edit_entry", entity: start!.id, comment: note, listed_in: start!.folder };
    if (title.trim() !== start!.title) edit.title = title.trim();
    if (orNull(username) !== start!.username) edit.username = orNull(username);
    if (orNull(address) !== start!.address) edit.address = orNull(address);
    if (orNull(notes) !== start!.notes) edit.notes = orNull(notes);
    if ((expires || null) !== start!.expires) edit.expires = expires || null;
    if ((into || null) !== start!.folder) edit.folder = into || null;
    return edit;
  }

  async function save(event?: SubmitEvent): Promise<void> {
    event?.preventDefault();
    if (!ready) return;
    saving = true;
    failed = null;
    if (asks) rememberComment(comment.trim());
    try {
      const answer = await send(write());
      if (answer.kind === "written") {
        ondone(answer.entity_id, kind === "change" ? "Changed the password" : kind === "new" ? "Added the entry" : "Saved the entry");
        onclose();
        return;
      }
      failed =
        answer.kind === "unreachable"
          ? `${source.name} does not answer, so nothing was saved. A new password is never kept waiting on this machine.`
          : `${source.name} did not answer after the change was sent. Sync now to see whether it arrived.`;
    } catch (cause) {
      failed = String((cause as { message?: string })?.message ?? cause);
    } finally {
      saving = false;
    }
  }

  // A change of password starts with a generated one.
  $effect(() => {
    if (kind !== "edit" && generator !== null && password === "") void generator.make();
  });
</script>

<TfDialog title={heading} {...layers ? { layers } : {}} {onclose} {why}>
  {#snippet body()}
    <form class="tf-formgrid tf-pw-form" id="tf-pw-write" onsubmit={(event) => void save(event)}>
      {#if kind !== "change"}
        <label for="tf-pw-title">Title</label>
        <input id="tf-pw-title" type="text" autocomplete="off" bind:value={title} />
        <label for="tf-pw-user">Username</label>
        <input id="tf-pw-user" type="text" autocomplete="off" class="tf-mono" bind:value={username} />
        <label for="tf-pw-address">Address</label>
        <input id="tf-pw-address" type="text" autocomplete="off" class="tf-mono" bind:value={address} />
        <label for="tf-pw-folder">Folder</label>
        <select id="tf-pw-folder" bind:value={into}>
          <option value="">Not in a folder</option>
          {#each folders as f (f.id)}<option value={f.id}>{f.path}</option>{/each}
        </select>
        {#if source.offers.expiry}
          <label for="tf-pw-expires">Expires</label>
          <input id="tf-pw-expires" type="date" bind:value={expires} />
        {/if}
        <label for="tf-pw-notes">Notes</label>
        <textarea id="tf-pw-notes" rows="3" bind:value={notes}></textarea>
      {/if}
      {#if kind !== "edit"}
        <label for="tf-pw-new">{kind === "change" ? "New password" : "Password"}</label>
        <div class="tf-pw-newpw">
          <input id="tf-pw-new" type="text" autocomplete="off" spellcheck="false" class="tf-mono" bind:value={password} />
          <button type="button" class="tf-btn" onclick={() => void generator?.make()}>Generate another</button>
          <button type="button" class="tf-btn" disabled={password === ""} onclick={() => void copySecret(password, kind === "change" ? "new password" : "password")}>Copy</button>
        </div>
        <span></span>
        <TfPwGenerator bind:this={generator} onvalue={(value) => (password = value)} />
      {/if}
      {#if kind === "new" && source.offers.one_time_codes}
        <label for="tf-pw-key">Code key</label>
        <input id="tf-pw-key" type="text" autocomplete="off" spellcheck="false" class="tf-mono" placeholder="Base32, optional" bind:value={codeKey} />
      {/if}
      {#if asks}
        <label for="tf-pw-comment">Usage comment</label>
        <input id="tf-pw-comment" type="text" autocomplete="off" bind:value={comment} />
      {/if}
    </form>
    {#if kind === "change"}
      <p class="tf-dlg-note">Save stores the new password in the password manager. It does not change it{host ? ` at ${host}` : " where it is used"}: do that there, then save.</p>
    {:else if kind === "edit"}
      <p class="tf-dlg-note">Edit reads no password. A change is sent to {source.name} when you save, and nothing is kept here if it does not answer.</p>
    {/if}
    {#if asks}<p class="tf-dlg-note">{source.name} asks for a usage comment for every change here and keeps it in its log{source.account ? `, under ${source.account}` : ""}.</p>{/if}
    {#if failed}<p class="tf-dlg-note tf-stg-errt" role="alert">{failed}</p>{/if}
  {/snippet}
  {#snippet footer()}
    <button class="tf-btn tf-ghost" type="button" onclick={onclose}>Cancel</button>
    <button class="tf-btn tf-primary" type="submit" form="tf-pw-write" disabled={!ready}>{saving ? "Saving…" : "Save"}</button>
  {/snippet}
</TfDialog>
