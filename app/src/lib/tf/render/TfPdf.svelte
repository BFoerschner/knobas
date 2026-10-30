<!--
  A PDF in the file viewer (#2260, spec #2234 step 5): its bytes from the
  byte read (#2256), opened by pdf.js with scripting and form actions off
  (`render/pdf-core.ts`), every page a canvas with its text over it to
  select, drawn as it scrolls near the view and again at a new zoom.

  It draws in the document, not in a sandboxed frame: a canvas, text set as
  `textContent`, and links this component makes itself. No markup from the
  PDF is ever parsed. Links follow the Markdown rules (#2255): a web page
  goes to the opener (a middle click too), a link to a page scrolls to it.
  #2282's diff draws two of these side by side.
-->
<script lang="ts">
  import { ipcErrorMessage } from "../../ipc";
  import { fitWidth, zoomStep } from "../../render/pdf-view";
  import { renderPort, type PdfDocument, type PdfLink, type RenderPort } from "../../render/port";
  import { BYTES_CAP } from "../../render/registry";
  import { openExternal as realOpenExternal } from "../../shell/open-external";
  import { push } from "../../shell/toasts.svelte";

  let {
    read,
    size,
    render = renderPort(),
    openExternal = realOpenExternal,
  }: {
    /** The file's bytes; a rejection is the byte read's refusal, its message shown. */
    read: () => Promise<Uint8Array>;
    /** The file's size as its block says, known before the read: past the cap nothing is read. */
    size: number;
    /** The engines (#2254): the app's, a fake in tests. */
    render?: RenderPort;
    openExternal?: (url: string) => Promise<void>;
  } = $props();

  type Shown =
    | { kind: "reading" }
    | { kind: "open"; pdf: PdfDocument; sizes: { width: number; height: number }[] }
    | { kind: "failed"; title: string; why: string };

  // Raw: the document is pdf.js's, not something to proxy.
  let shown = $state.raw<Shown>({ kind: "reading" });
  let scale = $state(1);
  let pane = $state<HTMLElement | null>(null);

  const mib = (bytes: number) => `${(bytes / 1024 / 1024).toFixed(1)} MiB`;

  $effect(() => {
    const reading = read;
    const engine = render.pdf;
    let live = true;
    let opened: PdfDocument | null = null;
    scale = 1;
    if (size > BYTES_CAP) {
      shown = { kind: "failed", title: "This PDF is too large to draw.", why: `It is ${mib(size)}; knobas reads a file's bytes up to ${mib(BYTES_CAP)}.` };
      return;
    }
    if (engine === undefined) {
      shown = { kind: "failed", title: "PDFs are not drawn here.", why: "No PDF engine runs in this window." };
      return;
    }
    shown = { kind: "reading" };
    void (async () => {
      let bytes: Uint8Array;
      try {
        bytes = await reading();
      } catch (error) {
        if (live) shown = { kind: "failed", title: "Could not read this PDF.", why: ipcErrorMessage(error) };
        return;
      }
      const answer = await engine(bytes);
      if ("error" in answer) {
        if (live) shown = { kind: "failed", title: "PDF could not draw this.", why: answer.error };
        return;
      }
      if (!live) return answer.pdf.close();
      opened = answer.pdf;
      const pages = await Promise.all(Array.from({ length: answer.pdf.pages }, (_, i) => answer.pdf.page(i + 1)));
      if (live) shown = { kind: "open", pdf: answer.pdf, sizes: pages.map(({ width, height }) => ({ width, height })) };
    })();
    return () => {
      live = false;
      opened?.close();
    };
  });

  const widest = $derived(shown.kind === "open" ? Math.max(...shown.sizes.map((s) => s.width)) : 0);

  /** Draws page `n` into its box, when the box comes near the view; again whenever the zoom changes. */
  function pageAt(pdf: PdfDocument, n: number, width: number, height: number) {
    return (node: HTMLElement) => {
      const at = scale;
      // Through the CSSOM: the app's CSP refuses `style=""` in markup, not this.
      node.style.width = `${Math.round(width * at)}px`;
      node.style.height = `${Math.round(height * at)}px`;
      let stop = () => {};
      const go = () => (stop = paint(pdf, n, node, at));
      if (typeof IntersectionObserver === "undefined") {
        go();
        return () => stop();
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

  function paint(pdf: PdfDocument, n: number, node: HTMLElement, at: number): () => void {
    let live = true;
    let cancel = () => {};
    void pdf.page(n).then(async (page) => {
      if (!live) return;
      const canvas = node.querySelector("canvas")!;
      const drawing = page.draw(canvas, at);
      cancel = drawing.cancel;
      drawing.done.catch(() => {});
      const [links] = await Promise.all([page.links(), page.text(node.querySelector<HTMLElement>(".tf-pdf-text")!, at).catch(() => {})]);
      if (live) node.querySelector(".tf-pdf-links")!.replaceChildren(...links.map((link) => anchor(link, at)));
    });
    return () => {
      live = false;
      cancel();
    };
  }

  function anchor(link: PdfLink, at: number): HTMLAnchorElement {
    const a = document.createElement("a");
    const [left, top, width, height] = link.rect;
    if ("url" in link.target) {
      a.href = link.target.url;
      a.className = "tf-ext";
      a.title = link.target.url;
    } else {
      a.href = "#";
      a.dataset["pdfPage"] = String(link.target.page);
      a.title = `Page ${link.target.page}`;
    }
    a.style.left = `${left * at}px`;
    a.style.top = `${top * at}px`;
    a.style.width = `${width * at}px`;
    a.style.height = `${height * at}px`;
    return a;
  }

  const refused = (error: unknown) => push({ text: error instanceof Error ? error.message : String(error) });

  function follow(node: HTMLElement) {
    const linkOf = (event: MouseEvent) => {
      const link = event.target instanceof Element ? event.target.closest<HTMLAnchorElement>("a[href]") : null;
      return link && node.contains(link) ? link : null;
    };
    const click = (event: MouseEvent) => {
      const link = linkOf(event);
      if (!link) return;
      event.preventDefault();
      const page = link.dataset["pdfPage"];
      if (page !== undefined) node.querySelector(`[data-page="${page}"]`)?.scrollIntoView?.({ block: "start" });
      else if (link.classList.contains("tf-ext")) openExternal(link.getAttribute("href") ?? "").catch(refused);
    };
    // A middle click opens no new webview window: an external link goes to the opener, any other does nothing.
    const auxclick = (event: MouseEvent) => {
      const link = linkOf(event);
      if (!link) return;
      event.preventDefault();
      if (event.button === 1 && link.classList.contains("tf-ext")) openExternal(link.getAttribute("href") ?? "").catch(refused);
    };
    node.addEventListener("click", click);
    node.addEventListener("auxclick", auxclick);
    return () => {
      node.removeEventListener("click", click);
      node.removeEventListener("auxclick", auxclick);
    };
  }
</script>

{#if shown.kind === "reading"}
  <div class="tf-empty"><p>Reading the PDF…</p></div>
{:else if shown.kind === "failed"}
  <div class="tf-fpdf"><div class="tf-stepfail" role="alert"><b>{shown.title}</b><pre>{shown.why}</pre></div></div>
{:else}
  {@const pdf = shown.pdf}
  <div class="tf-fpdf" bind:this={pane}>
    <div class="tf-pdf-bar">
      <span class="tf-dim tf-small">{shown.sizes.length} {shown.sizes.length === 1 ? "page" : "pages"}</span>
      <span class="tf-acts">
        <button type="button" class="tf-btn tf-sm" aria-label="Zoom out" onclick={() => (scale = zoomStep(scale, -1))}>−</button>
        <span class="tf-pdf-zoom" aria-live="polite">{Math.round(scale * 100)}%</span>
        <button type="button" class="tf-btn tf-sm" aria-label="Zoom in" onclick={() => (scale = zoomStep(scale, 1))}>+</button>
        <button type="button" class="tf-btn tf-sm tf-ghost" onclick={() => (scale = fitWidth(widest, pane?.clientWidth ?? 0))}>Fit width</button>
      </span>
    </div>
    <div class="tf-pdf-pages" {@attach follow}>
      {#each shown.sizes as page, i (i)}
        <div class="tf-pdf-page" data-page={i + 1} {@attach pageAt(pdf, i + 1, page.width, page.height)}>
          <canvas aria-label="Page {i + 1}"></canvas>
          <div class="tf-pdf-text"></div>
          <div class="tf-pdf-links"></div>
        </div>
      {/each}
    </div>
  </div>
{/if}
