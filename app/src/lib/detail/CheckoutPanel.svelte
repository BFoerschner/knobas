<!--
  Where this repo's clone is on this disk — the repo and branch details' answer
  to "where do I stand" (spec §491 story 28, issue #499).

  A **checkout** (`CONTEXT.md`) is knobas-owned data about the local disk, never
  a field of the mirrored repo. Three states and each one says what it is:

  * a path the scan found under the clones root,
  * a path somebody set by hand, which wins,
  * *no checkout* — with the clone command to copy, when the repo has a URL.

  **Nothing here runs git** (ADR-0016). The clone command is text a person
  copies; knobas issues no clone, no checkout and no fetch, and the button
  beside it says *Copy* rather than *Clone* for that reason.

  Presentational except for the two writes it owns (set and clear the
  override), which it makes through the same command that answers the view, so
  the panel redraws from the backend's answer rather than from a hope.
-->
<script lang="ts">
  import { ipcErrorMessage } from "../ipc";
  import { entityCheckout, setCheckoutOverride, type CheckoutView } from "../ipc/entity";
  import { push } from "../shell/toasts.svelte";

  let { entityId }: { entityId: string } = $props();

  let checkout = $state<CheckoutView | null>(null);
  /** The read's own generation — a slow answer for a panel that moved on is dropped. */
  let token = 0;
  let editing = $state(false);
  let draft = $state("");
  let saving = $state(false);

  $effect(() => {
    const id = entityId;
    const mine = ++token;
    checkout = null;
    editing = false;
    void entityCheckout(id)
      .then((answer) => {
        if (mine !== token) return;
        checkout = answer;
      })
      .catch(() => {
        // Swallowed, and it is the panel's own read: a repo detail is worth
        // drawing without it, and a toast about a checkout nobody asked to see
        // would be noise over the item they did open. A `null` view renders
        // the reading line and then nothing.
      });
  });

  /** Set or clear the override, and redraw from what the backend now says. */
  async function store(path: string | null) {
    saving = true;
    try {
      checkout = await setCheckoutOverride(entityId, path);
      editing = false;
    } catch (rejection) {
      push({ text: `Could not set the checkout path: ${ipcErrorMessage(rejection)}`, tone: "err" });
    } finally {
      saving = false;
    }
  }

  async function copyCommand(command: string) {
    try {
      await navigator.clipboard.writeText(command);
      push({ text: "Clone command copied." });
    } catch {
      // A webview with no clipboard permission is a real state, and the
      // command is on screen to select by hand — so this says what happened
      // rather than pretending it worked.
      push({ text: "Could not reach the clipboard — select the command instead.", tone: "err" });
    }
  }

  function edit() {
    draft = checkout?.path ?? "";
    editing = true;
  }
</script>

<div class="sec">
  <div class="sec-h">
    <span class="lab">Checkout</span>
    {#if checkout?.found_by === "override"}
      <span class="k muted">set by hand</span>
    {:else if checkout?.found_by === "scan"}
      <span class="k muted">found under the clones root</span>
    {/if}
    <span class="acts">
      {#if checkout}
        <button class="btn sm" disabled={saving} onclick={edit}>
          {checkout.found_by === "override" ? "Change path…" : "Set path…"}
        </button>
        {#if checkout.found_by === "override"}
          <button
            class="btn sm"
            disabled={saving}
            title="Clear the path you set and go back to the clones-root scan"
            onclick={() => void store(null)}
          >
            Clear
          </button>
        {/if}
      {/if}
    </span>
  </div>

  {#if !checkout}
    <p class="muted">Reading…</p>
  {:else if checkout.path}
    <p class="cpath">{checkout.path}</p>
  {:else}
    <div class="empty">
      <p>No checkout on this machine.</p>
      {#if checkout.clones_root === null}
        <p>No clones root is set. Set one in Settings, or give this repo a path of its own.</p>
      {:else}
        <p>Nothing under {checkout.clones_root} has this repo as its origin.</p>
      {/if}
      {#if checkout.clone_command}
        <!--
          Text to copy, and only that. knobas never runs it: no clone, no
          checkout, no fetch (ADR-0016).
        -->
        {@const command = checkout.clone_command}
        <p class="ccmd">{command}</p>
        <button class="btn" onclick={() => void copyCommand(command)}>
          Copy clone command
        </button>
      {/if}
    </div>
  {/if}

  {#if editing}
    <div class="cedit">
      <label class="lab" for="checkout-path-{entityId}">Checkout path</label>
      <input
        id="checkout-path-{entityId}"
        class="inp"
        bind:value={draft}
        placeholder="/Users/you/src/payout-service"
      />
      <p class="muted">
        A path knobas will remember for this repo, ahead of anything the scan finds — for a clone
        outside the clones root, or a worktree.
      </p>
      <div class="acts">
        <button class="btn" disabled={saving || draft.trim() === ""} onclick={() => void store(draft)}>
          Save
        </button>
        <button class="btn" disabled={saving} onclick={() => (editing = false)}>Cancel</button>
      </div>
    </div>
  {/if}
</div>

<style>
  /*
    A filesystem path and a shell command are both monospace, and both may be
    longer than the panel: they wrap rather than widen it, the rule the body
    text follows.
  */
  .cpath,
  .ccmd {
    font: 12px/1.6 var(--mono);
    overflow-wrap: anywhere;
    margin: 0;
  }

  .ccmd {
    color: var(--muted);
    margin: 8px 0;
  }

  .cedit {
    margin-top: 10px;
    display: grid;
    gap: 6px;
  }
</style>
