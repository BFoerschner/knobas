<!--
  The Assets view's **Monitors** tab (issue #448, spec #427 story 68): every
  monitor Uptime Kuma publishes, what state knobas last recorded it in, the
  last day of it, and what it is watching.

  `#/assets/monitors`, behind the tab strip `AssetsView` draws — one view to a
  reader, two components underneath, because this one has no columns, no pane
  and no search box, and folding it into the Tree would have meant loading the
  whole estate to show a roster of the mirror.

  **One read for the whole tab.** `monitorRoster()` answers with every mirrored
  monitor and its last day of samples; the chips' counts are counts of *those
  rows* (`monitors.ts`), so nothing here can show a chip reading `warn 3` over
  a list of two.

  **The chips are a filter and an address is not.** Clicking one narrows the
  list and clicking it again clears it. The filter deliberately does *not* ride
  in the address: `#/assets/monitors` is the tab, and the roster's whole point
  is that a reader can see the estate's monitoring at a glance — an address
  that remembered a filter would send somebody a link to four of eight
  monitors without saying so.

  **The bar is knobas' own history, not Kuma's.** Uptime Kuma prunes its
  heartbeats to a day and `/metrics` publishes no history at all, so what is
  drawn here exists because the sync engine wrote a sample every poll (#443,
  migration `0021`). A gap in a bar is therefore a real fact — the app was
  closed, the source was off, or the monitor did not exist yet — and it is
  drawn as a gap rather than filled in with the reading either side of it.

  **What this tab does not draw.** Open-alert cards and the *Not monitored*
  roster of assets with no monitor are the other half of story 68 and are
  #449's; the alerts they are made of are #444's. Creating or pausing a monitor
  is the Kuma write half (story 81) and is not in M4.1's committed scope.
-->
<script lang="ts">
  import { ipcErrorMessage } from "../ipc";
  import {
    monitorRoster as realMonitorRoster,
    type MonitorRow,
    type UptimeRatio,
  } from "../ipc/assets";
  import { hashFor, type Router } from "../shell/router.svelte";
  import { openExternal as realOpenExternal } from "../shell/open-external";
  import { ago } from "../shell/time";
  import AssetsTabs from "./AssetsTabs.svelte";
  import {
    bar,
    CHIP_LABELS,
    chipCounts,
    chipOf,
    filtered,
    type ChipState,
  } from "./monitors";

  /** The bridge this view needs, injectable so a test needs no Tauri. */
  interface MonitorPorts {
    monitorRoster: typeof realMonitorRoster;
    /** *Open in Kuma* (story 71) — the OS browser, so a test can press it. */
    openExternal: typeof realOpenExternal;
  }

  let {
    router,
    ports,
    now = () => new Date(),
  }: {
    router: Router;
    ports?: Partial<MonitorPorts>;
    /**
     * Injectable clock. Two things read it and both are about *when*: the
     * *last check* readings, and the bar's window — a bar is 24 hours ending
     * now, so a fixture that could not place `now` could not place a sample
     * in it either.
     */
    now?: () => Date;
  } = $props();

  // svelte-ignore state_referenced_locally
  // Read once at init, `AssetsView`'s rule: a bridge swapped mid-life would
  // leave what is on screen read through one set of ports and re-read through
  // another.
  const io: MonitorPorts = {
    monitorRoster: realMonitorRoster,
    openExternal: realOpenExternal,
    ...ports,
  };

  let roster = $state<MonitorRow[]>([]);
  let failure = $state<string | null>(null);
  /**
   * Nothing has come back yet — told apart from *a mirror with no monitors in
   * it*, which is what every profile has before a Kuma source is configured
   * and is a sentence this tab has to be able to say.
   */
  let loaded = $state(false);
  /** The chip in force, or `null` for the whole roster. */
  let chip = $state<ChipState | null>(null);

  /**
   * The clock the bar and the *last check* readings are placed against, taken
   * once per read rather than per row: forty-eight buckets times eight
   * monitors placed against forty-eight slightly different instants is a bar
   * whose left edge is ragged for no reason a reader could ever see.
   */
  // svelte-ignore state_referenced_locally
  // Read once at init, like `io` above: the clock is re-read on every load
  // (`read`), which is the only moment the bars are redrawn anyway.
  let clock = $state(now());

  $effect(() => {
    void read();
  });

  async function read(): Promise<void> {
    try {
      roster = await io.monitorRoster();
      failure = null;
    } catch (error) {
      failure = ipcErrorMessage(error);
    } finally {
      clock = now();
      loaded = true;
    }
  }

  const counts = $derived(chipCounts(roster));
  const shown = $derived(filtered(roster, chip));

  /** Click to narrow, click again to clear. */
  function toggle(state: ChipState): void {
    chip = chip === state ? null : state;
  }

  /**
   * What one uptime ratio reads as: `1d 98.2%`.
   *
   * One decimal, because the ratios that matter are the ones near the top —
   * `100%` and `99.6%` are a different month and round to the same integer.
   */
  function uptimeText(ratio: UptimeRatio): string {
    return `${ratio.window} ${(ratio.ratio * 100).toFixed(1)}%`;
  }

  /** What a segment's tooltip says: the half hour, and what happened in it. */
  function segmentTitle(from: number, state: string | null): string {
    const at = new Date(from).toLocaleString();
    return state === null ? `${at} — no reading` : `${at} — ${state}`;
  }
