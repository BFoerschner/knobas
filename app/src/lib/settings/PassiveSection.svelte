<!--
  The **Passive attribution** section of the settings view (issue #282, spec
  #272; `CONTEXT.md`'s *passive attribution*).

  ## Why this is a section and not a checkbox

  It is the one setting in knobas that changes what knobas *records about the
  person using it*, so the section says in full what it will and will not do
  before it offers the switch: what is recorded, that it never leaves the
  machine, and that nothing recorded this way is ever logged to a ticket
  without somebody saying so. A toggle with a four-word label would be a
  smaller surface and a worse one.

  ## Why the toggle draws the stored value and not the click

  `set_passive_attribution` answers with what is now stored, and that answer is
  what this renders — the rule `BackupSection` follows for the schedule. A
  checkbox that flipped optimistically would tell the reader they had opted in
  on the one run where the write failed.
-->
<script lang="ts">
  import { ipcErrorMessage } from "../ipc";
  import {
    passiveAttribution as realRead,
    setPassiveAttribution as realWrite,
  } from "../ipc/time";
  import { latestRead } from "../shell/latest-read";

  let {
    ports,
  }: {
    /** The bridge, injectable so a test needs no Tauri — `DayReview`'s shape. */
    ports?: Partial<{
      passiveAttribution: typeof realRead;
      setPassiveAttribution: typeof realWrite;
    }>;
  } = $props();

  // svelte-ignore state_referenced_locally
  const io = {
    passiveAttribution: realRead,
    setPassiveAttribution: realWrite,
    ...ports,
  };

  /** `null` until the first read answers — *unknown*, which is not *off*. */
  let enabled = $state<boolean | null>(null);
  let failure = $state<string | null>(null);
  let saving = $state(false);

  /**
   * Which read is the current one. Two presses over a slow bridge put two
   * writes in flight and nothing orders their answers — the guard #107 exists
   * for, in the module that keeps its rejection half.
   */
  const read = latestRead<boolean>();

  function load() {
    return read(io.passiveAttribution, {
      ok: (stored) => {
        enabled = stored;
        failure = null;
      },
      fail: (cause) => {
        // Not `false`: "off" is a claim about what is stored, and a section
        // that could not ask has not earned it. The rule the sources view and
        // the backup section both follow.
        failure = ipcErrorMessage(cause);
      },
    });
  }

  $effect(() => {
    void load();
  });

  async function toggle(to: boolean) {
    saving = true;
    try {
      enabled = await io.setPassiveAttribution(to);
      failure = null;
    } catch (cause) {
      failure = ipcErrorMessage(cause);
    } finally {
      saving = false;
    }
  }
</script>

<div class="tile-h">
  <span class="lab">Passive attribution</span>
</div>

<div class="sec-b">
  <p>
    While this is on, knobas records what was in front of you — the detail you
    had open, else the room you were in — every thirty seconds that its window
    is focused, and turns those observations into <em>passive</em> blocks on the
    day review. Anything shorter than two minutes is dropped, and a stretch it
    is unsure about stays an unaccounted gap.
  </p>
  <p class="sub">
    Nothing recorded this way leaves your machine, and nothing passive is ever
    logged to a ticket until you assign it and say so. Switching this off stops
    the recording; the blocks already offered stay where they are.
  </p>

  {#if failure}
    <p class="fail">{failure}</p>
    <button class="btn" onclick={() => void load()}>Retry</button>
  {:else if enabled !== null}
    <label class="chk">
      <input
        type="checkbox"
        checked={enabled}
        disabled={saving}
        onchange={(event) => void toggle(event.currentTarget.checked)}
      />
      Record what was open, and offer it on the day review
    </label>
  {/if}
</div>

<style>
  .sec-b {
    padding: 12px;
    font-size: 12px;
    line-height: 1.6;
  }

  .sec-b p {
    max-width: 78ch;
  }

  .sec-b p + p {
    margin-top: 8px;
  }

  .sub {
    color: var(--muted);
  }

  .fail {
    color: var(--fail);
  }

  .chk {
    display: flex;
    gap: 8px;
    align-items: center;
    margin-top: 12px;
    font-size: 12px;
  }

  .chk input {
    accent-color: var(--text);
  }
</style>
