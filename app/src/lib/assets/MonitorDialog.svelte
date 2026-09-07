<!--
  *Create monitor for this asset* — the pane's small form (issue #453, spec
  #427 story 70).

  ## Two writes, in this order, and the order is the design

  Pressing *Create* does two things and they are not interchangeable:

  1. **the name goes onto the asset** (`editAsset`, `field: "monitors"`), and
  2. **the create is queued** (`submitWrite`, `CreateMonitor`).

  The name first, because the name is what *attaches* the monitor: the write
  reaches Kuma, Kuma starts checking, the next poll mirrors the monitor, and
  `knobas_sync::attach` draws the `monitored-by` link to every asset carrying
  that name. A create whose write landed with no name recorded would be a
  monitor watching this asset that knobas joined to nothing — invisible in the
  pane and on the *Not monitored* roster at the same time.

  Written the other way round, the failure is recoverable and it is **visible**:
  the name is on the asset, the pane lists it under *Named by the import, not
  in Kuma yet*, and pressing *Create* again is the retry. That is the state an
  estate file written before its monitors existed has been in since #439, so it
  is a state the pane already draws and a reader already has a word for.

  ## What it does not do

  **No optimistic monitor.** The roster the pane draws is the mirror's, and the
  mirror learns about this one on the next poll — up to a minute away. So the
  toast says what knobas did (*queued*) and the pane catches up on its own,
  which is `MonitorsView`'s rule for pause and resume, for the same reason.

  **No schedule, no notification, no monitor type.** The adapter creates an
  HTTP check on Kuma's own default schedule and says why
  (`knobas_source_kuma::create`); tuning one is a click away on its own page in
  Kuma (story 71). A form that asked would be asking a reader for numbers
  before knobas has anything to say about which ones are right.

  **No source picker unless there are two.** `monitor_targets` is almost always
  empty or one long, and a `<select>` over one option is a question with one
  answer.
-->
<script lang="ts">
  import { ipcErrorMessage } from "../ipc";
  import type { AssetDetail, editAsset, MonitorTarget } from "../ipc/assets";
  import type { submitWrite } from "../ipc/entity";
  import Modal from "../shell/Modal.svelte";
  import { prefillFor } from "./create-monitor";

  let {
    detail,
    record,
    queue,
    onclose,
    oncreated,
  }: {
    /** The asset the monitor is for, as the pane read it. */
    detail: AssetDetail;
    /** Record the monitor's name on the asset — the first of the two writes. */
    record: typeof editAsset;
    /** Queue the create — the second. */
    queue: typeof submitWrite;
    onclose: () => void;
    /**
     * Both writes went. The pane re-reads and says what was queued; this
     * dialog is done.
     */
    oncreated: (what: { name: string; source: string }) => void;
  } = $props();

  // svelte-ignore state_referenced_locally
  // Read once, at init, `RouteDialog`'s rule: re-deriving would overwrite what
  // the reader has typed the next time the pane re-read.
  const opening = prefillFor(detail);
  let name = $state(opening.name);
  let url = $state(opening.url);
  // svelte-ignore state_referenced_locally
  let target = $state<MonitorTarget | null>(detail.monitor_targets[0] ?? null);

  let writing = $state(false);
  let failure = $state<string | null>(null);

  /** Unique per instance, so two stacked dialogs cannot share a field id. */
  const fieldId = `monitor-${Math.random().toString(36).slice(2, 9)}`;

  const targets = $derived(detail.monitor_targets);
  const ready = $derived(!writing && name.trim() !== "" && url.trim() !== "" && target !== null);

  function onfieldkeydown(event: KeyboardEvent) {
    if (event.key !== "Enter") return;
    event.preventDefault();
    void submit();
  }

  /** Pick a source by id — a `<select>` carries strings, not objects. */
  function pick(sourceId: string) {
    target = targets.find((row) => row.source_id === sourceId) ?? null;
  }

  async function submit() {
    const chosen = target;
    if (!ready || chosen === null) return;
    writing = true;
    failure = null;
    const wanted = name.trim();
    try {
      // First. See the header: the name is what attaches the monitor, and the
      // half-done state this order leaves is the one the pane can already say
      // out loud.
      await record(detail.asset.id, [{ field: "monitors", added: [wanted] }]);
      await queue({ CreateMonitor: { entity: chosen.roster, name: wanted, url: url.trim() } });
      oncreated({ name: wanted, source: chosen.display_name });
    } catch (rejection) {
      // In place and in the backend's own words: a URL with no scheme is
      // refused by the adapter by name, and a source that lost its account
      // between the read and the press is refused by `submit_write`.
      failure = ipcErrorMessage(rejection);
    } finally {
      writing = false;
    }
  }
</script>

<Modal title="Create monitor" subtitle={`For ${detail.asset.name}`} {onclose}>
  {#snippet body()}
    <div class="fields">
      <div class="fld">
        <label class="l" for="{fieldId}-name">Name</label>
        <!-- `Modal` focuses the first focusable control, which is this one. -->
        <input
          class="inp"
          id="{fieldId}-name"
          type="text"
          autocomplete="off"
          placeholder="gitea"
          bind:value={name}
          onkeydown={onfieldkeydown}
        />
        <p class="hint">
          What it is called in Uptime Kuma — and what attaches it here. The next poll
          finds a monitor of this name and draws the <em>monitored-by</em> link.
        </p>
      </div>

      <div class="fld">
        <label class="l" for="{fieldId}-url">URL to check</label>
        <input
          class="inp"
          id="{fieldId}-url"
          type="text"
          autocomplete="off"
          placeholder="http://127.0.0.1:3000/"
          bind:value={url}
          onkeydown={onfieldkeydown}
        />
        <p class="hint">
          An HTTP check, once a minute. Uptime Kuma reaches it from where <em>it</em> runs,
          which is not always where you are.
        </p>
      </div>

      {#if targets.length > 1}
        <div class="fld">
          <label class="l" for="{fieldId}-src">Create it in</label>
          <select
            class="inp"
            id="{fieldId}-src"
            value={target?.source_id ?? ""}
            onchange={(event) => pick(event.currentTarget.value)}
            onkeydown={onfieldkeydown}
          >
            {#each targets as row (row.source_id)}
              <option value={row.source_id}>{row.display_name}</option>
            {/each}
          </select>
        </div>
      {:else if target}
        <p class="in">In <span class="nm">{target.display_name}</span></p>
      {/if}

      {#if failure}
        <p class="fail" role="alert">{failure}</p>
      {/if}
    </div>
  {/snippet}

  {#snippet footer()}
    <button class="btn" onclick={onclose}>Cancel</button>
    <button class="btn pri" disabled={!ready} onclick={() => void submit()}>
      {writing ? "Queueing…" : "Create"}
    </button>
  {/snippet}
</Modal>

<style>
  .fields {
    display: grid;
    gap: 12px;
  }

  .fld .l {
    display: block;
    font: 500 10px var(--mono);
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--faint);
    margin-bottom: 4px;
  }

  /* `.inp`, `.btn` and `.fail` are `app.css`' -- redeclaring them here is how
     two dialogs come to look different. `RouteDialog` styles the same three
     things and no more. */
  .hint {
    margin-top: 6px;
    font-size: 11px;
    color: var(--muted);
  }

  .in {
    margin: 0;
    font-size: 12px;
    color: var(--muted);
  }

  .in .nm {
    color: var(--text);
  }
</style>
