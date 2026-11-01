<!--
  The ticket page's card (#816, R-4.2, spec #2396): the same card in two
  moods. On the yellow (`tf-card`, *Job card*) once the ticket is the chosen
  job; on a sheet (`tf-sheetcard`, *Ticket*) while it is only read. The mood
  class is the only difference: the same markup, no punched hole, no status
  select, no clock.

  Top to bottom (#2400): the key at the display step with a copy button
  beside it, which puts *KEY title* on the clipboard, turns the ok tone for a
  moment and toasts *Copied*; the source when another source holds the same
  key (#1476), with a link to each other one; the status as the card's chip,
  with the blocked chip beside it; then the facts list, the Tickets pane's
  `dl.tf-facts`: Epic as a chip that opens the epic's page (cut with an
  ellipsis, the whole name its title), Reporter and Assignee each as an
  avatar and a name (full ink when it is the reader), Type, Priority,
  Labels, and Logged (#2403): *2h 10m of 6h* over a meter whose track is the
  estimate, tinted once spent passes it, with the reader's share in full ink
  and *…, 1h 40m yours* when somebody else has logged too (`loggedRow`). A
  row the source leaves empty is not drawn. At the
  foot one large button (`footAction` in `job.ts`): the decision while
  reading, the timer once pinned. The running time is the top bar's chip
  alone. The title, the description and the conversation are the reading
  column's (#2399).
-->
<script lang="ts">
  import type { EntityDetail } from "../../ipc/entity";
  import { push } from "../../shell/toasts.svelte";
  import { ipcErrorMessage } from "../../ipc";
  import { ticketHash } from "../route";
  import { parentOf } from "../sets/apply";
  import { epicLine, initials } from "../tickets/tickets";
  import { blockers, keyOf, loggedRow } from "./job";

  let {
    detail,
    others = [],
    parentTitle = null,
    mine = false,
    me = null,
    pinned,
    action,
    onaction,
    copy = (text: string) => navigator.clipboard.writeText(text),
  }: {
    detail: EntityDetail;
    /** The other sources' tickets under the same key (#1476); none draws no source. */
    others?: EntityDetail[];
    /** The direct parent's title, for the epic chip; `null` draws its key alone. */
    parentTitle?: string | null;
    /** Whether the ticket is the reader's own (`isMine`): the assignee's avatar in full ink, *(you)* after the name. */
    mine?: boolean;
    /** The reader's username in the ticket's source (`username`): the reporter in full ink when it is the reader, and the reader's share of the logged time. */
    me?: string | null;
    /** Whether the ticket is the chosen job: the yellow, else the sheet. */
    pinned: boolean;
    /** The foot's one button, or `null` where the card offers none. */
    action: { label: string } | null;
    onaction: () => void;
    /** Writes the clipboard; the browser's unless a test hands its own. */
    copy?: (text: string) => Promise<void>;
  } = $props();

  const key = $derived(keyOf(detail.row.entity_id));
  const blockedBy = $derived(blockers(detail));
  const epicId = $derived(parentOf(detail.row));
  const epic = $derived(epicLine({ path: detail.row.path, parent_title: parentTitle }));
  const labels = $derived(detail.labels ?? []);
  const reporter = $derived(detail.reporter ?? null);
  const reporterMine = $derived(reporter !== null && me !== null && reporter === me);
  const logged = $derived(loggedRow(detail, me));
  /** A meter segment's width, through the CSSOM: the app's CSP refuses `style=""` in markup, not this. */
  const width = (share: number) => (node: HTMLElement) => {
    node.style.width = `${share.toFixed(1)}%`;
  };

  /** The copy button's ok tone, for a moment after a copy. */
  let copied = $state(false);
  let settle: ReturnType<typeof setTimeout> | undefined;
  async function copyKey() {
    try {
      await copy(`${key} ${detail.row.title}`);
      push({ text: "Copied" });
      copied = true;
      clearTimeout(settle);
      settle = setTimeout(() => (copied = false), 1500);
    } catch (cause) {
      push({ text: `Could not copy: ${ipcErrorMessage(cause)}`, tone: "err" });
    }
  }
  $effect(() => () => clearTimeout(settle));
</script>

<article
  class="tf-jobcard tf-ticketcard"
  class:tf-card={pinned}
  class:tf-sheetcard={!pinned}
  aria-label={pinned ? "Job card" : "Ticket"}
>
  <div>
    <div class="tf-tkhead">
      <div class="tf-jobkey">{key}</div>
      <button
        type="button"
        class="tf-iconbtn tf-sm tf-tkcopy"
        class:tf-tkcopied={copied}
        title={copied ? "Copied" : "Copy the key and the title"}
        aria-label="Copy the key and the title"
        onclick={() => void copyKey()}
        ><svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" aria-hidden="true"
          ><rect x="5.5" y="5.5" width="8" height="8" rx="1.5" /><path d="M10.5 5.5V3.5a1 1 0 0 0-1-1h-6a1 1 0 0 0-1 1v6a1 1 0 0 0 1 1h2" /></svg
        ></button
      >
    </div>
    {#if others.length > 0}
      <p class="tf-jobsrc">
        In {detail.source.display_name} · also in {#each others as other, at (other.row.entity_id)}{#if at > 0},
          {/if}<a href={ticketHash(other.row.entity_id)}>{other.source.display_name}</a>{/each}
      </p>
    {/if}
  </div>
  <div class="tf-jobrow">
    <span class="tf-chip">{detail.status ?? "No status"}</span>
    {#each blockedBy as blocker (blocker)}
      <span class="tf-chip tf-err">Blocked by {blocker}</span>
    {/each}
  </div>
  <dl class="tf-facts">
    {#if epic !== null && epicId !== null}
      <dt>Epic</dt>
      <dd><a class="tf-chip tf-tkepicchip" href={ticketHash(epicId)} title={epic}>{epic}</a></dd>
    {/if}
    {#if reporter !== null}
      <dt>Reporter</dt>
      <dd>
        <span class="tf-av" class:tf-me={reporterMine} aria-hidden="true">{initials(reporter)}</span><span
          >{reporter}{reporterMine ? " (you)" : ""}</span
        >
      </dd>
    {/if}
    <dt>Assignee</dt>
    <dd>
      <span class="tf-av" class:tf-me={mine && detail.author !== null} aria-hidden="true">{initials(detail.author)}</span><span
        >{detail.author ?? "Unassigned"}{mine && detail.author !== null ? " (you)" : ""}</span
      >
    </dd>
    {#if detail.issue_type}<dt>Type</dt><dd>{detail.issue_type}</dd>{/if}
    {#if detail.priority}<dt>Priority</dt><dd>{detail.priority}</dd>{/if}
    {#if labels.length > 0}
      <dt>Labels</dt>
      <dd>{#each labels as label (label)}<span class="tf-lab">{label}</span>{" "}{/each}</dd>
    {/if}
    {#if logged !== null}
      {@const line = logged.yours === null ? logged.text : `${logged.text}, ${logged.yours}`}
      <dt>Logged</dt>
      <dd>
        <span>{line}</span>
        {#if logged.meter !== null}
          <span class="tf-tkmeter" class:tf-tkover={logged.meter.over} role="img" aria-label="{logged.text} logged{logged.yours === null ? '' : `, ${logged.yours}`}"
            >{#if logged.meter.mine > 0}<i class="tf-tkme" {@attach width(logged.meter.mine)}></i>{/if}{#if logged.meter.others > 0}<i
                class="tf-tkothers"
                {@attach width(logged.meter.others)}
              ></i>{/if}</span
          >
        {/if}
      </dd>
    {/if}
  </dl>
  {#if action !== null}
    <div class="tf-go">
      <button type="button" class="tf-btn tf-primary tf-lg" onclick={onaction}>{action.label}</button>
    </div>
  {/if}
</article>
