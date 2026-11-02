<!--
  The ticket page's conversation (#819, R-5.2 to R-5.4, #2399): the whole
  thread in the reading column, oldest first, with no fold and no filter, and
  the compose box at its end. A blue rule marks where the reader's last visit
  ends; new comments are edged blue, mentions edged yellow with a chip and the
  `@name` in blue. The thread is the ticket's comments and nothing else
  (#2163). The rules are `conversation.ts`'s; this draws them.

  The comment box at the end queues a `Comment` write on any ticket, the
  chosen job or not (spec #2396): the box clears on queue, and the comment is
  drawn at the end of the thread at once, muted with its echo (#1424, #2103),
  *queued* and then *sent*, until a read of the thread (the refresh after the
  write, or a run's end) holds its words. *Edit and resend* on a held or
  refused one puts its words back in the box. A comment whose source writes
  markup (`markdown`, `jira_wiki`, #2308) is drawn with the Markdown
  component. A comment the source's credential cannot make (`refused`, #2052)
  draws *Comment* disabled with the reason under the box, and queues nothing.
  The box is one line until it is used (`job.css`).
-->
<script lang="ts">
  import type { Thread } from "../../ipc/entity";
  import TfMarkdown from "../markdown/TfMarkdown.svelte";
  import { proseMarkdown } from "../../render/prose";
  import { refusedAttrs } from "../../shell/refused-writes";
  import { takeAgain, type Echo } from "../queued.svelte";
  import { initials } from "../tickets/tickets";
  import { firstName, mentionRuns, since, stamp, wholeThread } from "./conversation";

  let {
    jobKey,
    entityId,
    queued = [],
    thread,
    lastVisit,
    failure,
    now,
    refused = null,
    oncomment,
  }: {
    jobKey: string;
    /** The ticket's entity id, which *Edit and resend* names. */
    entityId: string;
    /** The comments queued here that the thread does not hold yet (#1424). */
    queued?: Pick<Echo, "text" | "state">[];
    /** `null` while the read is out. */
    thread: Thread | null;
    /** When the reader last opened this job before now; `null` for never. */
    lastVisit: string | null;
    /** What the thread's read failed with, if it did. */
    failure: string | null;
    now: Date;
    /** Why the source's credential cannot comment here (#2052), or `null`. */
    refused?: string | null;
    /** Queue a comment; resolves whether it was queued, so the box keeps a refused text. */
    oncomment: (body: string) => Promise<boolean>;
  } = $props();

  let draft = $state("");
  let sending = $state(false);
  let box = $state<HTMLTextAreaElement | null>(null);

  // *Edit and resend* on a held or refused comment: its words back in the box.
  $effect(() => {
    const again = takeAgain(entityId, "comment");
    if (again === null) return;
    draft = again;
    box?.focus();
  });

  /** The reader as the thread names them: the author of their own comments, else their account. */
  const myName = $derived(thread?.comments.find((c) => c.mine)?.author ?? thread?.me[0] ?? null);
  const drawn = $derived(thread === null ? null : wholeThread(thread, lastVisit));

  async function send() {
    const body = draft.trim();
    if (body === "" || sending || refused !== null) return;
    sending = true;
    try {
      if (await oncomment(body)) draft = "";
    } finally {
      sending = false;
    }
  }
</script>

<div class="tf-convo">
  {#if failure !== null}
    <p class="tf-dim">Could not read the conversation: {failure}</p>
  {:else if thread === null || drawn === null}
    <p class="tf-dim">Reading the conversation…</p>
  {:else}
    {#each drawn.entries as entry, index (index)}
      {@const comment = entry.comment}
      {@const markdown = proseMarkdown(comment.body, comment.markup)}
      {#if index === drawn.newAt && lastVisit !== null}
        <div class="tf-newline">New since your last visit, {since(lastVisit, now)}</div>
      {/if}
      <article class="tf-msg" class:tf-new={entry.fresh} class:tf-mention={comment.mentions_me}>
        <span class="tf-av" class:tf-me={comment.mine} aria-hidden="true">{initials(comment.author)}</span>
        <div class="tf-msg-body">
          <div class="tf-who">
            <b>{comment.mine ? "You" : firstName(comment.author)}</b>{stamp(comment.at, now)}
            {#if comment.mentions_me}<span class="tf-chip tf-chip-mention">mentions you</span>{/if}
          </div>
          {#if markdown !== null}
            <TfMarkdown text={markdown} />
          {:else}
            <p>
              {#each mentionRuns(comment.body, thread.me) as run, at (at)}{#if run.at}<span class="tf-at">{run.text}</span>{:else}{run.text}{/if}{/each}
            </p>
          {/if}
        </div>
      </article>
    {/each}
    {#if drawn.entries.length === 0 && queued.length === 0}
      <p class="tf-dim">No comments yet.</p>
    {/if}
  {/if}
  {#each queued as echo, n (n)}
    <article class="tf-msg tf-queued">
      <span class="tf-av tf-me" aria-hidden="true">{initials(myName)}</span>
      <div class="tf-msg-body">
        <div class="tf-who"><b>You</b><span class="tf-chip tf-info">{echo.state}</span></div>
        <p>{echo.text}</p>
      </div>
    </article>
  {/each}
  <div class="tf-compose">
    <textarea bind:this={box} bind:value={draft} rows="1" placeholder="Comment on {jobKey}" aria-label="Comment on {jobKey}"></textarea>
    <div class="tf-acts">
      {#if refused !== null}<span class="tf-dim tf-small">{refused}</span>{/if}
      <button type="button" class="tf-btn tf-primary" {...refusedAttrs(refused)} disabled={draft.trim() === "" || sending} onclick={() => void send()}>Comment</button>
    </div>
  </div>
</div>
