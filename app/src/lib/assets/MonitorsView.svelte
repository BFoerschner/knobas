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

  **Three lists, and the tab is the answer to two questions.** #449 added the
  other half of story 68: **open-alert cards** above the roster — what is wrong
  right now, with the ack (#446) and one step to the affected asset — and the
  **Not monitored** roster below it, every asset nothing watches. *What is
  broken* and *what is unwatched* are the two ways monitoring fails, and the
  second one is invisible on a page that only lists monitors: a monitor nobody
  ever created draws no row anywhere.

  **The cards read the shared store, not a port of their own.** `alerts` is the
  same store the top strip's badge and the Assets view's strip read
  (`alerts.svelte.ts`), so an ack pressed here moves all three, and a monitor
  that recovers takes its card away through the store's own `sync:state`
  re-read rather than through anything this tab decides.

  Creating or pausing a monitor is the Kuma write half (story 81) and is not in
  M4.1's committed scope.
-->
<script lang="ts">
  import { ipcErrorMessage } from "../ipc";
  import {
    monitorRoster as realMonitorRoster,
    unmonitoredAssets as realUnmonitoredAssets,
    type MonitorRow,
    type UnmonitoredAsset,
    type UptimeRatio,
  } from "../ipc/assets";
  import { hashFor, type Router } from "../shell/router.svelte";
  import { openExternal as realOpenExternal } from "../shell/open-external";
  import { ago } from "../shell/time";
  import { alerts as sharedAlerts, type Alerts } from "./alerts.svelte";
  import AssetsTabs from "./AssetsTabs.svelte";
  import {
    bar,
    byType,
    CHIP_LABELS,
    chipCounts,
    chipOf,
    filtered,
    typeCounts,
    type ChipState,
  } from "./monitors";

  /** The bridge this view needs, injectable so a test needs no Tauri. */
  interface MonitorPorts {
    monitorRoster: typeof realMonitorRoster;
    /** The *Not monitored* roster (#449) — the assets nothing watches. */
    unmonitoredAssets: typeof realUnmonitoredAssets;
    /** *Open in Kuma* (story 71) — the OS browser, so a test can press it. */
    openExternal: typeof realOpenExternal;
  }

  let {
    router,
    ports,
    alerts = sharedAlerts,
    now = () => new Date(),
  }: {
    router: Router;
    ports?: Partial<MonitorPorts>;
    /**
     * The live open-alert store the cards are drawn from (#444, #449).
     *
     * A **prop with the module singleton as its default**, `AssetsView`'s and
     * `TopStrip`'s shape and the same store all three read: the cards here,
     * the strip above the Tree's columns and the badge in the top strip are
     * one list and one number, so an ack pressed on a card cannot leave the
     * badge counting an alert this tab has stopped drawing as unseen.
     */
    alerts?: Alerts;
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
    unmonitoredAssets: realUnmonitoredAssets,
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

  /** The assets nothing watches (#449), and the read's own failure. */
  let unmonitored = $state<UnmonitoredAsset[]>([]);
  let unmonitoredFailure = $state<string | null>(null);
  /**
   * Nothing has come back for the *Not monitored* read yet — told apart from
   * *an estate where everything is watched*, for {@link loaded}'s reason: the
   * second is a sentence this tab has to be able to say, and it is the one
   * every reader is working towards.
   */
  let unmonitoredLoaded = $state(false);
  /** The type in force on the *Not monitored* roster, or `null` for all of it. */
  let assetType = $state<string | null>(null);

  /**
   * The monitor whose ack is in flight, or `null`.
   *
   * Per card and **not** a flag on the store, although the inbox has one
   * there: two acks on two monitors are independent writes, and a store-wide
   * guard would either drop the second one silently or grey out a button the
   * reader has every right to press. What it is for is the double-click on
   * *one* card, which is the only race there is.
   */
  let acking = $state<string | null>(null);

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
    void readUnmonitored();
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

  /**
   * The *Not monitored* read, separate from {@link read} and deliberately not
   * awaited with it: the two lists answer different questions and one of them
   * failing is not a reason to draw neither. A tab whose roster loaded and
   * whose gaps did not still tells the reader most of what they came for, and
   * says so where the missing half would have been.
   */
  async function readUnmonitored(): Promise<void> {
    try {
      unmonitored = await io.unmonitoredAssets();
      unmonitoredFailure = null;
    } catch (error) {
      unmonitoredFailure = ipcErrorMessage(error);
    } finally {
      unmonitoredLoaded = true;
    }
  }

  const counts = $derived(chipCounts(roster));
  const shown = $derived(filtered(roster, chip));
  const types = $derived(typeCounts(unmonitored));
  const gaps = $derived(byType(unmonitored, assetType));

  /** Click to narrow, click again to clear. */
  function toggle(state: ChipState): void {
    chip = chip === state ? null : state;
  }

  /** Ack one card, keeping that card's button down until the write is over. */
  async function ack(monitorId: string): Promise<void> {
    acking = monitorId;
    try {
      await alerts.ack(monitorId);
    } finally {
      acking = null;
    }
  }

  /** The same gesture on the *Not monitored* roster's type filter. */
  function toggleType(typeId: string): void {
    assetType = assetType === typeId ? null : typeId;
  }

  /**
   * What an asset's path reads as.
   *
   * `"top level"` is the word `AssetsTile.svelte` already draws for an asset
   * with no ancestors, and every list on this tab goes through here so one
   * estate is described one way on one surface. It is a helper and not three
   * inline `??`s for exactly the reason the tab now has three lists: a fourth
   * spelling of *nowhere* would be a reader wondering whether it meant
   * something different.
   */
  function pathText(path: string | null): string {
    return path ?? "top level";
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

  <!--
    The alert read's own failure, **outside** the cards below and not inside
    them: the store keeps its last list when a read fails, so a first read that
    failed leaves the count at zero — and a message that only appeared when
    there were already cards would be silent in exactly the case where the tab
    has nothing to show and no idea whether that is good news.
  -->
  {#if alerts.error}
    <p class="fail rf" role="alert">The open alerts could not be read: {alerts.error}</p>
  {/if}

  <!--
    **The scrolling half of the tab.** Three lists in one scroller rather than
    three panes: they are read top to bottom in one pass — what is broken, what
    is watched, what is not — and a reader who has scrolled past the cards has
    read them.
  -->
  <div class="scroll">
    <!--
      **What is wrong right now**, as cards (spec #427 stories 57, 58 and 62;
      issue #449).

      Cards and not the Assets view's one-line rows, because these carry a
      **button**: an ack is a thing a reader does, and a row of six words with
      a control at the end of it is a row where the control is the accident.

      **One card per alert, and not one per watched asset**, which is the one
      place this deliberately differs from the Tree's strip. That strip is a
      list of *where to go next* and splits a monitor watching two assets into
      two rows for exactly that reason. A card is a list of *what to do next*,
      and the thing to do is one ack — two cards offering the same ack would be
      a reader wondering what the second one does.

      **Absent when there is nothing wrong**, the top strip badge's rule and
      for its reason: a permanently empty box is a place the eye keeps
      checking.
    -->
    {#if alerts.count > 0}
      <section class="cards" aria-label="Open alerts">
        <ul>
          {#each alerts.open as alert (alert.id)}
            <li class="card {alert.state}">
              <div class="chead">
                <span class="st {alert.state}" aria-hidden="true"></span>
                <span class="mn">{alert.monitor_name}</span>
                <span class="sw">{alert.state === "down" ? "Down" : "Warn"}</span>
                <span class="wh faint">{ago(alert.opened_at, clock)}</span>
                <!--
                  Acked is drawn and never hidden: an ack clears the reader's
                  inbox item and **leaves the alert open** (#446), so a card
                  that vanished would say the estate was well. The button goes
                  instead — there is nothing a second ack would say.
                -->
                {#if alert.acked_at}
                  <span class="ak">Acked {ago(alert.acked_at, clock)}</span>
                {:else}
                  <button
                    class="ack"
                    disabled={acking === alert.monitor_id}
                    onclick={() => void ack(alert.monitor_id)}
                  >
                    Ack
                  </button>
                {/if}
              </div>

              <!--
                What it watches, and the way in. The path rides with the name
                for the roster's reason: two containers called `postgres` are
                told apart by it and by nothing else. A monitor watching
                nothing says so and offers no way in — an alert nobody can act
                on is the one most worth saying out loud.
              -->
              <div class="cbody">
                {#if alert.assets.length === 0}
                  <span class="faint">watching nothing</span>
                {:else}
                  {#each alert.assets as watched (watched.id)}
                    <a
                      class="open"
                      href={hashFor({ view: "assets", tab: "tree", assetId: watched.id })}
                    >
                      <span class="nm">{watched.name}</span>
                      <span class="pth faint">{pathText(watched.path)}</span>
                    </a>
                  {/each}
                {/if}
              </div>
            </li>
          {/each}
        </ul>
      </section>
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
                    {asset.name}<span class="pth faint">{pathText(asset.path)}</span>
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

    <!--
      **What is unwatched** (issue #449, spec #427 story 68's second half).

      On this tab and not in the Tree, because it is the question the Monitors
      tab is the only surface that can answer: a monitor nobody ever created
      draws no row in the roster above, so a page that only listed monitors
      would be quietest about exactly the assets nothing is looking at.

      **Below the roster and not above it**, although both are lists of gaps:
      the cards are what is happening now and the roster is what is being
      watched, and *nobody is watching this* is the thing a reader goes looking
      for rather than the thing that interrupts them.

      The type filter's options are the types **this list holds** — see
      `typeCounts` — and every type is in the read, because which types are
      worth monitoring is a judgement about a particular estate and the filter
      is where the reader makes it.
    -->
    <section class="unmon" aria-labelledby="unmon-heading">
      <div class="uhead">
        <h2 id="unmon-heading">Not monitored</h2>
        {#if types.length > 0}
          <div class="tfs" role="group" aria-label="Filter unmonitored assets by type">
            {#each types as entry (entry.type_id)}
              <button
                class="tf {assetType === entry.type_id ? 'on' : ''}"
                aria-pressed={assetType === entry.type_id}
                onclick={() => toggleType(entry.type_id)}
              >
                <span class="lab">{entry.label}</span>
                <span class="n">{entry.count}</span>
              </button>
            {/each}
          </div>
        {/if}
      </div>

      {#if unmonitoredFailure}
        <p class="fail" role="alert">
          The unmonitored assets could not be read: {unmonitoredFailure}
        </p>
      {/if}

      <ol class="gaps">
        {#each gaps as gap (gap.id)}
          <li class="gap">
            <a href={hashFor({ view: "assets", tab: "tree", assetId: gap.id })}>
              <span class="mono" aria-hidden="true">{gap.monogram}</span>
              <span class="nm">{gap.name}</span>
              <span class="pth faint">{pathText(gap.path)}</span>
              <span class="ty faint">{gap.type_label}</span>
            </a>
          </li>
        {/each}
      </ol>

      {#if unmonitoredLoaded && !unmonitoredFailure && unmonitored.length === 0}
        <p class="none">Every asset has a monitor attached.</p>
      {:else if unmonitoredLoaded && assetType !== null && gaps.length === 0}
        <p class="none">No unmonitored asset is of that type.</p>
      {/if}
    </section>
  </div>
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

  /* One scroller for the three lists (#449). It is the flex child that gives,
     which is why the lists inside it are laid out at their natural height:
     three independently scrolling panes would mean a reader could have the
     cards off screen while looking at a roster of green. */
  .scroll {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
  }

  /* No markers: the roster is a list of monitors and not a numbered one, and
     `app.css`' reset zeroes the padding without touching `list-style` — so an
     `ol` here draws `1.` beside every monitor unless it is said. `ol` and not
     `ul` because the order is the answer's, by name. */
  .roster {
    padding: 4px 14px 14px;
    list-style: none;
  }

  /* The open-alert cards (#449). A band at the top of the scroller with a
     ground of its own, so that *what is wrong* is a different surface from
     *what is watched* rather than three more rows of the same list. */
  .cards {
    padding: 10px 14px;
    border-bottom: 1px solid var(--hair);
    background: var(--raised);
  }

  .cards ul {
    display: flex;
    flex-direction: column;
    gap: 6px;
    list-style: none;
  }

  .card {
    padding: 8px 10px;
    border: 1px solid var(--hair2);
    /* The state on the left edge and not as a fill: a card of solid red is
       unreadable, and the eye still finds the stripe. */
    border-left: 3px solid var(--muted);
    border-radius: 3px;
    background: var(--bg);
  }

  .card.down {
    border-left-color: var(--fail);
  }

  .card.warn {
    border-left-color: var(--amber);
  }

  .chead {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }

  .card .mn {
    font: 500 13px var(--mono);
    color: var(--text);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .card .sw {
    font: 500 11px var(--mono);
  }

  .card.down .sw {
    color: var(--fail);
  }

  .card.warn .sw {
    color: var(--amber);
  }

  .card .wh {
    font-size: 11px;
  }

  /* Pushed to the far end, `Open in Kuma`'s place on the roster below: the
     control a reader is reaching for sits in the same column on both lists. */
  .card .ack,
  .card .ak {
    margin-left: auto;
    flex: none;
    font-size: 11px;
  }

  .card .ack {
    height: 22px;
    padding: 0 10px;
    border: 1px solid var(--hair2);
    border-radius: 3px;
    color: var(--text);
  }

  .card .ack:hover:not(:disabled) {
    background: var(--raised);
    border-color: var(--text);
  }

  .card .ack:disabled {
    color: var(--muted);
  }

  .card .ak {
    color: var(--muted);
    font-family: var(--mono);
  }

  .cbody {
    display: flex;
    flex-wrap: wrap;
    gap: 4px 12px;
    margin-top: 4px;
    font-size: 11px;
    color: var(--muted);
  }

  .cbody .open {
    display: flex;
    gap: 6px;
    color: var(--link);
    text-decoration: none;
  }

  .cbody .open:hover {
    text-decoration: underline;
  }

  /* The *Not monitored* roster (#449), under the monitors with a rule between
     them: it is a list of assets and the one above is a list of monitors, and
     the two are answers to different questions. */
  .unmon {
    padding: 12px 14px 18px;
    border-top: 1px solid var(--hair);
  }

  .uhead {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-wrap: wrap;
    margin-bottom: 8px;
  }

  .uhead h2 {
    font: 500 12px var(--mono);
    color: var(--text);
  }

  .tfs {
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
  }

  /* The chips' shape, deliberately: it is the same gesture on the same tab,
     and a second visual language for it would say the two filters behave
     differently. */
  .tf {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 22px;
    padding: 0 9px;
    border: 1px solid var(--hair2);
    border-radius: 11px;
    color: var(--muted);
    font: 500 11px var(--mono);
  }

  .tf:hover,
  .tf.on {
    background: var(--raised);
    color: var(--text);
  }

  .tf.on {
    border-color: var(--text);
  }

  .tf .n {
    color: var(--text);
  }

  .gaps {
    display: flex;
    flex-direction: column;
    list-style: none;
  }

  .gap a {
    display: flex;
    align-items: baseline;
    gap: 8px;
    padding: 3px 4px;
    border-radius: 2px;
    color: var(--text);
    text-decoration: none;
    min-width: 0;
  }

  .gap a:hover {
    background: var(--raised);
  }

  .gap .mono {
    flex: none;
    width: 20px;
    color: var(--muted);
    font: 500 10px var(--mono);
  }

  .gap .nm {
    font: 500 12px var(--mono);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .gap .pth,
  .gap .ty {
    font-size: 11px;
  }

  /* The type at the far end, where the filter that hides it is: a reader who
     has narrowed to VMs is not reading the word `VM` nineteen times. */
  .gap .ty {
    margin-left: auto;
    flex: none;
  }

  .unmon .fail {
    font-size: 12px;
    margin-bottom: 6px;
  }

  .unmon .none {
    padding: 6px 0;
    color: var(--muted);
    font-size: 12px;
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
