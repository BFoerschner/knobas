/** What a menu's keys move between: an item that can be pressed, of any of the three roles. */
export const MENU_ITEMS = '[role^="menuitem"]:not([disabled]):not([aria-disabled="true"])';

/**
 * The keys of a `role="menu"` (design system, *Layers*): ↑ and ↓ move between its items, Home and End go to its ends,
 * and it closes when focus leaves it. A press never scrolls the page behind it. Attach it to the menu; `close` is the
 * menu's own close, which is told to keep focus where it went. Focus going to `opener` is no leaving: the press that
 * moved it there is about to answer, and closing here would have that press open the menu again.
 */
export function menuKeys(close: () => void, opener: () => HTMLElement | null = () => null) {
  return (menu: HTMLElement) => {
    const items = () => [...menu.querySelectorAll<HTMLElement>(MENU_ITEMS)];
    function onkey(event: KeyboardEvent) {
      const all = items();
      if (all.length === 0) return;
      const at = all.indexOf(document.activeElement as HTMLElement);
      let to: number | null = null;
      if (event.key === "ArrowDown") to = at === -1 ? 0 : (at + 1) % all.length;
      else if (event.key === "ArrowUp") to = at === -1 ? all.length - 1 : (at - 1 + all.length) % all.length;
      else if (event.key === "Home") to = 0;
      else if (event.key === "End") to = all.length - 1;
      if (to === null) return;
      event.preventDefault();
      event.stopPropagation();
      all[to]?.focus();
    }
    function onleave(event: FocusEvent) {
      const next = event.relatedTarget;
      // Focus going nowhere is a click on nothing, which the menu's own outside-click answers.
      if (next instanceof Node && !menu.contains(next) && !opener()?.contains(next)) close();
    }
    menu.addEventListener("keydown", onkey);
    menu.addEventListener("focusout", onleave);
    return () => {
      menu.removeEventListener("keydown", onkey);
      menu.removeEventListener("focusout", onleave);
    };
  };
}
