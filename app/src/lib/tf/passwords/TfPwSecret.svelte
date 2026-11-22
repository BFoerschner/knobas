<!--
  One secret of an entry (ADR-0056): its password, or a custom field the mirror holds no value for. Covered with twelve
  dots whatever its length. *Show* draws it for thirty seconds with the comb counting down; *Copy* puts it on the
  clipboard on the Rust side and never brings it here. A copy of a shown value reads nothing.

  **Every press takes one path.** Offline and an open access request stop a read before it starts. Where the record
  says a usage comment is needed, the prompt opens in the row first (a field, pre-filled, `Enter` sends it); where the
  source answers that it needs one, it opens then. After a read on a source with an audit log the row says when it
  happened and that the source recorded it under the account.

  **Nothing moves under a pointer.** The row always reserves its hint line, said or not. The comment form is taller and
  opens in place: the person's own press.
-->
<script lang="ts">
  import type { PwEntry, PwSource, SecretRef } from "../../ipc/passwords";
  import TfPwComb from "./TfPwComb.svelte";
  import { clip, clipped } from "./clip.svelte";
  import { rememberComment, startingComment, type PasswordPorts } from "./ports";

  let {
    entry,
    which,
    label,
    main = false,
    source,
    word,
    scope,
    timedTicket,
    blocked = false,
    ports,
  }: {
    entry: PwEntry;
    which: SecretRef;
    label: string;
    /** The entry's password: the large row with the page's keys. */
    main?: boolean;
    source: PwSource;
    /** *the password manager*, or the source's name with several. */
    word: string;
    /** The folder that asks for a usage comment, as a path; empty when none asks. */
    scope: string[];
    timedTicket: string | null;
    /** Behind an access request that is not approved. */
    blocked?: boolean;
    ports: PasswordPorts;
  } = $props();

  type Verb = "show" | "copy";
  let reading = $state(false);
  let asking = $state<Verb | null>(null);
  let askScope = $state<string[]>([]);
  let draft = $state("");
  let shown = $state<{ value: string; until: number } | null>(null);
  let receipt = $state<{ at: string; comment: boolean } | null>(null);
  let failed = $state<string | null>(null);
  let input = $state<HTMLInputElement | null>(null);

  const down = $derived(source.down_at !== null);
  const what = $derived(which.kind === "password" ? "password" : which.kind === "field" ? which.name : "earlier password");
  const onClipboard = $derived(clip().entity_id === entry.id && clip().what === what && clip().until !== null);
  const clipUntil = $derived(clip().until ? Date.parse(clip().until!) : 0);
  const disabled = $derived(shown === null && (down || blocked || reading || !entry.can_read));
  const Word = $derived(word.charAt(0).toUpperCase() + word.slice(1));
  const why = $derived(
    down ? `${source.name} does not answer` : blocked ? "Hidden until your request is approved" : !entry.can_read ? "You may not read this entry's password" : "",
  );

  function clock(iso: string): string {
    const at = new Date(iso);
    return at.toLocaleTimeString("en-GB", { hour: "2-digit", minute: "2-digit", second: "2-digit" });
  }

  function ask(verb: Verb, scopeNow: string[]): void {
    asking = verb;
    askScope = scopeNow.length > 0 ? scopeNow : scope;
    draft = startingComment(timedTicket);
    queueMicrotask(() => {
      input?.focus();
      input?.select();
    });
  }

  /** Show or copy: from the hold if shown, else a read, asking a comment first where the record says so. */
  export async function press(verb: Verb, comment: string | null = null): Promise<void> {
    if (verb === "copy" && shown !== null) {
      // The value is in memory on the Rust side: a copy of it reads nothing.
    } else if (disabled) {
      return;
    }
    if (comment === null && shown === null && entry.comment.read && source.offers.usage_comments) {
      ask(verb, scope);
      return;
    }
    reading = shown === null;
    failed = null;
    try {
      const answer = verb === "show" ? await ports.show(entry.id, which, comment) : await ports.copy(entry.id, which, comment);
      switch (answer.kind) {
        case "shown":
          shown = { value: answer.value, until: Date.parse(answer.until) };
          receipt = { at: answer.read_at, comment: comment !== null };
          break;
        case "copied":
          clipped({ what, entity_id: entry.id, until: answer.until });
          if (shown === null) receipt = { at: answer.read_at, comment: comment !== null };
          break;
        case "comment_required":
          ask(verb, answer.scope);
          break;
        case "approval_required":
          failed = answer.approver ? `${answer.approver} approves each use of this entry.` : "This entry needs an approval. Ask for it in the password manager's own client, then try again.";
          break;
        case "unreachable":
          failed = `${Word} did not answer, so nothing was read.`;
          break;
        case "not_offered":
          failed = `${Word} has no ${what} for this entry.`;
          break;
        case "refused":
          // The server's own sentence: "Access denied. … does not have permission to view the password for …".
          failed = answer.reason;
          break;
      }
    } catch (cause) {
      failed = cause instanceof Error ? cause.message : String((cause as { message?: string })?.message ?? cause);
    } finally {
      reading = false;
    }
  }

  export async function toggle(): Promise<void> {
    if (shown !== null) hide();
    else await press("show");
  }

  function hide(): void {
    shown = null;
    void ports.hide(entry.id, which).catch(() => {});
  }

  function send(event: SubmitEvent): void {
    event.preventDefault();
    const comment = draft.trim();
    if (comment === "" || asking === null) return;
    const verb = asking;
    asking = null;
    rememberComment(comment);
    void press(verb, comment);
  }

  /** The person's two candidates: what the field starts with, and the timed ticket where they differ. */
  const other = $derived(timedTicket !== null && draft.trim() !== timedTicket ? timedTicket : null);
