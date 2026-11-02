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
  row the source leaves empty is not drawn. Under
  the facts the sections table (#2401, `card-sections.ts`): a bordered list,
  one row a section with its count, its worst state and a chevron, opening
  the section's view narrowed to this ticket; a ticket with sub-tasks leads
  with *Sub-tasks*; one that reaches nothing says *Nothing connected yet.*
  Under it, in the same shape, *Open in <source>* (the web address through
  the browser) and *Link something to it* (the Connections drawer). At the
  foot one large button (`footAction` in `job.ts`): the decision while
  reading, the timer once pinned. The running time is the top bar's chip
  alone. The title, the description and the conversation are the reading
  column's (#2399).

  The card's width (#2402, `card-width.svelte.ts`): a six-pixel separator on
  its right edge drags the column between 240px and 720px, a double click (or
  Enter) fits it so nothing on the card wraps, and the arrows step it. Below
  900px the columns stack and the handle is not drawn.
-->
<script lang="ts">
  import type { EntityDetail } from "../../ipc/entity";
  import { push } from "../../shell/toasts.svelte";
  import { ipcErrorMessage } from "../../ipc";
  import { openExternal } from "../../shell/open-external";
  import { pressSection, type CardTable } from "./card-sections";
  import { cardWidth, naturalWidth, SIDE_MAX, SIDE_MIN } from "./card-width.svelte";
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
    table,
    onlink,
    copy = (text: string) => navigator.clipboard.writeText(text),
    open = openExternal,
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
    /** The sections table's rows (#2401), and whether every read has answered. */
    table: CardTable;
    /** *Link something to it*: the Connections drawer. */
    onlink: () => void;
    /** Hands a web address to the browser; the shell's opener unless a test hands its own. */
    open?: (url: string) => Promise<void>;
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

  // ---- the card's width (#2402) ----
  /** The window at which the columns stack and the handle goes (`narrow.css`). */
  const NARROW = "(max-width: 900px)";
  /** How far an arrow key moves the edge. */
  const STEP = 16;
  let article = $state<HTMLElement | null>(null);
  let narrow = $state(false);
  $effect(() => {
    const query = typeof window.matchMedia === "function" ? window.matchMedia(NARROW) : null;
    if (query === null) return;
    narrow = query.matches;
    const changed = (event: MediaQueryListEvent) => (narrow = event.matches);
    query.addEventListener("change", changed);
    return () => query.removeEventListener("change", changed);
  });

  /** A drag in progress: what lets go of the window's pointer listeners. */
  let release: (() => void) | null = $state(null);
  function startDrag(event: PointerEvent) {
    if (event.button !== 0 || article === null) return;
    event.preventDefault();
    release?.();
    const left = article.getBoundingClientRect().left;
    const move = (at: PointerEvent) => cardWidth.set(at.clientX - left);
    const up = () => release?.();
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
    release = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      release = null;
    };
  }
  $effect(() => () => release?.());

  /** The width at which nothing on the card wraps, up to the cap. */
  function fit() {
    if (article !== null) cardWidth.set(naturalWidth(article));
  }

  function edgeKey(event: KeyboardEvent) {
    if (event.key === "ArrowLeft") cardWidth.set(cardWidth.px - STEP);
    else if (event.key === "ArrowRight") cardWidth.set(cardWidth.px + STEP);
    else if (event.key === "Enter") fit();
    else return;
    event.preventDefault();
  }

  /** *Open in <source>*: the ticket's web address through the browser, never the webview. */
  function openWeb(event: MouseEvent, url: string) {
    event.preventDefault();
    open(url).catch((cause: unknown) => push({ text: `Could not open it: ${ipcErrorMessage(cause)}`, tone: "err" }));
  }
</script>

<article
  bind:this={article}
  class="tf-jobcard tf-ticketcard"
  class:tf-card={pinned}
  class:tf-sheetcard={!pinned}
  class:tf-tkdragging={release !== null}
  aria-label={pinned ? "Job card" : "Ticket"}
>
  {#if !narrow}
    <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
    <div
      class="tf-tkhandle"
      role="separator"
      tabindex="0"
      aria-orientation="vertical"
      aria-label="Card width. Drag to resize, double-click to fit."
      aria-valuemin={SIDE_MIN}
      aria-valuemax={SIDE_MAX}
      aria-valuenow={cardWidth.px}
      title="Drag to resize, double-click to fit"
      onpointerdown={startDrag}
      ondblclick={fit}
      onkeydown={edgeKey}
    ></div>
  {/if}
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
          <span class="tf-tkmeter" class:tf-tkover={logged.meter.past > 0} role="img" aria-label="{logged.text} logged{logged.yours === null ? '' : `, ${logged.yours}`}"
            >{#if logged.meter.mine > 0}<i class="tf-tkme" {@attach width(logged.meter.mine)}></i>{/if}{#if logged.meter.others > 0}<i
                class="tf-tkothers"
                {@attach width(logged.meter.others)}
              ></i>{/if}{#if logged.meter.past > 0}<i class="tf-tkpast" {@attach width(logged.meter.past)}></i>{/if}</span
          >
        {/if}
      </dd>
    {/if}
  </dl>
  {#if table.rows.length > 0}
    <div class="tf-tktable tf-tksecs">
      {#each table.rows as row (row.name)}
        <a
          href={row.href}
          title={row.opens === "subtasks" ? `Tickets under ${key}. ${row.words}` : `${row.name} view, this ticket only. ${row.words}`}
          onclick={() => pressSection(row, detail.row.entity_id)}
          ><span class="tf-tkname">{row.name}</span><span class="tf-n">{row.count}</span>{#if row.state !== null}<span
              class="tf-st"
              class:tf-asks={row.state.tone === "err" || row.state.tone === "warn"}
              ><span class="tf-dot tf-{row.state.tone}" aria-hidden="true"></span>{row.state.text}</span
            >{:else}<span></span>{/if}<span class="tf-chev" aria-hidden="true">›</span></a
        >
      {/each}
    </div>
  {:else if table.settled}
    <p class="tf-tknone">Nothing connected yet.</p>
  {/if}
  <div class="tf-tktable tf-tkacts">
    {#if detail.web_url}
      {@const url = detail.web_url}
      <a href={url} onclick={(event) => openWeb(event, url)}
        ><span>Open in {detail.source.display_name}</span><span class="tf-chev" aria-hidden="true">›</span></a
      >
    {/if}
    <button type="button" onclick={onlink}><span>Link something to it</span><span class="tf-chev" aria-hidden="true">›</span></button>
  </div>
  {#if action !== null}
    <div class="tf-go">
      <button type="button" class="tf-btn tf-primary tf-lg" onclick={onaction}>{action.label}</button>
    </div>
  {/if}
</article>
