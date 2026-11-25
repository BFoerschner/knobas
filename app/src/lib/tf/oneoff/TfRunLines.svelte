<!--
  A run's output (ADR-0060, section 25): every row 20 px in the code face, and only the rows in view and a margin of
  twelve drawn, so a hundred thousand lines cost what forty do. The rows are one height, so the window is arithmetic,
  not measured. The box the rows stand in has the height of every row and the width of the longest line in `ch`, so
  neither scrollbar changes as the window moves.

  *Wrap* cuts a line at the column the box holds, as a terminal cuts it, so every row keeps its height: the first row
  of each line is kept in a typed array, filled as lines land and counted again only where the newest lines grew.
  *Follow* keeps the newest line in view while a run prints and lets go the moment the person scrolls up.

  A place is a link in the program's own colour; a press opens the file at its line. An address is a link to the
  browser. Nothing else a program printed is ever a link.
-->
<script lang="ts">
  import { untrack } from "svelte";
  import type { OneOffLine, OneOffPlace } from "../../ipc/oneoff";
  import { pieces, size } from "../../render/oneoff";
  import type { Run } from "../../render/oneoff.svelte";

  let {
    run,
    onplace,
    onurl,
    "data-arrive": arrive = false,
  }: {
    run: Run;
    onplace: (place: OneOffPlace, wide: boolean) => void;
    onurl: (href: string) => void;
    /** The dialog arrives here (`TfDialog`'s arrival focus), not on the stream's select before it. */
    "data-arrive"?: boolean;
  } = $props();

  const ROW = 20;
  const MARGIN = 12;
  const PAD = 6;

  let scroller = $state<HTMLDivElement | null>(null);
  let tall = $state<HTMLDivElement | null>(null);
  let win = $state<HTMLPreElement | null>(null);
  let top = $state(0);
  let height = $state(600);
  let width = $state(800);
  let charW = $state(0);

  /** The lines of the stream shown. */
  const rows = $derived.by((): OneOffLine[] => {
    void run.version;
    const all = run.lines;
    return run.stream === "both" ? all : all.filter((l) => l.stream === run.stream);
  });
  /** A row standing for the lines not kept, after the first 2,000. */
  const gapAt = $derived(run.dropped > 0 && run.stream === "both" ? rows.findIndex((l) => l.n >= run.tailFrom) : -1);
  const cols = $derived(run.wrap && charW > 0 ? Math.max(20, Math.floor((width - 58 - 18) / charW)) : 0);

  /** With *Wrap*, the first row of line `i` is `starts[i]`. */
  let starts = new Uint32Array(1);
  let startsFor = { cols: 0, rows: null as OneOffLine[] | null, upto: 0 };
  function rowStarts(list: OneOffLine[], c: number): Uint32Array {
    if (startsFor.cols !== c || startsFor.rows !== list || starts.length < list.length + 1) {
      const fresh = new Uint32Array(Math.max(list.length + 1, starts.length * (startsFor.cols === c ? 2 : 1), 1024));
      // The same list grown: what was counted stays, but for the last line, which may have grown.
      const keep = startsFor.cols === c && startsFor.rows === list ? Math.max(0, startsFor.upto - 1) : 0;
      fresh.set(starts.subarray(0, keep + 1));
      starts = fresh;
      startsFor = { cols: c, rows: list, upto: keep };
    }
    for (let i = Math.max(0, startsFor.upto - 1); i < list.length; i++) {
      starts[i + 1] = (starts[i] as number) + Math.max(1, Math.ceil((list[i] as OneOffLine).text.length / c));
    }
    startsFor.upto = list.length;
    return starts;
  }

  const total = $derived.by(() => {
    void run.version;
    const gap = gapAt >= 0 ? 1 : 0;
    if (cols === 0) return rows.length + gap;
    return (rowStarts(rows, cols)[rows.length] as number) + gap;
  });

  interface Drawn {
    key: string;
    line: OneOffLine | null;
    from: number;
    to: number;
    first: boolean;
  }

  /** The rows in view and the margin. */
  const drawn = $derived.by((): Drawn[] => {
    void run.version;
    const first = Math.max(0, Math.floor((top - PAD) / ROW) - MARGIN);
    const last = Math.min(total, first + Math.ceil(height / ROW) + MARGIN * 2);
    const out: Drawn[] = [];
    if (cols === 0) {
      for (let r = first; r < last; r++) {
        if (gapAt >= 0 && r === gapAt) {
          out.push({ key: "gap", line: null, from: 0, to: 0, first: true });
          continue;
        }
        const line = rows[gapAt >= 0 && r > gapAt ? r - 1 : r];
        if (line) out.push({ key: `${line.n}`, line, from: 0, to: line.text.length, first: true });
      }
      return out;
    }
    const s = rowStarts(rows, cols);
    let lo = 0;
    let hi = rows.length - 1;
    while (lo < hi) {
      const mid = (lo + hi + 1) >> 1;
      if ((s[mid] as number) <= first) lo = mid;
      else hi = mid - 1;
    }
    for (let r = first, i = lo; r < last && i < rows.length; ) {
      const line = rows[i] as OneOffLine;
      const part = r - (s[i] as number);
      out.push({ key: `${line.n}:${part}`, line, from: part * cols, to: Math.min(line.text.length, (part + 1) * cols), first: part === 0 });
      r++;
      if (r >= (s[i + 1] as number)) i++;
    }
    return out;
  });
  const firstRow = $derived(Math.max(0, Math.floor((top - PAD) / ROW) - MARGIN));

  // The box's height and width, and the window's place: through the CSSOM, as CSP refuses styles in markup.
  $effect(() => {
    if (tall === null) return;
    tall.style.height = `${total * ROW + PAD * 2}px`;
    tall.style.minWidth = cols > 0 ? "" : `calc(var(--oo-gut) + ${run.widest}ch + 22px)`;
  });
  $effect(() => {
    if (win !== null) win.style.transform = `translateY(${firstRow * ROW}px)`;
  });

  // Follow: the newest line in view while it prints.
  let setting = false;
  $effect(() => {
    void total;
    if (scroller === null || !run.follow || !run.running) return;
    untrack(() => {
      setting = true;
      scroller!.scrollTop = scroller!.scrollHeight;
      top = scroller!.scrollTop;
      setting = false;
    });
  });

  // Where a place press or N/P stands: brought into view.
  $effect(() => {
    const at = run.at;
    if (at === null || scroller === null) return;
    untrack(() => {
      const i = rows.findIndex((l) => l.n === at);
      if (i < 0) return;
      const r = (cols > 0 ? (rowStarts(rows, cols)[i] as number) : i) + (gapAt >= 0 && i >= gapAt ? 1 : 0);
      const y = r * ROW + PAD;
      if (y < scroller!.scrollTop || y + ROW > scroller!.scrollTop + scroller!.clientHeight) {
        scroller!.scrollTop = Math.max(0, y - scroller!.clientHeight / 3);
        top = scroller!.scrollTop;
      }
    });
  });

  function onscroll() {
    if (scroller === null) return;
    top = scroller.scrollTop;
    if (setting) return;
    const atEnd = scroller.scrollTop + scroller.clientHeight >= scroller.scrollHeight - ROW;
    if (!atEnd && run.follow && run.running) run.follow = false;
  }

  /** The box's size, and the code face's advance for *Wrap*. */
  function measure(el: HTMLDivElement) {
    const read = () => {
      height = el.clientHeight || height;
      width = el.clientWidth || width;
    };
    read();
    const probe = document.createElement("span");
    probe.textContent = "0".repeat(100);
    probe.className = "tf-oo-probe";
    el.appendChild(probe);
    charW = probe.getBoundingClientRect().width / 100 || 8;
    probe.remove();
    if (typeof ResizeObserver === "undefined") return;
    const seen = new ResizeObserver(read);
    seen.observe(el);
    return () => seen.disconnect();
  }

  function press(event: MouseEvent, place: OneOffPlace) {
    event.preventDefault();
    onplace(place, event.metaKey || event.ctrlKey);
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_tabindex: a scroller takes focus so the arrows scroll it (arrival focus, design system Dialogs) -->
<div
  class="tf-oo-out"
  tabindex="0"
  data-arrive={arrive ? "" : undefined}
  role="log"
  aria-label="What {run.command.name} printed"
  bind:this={scroller}
  {onscroll}
  {@attach measure}
>
  <div class="tf-oo-tall" bind:this={tall}>
    <pre class="tf-oo-win" bind:this={win}>{#each drawn as row, slot (slot)}{#if row.line === null}<div class="tf-oo-l tf-oo-gap"><span></span><span>{run.dropped.toLocaleString("en")} lines here were not kept: a run keeps its first 2,000 lines and its newest.</span></div>{:else}<div class="tf-oo-l" class:tf-at={run.at === row.line.n}><span>{row.first ? row.line.n + 1 : ""}</span><span>{#each pieces(row.line, row.from, row.to) as piece, k (k)}{#if piece.place !== null}<a class="tf-oo-link" href={"#"} title="Open {piece.place.path}{piece.place.line !== null ? ` at line ${piece.place.line}` : ""}" onclick={(event) => press(event, piece.place!)}><span class={piece.cls}>{piece.text}</span></a>{:else if piece.url !== null}<a class="tf-oo-link" href={piece.url.href} title="Open in the browser" onclick={(event) => { event.preventDefault(); onurl(piece.url!.href); }}><span class={piece.cls}>{piece.text}</span></a>{:else if piece.cls !== ""}<span class={piece.cls}>{piece.text}</span>{:else}{piece.text}{/if}{:else}{" "}{/each}{#if row.line.cut && row.to >= row.line.text.length}<span class="tf-oo-more"> … {size(row.line.cut)} more on this line</span>{/if}</span></div>{/if}{/each}</pre>
  </div>
</div>
