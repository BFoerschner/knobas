<!--
  What an empty box shows (spec §4), top to bottom: the smart lists with their
  counts and change badges; the inbox's count and its newest three items; the
  time tracked today and what is still to log; the source-health strip; and
  the newest items across every kind. Every block but the lists and the strip
  is a list of rows the arrows walk (`rows.ts`), in that order.

  **A block with nothing to show is absent, not empty.** No inbox items, no
  inbox block; nothing tracked today, no time block. An empty column with a
  heading over it teaches a reader that the app is broken. §4's contexts are
  not on the board by decision: the #1427 ruling keeps them as the job's
  hidden working set (2026-10-21).

  The inbox and the time are the shell's live reads handed in as props
  (#1418) — the store the top bar's inbox count reads, and the Day view's own
  read and figures — not fields on `launcher_home`.

  The recent items use the same `Row` the results use, so their provenance
  looks identical — which is the point of §4 promising it "on every result".
-->
<script lang="ts">
  import type { LauncherHome } from "../ipc";
  import { clockLabel, minutesLabel } from "../time/format";
  import { failed, sentence } from "../tf/inbox/inbox";
  import type { CredentialHealth, SourceSyncStatus } from "../ipc/sources";
  import { isActionable, type HealthPhase } from "../shell/health.svelte";
  import { sourceMonogram } from "../shell/monogram";
  import { ago } from "../shell/time";
  import { sourceState } from "../tf/settings/settings";
  import { sharesKey } from "./format";
  import Row from "./Row.svelte";
  import type { LauncherRow } from "./rows";

  let {
    home,
    inboxCount = 0,
    sources,
    runs = [],
    reading: readingProp,
    rows,
    selected,
    renaming,
    optionId,
    now,
    onopen,
    onhover,
    onstartrename,
    onrename,
    ondelete,
  }: {
    home: LauncherHome;
    /**
     * How many inbox items need the reader now — the inbox store's count, the
     * number the top bar draws (#1418). The heading's number; the rows under
     * it are the newest few of that many, from `rows`.
     */
    inboxCount?: number;
    /**
     * Whether `sources` has been read (#746). An empty list is "none
     * configured" only when `phase` is `ready`; while the shell's seed waits
     * or has failed it is the absence of an answer, and the strip says that
     * instead of making the claim.
     */
    reading?: { phase: HealthPhase; error: string | null; retry: () => void } | undefined;
    /**
     * The credential health the strip draws — the launcher's, which is the
     * live `source:health` store when the shell supplies one and the board's
     * own per-opening copy when it does not.
     *
     * Passed in rather than read off `home.sources` here: the chips above this
     * strip are drawn from the live store, and two copies is two chances for
     * the strip and a result row's badge to say different things about the
     * same source — which is the divergence #27's review found and this PR
     * claimed to have closed. The recent rows below the strip read it too
     * (#1420); nothing on the board reads `home.sources`.
     */
    sources: CredentialHealth[];
    /**
     * Each source's last run (#1581). A source whose runs keep ending in an
     * error reads as the source page reads it, in the warn tone. Omitted, the
     * strip is credential health alone.
     */
    runs?: SourceSyncStatus[];
    rows: LauncherRow[];
    selected: number;
    /**
     * The saved list whose name is being edited, or `null` (#506, story 58).
     *
     * Owned by the launcher rather than by this component, because the `Tab`
     * chain on a saved row is what starts a rename from the keyboard and the
     * chain is the launcher's — two owners of one editing state is two ways
     * for the input to be open.
     */
    renaming: string | null;
    /** The element id of the option at a flat-list index (#1411). */
    optionId: (index: number) => string;
    now?: Date | undefined;
    onopen: (row: LauncherRow) => void;
    onhover: (index: number) => void;
    /** Begin renaming a saved list — the row's name becomes a field. */
    onstartrename: (id: string) => void;
    /**
     * Commit a rename, or abandon it: `label` is `null` for Escape and the
     * blur that follows it.
     *
     * One callback for both, because the launcher has one thing to do either
     * way — close the field — and two would let it close on one path and not
     * the other.
     */
    onrename: (id: string, label: string | null) => void;
    ondelete: (id: string) => void;
  } = $props();

  /**
   * The rename field takes focus and selects the name as it opens (#1412), so
   * what is typed next replaces the name.
   *
   * An action rather than `autofocus`: Svelte honours that attribute only
   * while nothing but the body has focus, and the query box holds focus for
   * as long as the launcher is open, so the attribute never fired and the
   * typed name went into the query instead.
   */
  function takeFocus(node: HTMLInputElement) {
    node.focus();
    node.select();
  }

  const reading = $derived(readingProp ?? { phase: "ready" as const, error: null, retry: () => {} });

  /** The inbox rows and the day's row, as `rows.ts` placed them after the lists. */
  const inboxRows = $derived(
    rows.flatMap((row, index) => (row.kind === "inbox" ? [{ row, item: row.item, index }] : [])),
  );
  const todayRow = $derived(
    rows.flatMap((row, index) => (row.kind === "today" ? [{ row, today: row.today, index }] : []))[0],
  );

  /** Where the recent rows start in the flat list; `rows.ts` decides the order. */
  const recentFrom = $derived(rows.findIndex((row) => row.kind === "recent"));
  /** Recent items another source's item of the same key sits beside (#1476): they name their source. */
  const shares = $derived(sharesKey(home.recent));

  /**
   * The saved list whose *Delete* has been pressed once.
   *
   * Two presses, and the second one is the delete. A saved list is a query
   * somebody wrote down and there is no undo, so the one-pixel miss that costs
   * a row is worth a second press; and the arming lives here rather than in
   * the launcher because it is not a state anything else can act on.
   *
   * Cleared whenever the cursor moves to another row, so a half-armed button
   * cannot sit waiting on a row nobody is looking at.
   */
  let armed = $state<string | null>(null);

  /**
   * The sentence above, as code: an armed *Delete* belongs to the row under
   * the cursor, exactly as `Launcher.svelte`'s action chain belongs to the row
   * it was opened on.
   *
   * Derived rather than cleared by an effect, so the button is never armed and
   * on the wrong row for the tick it would take an effect to notice.
   */
  const confirming = $derived(
    armed !== null && home.smart_lists[selected]?.id === armed ? armed : null,
  );

  /** One line per source: what it is, whether knobas can still read it, and whether its last run failed or left part of the source out (#2374). */
  const strip = $derived(
    sources.map((source) => {
      const actionable = isActionable(source.state);
      // The source page's own reading (`sourceState`), so a run that ended in
      // an error says here what it says there (#1581). The board knows no more
      // of the source than its health and its last run: `enabled` is assumed,
      // and a role it did not draw is the page's to say.
      const run = sourceState(
        { enabled: true, last_run: null, role_misses: [], auth_kind: "Pat" },
        source,
        runs.find((row) => row.source_id === source.source_id) ?? null,
        now,
      );
      const runWarns = !actionable && run.tone === "warn";
      return {
        id: source.source_id,
        monogram: sourceMonogram(source.source_id),
        // `shell/health.svelte` owns the reading of a health state, so this
        // strip and a result row's badge cannot come to different conclusions
        // about the same source.
        failing: actionable,
        warning: runWarns,
        note: actionable
          ? (source.detail ?? source.state)
          : runWarns
            ? run.text
            : source.checked_at
              ? `checked ${ago(source.checked_at, now)}`
              : "not checked yet",
      };
    }),
  );
</script>

<div class="tf-ln-h"><span class="tf-ln-lab">Smart lists</span><span class="tf-ln-n">saved local queries</span></div>
{#each home.smart_lists as list, i (list.id)}
  {@const row = rows[i]}
  <!--
    The hover is on the row and not on the button inside it, so moving the
    pointer onto *Rename* or *Delete* selects the row those act on rather than
    leaving the cursor two rows above.

    `role="presentation"`: the listbox's options are the `.tf-ln-sl` buttons, and
    this wrapper exists only to put the two controls beside one. Marking it
    presentational is what keeps it out of the listbox's own structure — the
    same thing `TfDialog`'s scrim does with its click target. A row
    being renamed has *no* option, which is honest: its name is a text field
    for as long as the field is open.
  -->
  <div
    class="tf-ln-slrow"
    class:tf-ln-on={i === selected}
    role="presentation"
    onmouseenter={() => onhover(i)}
  >
    {#if renaming === list.id}
      <!--
        Renaming in place, on the row it renames. `blur` abandons rather than
        commits, and Enter is the only thing that commits: a name half-typed
        when the reader clicked away is not a name they chose.
      -->
      <input
        class="tf-ln-ren"
        type="text"
        use:takeFocus
        value={list.label}
        aria-label="Rename {list.label}"
        onkeydown={(event) => {
          if (event.key === "Enter") {
            event.preventDefault();
            event.stopPropagation();
            onrename(list.id, event.currentTarget.value);
          } else if (event.key === "Escape") {
            event.preventDefault();
            // Never past this input: the launcher's own Escape ladder would
            // read the same keystroke as "clear the box" and then "close".
            event.stopPropagation();
            onrename(list.id, null);
          }
        }}
        onblur={() => onrename(list.id, null)}
      />
    {:else}
      <button
        id={optionId(i)}
        class="tf-ln-sl"
        role="option"
        aria-selected={i === selected}
        disabled={list.needs_attention}
        onclick={() => row && onopen(row)}
      >
        <span class="tf-ln-nm">
          {list.label}
          <small class:tf-ln-att={list.needs_attention}>{list.description}</small>
        </span>
        <span class="tf-ln-c" class:tf-ln-chg={list.changed}>
          {#if list.needs_attention}
            <span class="tf-ln-att">Needs attention</span>
          {:else}
            {list.count}{#if list.changed}<span class="tf-ln-dot" title="something in it is new"
                >●</span
              >{/if}
          {/if}
        </span>
      </button>
      {#if list.saved}
        <!--
          The two controls a saved list has and a built-in does not. On the row
          rather than in a menu, because there are two of them; reachable from
          the keyboard through the row's `Tab` chain, which is where every
          other per-row action in this box lives.
        -->
        <span class="tf-ln-own" role="presentation">
          <button
            class="tf-ln-mini"
            onclick={() => {
              onhover(i);
              onstartrename(list.id);
            }}>Rename</button
          >
          <button
            class="tf-ln-mini tf-ln-del"
            class:tf-ln-armed={confirming === list.id}
            onclick={() => {
              // Select first: `confirming` belongs to the row under the
              // cursor, so a press on a row the cursor is not on has to move
              // it there or the button could never arm.
              onhover(i);
              if (confirming === list.id) {
                armed = null;
                ondelete(list.id);
              } else {
                armed = list.id;
              }
            }}
          >
            {confirming === list.id ? "Confirm" : "Delete"}
          </button>
        </span>
      {/if}
    {/if}
  </div>
{/each}

{#if inboxRows.length > 0}
  <div class="tf-ln-h tf-ln-inboxh"><span class="tf-ln-lab">Inbox</span><span class="tf-ln-n">{inboxCount}</span></div>
  <div class="tf-ln-inbox">
    {#each inboxRows as { row, item, index } (row.id)}
      <div class="tf-ln-slrow" class:tf-ln-on={index === selected} role="presentation" onmouseenter={() => onhover(index)}>
        <button
          id={optionId(index)}
          class="tf-ln-sl"
          role="option"
          aria-selected={index === selected}
          onclick={() => onopen(row)}
        >
          <span class="tf-ln-nm">{#if failed(item)}<span class="tf-ln-errdot" aria-hidden="true">●</span>{" "}{/if}{sentence(item)}</span>
          <span class="tf-ln-c">{clockLabel(item.occurred_at, now ?? new Date(), true)}</span>
        </button>
      </div>
    {/each}
  </div>
{/if}

{#if todayRow}
  {@const { row, today, index } = todayRow}
  <div class="tf-ln-h"><span class="tf-ln-lab">Today</span><span class="tf-ln-n">time</span></div>
  <div class="tf-ln-today">
    <div class="tf-ln-slrow" class:tf-ln-on={index === selected} role="presentation" onmouseenter={() => onhover(index)}>
      <button id={optionId(index)} class="tf-ln-sl" role="option" aria-selected={index === selected} onclick={() => onopen(row)}>
        <span class="tf-ln-nm"><b>{minutesLabel(today.tracked)}</b> tracked</span>
        <span class="tf-ln-c" class:tf-ln-warn={today.toLog > 0}>{minutesLabel(today.toLog)} to log</span>
      </button>
    </div>
  </div>
{/if}

<!--
  The source-health strip. Not selectable, and deliberately not: a row here is
  a *reading*, and the thing to do about a bad one is the sources view, which
  `>` already offers. Making it another Enter target would put two different
  meanings on one key.
-->
<!-- The count and the gate read the same list the strip does, not the copy the
     board arrived with — a header saying "2" over one line is the same lie in
     a smaller place. -->
<div class="tf-ln-h">
  <span class="tf-ln-lab">Sources</span>{#if reading.phase === "ready" || sources.length > 0}<span class="tf-ln-n">{sources.length}</span>{/if}
</div>
{#if sources.length === 0 && reading.phase === "loading"}
  <p class="tf-ln-none">Reading the sources…</p>
{:else if sources.length === 0 && reading.phase === "failed"}
  <p class="tf-ln-none tf-ln-err">Could not read the sources{reading.error ? `: ${reading.error}` : "."}</p>
  <button class="tf-ln-opt" onclick={() => reading.retry()}>
    <span class="tf-ln-nm">Retry</span>
    <span class="tf-ln-s">Read the sources again.</span>
  </button>
{:else if sources.length === 0}
  <p class="tf-ln-none">No source is configured yet.</p>
{:else}
  <div class="tf-ln-strip">
    {#each strip as source (source.id)}
      <span class="tf-ln-src" class:tf-ln-fail={source.failing} class:tf-ln-warn={source.warning}>
        <span class="tf-ln-mg" title={source.id} aria-label={source.id}>{source.monogram}</span>
        <span class="tf-ln-sid">{source.id}</span>
        <span class="tf-ln-note">{source.note}</span>
      </span>
    {/each}
  </div>
{/if}

<div class="tf-ln-h"><span class="tf-ln-lab">Recent</span><span class="tf-ln-n">{home.recent.length}</span></div>
{#each home.recent as item, i (item.entity_id)}
  {@const index = recentFrom + i}
  {@const row = rows[index]}
  {#if row}
    <Row
      entityId={item.entity_id}
      kind={item.kind}
      sourceId={item.source_id}
      title={item.title}
      syncedAt={item.synced_at}
      path={item.path}
      state={item.state}
      named={shares(item)}
      {sources}
      {now}
      id={optionId(index)}
      selected={index === selected}
      onopen={() => onopen(row)}
      onhover={() => onhover(index)}
    />
  {/if}
{/each}

{#if home.recent.length === 0}
  <p class="tf-ln-none">
    Nothing has been synced yet. Add a source from the gear, or start knobas
    with <strong>--demo</strong>.
  </p>
{/if}
