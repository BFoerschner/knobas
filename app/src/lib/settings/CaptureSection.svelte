<!--
  The **Capture shortcut** section of the settings view (issue #503, spec #491
  story 40).

  One field: the key combination that opens the capture window over whatever is
  in front. **Empty by default**, and that is the decision rather than an
  oversight — a global shortcut is the one preference an application can take
  away from every other application on the machine, so knobas registers nothing
  at all until somebody chooses one.

  The field draws what is **stored**, which is `CheckoutsSection`'s rule and for
  its reason: a field that kept what was typed would tell the reader they had
  set a shortcut on the one run where the write failed.

  A combination the plugin cannot parse, or one the operating system will not
  hand over, is **not** a refused write. It is stored, and the answer says why
  it is not holding — so this section has one state to draw for both a
  combination that was never valid and one another application took last
  Tuesday, which is the only shape that can report the second at all.
-->
<script lang="ts">
  import { ipcErrorMessage } from "../ipc";
  import {
    captureShortcut as realRead,
    setCaptureShortcut as realWrite,
    type CaptureShortcut,
  } from "../ipc/entity";
  import { latestRead } from "../shell/latest-read";

  let {
    ports,
  }: {
    /** The bridge, injectable so a test needs no Tauri — `PassiveSection`'s shape. */
    ports?: Partial<{
      captureShortcut: typeof realRead;
      setCaptureShortcut: typeof realWrite;
    }>;
  } = $props();

  // svelte-ignore state_referenced_locally
  const io = {
    captureShortcut: realRead,
    setCaptureShortcut: realWrite,
    ...ports,
  };

  /** `undefined` until the first read answers — *unknown*, which is not *unset*. */
  let stored = $state<CaptureShortcut | undefined>(undefined);
  let draft = $state("");
  let failure = $state<string | null>(null);
  let saving = $state(false);

  const read = latestRead<CaptureShortcut>();

  /** Take a fresh answer, draft included. */
  function take(answer: CaptureShortcut) {
    stored = answer;
    draft = answer.accelerator ?? "";
    failure = null;
  }

  function load() {
    return read(io.captureShortcut, {
      ok: take,
      fail: (cause) => {
        failure = ipcErrorMessage(cause);
      },
    });
  }

  $effect(() => {
    void load();
  });

  async function save(value: string | null) {
    saving = true;
    try {
      take(await io.setCaptureShortcut(value));
    } catch (cause) {
      failure = ipcErrorMessage(cause);
    } finally {
      saving = false;
    }
  }
</script>

<div class="tile-h">
  <span class="lab">Capture shortcut</span>
</div>

<div class="sec-b">
  <p>
    A key combination that opens a small window over whatever you are looking
    at. What you type becomes a note, linked to the context you were last
    standing in and to what was in front of you.
  </p>
  <p class="sub">
    Written the way <code>CmdOrCtrl+Shift+N</code> is: modifiers
    (<code>CmdOrCtrl</code>, <code>Shift</code>, <code>Alt</code>,
    <code>Ctrl</code>) and one key, joined by <code>+</code>. Empty means knobas
    registers nothing.
  </p>

  {#if failure}
    <p class="fail">{failure}</p>
    <button class="btn" onclick={() => void load()}>Retry</button>
  {:else if stored !== undefined}
    <div class="field">
      <label class="lab" for="capture-shortcut">Shortcut</label>
      <input
        id="capture-shortcut"
        class="inp"
        bind:value={draft}
        disabled={saving}
        placeholder="CmdOrCtrl+Shift+N"
      />
      <!--
        Each button says which field it belongs to, the accessibility fix
        `CheckoutsSection` records — and the name the desktop witness presses
        it by (#503), which `just witness-unit` pins.
      -->
      <div class="acts">
        <button
          class="btn"
          aria-label="Save capture shortcut"
          disabled={saving || draft.trim() === (stored.accelerator ?? "")}
          onclick={() => void save(draft)}
        >
          Save
        </button>
        {#if stored.accelerator !== null}
          <button
            class="btn"
            aria-label="Clear capture shortcut"
            disabled={saving}
            onclick={() => void save(null)}
          >
            Clear
          </button>
        {/if}
      </div>
      {#if stored.accelerator === null}
        <p class="sub">Not set — no shortcut is registered.</p>
      {:else if stored.refusal !== null}
        <p class="fail">Not registered: {stored.refusal}</p>
      {:else}
        <p class="sub">Registered.</p>
      {/if}
    </div>
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

  /*
    `field` and not `row`, for `CheckoutsSection`'s reason: the stylesheet's
    global `.row` is the list row, and a form laid out in it clips its own
    label and its last button.
  */
  .field {
    margin-top: 12px;
    display: grid;
    gap: 6px;
    max-width: 60ch;
  }

  .acts {
    display: flex;
    gap: 6px;
  }
</style>