</script>

<div class="tf-pw-f" class:tf-pw-top={asking !== null}>
  <div class="tf-pw-fl">{label}</div>
  <div class="tf-pw-fv">
    {#if shown}
      <code class="tf-pw-clear">{#each [...shown.value] as ch, i (i)}{#if /[0-9]/.test(ch)}<span class="tf-pw-dg">{ch}</span>{:else if /[^A-Za-z0-9]/.test(ch)}<span class="tf-pw-sy">{ch}</span>{:else}{ch}{/if}{/each}</code>
    {:else}
      <span class="tf-pw-cover" role="img" aria-label="Hidden"></span>
    {/if}
  </div>
  <div class="tf-pw-fa">
    <button type="button" class="tf-btn{main ? '' : ' tf-sm'}" aria-label={shown ? "Hide" : "Show"} aria-keyshortcuts={main ? "S" : undefined} disabled={shown === null && disabled} title={why || undefined} onclick={() => void toggle()}
      >{shown ? "Hide" : "Show"}{#if main}<kbd>S</kbd>{/if}</button
    >
    <button type="button" class="tf-btn{main ? ' tf-primary' : ' tf-sm'}" aria-label="Copy" aria-keyshortcuts={main ? "C" : undefined} disabled={disabled && shown === null} title={why || undefined} onclick={() => void press("copy")}
      >Copy{#if main}<kbd>C</kbd>{/if}</button
    >
  </div>
  {#if asking}
    <form class="tf-pw-ask" onsubmit={send}>
      <label for="tf-pw-ask-{entry.id}">Usage comment</label>
      <input
        class="tf-pw-in"
        id="tf-pw-ask-{entry.id}"
        type="text"
        autocomplete="off"
        required
        bind:value={draft}
        bind:this={input}
        onkeydown={(event) => {
          if (event.key === "Escape") {
            event.preventDefault();
            event.stopPropagation();
            asking = null;
          }
        }}
      />
      <button class="tf-btn tf-primary" type="submit">{asking === "show" ? "Show" : "Copy"}{which.kind === "password" ? " password" : ""} <kbd>⏎</kbd></button>
      <button class="tf-btn tf-ghost" type="button" onclick={() => (asking = null)}>Cancel</button>
      <p>
        {askScope.join(" / ") || "This folder"} requires one for every read. {Word} keeps it in its log{source.account ? `, under ${source.account}` : ""}.
        {#if other}<button type="button" class="tf-linkish" onclick={() => (draft = other)}>Use {other}</button>{/if}
      </p>
    </form>
  {:else}
    <div class="tf-pw-fh" class:tf-pw-two={main} role="status">
      {#if reading}
        Reading from {word}…
      {:else if failed}
        <span class="tf-stg-errt">{failed}</span>
      {:else if shown || onClipboard}
        {#if shown}<span class="tf-pw-timeleft"
            ><TfPwComb until={shown.until} label={["Hides itself in ", "."]} ended={() => (shown = null)} /></span
          >{/if}
        {#if onClipboard}<span class="tf-pw-timeleft"><TfPwComb until={clipUntil} size="sm" label={["On the clipboard. It clears in ", "."]} /></span>{/if}
        {#if receipt && source.offers.audit_log}Read at {clock(receipt.at)}. {Word} recorded it{source.account ? ` under ${source.account}` : ""}{receipt.comment ? ", with your comment" : ""}.{/if}
      {:else if main}
        {#if blocked}Hidden until your request is approved.
        {:else if down}Can't be read while {word} does not answer.
        {:else if !entry.can_read}You may not read this entry's password.
        {:else if entry.comment.read && source.offers.usage_comments}Asks for a usage comment first{scope.length ? `: ${scope.join(" / ")} requires one` : ""}.
        {:else if source.offers.audit_log}{Word} records each read{source.account ? ` under ${source.account}` : ""}.
        {:else}&nbsp;{/if}
      {:else}&nbsp;{/if}
    </div>
  {/if}
</div>
