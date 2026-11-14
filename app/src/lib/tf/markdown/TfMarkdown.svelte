<!--
  The one Markdown component (#2255, spec #2234 step 5): every surface that
  draws Markdown draws it with this (#2261). `render/markdown.ts` parses and
  sanitizes; this draws the fragment and wires what it does:

  - a repository-relative link is an ordinary in-app href (`fileLink`), so a
    click opens the file in the Files tab; with no `fileLink` it is text;
  - an external link (`tf-ext`) goes to the opener, never the webview;
  - an in-page `#anchor` scrolls to its heading;
  - a fence's *Copy* copies its code;
  - a relative image is read through `image` (the source's byte read) and
    drawn from a blob URL, an SVG sanitized first; one the read refuses is
    its alt text and the reason;
  - an attachment image (`attachment:<id>`, the Jira wiki's `!name!`, #2406)
    is drawn from the data URL `attachments` answers; one it refuses is the
    placeholder `attachments.missing` words, with its link to the opener;
  - where the surface previews files (`attachments.open`, #2415), an
    attachment link (the wiki's `[^name]`) and an attachment image are links
    that open the preview; elsewhere the link is its text;
  - fences are drawn by the renderer registry, highlighted where no renderer
    claims them, and stopped when the text changes or the component goes;
  - a long text (over `LONG` characters) draws its first blocks, then a piece
    of `PIECE` characters a task while the end of what is drawn is near the
    view (`whileNear`), as a Docs page's body does; an in-page anchor draws the
    rest first. Until it is whole the body carries `data-md-held`, so a find
    bar counts its matches as a floor (the render-cost pass, 2026-11-14: a
    300 KB comment built and laid out whole took 0.3 to 0.75 s of a
    quarter-speed main thread, and every fence's colours landing laid all of
    it out again).
-->
<script lang="ts">
  import {
    drawFences,
    fenceCode,
    markdownFragment,
    markdownPieces,
    nodesScope,
    resolveLink,
    type AttachmentImages,
    type MarkdownLinks,
    type Scope,
  } from "../../render/markdown";
  import { whileNear } from "../../shell/near-view";
  import { imageBlob } from "../../render/image";
  import { renderPort, type RenderPort } from "../../render/port";
  import { ipcErrorMessage } from "../../ipc";
  import { openExternal as realOpenExternal } from "../../shell/open-external";
  import { push } from "../../shell/toasts.svelte";

  let {
    text,
    base = "",
    fileLink = null,
    openExternal = realOpenExternal,
    copy = (code: string) => navigator.clipboard.writeText(code),
    render = renderPort(),
    image = null,
    attachments = null,
  }: {
    /** The Markdown. */
    text: string;
    /** The repository directory the text's file sits in, for relative links; `""` the top. */
    base?: string;
    /** The href that opens a repository file (at a line); `null` where the surface has no repository. */
    fileLink?: MarkdownLinks["fileLink"];
    /** Hands an external link to the browser. */
    openExternal?: (url: string) => Promise<void>;
    /** A fence's *Copy*. */
    copy?: (code: string) => Promise<void>;
    /** The engines fences are drawn with (#2254): the app's workers, a fake in tests. */
    render?: RenderPort;
    /**
     * The byte read of a repository file (#2256), for relative images; `null` where the surface has no repository,
     * and such an image loads nothing.
     */
    image?: ((path: string) => Promise<Uint8Array>) | null;
    /** The ticket's attachments, for `attachment:<id>` images (#2406); `null` where the surface has none, and such an image loads nothing. */
    attachments?: AttachmentImages | null;
  } = $props();

  /** A text longer than this, in characters of Markdown, draws in pieces. */
  const LONG = 24_000;
  /** About how many characters of the text each piece of a long text adds. */
  const PIECE = 12_000;

  /** The rest of a long text, drawn now: an in-page anchor needs its heading on the page. */
  let drawRest = () => {};

  /** Draws the text; again whenever it, its base, its links or its image read change. */
  function paint(node: HTMLElement) {
    const stops: (() => void)[] = [];
    const wire = (scope: Scope) => {
      stops.push(drawFences(scope, render));
      if (image !== null) stops.push(drawImages(scope, image));
      if (attachments !== null) stops.push(drawAttachments(scope, attachments));
    };
    const stop = () => stops.forEach((each) => each());
    if (text.length <= LONG) {
      node.replaceChildren(markdownFragment(text, { base, fileLink }));
      wire(node);
      return stop;
    }
    // A long text: parsed whole, drawn a piece of its top-level blocks at a time before an empty mark, whose nearing
    // the view draws the next.
    const pieces = markdownPieces(text, { base, fileLink });
    const mark = document.createElement("div");
    mark.className = "tf-md-more";
    mark.setAttribute("aria-hidden", "true");
    node.replaceChildren(mark);
    node.dataset["mdHeld"] = "";
    const grow = (budget: number): boolean => {
      const piece = pieces.next(budget);
      if (piece !== null) {
        const added = [...piece.childNodes];
        mark.before(piece);
        wire(nodesScope(added));
      }
      if (!pieces.done()) return true;
      mark.remove();
      delete node.dataset["mdHeld"];
      return false;
    };
    grow(PIECE);
    const stopNear = pieces.done() ? null : whileNear(() => grow(PIECE))(mark);
    drawRest = () => {
      if (!pieces.done()) grow(Infinity);
    };
    return () => {
      stopNear?.();
      drawRest = () => {};
      stop();
    };
  }

  /**
   * Fills each attachment image from the surface's read, which must answer a
   * `data:image/…` URL; anything else, or a refusal, makes the image the
   * placeholder the surface words. Returns how to stop: answers after it are dropped.
   */
  function drawAttachments(node: Scope, read: AttachmentImages) {
    let live = true;
    const watches: (() => void)[] = [];
    if (read.open) for (const link of node.querySelectorAll<HTMLAnchorElement>("a[data-md-attachment]")) link.setAttribute("href", "#");
    for (const img of node.querySelectorAll<HTMLImageElement>("img[data-md-attachment]")) {
      const id = img.dataset["mdAttachment"] ?? "";
      // Where the surface previews, the image is a link to the preview, named by the image's alt (#2415).
      const holder = read.open && img.closest("a") === null ? previewLink(img, id) : img;
      if (read.watch) watches.push(read.watch(img, id));
      const fail = (error: unknown) => {
        if (!live) return;
        const said = read.missing(img.alt, error);
        const missing = document.createElement("span");
        missing.className = "tf-md-noimg tf-md-attnone";
        missing.append(said.text);
        const to = said.link === null ? null : resolveLink(said.link.href, "");
        if (said.link !== null && to?.kind === "external") {
          const link = document.createElement("a");
          link.className = "tf-ext";
          link.rel = "noreferrer";
          link.href = to.url;
          link.textContent = said.link.label;
          missing.append(" ", link);
        }
        holder.replaceWith(missing);
      };
      read.load(id).then((url) => {
        if (!live) return;
        if (/^data:image\//i.test(url)) img.src = url;
        else fail(new Error("not an image"));
      }, fail);
    }
    return () => {
      live = false;
      for (const stop of watches) stop();
    };
  }

  /** Puts `img` inside a link to the preview of attachment `id`, and answers the link. */
  function previewLink(img: HTMLImageElement, id: string): HTMLAnchorElement {
    const link = document.createElement("a");
    link.href = "#";
    link.className = "tf-md-attlink";
    link.dataset["mdAttachment"] = id;
    img.replaceWith(link);
    link.append(img);
    return link;
  }

  /**
   * Fills each relative image from the byte read, as a blob URL; one the read
   * refuses becomes its alt text and the reason. Returns how to stop: answers
   * after it are dropped, and every URL made is revoked.
   */
  function drawImages(node: Scope, read: (path: string) => Promise<Uint8Array>) {
    let live = true;
    const urls: string[] = [];
    for (const img of node.querySelectorAll<HTMLImageElement>("img[data-md-path]")) {
      const path = img.dataset["mdPath"] ?? "";
      read(path).then(
        (bytes) => {
          if (!live) return;
          const url = URL.createObjectURL(imageBlob(bytes, path));
          urls.push(url);
          img.src = url;
        },
        (error: unknown) => {
          if (!live) return;
          const missing = document.createElement("span");
          missing.className = "tf-md-noimg";
          missing.textContent = img.alt === "" ? ipcErrorMessage(error) : `${img.alt}: ${ipcErrorMessage(error)}`;
          img.replaceWith(missing);
        },
      );
    }
    return () => {
      live = false;
      for (const url of urls) URL.revokeObjectURL(url);
    };
  }

  function follow(node: HTMLElement) {
    const click = (event: MouseEvent) => {
      const at = event.target instanceof Element ? event.target : null;
      const button = at?.closest<HTMLElement>(".tf-fence-copy");
      if (button && node.contains(button)) {
        const block = button.closest(".tf-fence");
        if (block) void copy(fenceCode(block)).then(() => push({ text: "Copied." }), () => push({ text: "Could not copy." }));
        return;
      }
      const link = at?.closest<HTMLAnchorElement>("a[href]");
      if (!link || !node.contains(link)) return;
      const anchor = link.dataset["mdAnchor"];
      const file = link.dataset["mdAttachment"];
      if (file !== undefined) {
        event.preventDefault();
        attachments?.open?.(file);
      } else if (anchor !== undefined) {
        event.preventDefault();
        drawRest();
        [...node.querySelectorAll("[id]")].find((el) => el.id === `user-content-${anchor}`)?.scrollIntoView?.({ block: "start" });
      } else if (link.classList.contains("tf-ext")) {
        event.preventDefault();
        openExternal(link.getAttribute("href") ?? "").catch((error: unknown) =>
          push({ text: error instanceof Error ? error.message : String(error) }),
        );
      }
    };
    // A middle click opens no new webview window: an external link goes to the opener, any other does nothing.
    const auxclick = (event: MouseEvent) => {
      const link = event.target instanceof Element ? event.target.closest<HTMLAnchorElement>("a[href]") : null;
      if (!link || !node.contains(link)) return;
      event.preventDefault();
      if (event.button === 1 && link.classList.contains("tf-ext"))
        openExternal(link.getAttribute("href") ?? "").catch((error: unknown) =>
          push({ text: error instanceof Error ? error.message : String(error) }),
        );
    };
    node.addEventListener("click", click);
    node.addEventListener("auxclick", auxclick);
    return () => {
      node.removeEventListener("click", click);
      node.removeEventListener("auxclick", auxclick);
    };
  }
</script>

<div class="tf-md" {@attach paint} {@attach follow}></div>
