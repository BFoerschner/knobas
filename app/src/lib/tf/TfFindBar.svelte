<!--
  **The find bar for a page with no box** (design system, the key table): a ticket, a pull request's tabs and every diff. The
  same bar and keys as a Docs page's: *Find*, *n of m*, `Enter` and `⇧Enter`, the first match at or below the reading line,
  and `Esc` goes back unless you stepped or scrolled.

  It stands at the top of the page's scroller and takes no room there, so nothing moves as it opens, and it is on screen
  wherever on the page it is mounted. It finds what is drawn (`find-text.ts`) and finds again as more is drawn, which is what a
  windowed file does as you scroll. So it stops at what isn't drawn: a collapsed file, and a windowed file's far rows. `more`
  says some of those are there, and the count reads *12+*.

  A page keep-alive hides closes its bar, so one bar at most is live.
-->
<script lang="ts">
  import { tick, untrack } from "svelte";

  import { addFindBar, filterKeys } from "./filter-keys";
  import { canMark, clearMatches, drawMatches, findIn, selectedWords } from "./find-text";
  import { pageHeading } from "./focus-keep";
  import { pageVisible } from "./kept-pages.svelte";
  import { layers } from "./layers.svelte";

  let {
    root,
    within = null,
    more = () => false,
  }: {
    /** The element whose words it finds. */
    root: () => Element | null;
    /** What the count covers, where *on this page* would claim too much: *in open files*. */
    within?: string | null;
    /** Rows are held back from the drawn text, so the count is a floor. */
    more?: () => boolean;
  } = $props();

  const shown = pageVisible();
  let open = $state(false);
  let words = $state("");
  let count = $state(0);
  let at = $state(-1);
  let floor = $state(false);
  let box = $state<HTMLInputElement | null>(null);

  let hold: HTMLElement | null = null;
  let scroller: HTMLElement | null = null;
  /** What held focus when ⌘F opened the bar. */
  let from: HTMLElement | null = null;
  let matches: Range[] = [];
  /** Where the current match stood, to find its neighbour if a redraw takes it. */
  let currentLine = 0;
  /** `top`: where `Esc` puts the page back, or `null` once stepping says to stay. `line`: what typed words search from. `landed`: where the page was last put. */
  let place: { top: number | null; line: number; landed: number } | null = null;
  /** A wheel, a press or a key since the page was last put: a windowed list moves it too, and that isn't the reader. */
  let touched = false;
  /** The current match is drawn as a selection, in a webview with no marks. */
  let selecting = false;

  /** The box that scrolls this page. A narrow window turns a ticket's column off as a scroller. */
  function scrollerOf(el: Element | null): HTMLElement | null {
    for (let at = el?.closest<HTMLElement>(".tf-scroller, .tf-main"); at; at = at.parentElement?.closest<HTMLElement>(".tf-scroller, .tf-main")) {
      if (getComputedStyle(at).overflowY !== "visible") return at;
    }
    return null;
  }

  const topOf = (): number => scroller?.scrollTop ?? 0;
  const barHeight = (): number => hold?.firstElementChild?.getBoundingClientRect().height ?? 0;
  const lineOf = (range: Range): number => (scroller === null ? 0 : scroller.scrollTop + range.getBoundingClientRect().top - scroller.getBoundingClientRect().top);
  /** The line being read: below the bar, or the page's first line at the very top. */
  const readingLine = (): number => (topOf() === 0 ? 0 : topOf() + barHeight());
  const movedByHand = (): boolean => place !== null && touched && Math.abs(topOf() - place.landed) > 1;

  /** Into view. `center`: always, as a step does. Otherwise only if it isn't in view, as typing does. A long unified line scrolls sideways in its file too. */
  function reveal(range: Range, center: boolean): void {
    if (scroller === null) return;
    const seen = range.getBoundingClientRect();
    const view = scroller.getBoundingClientRect();
    const hidden = seen.bottom > view.bottom || (topOf() > 0 && seen.top < view.top + barHeight());
    if (center || hidden) scroller.scrollTop += seen.top - (view.top + view.height / 2);
    const stop = root();
    for (let el = range.startContainer.parentElement; el !== null && el !== stop; el = el.parentElement) {
      if (el.scrollWidth <= el.clientWidth + 1 || getComputedStyle(el).overflowX === "visible" || getComputedStyle(el).overflowX === "clip") continue;
      const edge = el.getBoundingClientRect();
      if (seen.left < edge.left || seen.right > edge.right) el.scrollLeft += seen.left - (edge.left + edge.width / 2);
      break;
    }
  }

  /** The match to read now: from `seed` when ⌘F started on a selection, else the first at or below the reading line, past the last back to the first. */
  function land(seed: Range | null = null): void {
    if (place === null || movedByHand()) place = { top: null, line: readingLine(), landed: topOf() };
    touched = false;
    const { line } = place;
    at = Math.max(
      seed !== null
        ? matches.findIndex((match) => match.compareBoundaryPoints(Range.START_TO_START, seed) >= 0)
        : matches.findIndex((match) => lineOf(match) >= line - 1),
      0,
    );
    const current = matches[at]!;
    drawMatches(matches, at);
    reveal(current, false);
    currentLine = lineOf(current);
    place.landed = topOf();
  }

  function search(seed: Range | null = null): void {
    const el = root();
    matches = el === null ? [] : findIn(el, words);
    count = matches.length;
    floor = more();
    if (count > 0) land(seed);
    else {
      at = -1;
      clearMatches();
    }
  }

  /** `Enter`, `⇧Enter` or an arrow: where the reader meant to go, so `Esc` stays here, and words typed next search from here. */
  function step(by: number): void {
    if (count === 0) return;
    at = (at + by + count) % count;
    const current = matches[at]!;
    drawMatches(matches, at, true);
    selecting = !canMark();
    reveal(current, true);
    currentLine = lineOf(current);
    touched = false;
    place = { top: null, line: currentLine, landed: topOf() };
  }

  /** More was drawn, or something went: count again, and the current match stays current, or its nearest takes over. The page doesn't move. */
  function drawnAgain(): void {
    const el = root();
    if (el === null || words.trim() === "") return;
    const held = matches[at];
    matches = findIn(el, words);
    count = matches.length;
    floor = more();
    if (count === 0) {
      at = -1;
      clearMatches();
      return;
    }
    at = held === undefined ? -1 : matches.findIndex((match) => match.startContainer === held.startContainer && match.startOffset === held.startOffset);
    if (at === -1) {
      const lines = matches.map(lineOf);
      at = lines.reduce((best, line, n) => (Math.abs(line - currentLine) < Math.abs(lines[best]! - currentLine) ? n : best), 0);
    }
    drawMatches(matches, at);
  }

  function stop(): void {
    open = false;
    words = "";
    count = 0;
    at = -1;
    floor = false;
    matches = [];
    place = null;
    from = null;
  }

  /** The bar closes. Focus left in it, or on nothing, goes back to what held it, else the page's heading. The page goes back to where the bar opened, unless the reader stepped or scrolled. */
  function close(): void {
    const focus = document.activeElement;
    const free = focus === null || focus === document.body || hold?.contains(focus) === true;
    const back = from?.isConnected ? from : pageHeading();
    const top = movedByHand() ? null : (place?.top ?? null);
    if (!canMark()) getSelection()?.removeAllRanges();
    if (free) back?.focus({ preventScroll: true });
    if (scroller !== null && top !== null) scroller.scrollTop = top;
    stop();
  }

  /** ⌘F: open with focus in the box, starting from the selected text if there is any. Pressed again, it selects the box's words. */
  function openBar(): void {
    const focus = document.activeElement;
    const seed = hold?.contains(focus) === true ? null : selectedWords();
    const opening = !open;
    if (opening) {
      from = focus instanceof HTMLElement && focus !== document.body ? focus : null;
      scroller = scrollerOf(root());
      open = true;
    }
    if (seed !== null) words = seed.words;
    void tick().then(() => {
      if (opening) {
        touched = false;
        place = { top: topOf(), line: readingLine(), landed: topOf() };
      }
      box?.focus({ preventScroll: true });
      if (seed !== null) search(seed.range);
      box?.select();
    });
  }

  function onkey(event: KeyboardEvent): void {
    if (event.key === "Enter") {
      event.preventDefault();
      step(event.shiftKey ? -1 : 1);
      return;
    }
    // A stepped-to match drawn as a selection took the caret, and typing needs it back, after the words.
    if (selecting && event.key !== "Escape" && box !== null) box.setSelectionRange(box.value.length, box.value.length);
    selecting = false;
  }

  const label = $derived.by(() => {
    if (words.trim() === "") return "";
    if (count === 0) return `none ${within ?? "on this page"}`;
    // A windowed file's rows leave as they scroll by, so with rows held back a position would only mislead.
    if (floor) return `${count}+ ${within ?? "on this page"}`;
    return `${Math.max(at, 0) + 1} of ${count}${within === null ? "" : ` ${within}`}`;
  });

  $effect(() => addFindBar({ here: () => shown() && root()?.isConnected === true, open: openBar }));

  // A hidden page holds no bar.
  $effect(() => {
    if (open && !shown()) stop();
  });

  // While open: on the layer stack, so `Esc` closes it before it leaves the page wherever focus is, and watching for what the page draws.
  $effect(() => {
    if (!open) return;
    const el = root();
    const off = untrack(() => layers.add({ kind: "panel", close }));
    let waiting: ReturnType<typeof setTimeout> | null = null;
    // Marks change no DOM, and the bar stands outside `root`, so this can't feed itself.
    const seen = new MutationObserver(() => {
      waiting ??= setTimeout(() => {
        waiting = null;
        drawnAgain();
      }, 16);
    });
    if (el !== null) seen.observe(el, { subtree: true, childList: true, characterData: true, attributes: true, attributeFilter: ["open"] });
    const handled = (event: Event): void => {
      if (!(event.target instanceof Node && hold?.contains(event.target))) touched = true;
    };
    const page = scroller;
    page?.addEventListener("wheel", handled, { passive: true });
    page?.addEventListener("pointerdown", handled);
    window.addEventListener("keydown", handled, true);
    return () => {
      off();
      seen.disconnect();
      if (waiting !== null) clearTimeout(waiting);
      page?.removeEventListener("wheel", handled);
      page?.removeEventListener("pointerdown", handled);
      window.removeEventListener("keydown", handled, true);
      clearMatches();
    };
  });

  /** To the top of the scroller, whose first line the bar stands over wherever on the page it was asked for. */
  function hoist(el: HTMLElement): void {
    hold = el;
    scroller?.prepend(el);
  }
</script>

{#if open}
  <div class="tf-findhold" {@attach hoist}>
    <div class="tf-findbar" class:tf-findwide={within !== null} role="search">
      <span>Find</span>
      <input
        bind:this={box}
        value={words}
        oninput={(event) => {
          words = event.currentTarget.value;
          search();
        }}
        onkeydown={onkey}
        aria-label="Find in page"
        {@attach filterKeys({ close })}
      />
      <!-- A held width: the arrows beside it do not move as the words change. -->
      <span class="tf-findn">{label}</span>
      <button type="button" disabled={count === 0} onclick={() => step(-1)} aria-label="Previous">↑</button>
      <button type="button" disabled={count === 0} onclick={() => step(1)} aria-label="Next">↓</button>
      <button type="button" class="tf-findx" onclick={close} aria-label="Stop highlighting">✕</button>
    </div>
  </div>
{/if}
