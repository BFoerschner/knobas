<!--
  Where the repository page reads from (the clone-path pass, 2026-11-16; design system, *Controls*, the clone's sign):
  a sign in the line under the title, *Local* with how the clone stands (`main, 1 ahead, 1 changed`), or *Remote*
  where nothing is cloned, so the forge answers. It holds a few words and never the path, so a long path can't push or
  cut anything: Björn, "cloned at C:\x\y\z\foo\bar can get long in certain places and go out of bounds".

  The path is in its bubble, which opens on hover and on focus, and pinned by a press: *Cloned at*, the whole path
  wrapped (its folder in ink), how the clone stands, *Copy path*, and *Open in* the default application where one is
  set (the *Open in…* menu's first item). Closed, the bubble is visually hidden, not gone: the path is the sign's
  description and a run of text of its own, which a screen reader and the desktop witness read. `Esc` closes it and
  leaves focus on the sign; so do a press elsewhere and a move to another address.

  The sign and its bubble's behaviour are `TfSign`'s.
-->
<script lang="ts">
  import type { CheckoutView } from "../../ipc/entity";
  import { ipcErrorMessage } from "../../ipc";
  import { push } from "../../shell/toasts.svelte";
  import type { Layers } from "../layers.svelte";
  import { apps as sharedApps, type Apps } from "../openin/apps.svelte";
  import { menuOrder } from "../openin/openin";
  import { pathParts } from "../path";
  import TfSign from "../TfSign.svelte";
  import { checkoutSentence } from "./repo";

  let {
    entityId,
    checkout,
    source,
    apps = sharedApps,
    layers,
    copy = (text: string) => navigator.clipboard.writeText(text),
  }: {
    /** The repository or branch the checkout was read for: what *Open in* opens. */
    entityId: string;
    /** `undefined` while `entity_checkout` has not answered: nothing is drawn. */
    checkout: CheckoutView | undefined;
    /** The source's name, which a repository with no clone is read from. */
    source: string;
    apps?: Apps | undefined;
    layers?: Layers | undefined;
    /** Puts text on the clipboard; a test's own. */
    copy?: (text: string) => Promise<void>;
  } = $props();

  const COPIED_MS = 2000;

  const sentence = $derived(checkoutSentence(checkout));
  const path = $derived(sentence?.path ?? null);
  const parts = $derived(path === null ? null : pathParts(path));
  const standing = $derived(path === null ? "not cloned" : (sentence?.state ?? null));
  const opener = $derived(apps.settings === null ? null : (menuOrder(apps.settings).find((row) => row.isDefault)?.app ?? null));

  let copied = $state(false);
  let copiedTimer: ReturnType<typeof setTimeout> | undefined;

  $effect(() => {
    void apps.ensure();
  });

  async function copyPath() {
    if (path === null) return;
    try {
      await copy(path);
      copied = true;
      clearTimeout(copiedTimer);
      copiedTimer = setTimeout(() => (copied = false), COPIED_MS);
    } catch (cause) {
      push({ text: `Could not copy the path: ${ipcErrorMessage(cause)}`, tone: "err" });
    }
  }

  function openIn(close: () => void) {
    if (opener === null) return;
    close();
    void apps.run(entityId, opener.name, { application: opener.id, file: null, line: null });
  }

  $effect(() => () => clearTimeout(copiedTimer));
</script>

{#if checkout !== undefined}
  <TfSign class="tf-clonesign" faceClass={path === null ? "tf-remote" : ""} {layers}>
    {#snippet face()}{#if path !== null}<svg viewBox="0 0 16 16" aria-hidden="true"><path d="M1.75 3.75h4.5l1.5 1.5h6.5v7h-12.5z" /></svg
        ><span class="tf-sign-word">Local</span>{:else}<svg viewBox="0 0 16 16" aria-hidden="true"
          ><path d="M4.75 12.25h6.5a2.75 2.75 0 0 0 .35-5.48 3.75 3.75 0 0 0-7.2.98 2.25 2.25 0 0 0 .35 4.5z" /></svg
        ><span class="tf-sign-word">Remote</span>{/if}{#if standing !== null}<span class="tf-sign-state"
          ><span class="tf-sr">{", "}</span>{standing}</span
        >{/if}{/snippet}
    {#snippet bubble({ open, close, uid })}
      {#if path !== null && parts !== null}
        <span class="tf-signbubble-what">Cloned at</span>
        <span class="tf-sr" id="{uid}-desc">{path}</span>
        {#if open}
          <!-- The path drawn, its folder in ink; the hidden run above is what is read, as one run. -->
          <span class="tf-signbubble-drawn tf-mono tf-wrapall" aria-hidden="true"
            >{#if parts.head !== ""}<span class="tf-dim">{parts.head}</span>{/if}<b>{parts.tail}</b></span
          >
          {#if sentence?.state}<span class="tf-signbubble-state" aria-hidden="true">{sentence.state}</span>{/if}
          <span class="tf-signbubble-acts">
            <button type="button" class="tf-btn tf-sm" onclick={() => void copyPath()}>{copied ? "Copied" : "Copy path"}</button>
            {#if opener !== null}<button type="button" class="tf-btn tf-sm tf-ghost" onclick={() => openIn(close)}>Open in {opener.name}</button>{/if}
          </span>
        {/if}
      {:else}
        <span class="tf-signbubble-what" id="{uid}-desc">Read from {source}: not cloned on this machine.</span>
      {/if}
    {/snippet}
  </TfSign>
{/if}
