<!--
  A sign in a page's line under its title, with a bubble (design system, *Controls*, the sign): a few words on one line
  that stand for something too long for the line, a path or a chain, whose whole text is in the bubble. The clone's
  sign (`TfCloneSign`, the clone-path pass, 2026-11-16) and a pipeline's chain (`TfChainSign`, the build-header pass,
  2026-11-20) are this.

  The bubble opens on hover and on focus, and pinned by a press. Closed, it is visually hidden, not gone: what it holds
  for a reader (`bubble`'s run with the id `<uid>-desc`) is the sign's description, which a screen reader and the
  desktop witness read. `Esc` closes it and leaves focus on the sign; so do a press elsewhere and a move to another
  address. It stands in the page, under the sign and kept inside the window, rather than in the shell's float host: its
  actions come right after the sign in the `Tab` order.
-->
<script lang="ts">
  import { untrack, type Snippet } from "svelte";

  import { layers as sharedLayers, type Layers } from "./layers.svelte";

  let {
    face,
    bubble,
    class: className = "",
    faceClass = "",
    layers = sharedLayers,
  }: {
    /** The sign's words, on one line. */
    face: Snippet;
    /** What the bubble holds; `open` says whether it shows, `close` shuts it, `uid` names its description `<uid>-desc`. */
    bubble: Snippet<[{ open: boolean; close: () => void; uid: string }]>;
    class?: string;
    faceClass?: string;
    layers?: Layers | undefined;
  } = $props();

  const uid = $props.id();
  /** How long a pointer rests on the sign before its bubble opens, and how long it may take to cross into the bubble. */
  const HOVER_OPEN_MS = 150;
  const HOVER_CLOSE_MS = 200;
  /** What the bubble leaves between itself and the window's edge. */
  const EDGE = 8;

  let hovered = $state(false);
  let focused = $state(false);
  let pinned = $state(false);
  /** `Esc` closed it while focus or the pointer is still on it; the next move in opens it again. */
  let dismissed = $state(false);
  let wrap = $state<HTMLElement | null>(null);
  let faceEl = $state<HTMLButtonElement | null>(null);
  let bubbleEl = $state<HTMLElement | null>(null);
  const open = $derived(!dismissed && (pinned || hovered || focused));

  let hoverTimer: ReturnType<typeof setTimeout> | undefined;

  function close() {
    pinned = false;
    dismissed = true;
    if (bubbleEl?.contains(document.activeElement)) faceEl?.focus({ preventScroll: true });
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
    if (!open || bubbleEl === null) return;
    const node = bubbleEl;
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

  $effect(() => () => clearTimeout(hoverTimer));
</script>

<span
  class="tf-sign {className}"
  bind:this={wrap}
  role="group"
  onpointerenter={enter}
  onpointerleave={leave}
  onfocusin={focusIn}
  onfocusout={focusOut}
>
  <button
    bind:this={faceEl}
    type="button"
    class="tf-signbtn {faceClass}"
    aria-expanded={open}
    aria-controls="{uid}-bubble"
    aria-describedby="{uid}-desc"
    onclick={press}>{@render face()}</button
  >
  <span class="tf-signbubble" class:tf-sr={!open} id="{uid}-bubble" bind:this={bubbleEl}>
    {@render bubble({ open, close, uid })}
  </span>
</span>
