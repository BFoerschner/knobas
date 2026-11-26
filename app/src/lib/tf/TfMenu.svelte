<!--
  The one `role="menu"` (design system, *Layers*; the gate refuses another). It draws in the host at the shell's root
  (`floats.ts`), placed from its opener: under it or over it, inside the window, scrolling when it is taller than the
  room. It follows its opener through a scroll and a resize.

  It is on the layer stack as a dialog is, so the page's keys wait and the shell's `Esc` reaches it. `↑`, `↓`, `Home`
  and `End` move (`menu-keys.ts`). `Esc` and `Tab` close it and focus its opener, and so does a pick; a click anywhere
  else closes it and leaves focus where the click put it, as does a move to another address.

  The opener is the caller's button, which sets `open` and carries `aria-haspopup` and `aria-expanded`.

  A right-click's menu (`context/TfContextMenu.svelte`, 2026-11-16) stands at a point (`at`) instead: the pointer, or
  under the focused thing for a key. It goes where it fits beside the point, and a scroll, a press anywhere else or the
  window losing focus closes it, since what it was opened on may have moved. Its opener is what had focus, which focus
  goes back to. Its *Open in…* is a second `TfMenu` `beside` its item, and each `keep`s the other: a press or focus in
  one is not leaving the other.

  It owns its node to the end (the stuck right-click menu, ux27, 2026-11-21): drawn in the host, the node is outside the
  run of nodes Svelte removes, and when what holds the menu goes while it stands (the right-click menu's *Open in…* as
  its menu closes, a page's menu as the address moves), Svelte removes only that run and leaves the node to its own
  teardown, which takes it out of the host. One menu stands at a time: opening one closes every other, but its own
  submenu or the menu it is the submenu of.
-->
<script lang="ts" module>
  /** A menu that stands: its node, what it keeps, and how it closes. */
  interface Standing {
    node: HTMLElement;
    keep: () => Element | null;
    close: () => void;
  }
  /** Every menu standing in the window: one, or a menu and its submenu. */
  const standing = new Set<Standing>();

  /** Tests only: how many menus stand. */
  export function standingCount(): number {
    return standing.size;
  }

  /** Tests only: forget the menus a test left standing (`test-resets.ts`). */
  export function forgetStandingMenus(): void {
    standing.clear();
  }
</script>

<script lang="ts">
  import { untrack, type Snippet } from "svelte";

  import { inFloatHost, placeAt, placeBeside, placeFloat } from "./floats";
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
    at = null,
    beside = false,
    keep = () => null,
    focusItem = true,
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
    /** A point to stand at instead of under the opener: a right-click's menu. */
    at?: { x: number; y: number } | null;
    /** Beside the opener, its top at the opener's, as a submenu stands by its item. */
    beside?: boolean;
    /** Another float that counts as inside this one: a submenu, or its parent menu. */
    keep?: () => Element | null;
    /** `false`: the menu itself takes focus, not an item, as a pointer opening it asks; ↓ then goes to the first. */
    focusItem?: boolean;
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
      const unhost = inFloatHost(node);
      // One menu at a time: a menu opening closes every other but its own submenu or parent.
      const me: Standing = { node, keep, close: () => (open = false) };
      for (const other of [...standing]) if (other.node !== keep() && other.keep() !== node) other.close();
      standing.add(me);
      const point = at;
      const spot = () => {
        const edges = node.offsetHeight - node.clientHeight;
        const size = { width: node.offsetWidth, height: node.scrollHeight + edges };
        const view = { width: window.innerWidth, height: window.innerHeight };
        let spotted;
        if (point !== null) spotted = placeAt(point, size, view);
        else if (opener === null) return;
        else if (beside) spotted = placeBeside(opener.getBoundingClientRect(), size, view);
        else spotted = placeFloat(opener.getBoundingClientRect(), size, view, align);
        node.style.left = `${spotted.left}px`;
        node.style.top = `${spotted.top}px`;
        node.style.maxHeight = `${spotted.maxHeight}px`;
      };
      spot();
      const scrolled = (event: Event) => {
        if (event.target instanceof Node && (node.contains(event.target) || keep()?.contains(event.target))) return;
        // What a point was taken from has moved: the menu goes with it (a right-click's, and its submenu).
        if (point !== null || beside) open = false;
        else spot();
      };
      window.addEventListener("resize", spot);
      window.addEventListener("scroll", scrolled, true);
      // What it holds can change while it is open: *Open in…* on a code object reads its items first.
      const sized = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(spot);
      sized?.observe(node);
      const keys = menuKeys(
        () => (open = false),
        () => opener,
        keep,
      )(node);
      const onkey = (event: KeyboardEvent) => {
        // ← closes a submenu back to its item, as Esc does.
        if (beside && event.key === "ArrowLeft") {
          event.preventDefault();
          event.stopPropagation();
          dismiss();
          return;
        }
        if (event.key !== "Escape" && event.key !== "Tab") return;
        event.preventDefault();
        event.stopPropagation();
        dismiss();
      };
      node.addEventListener("keydown", onkey);
      return () => {
        menu = null;
        standing.delete(me);
        // Out of the host, also when what holds the menu goes while it stands and Svelte leaves the node here.
        unhost();
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
    // A right-click's opener is what had focus, not something pressed: a press on it is a press elsewhere.
    const pointed = untrack(() => at !== null);
    const away = (event: MouseEvent) => {
      const target = event.target;
      if (target instanceof Node && (menu?.contains(target) || keep()?.contains(target) || (!pointed && opener?.contains(target)))) return;
      open = false;
    };
    const moved = () => (open = false);
    window.addEventListener("mousedown", away, true);
    window.addEventListener("click", away, true);
    window.addEventListener("hashchange", moved);
    if (pointed) window.addEventListener("blur", moved);
    return () => {
      untrack(off);
      window.removeEventListener("mousedown", away, true);
      window.removeEventListener("click", away, true);
      window.removeEventListener("hashchange", moved);
      window.removeEventListener("blur", moved);
    };
  });

  $effect(() => {
    if (!open) return;
    const items = ready && focusItem;
    const asked = focusOn;
    queueMicrotask(() => {
      if (menu === null) return;
      const first = items ? ((asked === "" ? null : menu.querySelector<HTMLElement>(asked)) ?? menu.querySelector<HTMLElement>(MENU_ITEMS)) : null;
      (first ?? menu).focus({ preventScroll: true });
    });
  });
</script>

{#if open}
  <!-- svelte-ignore a11y_interactive_supports_focus -->
  <div class="tf-menu {className}" role="menu" tabindex="-1" aria-label={label} {@attach place}>
    {@render children()}
  </div>
{/if}
