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
  - fences are drawn by the renderer registry, highlighted where no renderer
    claims them, and stopped when the text changes or the component goes.
-->
<script lang="ts">
  import {
    drawFences,
    fenceCode,
    markdownFragment,
    resolveLink,
    type AttachmentImages,
    type MarkdownLinks,
  } from "../../render/markdown";
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

  /** Draws the text; again whenever it, its base, its links or its image read change. */
  function paint(node: HTMLElement) {
    node.replaceChildren(markdownFragment(text, { base, fileLink }));
    const stopFences = drawFences(node, render);
    const stopImages = image === null ? () => {} : drawImages(node, image);
    const stopAttachments = attachments === null ? () => {} : drawAttachments(node, attachments);
    return () => {
      stopFences();
      stopImages();
      stopAttachments();
    };
  }

  /**
   * Fills each attachment image from the surface's read, which must answer a
   * `data:image/…` URL; anything else, or a refusal, makes the image the
   * placeholder the surface words. Returns how to stop: answers after it are dropped.
   */
  function drawAttachments(node: HTMLElement, read: AttachmentImages) {
    let live = true;
    const watches: (() => void)[] = [];
    for (const img of node.querySelectorAll<HTMLImageElement>("img[data-md-attachment]")) {
      const id = img.dataset["mdAttachment"] ?? "";
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
        img.replaceWith(missing);
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

  /**
   * Fills each relative image from the byte read, as a blob URL; one the read
   * refuses becomes its alt text and the reason. Returns how to stop: answers
   * after it are dropped, and every URL made is revoked.
   */
  function drawImages(node: HTMLElement, read: (path: string) => Promise<Uint8Array>) {
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
      if (anchor !== undefined) {
        event.preventDefault();
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
