/**
 * Drag and drop on the ticket board (ticket-edit decisions, sections 7 to 10 and 14; ADR-0054). Björn: "it also shows,
 * after picking up the ticket, where you only can drag the ticket". One controller owns the whole gesture, pointer and
 * keys, attached once to the board's wrap by delegation: no listener a card.
 *
 * - **Press**: a primary `mouse` or `pen` button down on a card asks where it can go (`press`). Nothing is drawn. A
 *   finger on the board scrolls it: `touch` starts no drag.
 * - **Pickup**: the pointer travels more than {@link TREMBLE} before the release. In one handler the pointer is captured
 *   on the wrap, the card's own box is emptied (`tf-held`, its room kept), a copy of it goes to the float host
 *   (`.tf-tghost`), the wrap takes `data-held` (which no style reads), every column `data-drop` (`yes`, `ask`, `no`;
 *   none on its own) from `drop.ts`, and the columns' boxes are read once. The hand is a cover over the window
 *   (`.tf-tcover`, the pointer's only), so no style of the board changes for it. Reach is asked again (`pickup`); when it lands only `data-drop`
 *   changes, never a box.
 * - **Move**: the copy follows by `transform`. The column under the pointer is found by arithmetic on the boxes read at
 *   pickup, never by `elementsFromPoint`; its cards' boxes are read when the pointer first enters it. On a column that
 *   takes it the landing line (`.tf-tline`) stands in the gap the pointer is nearest; on one that does not, the reason
 *   (`.tf-twhy`) stands under the copy. Within 56 px of the scroller's edge the board scrolls, 2 to 18 px a frame.
 * - **Drop**: on a column of one reachable status the move goes; of several, a menu at the drop point asks which while
 *   the copy waits at the landing place; anywhere else the card is put back. The click the release makes is swallowed.
 * - **Put back**: `Esc`, a cancelled pointer, lost capture, the window losing focus, another address. Nothing was
 *   written and nothing moved.
 * - **The carry** (`MOVE_KEY`, `s`): the same pickup by key. `←`/`→` (`h`/`l`) step through the columns of the card's lane that are
 *   not `no`, its own included; `Enter` or `Space` drops at the column's top; `Esc` puts it back; every other key is
 *   swallowed. The live region says each step.
 *
 * Nothing moves during a drag: the cards do not part, no column is un-windowed, the copy, the line and the reason are
 * in the float host and move by `transform`. What stands in the host is the controller's to the end, also when the page
 * goes while a card is held.
 */
import type { Attachment } from "svelte/attachments";

import type { MiniBoardCard } from "../../ipc/entity";
import { TREMBLE } from "../pointer-drag";
import { dropOn, either, type Drop, type DropColumn, type DropFacts } from "./drop";
import { reachOf, type Reach } from "./reach.svelte";

/** What the board tells the controller. */
export interface CardDragHost {
  /** The drawn card for an entity id, and the column (place) and lane it stands in. */
  cardOf(entityId: string): { card: MiniBoardCard; place: string; lane: string } | null;
  /** Every drawn column, in reading order: its place, lane, what it holds, and its `.tf-col` element. */
  columns(): { place: string; lane: string; column: Omit<DropColumn, "own" | "sameLane">; node: HTMLElement }[];
  facts(card: MiniBoardCard): DropFacts;
  /**
   * The card was dropped: move it to `status`, standing before the drawn card `before` (or at the column's end), in the
   * lane `lane` (another lane edits the field it is laned by, section 26). Answers what the live region says, or nothing
   * for *moved to ⟨status⟩*.
   */
  drop(card: MiniBoardCard, to: { place: string; before: string | null; status: string; lane: string }): string | void;
  /** Several statuses to choose from, at the drop point: answers the pick, or `null` for none. */
  choose(card: MiniBoardCard, statuses: string[], at: { x: number; y: number }): Promise<string | null>;
  /** Ask where a card can go: the pointer rested on it, it was pressed, or picked up. */
  ask(card: MiniBoardCard, when: "rest" | "press" | "pickup"): Promise<Reach | null>;
  /** The board's scroller: `.tf-cols` with one lane, the wrap with lanes. */
  scroller(): HTMLElement;
  /** The page's polite live region. */
  say(text: string): void;
}

