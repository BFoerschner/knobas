<!--
  *Load the Tidewater dataset* (ux31): the demo profile's fixture work and estate (`demo_load`), offered where a fresh
  demo profile has nothing yet: Tickets' zero-source state and Settings › Sources' header. It moved here from the
  first-run wizard when that went. The parent draws it only on the demo profile and only with no source, the wizard's
  own condition: `demo_load` is refused outside the demo profile (P13), and once a source exists the button would mix
  the fixture into it.

  `demo_load` emits no `source:health`, so a load reseeds the health store itself: that is what tells every page a
  source exists (and what takes Tickets out of its zero-source state). A toast says it loaded; nothing navigates.

  Its two labels share one cell, the hidden one keeping its room, so *Loading…* does not narrow the button and move
  what stands beside it (nothing moves once drawn).
-->
<script lang="ts">
  import { ipcErrorMessage } from "../../ipc";
  import { demoLoad } from "../../ipc/sources";
  import { health as sharedHealth, type Health } from "../../shell/health.svelte";
  import { push } from "../../shell/toasts.svelte";

  let {
    small = false,
    load = demoLoad,
    health = sharedHealth,
    onloaded,
  }: {
    /** A page body's size (`tf-sm`); a header's default otherwise. */
    small?: boolean;
    /** `demo_load`, injectable for a test. */
    load?: (() => Promise<unknown>) | undefined;
    /** Reseeded after a load; `null` where the parent's own re-read replaces the store (Settings). */
    health?: Pick<Health, "reseed"> | null;
    /** After a load that worked: the parent reads its sources again. */
    onloaded?: (() => void) | undefined;
  } = $props();

  let loading = $state(false);

  async function press() {
    if (loading) return;
    loading = true;
    try {
      await load();
      if (health !== null) void health.reseed();
      push({ text: "Loaded the Tidewater dataset." });
      onloaded?.();
    } catch (cause) {
      push({ text: `Could not load the Tidewater dataset: ${ipcErrorMessage(cause)}`, tone: "err" });
    } finally {
      loading = false;
    }
  }
</script>

<button type="button" class="tf-btn" class:tf-sm={small} disabled={loading} aria-busy={loading ? "true" : undefined} onclick={() => void press()}>
  <span class="tf-ld-demo">
    <span class:tf-ld-off={loading} aria-hidden={loading ? "true" : undefined}>Load the Tidewater dataset</span>
    <span class:tf-ld-off={!loading} aria-hidden={loading ? undefined : "true"}>Loading…</span>
  </span>
</button>
