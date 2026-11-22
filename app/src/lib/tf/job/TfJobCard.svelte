<!--
  The ticket page's card (#816, R-4.2, spec #2396): the same card in two
  moods. On the yellow (`tf-card`, *Job card*) once the ticket is the chosen
  job; on a sheet (`tf-sheetcard`, *Ticket*) while it is only read. The mood
  class is the only difference: the same markup, no punched hole, no clock.
  The status is a control where the page hands one (`statusControl`,
  `TfStatusMove`, ADR-0054): a menu of the statuses the ticket can reach.

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

  The card's width (#2402) is the page's sidebar edge (`TfSideEdge`, drawn by
  `TfJob` on the line beside the card): it drags the column between 240px and
  720px, a double click (or Enter) fits it so nothing on the card wraps
  (`naturalWidth`), and the arrows step it. Below 900px the columns stack and
  there is no edge.
-->
<script lang="ts">
  import TfKey from "../TfKey.svelte";
  import { heldHead } from "../held-head.svelte";
  import TfKeyed from "../TfKeyed.svelte";
  import TfStateChip from "../TfStateChip.svelte";
  import { MediaQuery } from "svelte/reactivity";
  import type { Snippet } from "svelte";
  import type { EntityDetail } from "../../ipc/entity";
  import { push } from "../../shell/toasts.svelte";
  import { openWriteQueue } from "../../shell/write-queue.svelte";
  import { acknowledged, openingWords } from "../../shell/ack.svelte";
  import { ipcErrorMessage } from "../../ipc";
  import { routeWeb as openExternal, webPlaceName } from "../../shell/open-web";
  import { openKey } from "../page-open.svelte";
  import type { CardTable } from "./card-sections";
  import TfCardRows from "./TfCardRows.svelte";
  import { ticketHash } from "../route";
  import { parentOf } from "../sets/apply";
  import { epicLine, initials, shortKey } from "../tickets/tickets";
  import { blockers, keyOf, loggedRow } from "./job";
  import type { FieldEdits } from "./field-edits.svelte";
  import { chipWords } from "../tickets/edits.svelte";
  import TfFactControl from "./TfFactControl.svelte";
  import TfOwnField from "./TfOwnField.svelte";
  import type { Layers } from "../layers.svelte";

  let {
    detail,
    others = [],
    parentTitle = null,
    parentRead = true,
    mine = false,
    me = null,
    pinned,
    action,
    onaction,
    table,
    onlink,
    copy = (text: string) => navigator.clipboard.writeText(text),
    open = openExternal,
    statusControl,
    edits = null,
    assigning = $bindable(false),
    layers,
  }: {
    detail: EntityDetail;
    /** The other sources' tickets under the same key (#1476); none draws no source. */
    others?: EntityDetail[];
    /** The direct parent's title, for the epic chip; `null` draws its key alone. */
    parentTitle?: string | null;
    /** Whether the parent's title read has answered: the chip draws once it has, or once the hold is up. */
    parentRead?: boolean;
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
    /** The status as a control that moves the ticket (ADR-0054, `TfStatusMove`); absent, the plain chip. */
    statusControl?: Snippet | undefined;
    /** The fields its source lets a person change (ADR-0054); `null` draws every fact as text. */
    edits?: FieldEdits | null;
    /** The assignee's picker stands open: the page's `a`. */
    assigning?: boolean;
    layers?: Layers | undefined;
  } = $props();

  const key = $derived(keyOf(detail.row.entity_id));
  /**
   * The key as a ticket card draws it: a forge's issue by its repository and number (*payout-service#37*), the whole key
   * on hover, since its owner in a 32 px face cut a long one off at the card's edge (the Windows pass, 2026-11-14).
   */
  const shownKey = $derived(shortKey(key));
  const blockedBy = $derived(blockers(detail));
  const stacked = new MediaQuery("(max-width: 900px)");
  const epicId = $derived(parentOf(detail.row));
  const epic = $derived(epicLine({ path: detail.row.path, parent_title: parentTitle }));
  const labels = $derived(detail.labels ?? []);
  const reporter = $derived(detail.reporter ?? null);
  const reporterMine = $derived(reporter !== null && me !== null && reporter === me);
  /** What a person is called (F-107): the name the source keeps beside the account, as the Conversation below says it. */
  const reporterName = $derived(detail.reporter_name ?? reporter);
  const assigneeName = $derived(detail.author_name ?? detail.author);
  const logged = $derived(loggedRow(detail, me));
  /**
   * Nothing on the card moves once drawn (the nothing-moves pass, 2026-11-17, `held-head.svelte.ts`). The epic chip waits
   * for its parent's title, as the key alone widened 97 px to the key and title 100 ms later; the sections and the actions
   * under them wait together for every section's read, as each row that landed pushed *Open in* and *Link something to
   * it* 33 px down. Each shows after the hold at the latest, with what has answered.
   */
  const epicHeld = heldHead(
    () => detail.row.entity_id,
    () => parentRead,
  );
  const rowsHeld = heldHead(
    () => detail.row.entity_id,
    () => table.settled && (edits === null || edits.answered),
  );
  /**
   * The facts a source may let a person set where the read holds none (ADR-0054): a Priority or Labels row is drawn
   * empty where the field is editable, so the facts after the Assignee wait for the fields' answer, as the epic chip
   * waits for its title, and show after the hold at the latest.
   */
  const factsHeld = heldHead(
    () => detail.row.entity_id,
    () => edits === null || edits.answered,
  );
  const editable = (id: string) => edits?.control(id).can ?? false;
  /** The source's own fields (step 10), drawn under *More fields*. */
  const own = $derived(edits?.own() ?? []);
  /** The assignee as it reads with an edit not yet read back. */
  const assignEcho = $derived(edits?.control("assignee").echo ?? null);
  const assigneeShown = $derived(assignEcho === null ? (assigneeName ?? "Unassigned") : assignEcho.text);
  const note = (id: string) => edits?.control(id).note ?? null;
  /** A meter segment's width, through the CSSOM: the app's CSP refuses `style=""` in markup, not this. */
  const width = (share: number) => (node: HTMLElement) => {
    node.style.width = `${share.toFixed(1)}%`;
  };

  /** The labels as they stand with the edits not yet read back: a name added draws at once, one removed goes. */
  function withLabelEchoes(read: readonly string[]): string[] {
    let now = [...read];
    for (const echo of edits?.labelEchoes() ?? []) {
      const name = echo.text.slice(1);
      if (echo.text.startsWith("+")) {
        if (!now.includes(name)) now.push(name);
      } else now = now.filter((label) => label !== name);
    }
    return now;
  }
  /** The labels' chip: the newest label edit not yet read back, in the fixed words. */
  const labelChip = $derived.by(() => {
    const echo = edits?.labelEchoes().at(-1);
    if (echo === undefined) return null;
    return { words: chipWords("labels", echo.text, echo.state), queued: echo.state === "queued" };
  });

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

  /** While rows pass under the pinned foot, a line on its top edge: the rows end at it. Gone at the end of the card's scroll, where nothing is cut. */
  const footLine = (node: HTMLElement) => {
    if (typeof ResizeObserver === "undefined") return;
    const measure = () => node.classList.toggle("tf-tkcut", node.scrollHeight - node.clientHeight - node.scrollTop > 1);
    const seen = new ResizeObserver(measure);
    seen.observe(node);
    for (const child of node.children) seen.observe(child);
    node.addEventListener("scroll", measure, { passive: true });
    node.addEventListener("toggle", measure, true);
    return () => {
      seen.disconnect();
      node.removeEventListener("scroll", measure);
      node.removeEventListener("toggle", measure, true);
    };
  };

  /** *Open in <source>*: the ticket's web address through the browser, never the webview. */
  function openWeb(event: MouseEvent, url: string) {
    event.preventDefault();
    acknowledged(openingWords(webPlaceName(url, { source: detail.source.id })), () => open(url)).catch((cause: unknown) => push({ text: `Could not open it: ${ipcErrorMessage(cause)}`, tone: "err" }));
  }