</script>

<section class="view mons">
  <div class="room-bar">
    <h1>Assets</h1>
    <AssetsTabs {router} />
  </div>

  <!--
    The chips, with their counts. Buttons rather than links: the filter is not
    in the address (see the header), so this is a control and not a
    destination. `aria-pressed` is what says which one is in force — a toggle
    is what it is, and `aria-current` would claim it is where the reader is.
  -->
  <div class="chips" role="group" aria-label="Filter monitors by state">
    {#each counts as entry (entry.state)}
      <button
        class="chip {entry.state} {chip === entry.state ? 'on' : ''}"
        aria-pressed={chip === entry.state}
        onclick={() => toggle(entry.state)}
      >
        <span class="lab">{CHIP_LABELS[entry.state]}</span>
        <span class="n">{entry.count}</span>
      </button>
    {/each}
  </div>

  {#if failure}
    <p class="fail rf" role="alert">The monitors could not be read: {failure}</p>
  {/if}

  <ol class="roster">
    {#each shown as monitor (monitor.entity_id)}
      {@const drawn = bar(monitor.samples, clock.getTime())}
      {@const chipHere = chipOf(monitor)}
      <li class="mon">
        <div class="head">
          <span class="st {chipHere}" aria-hidden="true"></span>
          <span class="nm">{monitor.name}</span>
          <span class="state {chipHere}">{CHIP_LABELS[chipHere]}</span>
          <span class="meta faint">
            {monitor.monitor_type ?? "—"}
            {#if monitor.target}<span class="tgt">{monitor.target}</span>{/if}
          </span>
          <!--
            Pushed to the far end: *Open in browser* renders from `web_url` and
            nothing else (interfaces §8 P5), and a tombstoned monitor has no
            page left in Kuma to open — so the button is absent rather than
            dead.
          -->
          {#if monitor.web_url}
            {@const url = monitor.web_url}
            <button class="kuma" onclick={() => void io.openExternal(url)}>
              Open in Kuma
            </button>
          {/if}
        </div>

        <!--
          The bar. `img` with a label rather than a list of forty-eight
          nameless spans: what a reader of a screen reader wants is the
          sentence, not the segments, and each segment carries the half hour
          it stands for in its `title` for a pointer.
        -->
        <div
          class="bar"
          role="img"
          aria-label="The last 24 hours of {monitor.name}: {drawn.filter(
            (bucket) => bucket.samples > 0,
          ).length} of 48 half-hours sampled"
        >
          {#each drawn as bucket (bucket.from)}
            <span
              class="seg {bucket.state ?? 'gap'}"
              data-state={bucket.state ?? ""}
              title={segmentTitle(bucket.from, bucket.state)}
            ></span>
          {/each}
        </div>

        <div class="foot">
          <span class="rd">Last check {ago(monitor.checked_at, clock)}</span>
          {#if monitor.response_time_ms !== null}
            <span class="rd">{Math.round(monitor.response_time_ms)} ms</span>
          {/if}
          {#each monitor.uptime as ratio (ratio.window)}
            <span class="rd up">{uptimeText(ratio)}</span>
          {/each}
          {#if monitor.cert_days_remaining !== null}
            <span class="rd cert">Cert {Math.round(monitor.cert_days_remaining)} d</span>
          {/if}

          <!--
            What it is watching. The path rides with the name because two
            containers called `postgres` are told apart by it and by nothing
            else; a monitor attached to nothing says so, since *nobody has
            attached this* is the fact the estate file's monitor names exist to
            close.
          -->
          <span class="at">
            {#if monitor.assets.length === 0}
              <span class="faint">Attached to no asset</span>
            {:else}
              {#each monitor.assets as asset (asset.id)}
                <a class="ast" href={hashFor({ view: "assets", tab: "tree", assetId: asset.id })}>
                  {asset.name}<span class="pth faint">{asset.path ?? "top level"}</span>
                </a>
              {/each}
            {/if}
          </span>
        </div>
      </li>
    {/each}

    {#if loaded && roster.length === 0}
      <li class="none">
        Nothing is mirrored yet. Monitors arrive from an Uptime Kuma source —
        configure one in Sources and the roster fills on its next sync.
      </li>
    {:else if loaded && chip !== null && shown.length === 0}
      <!--
        Named by the chip and not by the state, because one of the six is not a
        state: "No monitor is other" is a sentence about nothing, and the chip
        is visibly pressed above this line either way.
      -->
      <li class="none">No monitor matches the {CHIP_LABELS[chip]} filter.</li>
    {/if}
  </ol>
</section>

<style>
  .mons {
    display: flex;
    flex-direction: column;
    min-height: 0;
    overflow: hidden;
  }

  /* A row of its own under the room bar, the shape the Tree's search strip
     has: `app.css` gives the room bar 40px and `overflow: hidden`, and eight
     chips with counts do not fit beside a heading and a tab strip. */
  .chips {
    display: flex;
    gap: 6px;
    padding: 8px 14px;
    border-bottom: 1px solid var(--hair);
    flex: none;
  }

  .chip {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 24px;
    padding: 0 9px;
    border: 1px solid var(--hair2);
    border-radius: 12px;
    color: var(--muted);
    font: 500 11px var(--mono);
  }

  .chip:hover {
    background: var(--raised);
    color: var(--text);
  }

  /* The chip in force reads as pressed and not merely as hovered: the list
     under it is narrowed, and a reader who cannot see which chip did it is
     looking at a roster that has silently lost rows. */
  .chip.on {
    background: var(--raised);
    color: var(--text);
    border-color: var(--text);
  }

  .chip .n {
    color: var(--text);
  }

  /* The state's colour is on the count, not on the whole chip: a row of six
     coloured pills is a traffic light nobody can read, and the number is the
     thing the eye is looking for. */
  .chip.up .n {
    color: var(--ok);
  }

  .chip.warn .n {
    color: var(--amber);
  }

  .chip.down .n {
    color: var(--fail);
  }

  .chip.pending .n,
  .chip.paused .n,
  .chip.other .n {
    color: var(--muted);
  }

  /* No markers: the roster is a list of monitors and not a numbered one, and
     `app.css`' reset zeroes the padding without touching `list-style` — so an
     `ol` here draws `1.` beside every monitor unless it is said. `ol` and not
     `ul` because the order is the answer's, by name. */
  .roster {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 4px 14px 14px;
    list-style: none;
  }

  .mon {
    padding: 10px 0;
    border-bottom: 1px solid var(--hair);
  }

  .head {
    display: flex;
    align-items: baseline;
    gap: 8px;
    min-width: 0;
  }

  .st {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--muted);
    flex: none;
    align-self: center;
  }

  .st.up {
    background: var(--ok);
  }

  .st.warn {
    background: var(--amber);
  }

  .st.down {
    background: var(--fail);
  }

  .nm {
    font: 500 13px var(--mono);
    color: var(--text);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .state {
    font: 500 11px var(--mono);
    color: var(--muted);
  }

  .state.up {
    color: var(--ok);
  }

  .state.warn {
    color: var(--amber);
  }

  .state.down {
    color: var(--fail);
  }

  .meta {
    font-size: 11px;
    display: flex;
    gap: 8px;
    min-width: 0;
    overflow: hidden;
  }

  .meta .tgt {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .head .kuma {
    margin-left: auto;
    flex: none;
    font-size: 11px;
    color: var(--link);
    padding: 0 4px;
    border-radius: 2px;
  }

  .head .kuma:hover {
    background: var(--raised);
  }

  /* Forty-eight equal segments, `flex: 1` each, so the bar is the width it is
     given and the arithmetic that decides *how many* stays in `monitors.ts`.
     A 1px gap, because forty-eight touching rectangles of the same colour read
     as one rectangle and the point of the bar is that a reader can see where
     one half hour ends. */
  .bar {
    display: flex;
    gap: 1px;
    height: 14px;
    margin: 8px 0 6px;
  }

  .bar .seg {
    flex: 1;
    min-width: 0;
    border-radius: 1px;
    background: var(--hair2);
  }

  .bar .seg.up {
    background: var(--ok);
  }

  .bar .seg.warn {
    background: var(--amber);
  }

  .bar .seg.down {
    background: var(--fail);
  }

  .bar .seg.pending,
  .bar .seg.maintenance {
    background: var(--muted);
  }

  .foot {
    display: flex;
    align-items: baseline;
    flex-wrap: wrap;
    gap: 4px 12px;
    font-size: 11px;
    color: var(--muted);
  }

  .foot .at {
    margin-left: auto;
    display: flex;
    gap: 10px;
    flex-wrap: wrap;
  }

  .foot .ast {
    color: var(--link);
    text-decoration: none;
    display: flex;
    gap: 5px;
  }

  .foot .ast:hover {
    text-decoration: underline;
  }

  .foot .pth {
    font-size: 10px;
  }

  .none {
    padding: 18px 0;
    color: var(--muted);
    font-size: 12px;
  }

  .rf {
    padding: 8px 14px;
    font-size: 12px;
  }
</style>
