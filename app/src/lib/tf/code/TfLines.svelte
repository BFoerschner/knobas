<!--
  **A file's lines** as Source draws them (#2251, #2254, #2299), taken out of the file view (`TfRepoFiles.svelte`) so the
  XML view draws the same rows (its design, 2026-11-23, section 6): Find in file, a line link, a copied selection and the
  right-click on a line work in both because the rows are the same rows.

  - **Windowed**: a file over 300 lines draws only the lines in view and a margin (`shell/windowed.svelte.ts`); copying,
    a line link and Find reach the lines not drawn.
  - **Coloured** by the highlight worker as it answers, the lines in view first, from the text at hand.
  - **Find in file**: Source's bar over the lines, or (`find="overlay"`) no box until ⌘F, then the same bar at the top
    of the lines' own scroller, taking no room (Fable's ruling for the XML view, 2026-11-23). Either way the hits are
    counted in the file's text, so *n of m* is exact in a windowed file.
  - **What a document view adds** comes in through `rows` (the lines left on screen by its folds), `scrolls` (the lines
    scroll in a box of their own), `clip` (a line longer than 400 characters drawn as its start and a count), and three
    snippets: the caret's column after the number, what follows a line's text (a fold's count), and the line's end.
    With none of them, the markup is Source's, unchanged.
-->
<script lang="ts">
  import { flushSync, tick, untrack, type Snippet } from "svelte";
  import type { Attachment } from "svelte/attachments";

  import { filterKeys, addFindBar } from "../filter-keys";
  import { pageVisible } from "../kept-pages.svelte";
  import { layers } from "../layers.svelte";
  import TfXIcon from "../TfXIcon.svelte";
  import { Windowed } from "../../shell/windowed.svelte";
  import { codeRows } from "../fonts.svelte";
  import type { HighlightJob, LineTokens, RenderPort } from "../../render/port";
  import { findCount, stepMatch } from "../builds/run-reads";
  import { fileHits, firstHitOn, lineOfHit, needles, segments } from "../repo/file-lines";
  import { fileLines, selectedText, type TextPlace } from "../repo/repo";
  import { piecesOf, type Link } from "../../render/doc-links";

  let {
    text,
    path,
    render,
    line = null,
    typed = $bindable(""),
    find = "bar",
    rows = null,
    scrolls = false,
    clip = null,
    rowHeight = null,
    label = null,
    rowClass = null,
    caret = null,
    tail = null,
    end = null,
    reach = null,
    onwindow = null,
    rowAttach = null,
    linksOf = null,
    code = $bindable(null),
    wholeUpTo = 300,
  }: {
    text: string;
    /** The path the highlighter reads the language from. */
    path: string;
    render: RenderPort;
    /** The line a link names (from 1): marked, and scrolled to. */
    line?: number | null;
    /** What is typed into Find in file: the file view keeps it across files. */
    typed?: string;
    /** Find's bar always over the lines, or no box until ⌘F (`overlay`). */
    find?: "bar" | "overlay";
    /** The lines drawn, in order (from 1): all of them where `null`. */
    rows?: Uint32Array | null;
    /** The lines scroll in a box of their own, not the page. */
    scrolls?: boolean;
    /** A line longer than `clip.over` characters is drawn as its first `clip.keep` and the `tail` snippet. */
    clip?: { over: number; keep: number } | null;
    /** A row's height in pixels where it is not Source's code row. */
    rowHeight?: number | null;
    /** What the lines' box is called, for a screen reader. */
    label?: string | null;
    rowClass?: ((n: number) => string) | null;
    caret?: Snippet<[number]> | null;
    /** After a line's text: the line and, where it is drawn cut, its whole length (else 0). */
    tail?: Snippet<[number, number]> | null;
    end?: Snippet<[number]> | null;
    /** Called before a step to line `n` the rows leave out (a hit in a fold): the holder opens what hides it. */
    reach?: ((n: number) => void) | null;
    /** Hears the lines drawn, the first and the last: what a document view reads values of. */
    onwindow?: ((from: number, to: number) => void) | null;
    /** What opens in a drawn line of a document view: addresses, paths, namespaces (`render/doc-links.ts`). */
    linksOf?: ((n: number, text: string) => readonly Link[]) | null;
    /** What each drawn row of a document view carries (its right-click's thing). */
    rowAttach?: ((n: number) => Attachment<HTMLElement>) | null;
    /** The element that holds the rows. */
    code?: HTMLElement | null;
    /**
     * Lines of a file this short or shorter are drawn whole (Source's 300, #2251). A document view, whose folds make a
     * long file short, passes about a screenful: three levels of an order were 96 rows, all built in the key's frame.
     */
    wholeUpTo?: number;
  } = $props();

  const lines = $derived(fileLines(text));
  /** The line (from 1) drawn at each row. */
  const shownAt = (i: number): number => (rows === null ? i + 1 : rows[i]!);
  const count = () => (rows === null ? lines.length : rows.length);

  /**
   * Files of more lines than this draw only the lines in view and a margin (#2251), as every windowed list does past 300
   * rows: drawn whole, a 400-line file was 90 ms of a return's build at 6x, and its tokens landing as much again (the
   * inner-6x pass, 2026-11-15). Copying, a line link and *Find in file* reach the lines not drawn. See `wholeUpTo`.
   */
  // A line is 13 px at its code row height (`fonts.svelte.ts`, `codeRows`): 16 px Joined, 20.8 Open.
  const rowsOf = codeRows(13, 1.6);
  // svelte-ignore state_referenced_locally
  const fixed = rowHeight;
  const win = new Windowed<number>({
    count,
    key: (i) => shownAt(i),
    estimate: fixed ?? rowsOf.estimate,
    // svelte-ignore state_referenced_locally
    from: wholeUpTo + 1,
    textSize: fixed === null ? rowsOf.textSize : null,
    // svelte-ignore state_referenced_locally
    scrolls,
  });
  /** The rows drawn now: `[index of the row, its line]`. */
  const drawn = $derived.by(() => {
    void lines;
    const out: number[] = [];
    const n = count();
    const from = win.on ? win.start : 0;
    const to = win.on ? win.end : n;
    for (let i = from; i < to && i < n; i += 1) out.push(i);
    return out;
  });

  /** The row a line stands in: the line itself, or the fold that hides it. */
  function rowOf(n: number): number {
    if (rows === null) return n - 1;
    let lo = 0;
    let hi = rows.length - 1;
    while (lo < hi) {
      const mid = (lo + hi + 1) >> 1;
      if (rows[mid]! <= n) lo = mid;
      else hi = mid - 1;
    }
    return lo;
  }

  /**
   * Each line's tokens as the worker sent them, by line index; not reactive
   * itself, `painted` counts the answers so the drawn rows read them again.
   */
  let tokensOf: (LineTokens | undefined)[] = [];
  let painted = $state(0);
  let highlighting: HighlightJob | null = null;
  /** The lines in view, from 0: what the worker colours next. */
  const focusOf = (): [number, number] => {
    const n = count();
    if (n === 0) return [0, 0];
    const from = Math.min(win.start, n - 1);
    const to = Math.max(from, Math.min(win.end, n) - 1);
    return rows === null ? [win.start, win.end] : [shownAt(from) - 1, shownAt(to)];
  };
  $effect(() => {
    const read = text;
    const at = path;
    const got: (LineTokens | undefined)[] = [];
    tokensOf = got;
    untrack(() => (painted += 1));
    const started = untrack(() => render).highlight(
      { text: read, path: at, focus: untrack(focusOf) },
      {
        onLines(from, answered) {
          if (tokensOf !== got) return;
          for (let i = 0; i < answered.length; i += 1) got[from + i] = answered[i];
          painted += 1;
        },
      },
    );
    highlighting = started;
    return () => {
      started.cancel();
      if (highlighting === started) highlighting = null;
    };
  });
  // The lines in view moved: the worker does them next.
  $effect(() => {
    void win.start;
    void win.end;
    void rows;
    const [from, to] = focusOf();
    untrack(() => highlighting?.focus(from, to));
  });

  // The lines drawn, for whoever reads what stands on them.
  $effect(() => {
    const hear = onwindow;
    const n = count();
    if (hear === null || n === 0) return;
    const from = win.on ? win.start : 0;
    const to = Math.min(n, win.on ? win.end : n) - 1;
    untrack(() => hear(shownAt(Math.max(0, from)), shownAt(Math.max(0, to))));
  });

  const noAttach: Attachment<HTMLElement> = () => {};

  /** *Find in file* (#2299): what is typed, its hits, and the hit stepped to. */
  let current = $state(0);
  const found = $derived(needles(typed));
  const hits = $derived(fileHits(lines, found));

  /** Line `n`'s whole length where it is drawn cut, else 0. */
  const cut = (n: number): number => (clip !== null && (lines[n - 1]?.length ?? 0) > clip.over ? lines[n - 1]!.length : 0);

  /** Line `index`'s segments: its tokens and hits, `null` to draw its text as it is. */
  function shade(at: string, index: number) {
    void painted;
    if (clip !== null && at.length > clip.over) return segments(at.slice(0, clip.keep), tokensOf[index]?.length === undefined ? undefined : clipTokens(tokensOf[index]!, clip.keep), found);
    return segments(at, tokensOf[index], found);
  }
  /** A line's tokens up to `keep` characters. */
  function clipTokens(tokens: LineTokens, keep: number): LineTokens {
    const out: [string, string][] = [];
    let left = keep;
    for (const [t, kind] of tokens as unknown as [string, string][]) {
      if (left <= 0) break;
      out.push([t.slice(0, left), kind]);
      left -= t.length;
    }
    return out as unknown as LineTokens;
  }
  const plain = (at: string) => (clip !== null && at.length > clip.over ? at.slice(0, clip.keep) : at);

  /** A drawn line's runs, cut where its links start and end. */
  function pieces(n: number, at: string, segs: ReturnType<typeof segments>) {
    const shown = plain(at);
    const runs = segs ?? [{ text: shown || " ", kind: "", hit: null }];
    const links = linksOf === null || shown.length < 4 ? [] : linksOf(n, shown);
    return { links, pieces: piecesOf(runs, links) };
  }

  function findIn(event: Event) {
    typed = (event.currentTarget as HTMLInputElement).value;
    current = 0;
  }

  /** Steps to the next or previous hit, drawing its line if the window left it out, and scrolls it to the middle. */
  function jump(step: 1 | -1) {
    if (hits.total === 0) return;
    current = stepMatch(current, step, hits.total);
    const at = lineOfHit(hits, current);
    if (rows !== null && rows[rowOf(at + 1)] !== at + 1) reach?.(at + 1);
    flushSync();
    win.reveal(rowOf(at + 1));
    flushSync();
    (code?.querySelector("mark.tf-curm") ?? code?.querySelector(`.tf-l[data-n="${at + 1}"]`))?.scrollIntoView?.({ block: "center" });
  }

  function findKeys(event: KeyboardEvent) {
    if (event.key !== "Enter") return;
    event.preventDefault();
    jump(event.shiftKey ? -1 : 1);
  }

  /** Line `n` (from 1) into view, drawn if the window left it out; `centre` puts it in the middle. */
  export function reveal(n: number, centre = true) {
    win.reveal(rowOf(n));
    flushSync();
    const row = code?.querySelector<HTMLElement>(`.tf-l[data-n="${n}"]`);
    if (row === null || row === undefined) return;
    if (centre || !scrolls) row.scrollIntoView?.({ block: centre ? "center" : "nearest" });
    else {
      // Within the lines' own box only: the page around it stays where it is.
      const box = code!;
      const top = row.offsetTop;
      const bottom = top + row.offsetHeight;
      if (top < box.scrollTop) box.scrollTop = top;
      else if (bottom > box.scrollTop + box.clientHeight) box.scrollTop = bottom - box.clientHeight;
    }
  }

  /** The file and line last scrolled to, so a refresh of the file leaves the reader where they are. */
  let shownLine: string | null = null;
  $effect(() => {
    const at = line;
    const where = path;
    if (at === null) return;
    const key = `${where}\u0000${at}`;
    if (key === shownLine) return;
    shownLine = key;
    void tick().then(() => {
      if (rows !== null && rows[rowOf(at)] !== at) reach?.(at);
      flushSync();
      reveal(at);
    });
  });

  /** Where `node` at `offset` sits in the file's text, if it is inside a drawn line's text. */
  function placeOf(node: Node | null, offset: number): TextPlace | null {
    const el = node instanceof Element ? node : (node?.parentElement ?? null);
    const row = el?.closest<HTMLElement>(".tf-l");
    if (!row || !code?.contains(row)) return null;
    const n = Number(row.dataset["n"]) - 1;
    const body = row.querySelector(":scope > .tf-lt") ?? row.lastElementChild;
    if (body === null || !body.contains(node)) return { line: n, offset: body !== null && row.lastElementChild !== body && body.compareDocumentPosition(node!) & Node.DOCUMENT_POSITION_FOLLOWING ? (lines[n]?.length ?? 0) : 0 };
    const before = document.createRange();
    before.setStart(body, 0);
    before.setEnd(node!, offset);
    return { line: n, offset: Math.min(before.toString().length, lines[n]?.length ?? 0) };
  }

  /** Copies the file's own text between the selection's ends: no line numbers, every line between. */
  function copyLines(event: ClipboardEvent) {
    const selection = document.getSelection();
    if (selection === null || selection.isCollapsed || event.clipboardData === null) return;
    const from = placeOf(selection.anchorNode, selection.anchorOffset);
    const to = placeOf(selection.focusNode, selection.focusOffset);
    if (from === null || to === null) return;
    event.clipboardData.setData("text/plain", selectedText(lines, from, to));
    event.preventDefault();
  }

  // ---- The overlay: no box until ⌘F (the XML view) ----
  const shown = pageVisible();
  let open = $state(false);
  let box = $state<HTMLInputElement | null>(null);
  /** What held focus when ⌘F opened the bar. */
  let from: HTMLElement | null = null;
  $effect(() => {
    if (find !== "overlay") return;
    return addFindBar({ here: () => shown() && code?.isConnected === true, open: openBar });
  });
  function openBar() {
    const focus = document.activeElement;
    if (!open) from = focus instanceof HTMLElement && focus !== document.body ? focus : null;
    const picked = getSelection()?.toString() ?? "";
    if (picked !== "" && !picked.includes("\n") && !(box?.contains(focus) ?? false)) {
      typed = picked;
      current = 0;
    }
    open = true;
    void tick().then(() => {
      box?.focus({ preventScroll: true });
      box?.select();
    });
  }
  function closeBar() {
    const focus = document.activeElement;
    const free = focus === null || focus === document.body || (box?.contains(focus) ?? false);
    open = false;
    typed = "";
    current = 0;
    if (free) (from?.isConnected ? from : code)?.focus({ preventScroll: true });
    from = null;
  }
  $effect(() => {
    if (!open) return;
    return untrack(() => layers.add({ kind: "panel", close: closeBar }));
  });
  $effect(() => {
    if (open && !shown()) closeBar();
  });
  /** The overlay stands at the top of the lines' own scroller and takes no room there. */
  function hoist(el: HTMLElement): () => void {
    code?.prepend(el);
    return () => el.remove();
  }
  // svelte-ignore state_referenced_locally
  const tall = rowHeight;
  function rowSize(node: HTMLElement) {
    if (tall !== null) node.style.setProperty("--tf-row", `${tall}px`);
  }
</script>

{#snippet piece(pc: { run: { kind: string; hit: number | null }; text: string }, base: number)}{#if pc.run.hit !== null}<mark class={pc.run.kind === "" ? undefined : `tf-hl-${pc.run.kind}`} class:tf-curm={base + pc.run.hit === current}>{pc.text}</mark>{:else if pc.run.kind !== ""}<span class="tf-hl-{pc.run.kind}">{pc.text}</span>{:else}{pc.text}{/if}{/snippet}

{#if find === "bar"}
  <div class="tf-findbar">
    <label class="tf-fsearch">
      <svg width="13" height="13" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.8" aria-hidden="true"
        ><circle cx="7" cy="7" r="5" /><path d="M11 11l3.5 3.5" /></svg
      >
      <input placeholder="Find in file" aria-label="Find in file" value={typed} oninput={findIn} onkeydown={findKeys} {@attach filterKeys()} />
      <span class="tf-dim tf-lqn" aria-live="polite">{findCount(typed, hits.total, current)}</span>
    </label>
    <span class="tf-nav2">
      <button type="button" aria-label="Previous match" onclick={() => jump(-1)}>↑</button><button
        type="button"
        aria-label="Next match"
        onclick={() => jump(1)}>↓</button
      >
    </span>
  </div>
{/if}
{#if rows === null && caret === null && end === null && clip === null}
  <pre class="tf-code" bind:this={code} oncopy={copyLines} {@attach win.list}>{#each win.slice(lines) as at, i (win.start + i)}{@const n = win.start + i + 1}{@const segs = shade(at, n - 1)}<div class="tf-l" class:tf-at={n === line} data-n={n} {@attach win.row(n)}><span>{n}</span><span>{#if segs === null}{at || " "}{:else}{@const base = firstHitOn(hits, n - 1) ?? 0}{#each segs as seg, k (k)}{#if seg.hit !== null}<mark class={seg.kind === "" ? undefined : `tf-hl-${seg.kind}`} class:tf-curm={base + seg.hit === current}>{seg.text}</mark>{:else if seg.kind !== ""}<span class="tf-hl-{seg.kind}">{seg.text}</span>{:else}{seg.text}{/if}{/each}{/if}</span></div>{/each}</pre>
{:else}
  <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
  <pre class="tf-code tf-lines" class:tf-lscroll={scrolls} bind:this={code} oncopy={copyLines} tabindex={scrolls ? 0 : undefined} aria-label={label ?? undefined} {@attach rowSize} {@attach win.list}>{#if win.before > 0 && scrolls}<div class="tf-wgap" aria-hidden="true" {@attach win.room("before")}></div>{/if}{#each drawn as i (shownAt(i))}{@const n = shownAt(i)}{@const at = lines[n - 1] ?? ""}{@const segs = shade(at, n - 1)}<div class="tf-l {rowClass?.(n) ?? ''}" class:tf-at={n === line} data-n={n} {@attach win.row(n)} {@attach rowAttach?.(n) ?? noAttach}><span>{n}</span>{#if caret !== null}{@render caret(n)}{/if}<span class="tf-lt">{#if linksOf !== null}{@const cut = pieces(n, at, segs)}{@const base = firstHitOn(hits, n - 1) ?? 0}{#each cut.pieces as pc, k (k)}{#if pc.link !== null}{@const l = cut.links[pc.link]!}<span class="tf-xv-link tf-xv-{l.kind}" role="link" tabindex="-1" data-xv-link="{n}:{pc.link}" title={l.kind === "namespace" ? `The namespace ${l.to}: a name, not a page` : `Open ${l.to}`}>{@render piece(pc, base)}</span>{:else}{@render piece(pc, base)}{/if}{/each}{:else if segs === null}{plain(at) || " "}{:else}{@const base = firstHitOn(hits, n - 1) ?? 0}{#each segs as seg, k (k)}{#if seg.hit !== null}<mark class={seg.kind === "" ? undefined : `tf-hl-${seg.kind}`} class:tf-curm={base + seg.hit === current}>{seg.text}</mark>{:else if seg.kind !== ""}<span class="tf-hl-{seg.kind}">{seg.text}</span>{:else}{seg.text}{/if}{/each}{/if}{#if tail !== null}{@render tail(n, cut(n))}{/if}</span>{#if end !== null}{@render end(n)}{/if}</div>{/each}{#if win.after > 0 && scrolls}<div class="tf-wgap" aria-hidden="true" {@attach win.room("after")}></div>{/if}</pre>
  {#if find === "overlay" && open}
    <div class="tf-findhold" {@attach hoist}>
      <div class="tf-findbar" role="search">
        <span>Find</span>
        <input
          bind:this={box}
          value={typed}
          oninput={findIn}
          onkeydown={findKeys}
          aria-label="Find in file"
          {@attach filterKeys({ close: closeBar })}
        />
        <span class="tf-findn">{findCount(typed, hits.total, current)}</span>
        <button type="button" disabled={hits.total === 0} onclick={() => jump(-1)} aria-label="Previous">↑</button>
        <button type="button" disabled={hits.total === 0} onclick={() => jump(1)} aria-label="Next">↓</button>
        <button type="button" class="tf-findx" onclick={closeBar} aria-label="Stop highlighting"><TfXIcon /></button>
      </div>
    </div>
  {/if}
{/if}