</script>

{#snippet fieldNote(id: string)}
  {@const said = note(id)}
  {#if said !== null}
    <dd class="tf-fact-note" class:tf-held={said.tone === "warn"} role="status">
      <span><strong>{said.lead}</strong> {said.words}</span>
      <button type="button" onclick={openWriteQueue}>Open write queue</button>
      <button type="button" class="tf-fact-x" title="Discard the edit" aria-label="Discard the edit" onclick={() => edits?.dismiss(said)}>×</button>
    </dd>
  {/if}
{/snippet}

<article
  class="tf-jobcard tf-ticketcard"
  class:tf-card={pinned}
  class:tf-sheetcard={!pinned}
  aria-label={pinned ? "Job card" : "Ticket"}
  {@attach footLine}
>
  <div>
    <div class="tf-tkhead">
      <div class="tf-jobkey" title={shownKey === key ? undefined : key}>{shownKey}</div>
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
    {#if statusControl}{@render statusControl()}{:else}<TfStateChip text={detail.status ?? "No status"} />{/if}
    {#each blockedBy as blocker (blocker)}
      <span class="tf-chip tf-err">Blocked by <TfKey value={blocker} /></span>
    {/each}
  </div>
  <!-- Stacked below 900 px the title follows the key, and the facts and rows fold, so the first screen is the ticket. -->
  <p class="tf-tkcardtitle" aria-hidden="true">{detail.row.title}</p>
  <details class="tf-tkstack" open={!stacked.current}>
    <summary>Details and connections</summary>
  <dl class="tf-facts">
    {#if epic !== null && epicId !== null}
      <dt>Epic</dt>
      <dd>{#if epicHeld.shown}<a class="tf-chip tf-tkepicchip" href={ticketHash(epicId)} title={epic}><TfKeyed text={epic} lead /></a>{/if}</dd>
    {/if}
    {#if reporter !== null}
      <dt>Reporter</dt>
      <dd>
        <span class="tf-av" class:tf-me={reporterMine} aria-hidden="true">{initials(reporterName)}</span><span
          >{reporterName}{reporterMine ? " (you)" : ""}</span
        >
      </dd>
    {/if}
    <dt>Assignee</dt>
    <dd class:tf-fwriting={!!edits?.control("assignee").chip}>
      <TfFactControl {edits} id="assignee" name="Assignee" {me} sourceName={detail.source.display_name} bind:open={assigning} {layers}
        ><span class="tf-av" class:tf-me={assignEcho === null && mine && detail.author !== null} aria-hidden="true">{initials(assigneeShown === "Unassigned" ? null : assigneeShown)}</span><span
          >{assigneeShown}{assignEcho === null && mine && detail.author !== null ? " (you)" : ""}</span
        ></TfFactControl
      >
    </dd>
    {@render fieldNote("assignee")}
    {#if detail.issue_type}<dt>Type</dt><dd>{detail.issue_type}</dd>{/if}
    {#if factsHeld.shown}
      {@const priorityEcho = edits?.control("priority").echo ?? null}
      {@const priority = priorityEcho?.text ?? detail.priority}
      {#if priority || editable("priority")}
        <dt>Priority</dt>
        <dd class:tf-fwriting={!!edits?.control("priority").chip}>
          <TfFactControl {edits} id="priority" name="Priority" sourceName={detail.source.display_name} {layers}
            ><span class:tf-dim={!priority}>{priority || "None"}</span></TfFactControl
          >
        </dd>
        {@render fieldNote("priority")}
      {/if}
      {#if labels.length > 0 || editable("labels") || (edits?.labelEchoes().length ?? 0) > 0}
        {@const shownLabels = withLabelEchoes(labels)}
        <dt>Labels</dt>
        <dd class:tf-fwriting={labelChip !== null}>
          <TfFactControl {edits} id="labels" name="Labels" sourceName={detail.source.display_name} {labels} {layers}
            >{#each shownLabels as label (label)}<span class="tf-lab">{label}</span>{" "}{:else}<span class="tf-dim">None</span>{/each}</TfFactControl
          >{#if labelChip !== null}<span class="tf-chip tf-fwrite" class:tf-info={labelChip.queued}>{labelChip.words}</span>{/if}
        </dd>
        {@render fieldNote("labels")}
      {/if}
    {/if}
    {#if factsHeld.shown && logged !== null}
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
  {#if rowsHeld.shown && edits?.noEdits}
    <p class="tf-tknoedits">{edits.noEdits}</p>
  {/if}
  {#if rowsHeld.shown && own.length > 0}
    <!-- A source's own fields (ADR-0054, step 10): closed, under the facts, in the same grid. -->
    <details class="tf-tkmore">
      <summary><span>More fields</span> <span class="tf-count">{own.length}</span><svg class="tf-tkmark" viewBox="0 0 12 12" fill="none" stroke="currentColor" stroke-width="1.6" aria-hidden="true"><path d="M4.5 2.5 8 6l-3.5 3.5" /></svg></summary>
      <dl class="tf-facts">
        {#each own as field (field.id)}
          <dt>{field.label}</dt>
          <dd class:tf-fwriting={!!edits?.control(field.id).chip}><TfOwnField {edits} {field} sourceName={detail.source.display_name} {layers} /></dd>
          {@render fieldNote(field.id)}
        {/each}
      </dl>
    </details>
  {/if}
  {#if rowsHeld.shown && table.rows.length > 0}
    <TfCardRows rows={table.rows} entityId={detail.row.entity_id} ticketKey={key} />
  {:else if table.settled}
    <p class="tf-tknone">Nothing connected yet.</p>
  {/if}
  <div class="tf-tktable tf-tkacts" class:tf-hidden={!rowsHeld.shown} inert={!rowsHeld.shown}>
    {#if detail.web_url}
      {@const url = detail.web_url}
      <a href={url} title="Open in {detail.source.display_name} (O)" onclick={(event) => openWeb(event, url)} {@attach openKey(() => ({ url, source: detail.source.id, item: key }))}
        ><span>Open in {detail.source.display_name}</span><span class="tf-chev" aria-hidden="true">›</span></a
      >
    {/if}
    <button type="button" onclick={onlink}><span>Link something to it</span><span class="tf-chev" aria-hidden="true">›</span></button>
  </div>
  </details>
  {#if action !== null}
    <div class="tf-go">
      <button type="button" class="tf-btn tf-primary tf-lg" onclick={onaction}>{action.label}</button>
    </div>
  {/if}
</article>
