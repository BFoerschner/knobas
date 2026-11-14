<!--
  The ticket page's conversation (#819, R-5.2 to R-5.4, #2399): the whole
  thread in the reading column, oldest first, with no fold and no filter, and
  the compose box at its end. A blue rule marks where the reader's last visit
  ends; new comments are edged blue, mentions edged yellow with a chip and the
  `@name` in blue. Opened from a mention (`toMention`, the ticket's
  `/mention` address) it brings the newest comment that mentions the reader
  into view once the thread is drawn. The thread is the ticket's comments and nothing else
  (#2163). The rules are `conversation.ts`'s; this draws them.

  The comment box at the end queues a `Comment` write on any ticket, the
  chosen job or not (spec #2396): the box clears on queue, and the comment is
  drawn at the end of the thread at once, muted with its echo (#1424, #2103),
  *queued* and then *sent*, until a read of the thread (the refresh after the
  write, or a run's end) holds its words. *Edit and resend* on a held or
  refused one puts its words back in the box. What is typed is the ticket's own draft (`drafts`), through
  `Esc`, a move to another ticket and a return, and ⌘Enter sends it. A comment whose source writes
  markup (`markdown`, `jira_wiki`, #2308) is drawn with the Markdown
  component, its `!name!` images and `[^name]` links resolved against the
  ticket's attachments and drawn as the description's are (#2415). A
  comment the source's credential cannot make (`refused`, #2052) draws
  *Comment* disabled with the reason under the box, and queues nothing.
  The box is one line until it is used (`job.css`).
-->
<script lang="ts">
  import TfStateChip from "../TfStateChip.svelte";
  import { flushSync, tick, untrack } from "svelte";

  import type { Attachment, Thread } from "../../ipc/entity";
  import type { AttachmentImages } from "../../render/markdown";
  import TfMarkdown from "../markdown/TfMarkdown.svelte";
  import { proseMarkdown } from "../../render/prose";
  import { refusedAttrs } from "../../shell/refused-writes";
  import { drafts } from "../drafts.svelte";
  import { sendKey } from "../inline-editor";
  import { takeAgain, type Echo } from "../queued.svelte";
  import { initials } from "../tickets/tickets";
  import { firstName, mentionRuns, since, stamp, wholeThread } from "./conversation";
  import { Windowed } from "../../shell/windowed.svelte";
  import { textHeight } from "../../shell/windowed";

  let {
    jobKey,
    entityId,
    queued = [],
    thread,
    lastVisit,
    failure,
    now,
    refused = null,
    toMention = false,
    oncomment,
    attachments = [],
    images = null,
    selfName = null,
  }: {
    /** What the page's card calls the reader, where it does: the composer's avatar before they have commented (F-107). */
    selfName?: string | null;
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
    /** Opened from a mention: bring the newest comment that mentions the reader into view, once. */
    toMention?: boolean;
    /** Queue a comment; resolves whether it was queued, so the box keeps a refused text. */
    oncomment: (body: string) => Promise<boolean>;
    /** The ticket's attachments, which a wiki comment's `!name!` and `[^name]` name (#2415). */
    attachments?: readonly Attachment[];
    /** How those draw and open: the page's inline images. */
    images?: AttachmentImages | null;
  } = $props();

  const commentKey = $derived(`comment:${entityId}`);
  const draft = $derived(drafts.get(commentKey));
  let sending = $state(false);
  let box = $state<HTMLTextAreaElement | null>(null);

  // *Edit and resend* on a held or refused comment: its words back in the box.
  $effect(() => {
    const again = takeAgain(entityId, "comment");
    if (again === null) return;
    untrack(() => drafts.put(commentKey, again));
    box?.focus();
  });

  /** The reader as the thread names them: the author of their own comments, else as the card does, else their account. */
  const myName = $derived(thread?.comments.find((c) => c.mine)?.author ?? selfName ?? thread?.me[0] ?? null);
  const drawn = $derived(thread === null ? null : wholeThread(thread, lastVisit));

  /** What the thread draws, in order: each comment, and the *New since your last visit* rule before the first new one. */
  type Row = { key: string; entry: (NonNullable<typeof drawn>["entries"])[number] } | { key: "new"; entry: null };
  const rows = $derived.by((): Row[] => {
    if (drawn === null) return [];
    const out: Row[] = [];
    drawn.entries.forEach((entry, index) => {
      if (index === drawn.newAt && lastVisit !== null) out.push({ key: "new", entry: null });
      out.push({ key: String(index), entry });
    });
    return out;
  });
  /**
   * A long thread draws only the comments near the viewport (the extremes pass, 2026-11-13: 500 comments were built
   * whole as the ticket opened). By place: a thread only grows at its end.
   */
  const win = new Windowed({
    count: () => rows.length,
    key: (i) => rows[i]!.key,
    estimate: 76,
    gap: 10,
    estimateOf: (key) => estimates.get(key) ?? 76,
  });
  /** Each row's room before it is drawn: a comment's by its length. */
  const estimates = $derived(new Map(rows.map((row) => [row.key, row.entry ? textHeight(row.entry.comment.body, { base: 46, line: 21, chars: 100 }) : 18])));

  let convo = $state<HTMLDivElement | null>(null);
  /** Whether this opening has been brought to its mention: once a thread, so a re-read does not scroll again. */
  let broughtTo = $state(false);

  // Opened from a mention: the newest comment that mentions the reader, once
  // the thread is drawn. A mention in the description has no comment, and
  // the page opens at its top, where the description is.
  $effect(() => {
    if (!toMention) {
      broughtTo = false;
      return;
    }
    if (drawn === null || convo === null || untrack(() => broughtTo)) return;
    // The newest mention may be a comment the thread holds back: brought into the drawn rows first.
    const at = untrack(() => {
      for (let i = rows.length - 1; i >= 0; i--) if (rows[i]!.entry?.comment.mentions_me === true) return i;
      return -1;
    });
    if (at < 0) return;
    broughtTo = true;
    untrack(() => win.reveal(at));
    flushSync();
    const mentions = convo.querySelectorAll<HTMLElement>(".tf-msg.tf-mention");
    mentions[mentions.length - 1]?.scrollIntoView?.({ block: "center" });
  });

  async function send() {
    const body = draft.trim();
    if (body === "" || sending || refused !== null) return;
    sending = true;
    // *Comment* goes off with its draft, so focus stays in the box, whichever way this was sent (F-266).
    box?.focus({ preventScroll: true });
    try {
      if (await oncomment(body)) drafts.forget(commentKey);
    } finally {
      sending = false;
    }
    // The comment has just taken room above the box, which focus came back to: all of it in view, *Comment* too (F-162).
    await tick();
    box?.closest(".tf-compose")?.scrollIntoView?.({ block: "nearest" });
  }
</script>

<div class="tf-convo" bind:this={convo}>
  {#if failure !== null}
    <p class="tf-dim">Could not read the conversation: {failure}</p>
  {:else if thread === null || drawn === null}
    <p class="tf-dim">Reading the conversation…</p>
  {:else}
    {#if rows.length > 0}
    <div class="tf-convo-rows" {@attach win.list}>
    {#each win.slice(rows) as row (row.key)}
      {#if row.entry === null}
        <div class="tf-newline" {@attach win.row(row.key)}>New since your last visit, {since(lastVisit ?? "", now)}</div>
      {:else}
      {@const entry = row.entry}
      {@const comment = entry.comment}
      {@const markdown = proseMarkdown(comment.body, comment.markup, attachments)}
      <article class="tf-msg" class:tf-new={entry.fresh} class:tf-mention={comment.mentions_me} {@attach win.row(row.key)}>
        <span class="tf-av" class:tf-me={comment.mine} aria-hidden="true">{initials(comment.author)}</span>
        <div class="tf-msg-body">
          <div class="tf-who">
            <b>{comment.mine ? "You" : firstName(comment.author)}</b>{stamp(comment.at, now)}
            {#if comment.mentions_me}<span class="tf-chip tf-chip-mention">mentions you</span>{/if}
          </div>
          {#if markdown !== null}
            <TfMarkdown text={markdown} attachments={images} />
          {:else}
            <p>
              {#each mentionRuns(comment.body, thread.me) as run, at (at)}{#if run.at}<span class="tf-at">{run.text}</span>{:else}{run.text}{/if}{/each}
            </p>
          {/if}
        </div>
      </article>
      {/if}
    {/each}
    </div>
    {/if}
    {#if drawn.entries.length === 0 && queued.length === 0}
      <p class="tf-dim">No comments yet.</p>
    {/if}
  {/if}
  {#each queued as echo, n (n)}
    <article class="tf-msg tf-queued">
      <span class="tf-av tf-me" aria-hidden="true">{initials(myName)}</span>
      <div class="tf-msg-body">
        <div class="tf-who"><b>You</b><TfStateChip text={echo.state} tone="info" /></div>
        <p>{echo.text}</p>
      </div>
    </article>
  {/each}
  <div class="tf-compose" tabindex="-1">
    <textarea bind:this={box} value={draft} oninput={(event) => drafts.put(commentKey, event.currentTarget.value)} rows="1" placeholder="Comment on {jobKey}" aria-label="Comment on {jobKey}" {@attach sendKey(() => void send())}></textarea>
    <div class="tf-acts">
      {#if refused !== null}<span class="tf-dim tf-small">{refused}</span>{/if}
      <button type="button" class="tf-btn tf-sm tf-primary" {...refusedAttrs(refused)} disabled={draft.trim() === "" || sending} onclick={() => void send()}>Comment</button>
    </div>
  </div>
</div>
