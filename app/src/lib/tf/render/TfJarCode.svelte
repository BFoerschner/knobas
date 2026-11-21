<!--
  Text inside a JAR (ux19): a resource opened (`application.yml`, a migration's SQL) or a class decompiled, drawn as
  the file viewer draws Source: numbered lines, highlighted by the highlighting worker for the name's language, the
  lines in view first. In a box of its own that scrolls itself, windowed, its height fixed from the line count before
  it draws (`rows`, at most `max`), so nothing under it moves as the tokens come.
-->
<script lang="ts">
  import { untrack } from "svelte";

  import { Windowed } from "../../shell/windowed.svelte";
  import { codeRows } from "../fonts.svelte";
  import { renderPort, type LineTokens, type RenderPort } from "../../render/port";

  let {
    text,
    path,
    max = 24,
    fixed = false,
    render = renderPort(),
  }: {
    text: string;
    /** The name whose extension picks the language. */
    path: string;
    /** The most lines the box shows before it scrolls. */
    max?: number;
    /** Hold `max` lines' height however few there are: a pane whose text is still coming. */
    fixed?: boolean;
    render?: RenderPort;
  } = $props();

  const lines = $derived(text.replace(/\n$/, "").split("\n"));
  const rows = $derived(fixed ? max : Math.max(1, Math.min(lines.length, max)));

  let tokensOf: (LineTokens | undefined)[] = [];
  let painted = $state(0);

  // A class's line is `.tf-code`'s, 13 px at its code row height (`codeRows`), and follows code's size and spacing.
  const rowsOf = codeRows(13, 1.6);
  const win = new Windowed<number>({ count: () => lines.length, key: (i) => i, estimate: rowsOf.estimate, scrolls: true, textSize: rowsOf.textSize });

  $effect(() => {
    const source = text;
    const name = path;
    const got: (LineTokens | undefined)[] = [];
    tokensOf = got;
    untrack(() => (painted += 1));
    const job = render.highlight(
      { text: source, path: name, focus: untrack(() => [win.start, win.end]) },
      {
        onLines(from, answered) {
          if (tokensOf !== got) return;
          for (let i = 0; i < answered.length; i += 1) got[from + i] = answered[i];
          painted += 1;
        },
      },
    );
    $effect(() => {
      const [from, to] = [win.start, win.end];
      untrack(() => job.focus(from, to));
    });
    return () => job.cancel();
  });

  function tokens(i: number): LineTokens | undefined {
    void painted;
    return tokensOf[i];
  }

  let box = $state<HTMLElement | null>(null);
  $effect(() => {
    // The box's height: its lines' at the code face's line height, set through the CSSOM (CSP refuses inline styles).
    box?.style.setProperty("--jar-rows", String(rows));
  });
</script>

<div class="tf-jar-code" bind:this={box}>
  <pre class="tf-code" {@attach win.list}>{#if win.before > 0}<div class="tf-wgap" aria-hidden="true" {@attach win.room("before")}></div>{/if}{#each win.slice(lines) as line, at (win.start + at)}{@const n = win.start + at}{@const segs = tokens(n)}<div class="tf-l" {@attach win.row(n)}><span>{n + 1}</span><span>{#if segs === undefined}{line || " "}{:else}{#each segs as [part, kind], s (s)}{#if kind === ""}{part}{:else}<span class="tf-hl-{kind}">{part}</span>{/if}{/each}{#if segs.length === 0}{" "}{/if}{/if}</span></div>{/each}{#if win.after > 0}<div class="tf-wgap" aria-hidden="true" {@attach win.room("after")}></div>{/if}</pre>
</div>
