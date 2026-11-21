/**
 * **A drag by pointer events** (the state-colours bug, 2026-11-17; Fable's ruling): what a chip on Settings › Appearance ›
 * State colours and a renderer pipeline's step move by, in place of HTML5 drag. On Windows Tauri lays its own drop target
 * over WebView2's (`dragDropEnabled`, on by default: wry `RevokeDragDrop`s WebView2's and registers its own), so a page's
 * HTML5 `drop` never fires there, and neither board could be dragged on Björn's PC. Pointer events are the page's own on
 * every platform, and headless Chrome drives them as a person does.
 *
 * A press on the handle starts nothing until the pointer travels more than a hand's tremble ({@link TREMBLE}, as
 * `press-guard.ts` measures one); then the pointer is captured, the dragged element follows it (`translate`, class
 * `tf-lifted`), and the target under it is told to the caller (`over`) for its highlight. Release over a target calls
 * `drop`; anywhere else, `Esc`, a cancelled pointer or lost capture put it back. The click that ends a drag is swallowed,
 * so a handle that also opens something on a click (a chip's menu) does not.
 */
import type { Attachment } from "svelte/attachments";

/** How far a press travels before it is a drag, in px: a double-click's drift is not one. */
export const TREMBLE = 4;

export type PointerDrag<T> = {
  /** The element that follows the pointer; the attached element when omitted. */
  lifted?: (handle: HTMLElement) => HTMLElement;
  /** What a drop on `at`, the topmost element under the pointer that is not the dragged one, lands on; `null` for nothing. */
  target: (at: Element) => T | null;
  /** The target under the pointer changed (`null`: none); the caller draws its highlight. */
  over?: (target: T | null) => void;
  /** Released over `target`. */
  drop: (target: T) => void;
  /** A press here is no drag: a field, or another button inside the handle. */
  ignore?: (at: Element) => boolean;
};

/** The topmost element at a point that is not inside `dragged`, which stands under the pointer while it follows it. */
function under(x: number, y: number, dragged: HTMLElement): Element | null {
  const all = typeof document.elementsFromPoint === "function" ? document.elementsFromPoint(x, y) : [];
  return all.find((el) => !dragged.contains(el)) ?? null;
}

/** Drag the attached element (its handle) by pointer, as the module says. */
export function pointerDrag<T>(drag: PointerDrag<T>): Attachment<HTMLElement> {
  return (handle) => {
    let start: { x: number; y: number; id: number } | null = null;
    let lifted: HTMLElement | null = null;
    let at: T | null = null;

    const point = (event: PointerEvent) => {
      const hit = lifted === null ? null : under(event.clientX, event.clientY, lifted);
      const next = hit === null ? null : drag.target(hit);
      if (next !== at) {
        at = next;
        drag.over?.(at);
      }
    };

    const end = (land: boolean) => {
      const was = lifted;
      const target = at;
      if (start !== null && handle.hasPointerCapture?.(start.id)) handle.releasePointerCapture(start.id);
      start = null;
      lifted = null;
      at = null;
      window.removeEventListener("keydown", escape, true);
      window.removeEventListener("pointermove", move, true);
      window.removeEventListener("pointerup", up, true);
      window.removeEventListener("pointercancel", cancel, true);
      if (was === null) return;
      was.classList.remove("tf-lifted");
      was.style.transform = "";
      drag.over?.(null);
      // The click a release makes on the handle is the drag's end, not a press.
      const swallow = (event: MouseEvent) => {
        event.preventDefault();
        event.stopPropagation();
      };
      window.addEventListener("click", swallow, { capture: true, once: true });
      setTimeout(() => window.removeEventListener("click", swallow, { capture: true }));
      if (land && target !== null) drag.drop(target);
    };

    const escape = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      event.preventDefault();
      event.stopPropagation();
      end(false);
    };

    const down = (event: PointerEvent) => {
      if (event.button !== 0 || !event.isPrimary || !(event.target instanceof Element)) return;
      if (drag.ignore?.(event.target)) return;
      start = { x: event.clientX, y: event.clientY, id: event.pointerId };
      // On the window, not the handle: a real pointer leaves a small chip on its first move, before any capture.
      window.addEventListener("pointermove", move, true);
      window.addEventListener("pointerup", up, true);
      window.addEventListener("pointercancel", cancel, true);
    };

    const move = (event: PointerEvent) => {
      if (start === null || event.pointerId !== start.id) return;
      const dx = event.clientX - start.x;
      const dy = event.clientY - start.y;
      if (lifted === null) {
        if (Math.hypot(dx, dy) <= TREMBLE) return;
        lifted = drag.lifted?.(handle) ?? handle;
        lifted.classList.add("tf-lifted");
        handle.setPointerCapture?.(start.id);
        window.addEventListener("keydown", escape, true);
      }
      event.preventDefault();
      lifted.style.transform = `translate(${dx}px, ${dy}px)`;
      point(event);
    };

    const up = (event: PointerEvent) => {
      if (start === null || event.pointerId !== start.id) return;
      if (lifted !== null) point(event);
      end(true);
    };
    const cancel = () => end(false);

    handle.addEventListener("pointerdown", down);
    handle.addEventListener("lostpointercapture", cancel);
    return () => {
      end(false);
      handle.removeEventListener("pointerdown", down);
      handle.removeEventListener("lostpointercapture", cancel);
    };
  };
}
