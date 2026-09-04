<!--
  The **Desktop notifications** section of the settings view (issue #290, spec
  #272 "Notifications").

  ## Why it draws a store and not its own ports

  Every other section here takes its bridge as a prop, because every other
  section is the only reader of the setting it draws. This one is not: the
  listener in `inbox/notify.svelte.ts` reads the same categories to decide whether
  an item may interrupt somebody, and two independent reads of one setting is
  two chances for the switch on screen to disagree with the switch that fires.
  So the store is the seam, and the prop exists for the test that drives it.

  ## Why the permission is asked for here and not on the way in

  Story 72: knobas asks for nothing it is not about to use. The prompt belongs
  to the click that switches the first category on, and never to the settings view
  opening — a person browsing their settings has not asked to be interrupted.
  A refusal leaves every box off, which is the store's rule and is stated here
  as well, because a checkbox that sprang back is otherwise indistinguishable
  from one that failed to save.
-->
<script lang="ts">
  import { notifications as shared, type Notifications } from "../inbox/notify.svelte";
  import type { InboxCategory } from "../ipc/entity";

  let {
    store = shared,
  }: {
    /** The one store, or a test's own. See the note above. */
    store?: Notifications;
  } = $props();

  /**
   * The five categories, in `knobas_core::inbox::Category::ALL`'s order and
   * with knobas' own words for them.
   *
   * Pinned to that list by `knobas_app::inbox`'s
   * `every_inbox_category_has_a_toggle_in_the_interface`: a sixth category
   * added on the Rust side would otherwise be a demand nobody can ever switch
   * desktop notifications on for, with nothing failing anywhere.
   */
  const KINDS: { id: InboxCategory; label: string }[] = [
    { id: "review_request", label: "Review requests" },
    { id: "mention", label: "Mentions" },
    { id: "failed_build", label: "Failed builds" },
    { id: "new_assignment", label: "New assignments" },
    { id: "credential_expiry", label: "Credentials about to expire" },
  ];

  $effect(() => {
    if (!store.loaded) void store.reseed();
  });

  /**
   * Send the click, then **put the box back where the store says it is**.
   *
   * A checkbox is the one control that changes itself before anybody agrees:
   * the browser has already flipped it by the time this runs, and neither a
   * refused permission nor a refused write moves `store.kinds` — so without
   * this line the reader is looking at a switch that reads *on* over a setting
   * that says off, which is the one failure this surface must not have. The
   * assignment is a no-op on the ordinary path, where the store already agrees.
   */
  async function toggle(kind: InboxCategory, input: HTMLInputElement) {
    await store.choose(kind, input.checked);
    input.checked = store.kinds.includes(kind);
  }
</script>

<div class="tile-h">
  <span class="lab">Desktop notifications</span>
</div>

<div class="sec-b">
  <p>
    When an inbox item arrives while this window is <em>not</em> focused, knobas can
    hand it to the operating system as a desktop notification. Clicking one opens the
    item.
  </p>
  <p class="sub">
    Nothing is sent while the window is focused — you are never told about
    something already on screen — and nothing is sent for an item you have
    already been told about. Every category is off until you switch it on, and
    the first one you switch on is when your operating system is asked whether
    knobas may notify you at all.
  </p>

  {#if store.error}
    <p class="fail">{store.error}</p>
    <button class="btn" onclick={() => void store.reseed()}>Retry</button>
  {:else if store.loaded}
    {#each KINDS as kind (kind.id)}
      <label class="chk">
        <input
          type="checkbox"
          checked={store.kinds.includes(kind.id)}
          disabled={store.busy}
          onchange={(event) => void toggle(kind.id, event.currentTarget)}
        />
        {kind.label}
      </label>
    {/each}

    {#if store.permission === "refused"}
      <p class="fail">
        Your operating system refused desktop notifications for knobas, so
        nothing was switched on. Allow them for knobas in the system settings,
        then try again.
      </p>
    {:else if store.permission === "granted"}
      <p class="sub ok">Your operating system allows knobas to notify you.</p>
    {/if}
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

  .ok {
    margin-top: 12px;
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
