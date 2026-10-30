<!--
  The one Markdown component (#2255, spec #2234 step 5): every surface that
  draws Markdown draws it with this (#2261). `render/markdown.ts` parses and
  sanitizes; this draws the fragment and wires what it does:

  - a repository-relative link is an ordinary in-app href (`fileLink`), so a
    click opens the file in the Files tab; with no `fileLink` it is text;
  - an external link (`tf-ext`) goes to the opener, never the webview;
  - an in-page `#anchor` scrolls to its heading;
  - a fence's *Copy* copies its code;
  - fences are drawn by the renderer registry, highlighted where no renderer
    claims them, and stopped when the text changes or the component goes.
-->
<script lang="ts">
  import { drawFences, fenceCode, markdownFragment } from "../../render/markdown";
  import { renderPort, type RenderPort } from "../../render/port";
  import { openExternal as realOpenExternal } from "../../shell/open-external";
  import { push } from "../../shell/toasts.svelte";

  let {
    text,
    base = "",
    fileLink = null,
    openExternal = realOpenExternal,
    copy = (code: string) => navigator.clipboard.writeText(code),
    render = renderPort(),
  }: {
    /** The Markdown. */
    text: string;
    /** The repository directory the text's file sits in, for relative links; `""` the top. */
    base?: string;
    /** The href that opens a repository file (at a line); `null` where the surface has no repository. */
    fileLink?: ((path: string, line: number | null) => string) | null;
    /** Hands an external link to the browser. */
    openExternal?: (url: string) => Promise<void>;
    /** A fence's *Copy*. */
    copy?: (code: string) => Promise<void>;
    /** The engines fences are drawn with (#2254): the app's workers, a fake in tests. */
    render?: RenderPort;
  } = $props();

  /** Draws the text; again whenever it, its base or its links change. */
  function paint(node: HTMLElement) {
    node.replaceChildren(markdownFragment(text, { base, fileLink }));
    return drawFences(node, render);
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
    node.addEventListener("click", click);
    return () => node.removeEventListener("click", click);
  }
</script>

<div class="tf-md" {@attach paint} {@attach follow}></div>
