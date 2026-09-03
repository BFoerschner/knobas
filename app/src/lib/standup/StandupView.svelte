<!--
  The standup digest — yesterday, today and blockers, at its own address
  (issue #288, spec #272 stories 58-63).

  **Three lists and nothing else on the screen.** Story 58 asks for the standup
  to be *a glance rather than a scroll through four systems*, so the view has no
  filters, no sort, no controls and nothing to configure: whatever the reader
  would have gone looking for in Gitea, TeamCity, Jira and Confluence is
  already on it.

  **Every line is a way in.** Story 59 — *every digest line links to the
  commit, PR, worklog, comment or transition it came from, so that a line is
  something I can check.* A line is a button that opens its item's address, and
  the address is built with `hashFor` for `InboxView`'s reason: entity keys
  carry `#` and `/`, and unencoded the first truncates the fragment at the
  browser level and the second reads as another path segment.

  The one line that is **not** a button is a running timer on an ad-hoc label,
  which has no item behind it. It is drawn as text rather than dropped: "DB
  config for the migration" is a legal thing to be working on, and a digest
  that left the reader's own afternoon out because it had nothing to link to
  would be answering a display problem with a missing fact.

  **Every line says where it came from.** The reason is the backend's own
  sentence, rendered as it arrives and never re-assembled here from the verb: a
  line whose provenance cannot be shown is not shippable, and a reason built by
  whichever surface happens to draw it is one that can be missing from the next
  surface.

  **An empty list says so, in its own words.** Three different silences —
  nothing found in a week, nothing yet today, nothing blocked — and the third
  is good news while the first is not. One shared "Nothing here" would tell the
  reader which list was empty and nothing else.
-->
<script lang="ts">
  import { ipcErrorMessage } from "../ipc";
  import { standupDigest as realStandupDigest, type DigestLine } from "../ipc/entity";
  import { dayKey, dayLabel } from "../time/day";
  import { latestRead } from "../shell/latest-read";
  import { hashFor, type Router } from "../shell/router.svelte";
  import { ago } from "../shell/time";
  import { digestWindows, LOOKBACK_DAYS } from "./standup";

  let {
    router,
    ports,
    now = () => new Date(),
  }: {
    router: Router;
    /** The bridge, injectable so a test needs no Tauri. */
    ports?: { standupDigest?: typeof realStandupDigest };
    /** Injectable clock — which day the digest is about. */
    now?: () => Date;
  } = $props();

  // svelte-ignore state_referenced_locally
  // Read once, at init, the decision `DayReview`'s own ports record: production
  // omits this prop and a bridge swapped mid-life would leave what is on screen
  // read through one set of ports and re-read through another.
  const io = { standupDigest: realStandupDigest, ...ports };

  const key = $derived(dayKey(now()));

  let yesterdayDay = $state<string | null>(null);
  let yesterday = $state<DigestLine[]>([]);
  let today = $state<DigestLine[]>([]);
  let blockers = $state<DigestLine[]>([]);
  /** The read failed. Its sentence stays on screen; there is nothing else. */
  let failure = $state<string | null>(null);
  /** Nothing has come back yet — told apart from "three empty lists". */
  let loaded = $state(false);

  const read = latestRead<Awaited<ReturnType<typeof realStandupDigest>>>();

  $effect(() => {
    void load(key);
  });

  async function load(on: string) {
    const { today: window, earlier } = digestWindows(on);
    await read(() => io.standupDigest(window, earlier), {
      ok: (digest) => {
        yesterdayDay = digest.yesterday_day;
        yesterday = digest.yesterday;
        today = digest.today;
        blockers = digest.blockers;
        failure = null;
        loaded = true;
      },
      fail: (cause) => {
        failure = ipcErrorMessage(cause);
        loaded = true;
      },
    });
  }

  /**
   * The address a line opens at — the kind-agnostic `#/entity/<id>` alias.
   *
   * The line's `kind` is the mirror's word and the router's kind segment is
   * the view's; resolving one from the other is `get_entity`'s job and it
   * already does it. The same route `InboxView` and the day review both take.
   */
  function addressOf(line: DigestLine): string {
    return hashFor({
      view: "room",
      ctx: router.ctx,
      detail: { kind: null, entityId: line.entity_id ?? "" },
    });
  }

  function open(line: DigestLine) {
    if (line.entity_id === null) return;
    router.go(addressOf(line));
  }
</script>

<section class="view">
  <div class="room-bar">
    <h1>
      Standup
      <span class="k">{dayLabel(key)}</span>
    </h1>
  </div>

  <div class="view-b">
    {#if failure}
      <p class="empty fail">{failure}</p>
    {/if}

    {#snippet list(heading: string, lines: DigestLine[], silence: string)}
      <section class="dg">
        <h2>{heading}</h2>
        {#if lines.length === 0}
          <!--
            Each list says its own silence. A week with nothing in it and a
            morning that has not started yet are different facts about the day,
            and "no blockers" is the only one of the three that is good news.
          -->
          <p class="empty">{loaded ? silence : "Reading…"}</p>
        {:else}
          <ol class="dg-l">
            {#each lines as line (line.verb + ":" + (line.entity_id ?? line.title) + ":" + line.at)}
              <li class="dg-i">
                {#if line.entity_id === null}
                  <!--
                    An ad-hoc timer label: the one line with nowhere to click.
                    A disabled button would promise a door that is not there.
                  -->
                  <span class="dg-t">{line.title}</span>
                {:else}
                  <button class="dg-t link" onclick={() => open(line)} title={addressOf(line)}>
                    {line.title}
                  </button>
                {/if}
                <span class="dg-w">{line.reason}</span>
                <span class="dg-a">{ago(line.at, now())}</span>
              </li>
            {/each}
          </ol>
        {/if}
      </section>
    {/snippet}

    {@render list(
      yesterdayDay === null ? "Yesterday" : `Yesterday — ${dayLabel(yesterdayDay)}`,
      yesterday,
      `Nothing of yours in the last ${LOOKBACK_DAYS} days.`,
    )}
    {@render list("Today", today, "Nothing touched yet today.")}
    {@render list("Blockers", blockers, "Nothing of yours is blocked.")}
  </div>
</section>

<style>
  .dg {
    margin: 0 0 18px;
  }

  .dg h2 {
    margin: 0 0 6px;
    font-family: var(--disp);
    font-size: 15px;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--muted);
  }

  .dg-l {
    display: grid;
    gap: 2px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .dg-i {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(0, 1.2fr) auto;
    align-items: baseline;
    column-gap: 10px;
    min-height: var(--row);
    padding: 4px 8px;
    border: 1px solid var(--hair);
    border-radius: 2px;
    background: var(--panel);
  }

  .dg-t {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    text-align: left;
    color: var(--text);
  }

  /* A line is a way in, so it reads as one. `button` rather than `a`: nothing
     here navigates the document, and an anchor with no `href` is a link the
     keyboard cannot reach. */
  button.dg-t {
    padding: 0;
    border: 0;
    background: none;
    font: inherit;
    color: var(--link);
    cursor: pointer;
  }

  button.dg-t:hover {
    text-decoration: underline;
  }

  .dg-w {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--muted);
    font-size: 12px;
  }

  .dg-a {
    color: var(--faint);
    font-size: 12px;
    white-space: nowrap;
  }
</style>
