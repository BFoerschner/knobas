<!--
  The one `role="menu"` (design system, *Layers*; the gate refuses another). It draws in the host at the shell's root
  (`floats.ts`), placed from its opener: under it or over it, inside the window, scrolling when it is taller than the
  room. It follows its opener through a scroll and a resize.

  It is on the layer stack as a dialog is, so the page's keys wait and the shell's `Esc` reaches it. `↑`, `↓`, `Home`
  and `End` move (`menu-keys.ts`). `Esc` and `Tab` close it and focus its opener, and so does a pick; a click anywhere
  else closes it and leaves focus where the click put it, as does a move to another address.

  The opener is the caller's button, which sets `open` and carries `aria-haspopup` and `aria-expanded`.
-->
<script lang="ts">
  import { untrack, type Snippet } from "svelte";

  import { inFloatHost, placeFloat } from "./floats";
  import { layers as sharedLayers, type Layers } from "./layers.svelte";
  import { menuKeys, MENU_ITEMS } from "./menu-keys";

  let {
    open = $bindable(false),
    opener,
    label,
    className = "",
    align = "end",
    ready = true,
    focusOn = "",
    layers = sharedLayers,
    children,
  }: {
    open: boolean;
    opener: HTMLElement | null;
    /** The menu's accessible name. */
    label: string;
    /** A class beside `tf-menu`, for a menu of its own width and rows. */
    className?: string;
    /** The opener's edge the menu lines up with. */
    align?: "start" | "end";
    /** `false` while its items are being read: focus waits on the menu itself, so its keys still work. */
    ready?: boolean;
    /** The item that takes focus on opening, when one matches; else the first. */
    focusOn?: string;
    layers?: Layers | undefined;
    children: Snippet;
  } = $props();

  let menu: HTMLElement | null = null;

  /** Focus goes back before what a pick set off can take it: a dialog it opened keeps this button to return to. */
  let shown = false;
  $effect.pre(() => {
    if (open) {
      shown = true;
    } else if (shown) {
      shown = false;
      if (menu?.contains(document.activeElement)) untrack(() => opener)?.focus({ preventScroll: true });
    }
  });

  function dismiss() {
    opener?.focus({ preventScroll: true });
    open = false;
  }

  function place(node: HTMLElement) {
    return untrack(() => {
      menu = node;
      inFloatHost(node);
      const spot = () => {
        if (opener === null) return;
        const edges = node.offsetHeight - node.clientHeight;
        const at = placeFloat(
          opener.getBoundingClientRect(),
          { width: node.offsetWidth, height: node.scrollHeight + edges },
          { width: window.innerWidth, height: window.innerHeight },
          align,
        );
        node.style.left = `${at.left}px`;
        node.style.top = `${at.top}px`;
        node.style.maxHeight = `${at.maxHeight}px`;
      };
      spot();
      const scrolled = (event: Event) => {
        if (!(event.target instanceof Node && node.contains(event.target))) spot();
      };
      window.addEventListener("resize", spot);
      window.addEventListener("scroll", scrolled, true);
      // What it holds can change while it is open: *Open in…* on a code object reads its items first.
      const sized = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(spot);
      sized?.observe(node);
      const keys = menuKeys(
        () => (open = false),
        () => opener,
      )(node);
      const onkey = (event: KeyboardEvent) => {
        if (event.key !== "Escape" && event.key !== "Tab") return;
        event.preventDefault();
        event.stopPropagation();
        dismiss();
      };
      node.addEventListener("keydown", onkey);
      return () => {
        menu = null;
        window.removeEventListener("resize", spot);
        window.removeEventListener("scroll", scrolled, true);
        sized?.disconnect();
        keys();
        node.removeEventListener("keydown", onkey);
      };
    });
  }

  $effect(() => {
    if (!open) return;
    const off = untrack(() => layers.add({ kind: "dialog", close: dismiss }));
    const away = (event: MouseEvent) => {
      if (event.target instanceof Node && (menu?.contains(event.target) || opener?.contains(event.target))) return;
      open = false;
    };
    const moved = () => (open = false);
    window.addEventListener("mousedown", away, true);
    window.addEventListener("click", away, true);
    window.addEventListener("hashchange", moved);
    return () => {
      untrack(off);
      window.removeEventListener("mousedown", away, true);
      window.removeEventListener("click", away, true);
      window.removeEventListener("hashchange", moved);
    };
  });

  $effect(() => {
    if (!open) return;
    const items = ready;
    const asked = focusOn;
    queueMicrotask(() => {
      if (menu === null) return;
      const first = items ? ((asked === "" ? null : menu.querySelector<HTMLElement>(asked)) ?? menu.querySelector<HTMLElement>(MENU_ITEMS)) : null;
      (first ?? menu).focus();
    });
  });
</script>

{#if open}
  <!-- svelte-ignore a11y_interactive_supports_focus -->
  <div class="tf-menu {className}" role="menu" tabindex="-1" aria-label={label} {@attach place}>
    {@render children()}
  </div>
{/if}