/** How near a scroller's edge the board scrolls, and how fast at most, a frame. */
const EDGE = 56;
const STEP_MOST = 18;
const STEP_LEAST = 2;
/** How long a pointer rests on a card before it is asked about, in ms. */
export const REST_MS = 120;
/**
 * How long past the frame after a drop a read the drag held back waits, in ms: the drop's own frames (the card in its
 * column with its chip, then its focus) come first, and the read's answer lands after them.
 */
export const AFTER_DROP_MS = 200;

interface Column {
  place: string;
  lane: string;
  column: DropColumn;
  node: HTMLElement;
  box: DOMRect;
  /** Its cards' boxes and ids, read when the pointer first enters it. */
  cards: { id: string; box: DOMRect }[] | null;
  drop: Drop;
}

interface Held {
  card: MiniBoardCard;
  place: string;
  lane: string;
  node: HTMLElement | null;
  ghost: HTMLElement;
  line: HTMLElement;
  why: HTMLElement;
  /** The pointer's hand over the window (`.tf-tcover`), `null` for the carry and once a status menu stands. */
  cover: HTMLElement | null;
  columns: Column[];
  over: Column | null;
  /** The drawn card the landing line stands above, or `null` for the column's end. */
  before: string | null;
  keys: boolean;
  pointer: number | null;
  grab: { x: number; y: number };
  height: number;
  x: number;
  y: number;
  /** A status menu stands: the copy waits, keys and pointer are the menu's. */
  choosing: boolean;
  /** The board's scroller and its box, read with the columns' (at pickup and after a scroll), never on a move. */
  scroller: HTMLElement;
  scrollerBox: DOMRect;
  /** What the line and the reason show now, so a move that changes neither writes nothing to them. */
  drawn: { why: string | null; lineWidth: number | null; lineShown: boolean };
}

export interface CardDrag {
  /** On the board's wrap. */
  attach: Attachment<HTMLElement>;
  /** The move key on a card: pick it up by key. Answers whether it was picked up. */
  carry(entityId: string): boolean;
  /** Whether a card is held. */
  holding(): boolean;
  /** Put back whatever is held: the page leaves, the layout changes. */
  putBack(): void;
  /**
   * Run `read` (the board read again: a move's echo, a run's end) now when no card is held; while one is, once it is
   * dropped or put back, {@link AFTER_DROP_MS} past the frame after, once however often it was asked meanwhile. A board
   * read under a held card draws its columns again under the copy, and its answer is a long task of its own.
   */
  afterDrag(read: () => void): void;
}

const statusesOf = (drop: Drop) => (drop.kind === "yes" || drop.kind === "ask" ? drop.statuses : []);
const takes = (drop: Drop) => drop.kind === "yes" || drop.kind === "ask";

