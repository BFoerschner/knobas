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

  The bubble stands in the page, under the sign and kept inside the window, rather than in the shell's float host: its
  actions come right after the sign in the `Tab` order.
-->
<script lang="ts">
  import { untrack } from "svelte";

  import type { CheckoutView } from "../../ipc/entity";
  import { ipcErrorMessage } from "../../ipc";
  import { push } from "../../shell/toasts.svelte";
  import { layers as sharedLayers, type Layers } from "../layers.svelte";
  import { apps as sharedApps, type Apps } from "../openin/apps.svelte";
  import { menuOrder } from "../openin/openin";
  import { pathParts } from "../path";
  import { checkoutSentence } from "./repo";

  let {
    entityId,
    checkout,
    source,
    apps = sharedApps,
    layers = sharedLayers,
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

  const uid = $props.id();
  /** How long a pointer rests on the sign before its bubble opens, and how long it may take to cross into the bubble. */
  const HOVER_OPEN_MS = 150;
  const HOVER_CLOSE_MS = 200;
  const COPIED_MS = 2000;
  /** What the bubble leaves between itself and the window's edge. */
  const EDGE = 8;

  const sentence = $derived(checkoutSentence(checkout));
  const path = $derived(sentence?.path ?? null);
  const parts = $derived(path === null ? null : pathParts(path));
  const standing = $derived(path === null ? "not cloned" : (sentence?.state ?? null));
  const opener = $derived(apps.settings === null ? null : (menuOrder(apps.settings).find((row) => row.isDefault)?.app ?? null));

  let hovered = $state(false);
  let focused = $state(false);
  let pinned = $state(false);
  /** `Esc` closed it while focus or the pointer is still on it; the next move in opens it again. */
  let dismissed = $state(false);
  let copied = $state(false);
  let wrap = $state<HTMLElement | null>(null);
  let face = $state<HTMLButtonElement | null>(null);
  let bubble = $state<HTMLElement | null>(null);
  const open = $derived(!dismissed && (pinned || hovered || focused));

  let hoverTimer: ReturnType<typeof setTimeout> | undefined;
  let copiedTimer: ReturnType<typeof setTimeout> | undefined;

  $effect(() => {
    void apps.ensure();
  });

  function close() {
    pinned = false;
    dismissed = true;
    if (bubble?.contains(document.activeElement)) face?.focus({ preventScroll: true });
  }

  function enter() {
    clearTimeout(hoverTimer);
    if (hovered) return;
    hoverTimer = setTimeout(() => {
      dismissed = false;
      hovered = true;
    }, HOVER_OPEN_MS);
  }

  function leave() {
    clearTimeout(hoverTimer);
    hoverTimer = setTimeout(() => (hovered = false), HOVER_CLOSE_MS);
  }

  function focusIn() {
    if (!focused) dismissed = false;
    focused = true;
  }

  function focusOut(event: FocusEvent) {
    if (event.relatedTarget instanceof Node && wrap?.contains(event.relatedTarget)) return;
    focused = false;
  }

  function press() {
    if (open && pinned) {
      close();
      return;
    }
    dismissed = false;
    pinned = true;
  }

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

  function openIn() {
    if (opener === null) return;
    pinned = false;
    dismissed = true;
    void apps.run(entityId, opener.name, { application: opener.id, file: null, line: null });
  }

  // Open: `Esc` reaches it first, a press elsewhere or another address closes it.
  $effect(() => {
    if (!open) return;
    const off = untrack(() => layers.add({ kind: "panel", close }));
    const away = (event: MouseEvent) => {
      if (event.target instanceof Node && wrap?.contains(event.target)) return;
      pinned = false;
      hovered = false;
    };
    const moved = () => {
      pinned = false;
      hovered = false;
    };
    window.addEventListener("mousedown", away, true);
    window.addEventListener("hashchange", moved);
    return () => {
      untrack(off);
      window.removeEventListener("mousedown", away, true);
      window.removeEventListener("hashchange", moved);
    };
  });

  // Kept inside the window: where the sign stands far to the right, the bubble moves left by what it would spill.
  $effect(() => {
    if (!open || bubble === null) return;
    const node = bubble;
    const fit = () => {
      node.style.translate = "";
      const box = node.getBoundingClientRect();
      const over = box.right - (window.innerWidth - EDGE);
      const room = box.left - EDGE;
      if (over > 0) node.style.translate = `${-Math.min(over, Math.max(0, room))}px 0`;
    };
    fit();
    window.addEventListener("resize", fit);
    return () => {
      window.removeEventListener("resize", fit);
      node.style.translate = "";
    };
  });

  $effect(() => () => {
    clearTimeout(hoverTimer);
    clearTimeout(copiedTimer);
  });
</script>

{#if checkout !== undefined}
  <span
    class="tf-clonesign"
    bind:this={wrap}
    role="group"
    onpointerenter={enter}
    onpointerleave={leave}
    onfocusin={focusIn}
    onfocusout={focusOut}
  >
    <button
      bind:this={face}
      type="button"
      class="tf-signbtn"
      class:tf-remote={path === null}
      aria-expanded={open}
      aria-controls="{uid}-bubble"
      aria-describedby="{uid}-path"
      onclick={press}
      >{#if path !== null}<svg viewBox="0 0 16 16" aria-hidden="true"><path d="M1.75 3.75h4.5l1.5 1.5h6.5v7h-12.5z" /></svg
        ><span class="tf-sign-word">Local</span>{:else}<svg viewBox="0 0 16 16" aria-hidden="true"
          ><path d="M4.75 12.25h6.5a2.75 2.75 0 0 0 .35-5.48 3.75 3.75 0 0 0-7.2.98 2.25 2.25 0 0 0 .35 4.5z" /></svg
        ><span class="tf-sign-word">Remote</span>{/if}{#if standing !== null}<span class="tf-sign-state"
          ><span class="tf-sr">{", "}</span>{standing}</span
        >{/if}</button
    >
    <span class="tf-signbubble" class:tf-sr={!open} id="{uid}-bubble" bind:this={bubble}>
      {#if path !== null && parts !== null}
        <span class="tf-signbubble-what">Cloned at</span>
        <span class="tf-sr" id="{uid}-path">{path}</span>
        {#if open}
          <!-- The path drawn, its folder in ink; the hidden run above is what is read, as one run. -->
          <span class="tf-signbubble-drawn tf-mono tf-wrapall" aria-hidden="true"
            >{#if parts.head !== ""}<span class="tf-dim">{parts.head}</span>{/if}<b>{parts.tail}</b></span
          >
          {#if sentence?.state}<span class="tf-signbubble-state" aria-hidden="true">{sentence.state}</span>{/if}
          <span class="tf-signbubble-acts">
            <button type="button" class="tf-btn tf-sm" onclick={() => void copyPath()}>{copied ? "Copied" : "Copy path"}</button>
            {#if opener !== null}<button type="button" class="tf-btn tf-sm tf-ghost" onclick={openIn}>Open in {opener.name}</button>{/if}
          </span>
        {/if}
      {:else}
        <span class="tf-signbubble-what" id="{uid}-path">Read from {source}: not cloned on this machine.</span>
      {/if}
    </span>
  </span>
{/if}
