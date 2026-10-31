<!--
  A PDF's *Rendered* diff (#2282, spec #2234 step 6e): before and after
  page by page side by side, one row per aligned pair of pages, the pages
  that differ marked: *changed*, or *added* / *removed* where the page
  counts differ. `render/pdf-diff.ts` says how pages are compared and
  aligned. An added or deleted PDF has one side (`null` for the other):
  every page is added or removed.

  Pages draw as their row comes within 800 px of the view, as the file
  viewer's do (`TfPdf.svelte`), as pictures only: no text layer, no links.
-->
<script lang="ts">
  import { comparePdfs, type PageRow } from "../../render/pdf-diff";
  import { zoomStep } from "../../render/pdf-view";
  import { renderPort, type PdfDocument, type RenderPort } from "../../render/port";

  let {
    before,
    after,
    path,
    render = renderPort(),
  }: {
    /** The PDF's bytes before, `null` for none (an added PDF). */
    before: Uint8Array | null;
    /** Its bytes after, `null` for none (a deleted PDF). */
    after: Uint8Array | null;
    path: string;
    render?: RenderPort;
  } = $props();

  type Size = { width: number; height: number };
  type Shown =
    | { kind: "comparing" }
    | { kind: "open"; docs: [PdfDocument | null, PdfDocument | null]; sizes: [Size[], Size[]]; rows: PageRow[] }
    | { kind: "failed"; why: string };

  // Raw: the documents are pdf.js's, not something to proxy.
  let shown = $state.raw<Shown>({ kind: "comparing" });
  let scale = $state(0.5);

  const sizesOf = async (pdf: PdfDocument | null): Promise<Size[]> =>
    pdf === null ? [] : (await Promise.all(Array.from({ length: pdf.pages }, (_, i) => pdf.page(i + 1)))).map(({ width, height }) => ({ width, height }));

  $effect(() => {
    const sides = [before, after] as const;
    const engine = render.pdf;
    let live = true;
    const opened: PdfDocument[] = [];
    if (engine === undefined) {
      shown = { kind: "failed", why: "No PDF engine runs in this window." };
      return;
    }
    shown = { kind: "comparing" };
    void (async () => {
      const docs: (PdfDocument | null)[] = [];
      for (const [i, bytes] of sides.entries()) {
        if (bytes === null) {
          docs.push(null);
          continue;
        }
        const answer = await engine(bytes);
        if ("error" in answer) {
          if (live) shown = { kind: "failed", why: `The ${i === 0 ? "before" : "after"} side is not a PDF pdf.js can read: ${answer.error}` };
          return;
        }
        if (!live) return answer.pdf.close();
        opened.push(answer.pdf);
        docs.push(answer.pdf);
      }
      const [a, b] = docs as [PdfDocument | null, PdfDocument | null];
      try {
        const [rows, sizesA, sizesB] = await Promise.all([comparePdfs(a, b), sizesOf(a), sizesOf(b)]);
        if (live) shown = { kind: "open", docs: [a, b], sizes: [sizesA, sizesB], rows };
      } catch (error) {
        if (live) shown = { kind: "failed", why: error instanceof Error ? error.message : String(error) };
      }
    })();
    return () => {
      live = false;
      for (const pdf of opened) pdf.close();
    };
  });

  const counts = $derived.by(() => {
    if (shown.kind !== "open") return "";
    const of = (mark: PageRow["mark"]) => shown.kind === "open" ? shown.rows.filter((row) => row.mark === mark).length : 0;
    const said = [`${of("changed")} changed`, `${of("added")} added`, `${of("removed")} removed`].filter((part) => !part.startsWith("0 "));
    return said.length === 0 ? "no page differs" : said.join(", ");
  });

  /** Draws page `n` of `pdf` into its box when the box comes near the view; again at a new zoom. */
  function pageAt(pdf: PdfDocument, n: number, size: Size) {
    return (node: HTMLElement) => {
      const at = scale;
      // Through the CSSOM: the app's CSP refuses `style=""` in markup, not this.
      node.style.width = `${Math.round(size.width * at)}px`;
      node.style.height = `${Math.round(size.height * at)}px`;
      let live = true;
      let cancel = () => {};
      const go = () =>
        void pdf.page(n).then((page) => {
          if (!live) return;
          const drawing = page.draw(node.querySelector("canvas")!, at);
          cancel = drawing.cancel;
          drawing.done.catch(() => {});
        });
      const stop = () => {
        live = false;
        cancel();
      };
      if (typeof IntersectionObserver === "undefined") {
        go();
        return stop;
      }
      const near = new IntersectionObserver(
        (entries) => {
          if (!entries.some((entry) => entry.isIntersecting)) return;
          near.disconnect();
          go();
        },
        { rootMargin: "800px 0px" },
      );
      near.observe(node);
      return () => {
        near.disconnect();
        stop();
      };
    };
  }

  const MARK = { same: "", changed: "changed", added: "added", removed: "removed" } as const;
</script>

{#snippet cell(pdf: PdfDocument | null, n: number | null, sizes: Size[], side: string)}
  <div class="tf-pdfd-cell">
    {#if pdf !== null && n !== null}
      <div class="tf-pdf-page" {@attach pageAt(pdf, n, sizes[n - 1]!)}><canvas aria-label="{path} page {n} {side}"></canvas></div>
    {:else}
      <div class="tf-pdfd-none">no page {side}</div>
    {/if}
  </div>
{/snippet}

{#if shown.kind === "comparing"}
  <p class="tf-dim tf-pdfd-wait" aria-live="polite">Comparing the pages…</p>
{:else if shown.kind === "failed"}
  <div class="tf-stepfail" role="alert"><b>PDF could not compare this.</b><pre>{shown.why}</pre></div>
{:else}
  {@const [a, b] = shown.docs}
  {@const [sizesA, sizesB] = shown.sizes}
  <div class="tf-pdfd">
    <div class="tf-pdf-bar">
      <span class="tf-dim tf-small tf-pdfd-sum">{sizesA.length} → {sizesB.length} pages · {counts}</span>
      <span class="tf-trail">
        <button type="button" class="tf-btn tf-sm" aria-label="Zoom out" onclick={() => (scale = zoomStep(scale, -1))}>−</button>
        <span class="tf-pdf-zoom" aria-live="polite">{Math.round(scale * 100)}%</span>
        <button type="button" class="tf-btn tf-sm" aria-label="Zoom in" onclick={() => (scale = zoomStep(scale, 1))}>+</button>
      </span>
    </div>
    <div class="tf-pdfd-head"><span>before</span><span>after</span></div>
    {#each shown.rows as row, i (i)}
      <div class="tf-pdfd-row tf-pdfd-{row.mark}" data-mark={row.mark}>
        <div class="tf-pdfd-at">
          <span class="tf-mono">p. {row.old ?? "–"} → {row.new ?? "–"}</span>
          {#if MARK[row.mark]}<span class="tf-pdfd-mark">{MARK[row.mark]}</span>{/if}
        </div>
        {@render cell(a, row.old, sizesA, "before")}
        {@render cell(b, row.new, sizesB, "after")}
      </div>
    {/each}
  </div>
{/if}