export function createCardDrag(host: CardDragHost): CardDrag {
  let wrap: HTMLElement | null = null;
  let held: Held | null = null;
  let press: { id: string; x: number; y: number; pointer: number } | null = null;
  let rest: ReturnType<typeof setTimeout> | undefined;
  let scrolling: number | null = null;
  let remeasure: number | null = null;
  /** The reads a held card holds back, and the wait before they run once it is let go. */
  const heldBack = new Set<() => void>();
  let releasing: { frame: number | null; timer: ReturnType<typeof setTimeout> | null } | null = null;

  function releaseHeldBack() {
    if (heldBack.size === 0 || releasing !== null) return;
    const wait: { frame: number | null; timer: ReturnType<typeof setTimeout> | null } = { frame: null, timer: null };
    releasing = wait;
    wait.frame = requestAnimationFrame(() => {
      wait.frame = null;
      wait.timer = setTimeout(() => {
        releasing = null;
        // Picked up again meanwhile: they wait for that one's end.
        if (held !== null) return;
        const reads = [...heldBack];
        heldBack.clear();
        for (const read of reads) read();
      }, AFTER_DROP_MS);
    });
  }

  function dropHeldBack() {
    heldBack.clear();
    if (releasing?.frame != null) cancelAnimationFrame(releasing.frame);
    if (releasing?.timer != null) clearTimeout(releasing.timer);
    releasing = null;
  }

  const floatHost = () => document.querySelector<HTMLElement>(".tf-floats") ?? document.body;

  /** Each column's standing for the held card, from what the window holds now: `data-drop` and the head's statuses. */
  function light() {
    if (held === null) return;
    const reach = reachOf(held.card);
    const facts = host.facts(held.card);
    for (const one of held.columns) {
      const column = { ...one.column, own: one.place === held.place, sameLane: one.lane === held.lane, lane: one.lane };
      one.column = column;
      one.drop = dropOn(held.card, column, reach, facts);
      // Written only where it changes: the answer landing mid-drag mostly says what the window already held.
      const kind = one.drop.kind === "own" ? undefined : one.drop.kind;
      if (one.node.dataset.drop !== kind) {
        if (kind === undefined) delete one.node.dataset.drop;
        else one.node.dataset.drop = kind;
      }
      const heads = one.node.querySelector<HTMLElement>(".tf-colsts");
      const words = one.drop.kind === "yes" && one.drop.statuses.length > 1 ? either(one.drop.statuses) : "";
      if (heads !== null && heads.textContent !== words) heads.textContent = words;
    }
  }

  /** The columns' boxes, read once at pickup and again after a scroll (at most once a frame). */
  function measure() {
    if (held === null) return;
    held.scrollerBox = held.scroller.getBoundingClientRect();
    for (const one of held.columns) {
      one.box = (one.node.querySelector(".tf-stack") ?? one.node).getBoundingClientRect();
      one.cards = null;
    }
  }

  function cardsOf(one: Column): { id: string; box: DOMRect }[] {
    one.cards ??= [...one.node.querySelectorAll<HTMLElement>(".tf-tcard[data-entity]")]
      .filter((node) => node.dataset.entity !== held?.card.entity_id)
      .map((node) => ({ id: node.dataset.entity!, box: node.getBoundingClientRect() }));
    return one.cards;
  }

  function pickup(entityId: string, at: { x: number; y: number } | null, keys: boolean, pointer: number | null): boolean {
    const found = host.cardOf(entityId);
    if (found === null || wrap === null) return false;
    const node = wrap.querySelector<HTMLElement>(`.tf-tcard[data-entity="${CSS.escape(entityId)}"]`);
    if (node === null) return false;
    const box = node.getBoundingClientRect();
    const ghost = node.cloneNode(true) as HTMLElement;
    ghost.classList.add("tf-tghost");
    ghost.classList.remove("tf-sel", "tf-held");
    ghost.removeAttribute("data-entity");
    ghost.removeAttribute("aria-current");
    ghost.setAttribute("aria-hidden", "true");
    ghost.tabIndex = -1;
    ghost.style.width = `${box.width}px`;
    const line = document.createElement("div");
    line.className = "tf-tline";
    line.hidden = true;
    const why = document.createElement("div");
    why.className = "tf-twhy";
    why.hidden = true;
    const scroller = host.scroller();
    // The hand over the whole window, under the copy, for the pointer: the board's own styles stay as they are. The wheel
    // still scrolls the board under it.
    let cover: HTMLElement | null = null;
    if (!keys) {
      cover = document.createElement("div");
      cover.className = "tf-tcover";
      cover.addEventListener("wheel", (event) => scroller.scrollBy(event.deltaX, event.deltaY), { passive: true });
    }
    floatHost().append(...(cover === null ? [] : [cover]), line, ghost, why);
    const columns = host.columns().map(
      (one): Column => ({
        ...one,
        column: { ...one.column, own: one.place === found.place, sameLane: one.lane === found.lane, lane: one.lane },
        box: (one.node.querySelector(".tf-stack") ?? one.node).getBoundingClientRect(),
        cards: null,
        drop: { kind: "own" },
      }),
    );
    held = {
      card: found.card,
      place: found.place,
      lane: found.lane,
      node,
      ghost,
      line,
      why,
      cover,
      columns,
      over: null,
      before: null,
      keys,
      pointer,
      grab: at === null ? { x: 24, y: 12 } : { x: at.x - box.left, y: at.y - box.top },
      height: box.height,
      x: at?.x ?? box.left + 24,
      y: at?.y ?? box.top + 12,
      choosing: false,
      scroller,
      scrollerBox: scroller.getBoundingClientRect(),
      drawn: { why: null, lineWidth: null, lineShown: false },
    };
    node.classList.add("tf-held");
    wrap.dataset.held = "";
    light();
    const card = found.card;
    void host.ask(card, "pickup").then(
      () => {
        if (held?.card !== card) return;
        light();
        if (held.over !== null && !held.keys) hover(held.over, held.y);
        else if (held.over !== null) hover(held.over, held.over.box.top - 1);
      },
      () => {},
    );
    window.addEventListener("keydown", onkey, true);
    window.addEventListener("blur", onblur);
    window.addEventListener("hashchange", onblur);
    if (!keys) place(held.x, held.y);
    return true;
  }

  function place(x: number, y: number) {
    if (held === null) return;
    held.x = x;
    held.y = y;
    held.ghost.style.transform = `translate3d(${x - held.grab.x}px, ${y - held.grab.y}px, 0)`;
    let hit: Column | null = null;
    for (const one of held.columns) {
      const { box } = one;
      if (x < box.left - 4 || x > box.right + 4 || y < box.top - 40 || y > Math.max(box.bottom, box.top + 200) + 40) continue;
      hit = one;
      break;
    }
    hover(hit, y);
    edgeScroll(x, y);
  }

  function hover(hit: Column | null, y: number) {
    if (held === null) return;
    if (held.over !== hit) {
      if (held.over !== null) delete held.over.node.dataset.over;
      held.over = hit;
      if (hit !== null) hit.node.dataset.over = "";
      held.cover?.classList.toggle("tf-no", hit?.drop.kind === "no");
    }
    const drop = hit?.drop ?? null;
    const refused = drop?.kind === "no" ? drop.why : null;
    const { drawn } = held;
    if (refused !== drawn.why) {
      held.why.hidden = refused === null;
      if (refused !== null) held.why.textContent = refused;
      drawn.why = refused;
    }
    if (refused !== null) held.why.style.transform = `translate3d(${held.x - held.grab.x}px, ${held.y - held.grab.y + held.height + 8}px, 0)`;
    const lands = hit !== null && drop !== null && takes(drop);
    const lineShown = lands && !held.keys;
    if (lineShown !== drawn.lineShown) {
      held.line.hidden = !lineShown;
      drawn.lineShown = lineShown;
    }
    if (!lands || hit === null) {
      held.before = null;
      return;
    }
    const cards = cardsOf(hit);
    let at = held.keys ? 0 : cards.findIndex(({ box }) => y < box.top + box.height / 2);
    if (at < 0) at = cards.length;
    held.before = cards[at]?.id ?? null;
    if (held.keys) return;
    const top = at === 0 ? hit.box.top + 1 : at === cards.length ? cards.at(-1)!.box.bottom + 2 : (cards[at - 1]!.box.bottom + cards[at]!.box.top) / 2 - 1;
    const width = Math.max(0, hit.box.width - 10);
    if (width !== drawn.lineWidth) {
      held.line.style.width = `${width}px`;
      drawn.lineWidth = width;
    }
    held.line.style.transform = `translate3d(${hit.box.left + 5}px, ${top}px, 0)`;
  }

  /** Within {@link EDGE} of the scroller's edge, scroll toward it while the pointer stays there. */
  function edgeScroll(x: number, y: number) {
    if (held === null) return;
    const { scroller, scrollerBox: box } = held;
    const depth = (near: number) => (near >= EDGE ? 0 : STEP_LEAST + ((EDGE - Math.max(0, near)) / EDGE) * (STEP_MOST - STEP_LEAST));
    const dx = x < box.left + EDGE ? -depth(x - box.left) : x > box.right - EDGE ? depth(box.right - x) : 0;
    const dy = y < box.top + EDGE ? -depth(y - box.top) : y > box.bottom - EDGE ? depth(box.bottom - y) : 0;
    if (dx === 0 && dy === 0) {
      if (scrolling !== null) cancelAnimationFrame(scrolling);
      scrolling = null;
      return;
    }
    if (scrolling !== null) return;
    const frame = () => {
      scrolling = null;
      if (held === null || held.keys || held.choosing) return;
      scroller.scrollBy(Math.round(dx), Math.round(dy));
      scrolling = requestAnimationFrame(frame);
    };
    scrolling = requestAnimationFrame(frame);
  }

  function onscroll() {
    if (held === null || remeasure !== null) return;
    remeasure = requestAnimationFrame(() => {
      remeasure = null;
      if (held === null || held.choosing) return;
      measure();
      if (!held.keys) place(held.x, held.y);
    });
  }

  /** Take everything the held card put on the board and in the float host away. Answers what was held. */
  function letGo(): Held | null {
    const was = held;
    if (was === null) return null;
    held = null;
    window.removeEventListener("keydown", onkey, true);
    window.removeEventListener("blur", onblur);
    window.removeEventListener("hashchange", onblur);
    if (scrolling !== null) cancelAnimationFrame(scrolling);
    if (remeasure !== null) cancelAnimationFrame(remeasure);
    scrolling = remeasure = null;
    for (const one of was.columns) {
      delete one.node.dataset.drop;
      delete one.node.dataset.over;
      const heads = one.node.querySelector<HTMLElement>(".tf-colsts");
      if (heads !== null) heads.textContent = "";
    }
    if (wrap !== null) delete wrap.dataset.held;
    // The card's own box may have left the DOM with its column's window: the one drawn now is cleared.
    wrap?.querySelector(`.tf-tcard[data-entity="${CSS.escape(was.card.entity_id)}"]`)?.classList.remove("tf-held");
    was.node?.classList.remove("tf-held");
    was.ghost.remove();
    was.line.remove();
    was.why.remove();
    was.cover?.remove();
    if (was.pointer !== null && wrap?.hasPointerCapture?.(was.pointer)) wrap.releasePointerCapture(was.pointer);
    releaseHeldBack();
    return was;
  }

  function putBackSaying(why: string | null) {
    const was = letGo();
    if (was === null) return;
    focusCard(was.card.entity_id);
    host.say(why ?? `${was.card.key} put back.`);
  }

  function focusCard(entityId: string) {
    wrap?.querySelector<HTMLElement>(`.tf-tcard[data-entity="${CSS.escape(entityId)}"]`)?.focus({ preventScroll: true });
  }

  /** The release, by pointer or key: the column under it decides. */
  function release() {
    if (held === null) return;
    const hit = held.over;
    const drop = hit?.drop ?? null;
    if (hit === null || drop === null || !takes(drop)) {
      putBackSaying(drop?.kind === "no" ? drop.why : null);
      return;
    }
    const statuses = statusesOf(drop);
    if (statuses.length === 1) return land(hit, statuses[0]!);
    choose(hit, statuses);
  }

  /** Several statuses: the copy waits at the landing place while the menu at the drop point asks which. */
  function choose(hit: Column, statuses: string[]) {
    if (held === null) return;
    const was = held;
    was.choosing = true;
    window.removeEventListener("keydown", onkey, true);
    const cards = cardsOf(hit);
    const at = was.before === null ? cards.length : cards.findIndex((one) => one.id === was.before);
    const top = at <= 0 ? hit.box.top + 6 : cards[at - 1]!.box.bottom + 6;
    was.line.hidden = true;
    was.why.hidden = true;
    // The menu's own outside press closes it: the hand goes with the pointer's part.
    was.cover?.remove();
    was.cover = null;
    was.drawn = { why: null, lineWidth: was.drawn.lineWidth, lineShown: false };
    was.ghost.style.transform = `translate3d(${hit.box.left + 5}px, ${top}px, 0)`;
    host.say(`${hit.column.name} holds ${either(statuses)}. Pick one.`);
    void host.choose(was.card, statuses, { x: hit.box.left + 29, y: top + was.height + 6 }).then((status) => {
      if (held !== was) return;
      if (status === null) putBackSaying(null);
      else land(hit, status);
    });
  }

  function land(hit: Column, status: string) {
    const was = letGo();
    if (was === null) return;
    const said = host.drop(was.card, { place: hit.place, before: was.before, status, lane: hit.lane });
    host.say(said ?? `${was.card.key} moved to ${status}. Queued.`);
    queueMicrotask(() => focusCard(was.card.entity_id));
  }

  // ---- the keys: the carry ----
  /** The columns the carry walks: of the card's lane, not `no`, its own included, in reading order. */
  const walkable = () => (held === null ? [] : held.columns.filter((one) => one.lane === held!.lane && one.drop.kind !== "no"));

  function stepTo(target: Column | undefined) {
    if (held === null || target === undefined) return;
    held.ghost.style.transform = `translate3d(${target.box.left + 5}px, ${target.box.top + 6}px, 0)`;
    hover(target, target.box.top - 1);
    target.node.scrollIntoView?.({ block: "nearest", inline: "nearest" });
  }

  function sayOver(first: boolean) {
    if (held === null || held.over === null) return;
    const { over, card } = held;
    const where = over.drop.kind === "own" ? `${card.key} is over ${over.column.name}, where it stands. Enter puts it back.` : `${card.key} over ${over.column.name}${statusesOf(over.drop).length > 1 ? ` (${either(statusesOf(over.drop))})` : ""}. Enter moves it, Esc puts it back.`;
    if (!first) return host.say(where);
    const shut = held.columns.filter((one) => one.lane === held!.lane && one.drop.kind === "no").map((one) => one.column.name);
    host.say(`${card.key} picked up. ${where}${shut.length > 0 ? ` It cannot go to ${shut.join(", ")}.` : ""}`);
  }

  function step(by: 1 | -1) {
    if (held === null) return;
    const can = walkable();
    const now = can.findIndex((one) => one === held!.over);
    stepTo(can[Math.min(can.length - 1, Math.max(0, now + by))]);
    sayOver(false);
  }

  function onkey(event: KeyboardEvent) {
    if (held === null || held.choosing) return;
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      putBackSaying(null);
      return;
    }
    if (!held.keys) return;
    event.preventDefault();
    event.stopPropagation();
    if (event.key === "ArrowRight" || event.key === "l") step(1);
    else if (event.key === "ArrowLeft" || event.key === "h") step(-1);
    else if (event.key === "Enter" || event.key === " ") release();
  }

  function onblur() {
    if (held !== null && !held.choosing) putBackSaying(null);
  }

  // ---- the pointer ----
  function cardAt(target: EventTarget | null): HTMLElement | null {
    if (!(target instanceof Element) || target.closest(".tf-trefused-x") !== null) return null;
    return target.closest<HTMLElement>(".tf-tcard[data-entity]");
  }

  function ondown(event: PointerEvent) {
    if (held !== null || event.button !== 0 || !event.isPrimary || (event.pointerType !== "mouse" && event.pointerType !== "pen")) return;
    const node = cardAt(event.target);
    if (node === null) return;
    const id = node.dataset.entity!;
    press = { id, x: event.clientX, y: event.clientY, pointer: event.pointerId };
    const found = host.cardOf(id);
    if (found !== null) void host.ask(found.card, "press").catch(() => {});
  }

  function onmove(event: PointerEvent) {
    if (held !== null) {
      if (held.keys || held.choosing || event.pointerId !== held.pointer) return;
      event.preventDefault();
      place(event.clientX, event.clientY);
      return;
    }
    if (press === null || event.pointerId !== press.pointer) return;
    if (Math.hypot(event.clientX - press.x, event.clientY - press.y) <= TREMBLE) return;
    const { id, pointer } = press;
    press = null;
    clearTimeout(rest);
    if (!pickup(id, { x: event.clientX, y: event.clientY }, false, pointer)) return;
    try {
      wrap?.setPointerCapture?.(pointer);
    } catch {
      // A synthetic pointer has no capture to take.
    }
  }

  function onup(event: PointerEvent) {
    press = null;
    if (held === null || held.keys || held.choosing || event.pointerId !== held.pointer) return;
    place(event.clientX, event.clientY);
    // The click the release makes is the drag's end, not a press on the card, nor a press outside the status menu the
    // drop may open in this handler: no other listener of the window hears it (the menu's outside-click is one).
    const swallow = (click: MouseEvent) => {
      click.preventDefault();
      click.stopImmediatePropagation();
    };
    window.addEventListener("click", swallow, { capture: true, once: true });
    setTimeout(() => window.removeEventListener("click", swallow, { capture: true }));
    release();
  }

  function oncancel() {
    press = null;
    if (held !== null && !held.keys && !held.choosing) putBackSaying(null);
  }

  /**
   * Capture lost while the button is still down is not the hand letting go: Chrome drops a capture on its own under a
   * board that redraws (measured in headless Chrome, 2026-11-22), and the window's listeners follow the pointer anyway.
   * Lost with no button down, the release went somewhere the board never heard of: put back.
   */
  function onlost(event: PointerEvent) {
    if (held === null || held.keys || held.choosing || event.pointerId !== held.pointer) return;
    if ((event.buttons & 1) === 1) return;
    putBackSaying(null);
  }

  function onover(event: PointerEvent) {
    clearTimeout(rest);
    if (held !== null || event.pointerType === "touch") return;
    const node = cardAt(event.target);
    if (node === null) return;
    const id = node.dataset.entity!;
    rest = setTimeout(() => {
      const found = host.cardOf(id);
      if (found !== null && held === null) void host.ask(found.card, "rest").catch(() => {});
    }, REST_MS);
  }

  return {
    attach: (node) => {
      wrap = node;
      node.addEventListener("pointerdown", ondown);
      node.addEventListener("pointerover", onover);
      node.addEventListener("lostpointercapture", onlost);
      node.addEventListener("scroll", onscroll, { capture: true, passive: true });
      window.addEventListener("pointermove", onmove, true);
      window.addEventListener("pointerup", onup, true);
      window.addEventListener("pointercancel", oncancel, true);
      return () => {
        letGo();
        // The page went: what it held back is no read of anyone's now.
        dropHeldBack();
        clearTimeout(rest);
        press = null;
        node.removeEventListener("pointerdown", ondown);
        node.removeEventListener("pointerover", onover);
        node.removeEventListener("lostpointercapture", onlost);
        node.removeEventListener("scroll", onscroll, { capture: true });
        window.removeEventListener("pointermove", onmove, true);
        window.removeEventListener("pointerup", onup, true);
        window.removeEventListener("pointercancel", oncancel, true);
        if (wrap === node) wrap = null;
      };
    },
    carry(entityId) {
      if (held !== null) return false;
      if (!pickup(entityId, null, true, null)) return false;
      // The first step: the nearest column to the right that takes it, else to the left, else where it stands.
      const can = walkable();
      const own = can.findIndex((one) => one.drop.kind === "own");
      const right = can.slice(own + 1).find((one) => takes(one.drop));
      const left = can.slice(0, Math.max(0, own)).reverse().find((one) => takes(one.drop));
      stepTo(right ?? left ?? can[own]);
      sayOver(true);
      return true;
    },
    holding: () => held !== null,
    putBack: () => {
      if (held !== null) putBackSaying(null);
    },
    afterDrag(read) {
      if (held === null && releasing === null) return read();
      heldBack.add(read);
    },
  };
}
