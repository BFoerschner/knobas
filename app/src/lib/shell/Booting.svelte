<!--
  The boot screen.

  The M0 carry-over, discharged: bring-up used to block the event loop behind a
  hidden window, so a first run — a PostgreSQL download plus an `initdb` — was
  a minute of nothing on screen. The window now appears immediately and says
  what it is doing.

  No spinner. A spinner is the same picture for "downloading 60 MB" and "wedged
  on a port that is already taken", and the difference between those is the
  only thing worth showing here.
-->
<script lang="ts">
  import type { DbState } from "../ipc/app";

  let {
    db,
    version,
    demo,
    onretry,
  }: {
    db: DbState;
    version: string;
    demo: boolean;
    onretry: () => void;
  } = $props();

  /** The line under the heading, or `null` when there is nothing true to say. */
  const detail = $derived(db.state === "starting" ? db.detail : null);
</script>

<div class="boot">
  <div class="boot-b">
    {#if db.state === "failed"}
      <h1>knobas cannot start its database</h1>
      <!-- Text, always: the message is whatever PostgreSQL or the OS said. -->
      <p class="mono fail">{db.message}</p>
      <p class="mono faint">
        knobas {version} · profile {demo ? "demo" : "default"}
      </p>
      <div class="acts">
        <button class="btn pri" onclick={onretry}>Retry</button>
      </div>
    {:else if db.state === "migrating"}
      <h1>Bringing the schema up to date</h1>
      <p class="mono muted">This takes a moment the first time after an update.</p>
    {:else}
      <h1>Starting the local database</h1>
      {#if detail}
        <p class="mono muted">{detail}</p>
      {/if}
    {/if}
  </div>
</div>

<style>
  .boot {
    display: flex;
    align-items: center;
    justify-content: center;
    /*
      Viewport units, not `100%`: this renders straight into `#app`, which has
      no height of its own, so a percentage would resolve against an
      auto-height box and collapse. `html, body { overflow: hidden }` means
      `100vh` is exactly the window.
    */
    height: 100vh;
    background: var(--bg);
  }

  .boot-b {
    max-width: 560px;
    padding: 0 24px;
  }

  .boot-b h1 {
    font: 600 26px/1.2 var(--disp);
    letter-spacing: 0.01em;
  }

  .boot-b p {
    margin-top: 10px;
    font-size: 12px;
    line-height: 1.6;
    /*
      The failure message is a raw error string with no line breaks of its own
      and often a long path in it. Wrapping it is what keeps it inside the
      window instead of running off the right-hand edge.
    */
    overflow-wrap: anywhere;
  }

  .acts {
    margin-top: 16px;
    display: flex;
    gap: 6px;
  }
</style>
