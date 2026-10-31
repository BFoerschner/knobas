<!--
  Settings › Desktop notifications (#831, R-13a.5): prototype-6's
  `STGV.notifications` over the one store the inbox notifier reads
  (`inbox/notify.svelte.ts`), so the switch on screen is the switch that fires.
  Switching the first one on asks the OS (story 72); a refusal leaves it off
  and says so.
-->
<script lang="ts">
  import type { Notifications } from "../../inbox/notify.svelte";
  import { ipcErrorMessage } from "../../ipc";
  import { push } from "../../shell/toasts.svelte";
  import type { SectionPorts } from "./ports";
  import { NOTIFY_KINDS } from "./sections";

  let {
    ports,
    store,
  }: {
    ports: Pick<SectionPorts, "notify">;
    store: Pick<Notifications, "kinds" | "loaded" | "permission" | "error" | "busy" | "reseed" | "choose">;
  } = $props();

  $effect(() => {
    if (!store.loaded) void store.reseed();
  });

  async function test() {
    try {
      await ports.notify({
        title: "knobas",
        body: "A test desktop notification. An inbox item you chose here arrives like this.",
        address: "#/settings/notifications",
      });
      push({ text: "Sent a test desktop notification" });
    } catch (cause) {
      push({ text: `Could not send a test desktop notification: ${ipcErrorMessage(cause)}`, tone: "err" });
    }
  }
</script>

<header class="tf-stg-h tf-stg-srch">
  <div>
    <h1>Desktop notifications</h1>
    <p>
      A desktop notification for each new inbox item you choose here, only while knobas is not in front. Clicking it
      opens the item.
    </p>
  </div>
  {#if store.error !== null && !store.loaded}
    <div class="tf-acts"><button class="tf-btn" onclick={() => void store.reseed()}>Try again</button></div>
  {:else if store.loaded}
    <div class="tf-acts"><button class="tf-btn" onclick={() => void test()}>Send a test desktop notification</button></div>
  {/if}
</header>
{#if store.error !== null && !store.loaded}
  <p class="tf-stg-p tf-stg-errt">Could not read the desktop notification settings: {store.error}</p>
{:else if store.loaded}
  {#each NOTIFY_KINDS as kind (kind.id)}
    {@const on = store.kinds.includes(kind.id)}
    <div class="tf-stg-row">
      <div class="tf-stg-l">{kind.label}</div>
      <div class="tf-stg-c">
        <button
          class="tf-stg-sw"
          role="switch"
          aria-checked={on}
          aria-label={kind.label}
          disabled={store.busy}
          onclick={() => void store.choose(kind.id, !on)}><i></i></button
        >
      </div>
    </div>
  {/each}
  {#if store.permission === "refused"}
    <p class="tf-stg-note tf-stg-errt" role="status">
      Your operating system refused desktop notifications for knobas, so nothing was switched on. Allow them for knobas
      in the system settings, then try again.
    </p>
  {:else if store.error !== null}
    <p class="tf-stg-note tf-stg-errt" role="status">Could not save: {store.error}</p>
  {/if}
{/if}
