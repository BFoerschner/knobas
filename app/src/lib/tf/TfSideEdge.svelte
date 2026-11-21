<!--
  @component
  **A sidebar's edge** (`side-width.svelte.ts`): one separator on the line
  between a sidebar and the page's main column. It is a direct child of the
  grid that holds both, placed out of flow on the column line (`side-edge.css`),
  so it neither takes a cell nor scrolls with either column.

  - **Drag** sets the width. Pointer moves are coalesced into one write a
    frame of the grid's `--side-<id>`, and nothing Svelte draws re-renders
    until the pointer lets go, when the width is kept. While the pointer is
    down a cover over the window holds the resize cursor, so nothing under it
    restyles on hover and no text is selected. Where the page cannot lay
    itself out at a frame's pace (`side-pace.ts`), the drag becomes a ghost:
    only the edge's line follows the pointer, and the width lands when the
    pointer rests and when it lets go.
  - **Double click** returns the sidebar to its default (`onfit` instead, where
    a sidebar has a better width to offer: the ticket card's fits its words).
  - **Keys**: ←/→ step 16 px, 64 with Shift, Home and End go to the floor and
    the cap, Enter does what the double click does.
-->
<script lang="ts">
  import {
    clampWidth,
    cssWidth,
    maxWidth,
    SIDE_STEP,
    SIDE_STEP_BIG,
    sideProperty,
    sideWidths,
    specOf,
    type SideId,
  } from "./side-width.svelte";
  import { DragPace, REST_MS, REST_PX, startsAsGhost } from "./side-pace";

  let {
    side,
    onfit,
  }: {
    side: SideId;
    /** The width a double click or Enter sets, given the grid, in place of the default; `null` sets nothing. */
    onfit?: ((grid: HTMLElement) => number | null) | undefined;
  } = $props();

  const spec = $derived(specOf(side));
  /** +1 where moving right widens the sidebar (a left rail), -1 for a right column. */
  const sign = $derived(spec.at === "start" ? 1 : -1);

  let edge: HTMLDivElement | undefined = $state();
  /** The drawn width and the grid's, as last measured: a focus or a drag's start. */
  let drawn: number | null = $state(null);
  let gridWidth = $state(0);
  let dragging = $state(false);
  /** Below 900 px every sidebar stacks or folds behind its bar, and there is no edge to hold. */
  let narrow = $state(false);
  $effect(() => {
    const query = typeof window.matchMedia === "function" ? window.matchMedia("(max-width: 900px)") : null;
    if (query === null) return;
    narrow = query.matches;
    // A stand-in query with no events (a test's) answers once.
    if (typeof query.addEventListener !== "function") return;
    const changed = (event: MediaQueryListEvent) => (narrow = event.matches);
    query.addEventListener("change", changed);
    return () => query.removeEventListener("change", changed);
  });

  const now = $derived(sideWidths.px(side) ?? drawn ?? spec.base);
  const cap = $derived(Math.round(maxWidth(side, gridWidth > 0 ? gridWidth : window.innerWidth)));

  /** The sidebar's drawn width and the grid's content width: two reads of one layout. */
  function measure(grid: HTMLElement): { width: number; grid: number } {
    const style = getComputedStyle(grid);
    const tracks = style.gridTemplateColumns.split(" ").filter((track) => track.endsWith("px"));
    const track = Number.parseFloat((spec.at === "start" ? tracks[0] : tracks[tracks.length - 1]) ?? "");
    const box = grid.getBoundingClientRect().width;
    const inner = box - (Number.parseFloat(style.paddingLeft) || 0) - (Number.parseFloat(style.paddingRight) || 0);
    return { width: Number.isFinite(track) && track > 0 ? track : (sideWidths.px(side) ?? spec.base), grid: inner > 0 ? inner : 0 };
  }

  function gridOf(): HTMLElement | null {
    return edge?.parentElement ?? null;
  }

  /** A drag in progress: what lets go of it. */
  let release: (() => void) | null = null;

  function startDrag(event: PointerEvent) {
    if (event.button !== 0 || edge === undefined) return;
    const grid = gridOf();
    if (grid === null) return;
    event.preventDefault();
    release?.();
    edge.focus({ preventScroll: true });
    const at = measure(grid);
    const from = at.width;
    const x0 = event.clientX;
    const property = sideProperty(side);
    const target = edge;
    const id = side;
    const pace = new DragPace(startsAsGhost(id), id);
    let x = x0;
    /** The width the grid draws, and where the pointer would put it. */
    let width = from;
    let moved = false;
    /** In a ghost: where the pointer last moved more than `REST_PX`, and when. */
    let restX = x0;
    let restAt = 0;
    const aim = () => clampWidth(id, from + sign * (x - x0), at.grid);
    /** Write `next` to the grid, the ghost's line back on the edge in the same frame. */
    const write = (next: number) => {
      target.style.removeProperty("transform");
      if (next === width) return false;
      moved = true;
      width = next;
      grid.style.setProperty(property, cssWidth(id, next));
      target.setAttribute("aria-valuenow", String(next));
      return true;
    };
    // Every frame while the pointer is down, so a frame that wrote a width is costed by the one after it (`DragPace`).
    let frame = 0;
    const tick = (ts: number) => {
      pace.frame(ts);
      const next = aim();
      if (!pace.ghost) {
        if (write(next)) pace.wrote(ts);
      } else {
        if (Math.abs(x - restX) > REST_PX) {
          restX = x;
          restAt = ts;
        }
        // A landing at rest: costed as a written frame is (`DragPace`), what marks a sidebar slow or clears it.
        if (ts - restAt >= REST_MS && next !== width) {
          if (write(next)) pace.wrote(ts, true);
        }
        else if (next !== width) {
          // Only the line follows the pointer, where the width would clamp.
          target.style.transform = `translateX(${sign * (next - width)}px)`;
          target.setAttribute("aria-valuenow", String(next));
        } else target.style.removeProperty("transform");
      }
      frame = requestAnimationFrame(tick);
    };
    frame = requestAnimationFrame(tick);
    const move = (to: PointerEvent) => {
      x = to.clientX;
    };
    const cover = document.createElement("div");
    cover.className = "tf-sdrag-cover";
    document.body.append(cover);
    const up = (to: PointerEvent) => {
      x = to.clientX;
      finish(true);
    };
    const cancel = () => finish(false);
    const finish = (keep: boolean) => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      window.removeEventListener("pointercancel", cancel);
      window.removeEventListener("blur", cancel);
      if (frame !== 0) cancelAnimationFrame(frame);
      frame = 0;
      cover.remove();
      dragging = false;
      release = null;
      target.style.removeProperty("transform");
      if (keep) {
        const before = width;
        write(aim());
        // The frame that lays the page out at the new width, costed as a drag's frame is: a landing.
        if (width !== before) requestAnimationFrame((t1) => requestAnimationFrame((t2) => pace.released(t2 - t1)));
      }
      if (moved) {
        if (keep) sideWidths.set(id, width, at.grid);
        else {
          // Put back what was drawn before the drag.
          const kept = sideWidths.px(id);
          if (kept === null) grid.style.removeProperty(property);
          else grid.style.setProperty(property, cssWidth(id, kept));
          target.setAttribute("aria-valuenow", String(kept ?? from));
        }
      }
      drawn = keep && moved ? width : from;
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
    window.addEventListener("pointercancel", cancel);
    window.addEventListener("blur", cancel);
    gridWidth = at.grid;
    drawn = from;
    dragging = true;
    release = () => finish(true);
  }
  $effect(() => () => release?.());

  /** The double click and Enter: the sidebar's own fit, or its default. */
  function fit() {
    const grid = gridOf();
    if (grid === null) return;
    const width = onfit?.(grid);
    if (width === undefined) {
      sideWidths.reset(side);
      drawn = null;
    } else if (width !== null) {
      sideWidths.set(side, width, measure(grid).grid);
    }
  }

  function step(to: number) {
    const grid = gridOf();
    if (grid === null) return;
    const at = measure(grid);
    gridWidth = at.grid;
    sideWidths.set(side, to, at.grid);
  }

  function keys(event: KeyboardEvent) {
    const by = event.shiftKey ? SIDE_STEP_BIG : SIDE_STEP;
    const current = () => sideWidths.px(side) ?? (gridOf() === null ? spec.base : measure(gridOf()!).width);
    if (event.key === "ArrowRight") step(current() + sign * by);
    else if (event.key === "ArrowLeft") step(current() - sign * by);
    else if (event.key === "Home") step(spec.min);
    else if (event.key === "End") step(Number.POSITIVE_INFINITY);
    else if (event.key === "Enter") fit();
    else return;
    event.preventDefault();
  }

  /** On focus the separator's numbers come from the drawn layout, which the page's CSS decides until a width is kept. */
  function focused() {
    const grid = gridOf();
    if (grid === null) return;
    const at = measure(grid);
    drawn = at.width;
    gridWidth = at.grid;
  }
</script>

{#if !narrow}
<!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
<div
  bind:this={edge}
  class="tf-sedge"
  class:tf-sedge-on={dragging}
  data-side={side}
  data-at={spec.at}
  role="separator"
  tabindex="0"
  aria-orientation="vertical"
  aria-label={spec.label}
  aria-valuemin={spec.min}
  aria-valuemax={cap}
  aria-valuenow={now}
  title={onfit ? "Drag to resize, double-click to fit" : "Drag to resize, double-click for the default width"}
  onpointerdown={startDrag}
  ondblclick={fit}
  onkeydown={keys}
  onfocus={focused}
></div>
{/if}
