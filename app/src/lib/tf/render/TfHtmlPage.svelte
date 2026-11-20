<!--
  A page (HTML) drawn as ADR-0044 draws a folder renderer's: sanitized by
  DOMPurify (`frameDocument`: no script, form, frame, object or embed, the
  page-scrolls layer), kept by the backend (`folder_render_frame`) and served
  from `knobas-render://localhost/folder.<token>` under the policy with no
  script and no request, in a frame with `sandbox=""`. Without the app's
  bridge (dev, tests) the sanitized page is the frame's own document, still
  sandboxed. The frame takes its room down to the pane's bottom and lets ⌘ or
  Ctrl and the wheel through to the window's zoom.

  `TfFolderRender`'s HTML drawing, and the built-in *Page* renderer's
  (ADR-0046). A page's relative styles, images and links do not load: it
  carries its own.
-->
<script lang="ts">
  import { tauriBridge } from "../../render/command";
  import { FRAME_SCHEME, frameDocument, type FrameBridge } from "../../render/frame";
  import { fitRoom, zoomThrough } from "../../render/frame-room";

  let {
    html,
    title,
    frame,
    room,
    bridge = tauriBridge,
    extra = "",
  }: {
    /** The page's source. */
    html: string;
    /** The frame's title, for assistive technology. */
    title: string;
    /** Keep the sanitized document, answering the token it is served by. */
    frame: (document: string) => Promise<string>;
    /** The pane the frame takes its room in (`fitRoom`). */
    room: string;
    bridge?: FrameBridge;
    /** More classes for the frame. */
    extra?: string;
  } = $props();

  const page = $derived(frameDocument("html", html));
  const local = $derived(!bridge.isTauri());
  let src = $state<string | undefined>(undefined);

  $effect(() => {
    const doc = page;
    if (local) return;
    let live = true;
    void frame(doc).then((token) => {
      if (live) src = bridge.convertFileSrc(`folder.${token}`, FRAME_SCHEME);
    });
    return () => {
      live = false;
    };
  });
</script>

{#if local}
  <iframe class="tf-rframe {extra}" sandbox="" {title} srcdoc={page} {@attach fitRoom(room)} {@attach zoomThrough}></iframe>
{:else if src !== undefined}
  <iframe class="tf-rframe {extra}" sandbox="" {title} {src} {@attach fitRoom(room)} {@attach zoomThrough}></iframe>
{/if}
