<!--
  A folder drawn by a folder renderer (ux17, ADR-0044): the Code page's Files
  tab draws it in the file's place when the address is `render/<id>/<folder>`,
  and Settings › Renderers' page draws it under *Try it*.

  It asks the backend (`folder_render`) and is answered at once: what is kept
  for the folder as it is now, else the last drawing of it (under a line that
  says it is from an earlier commit), and where the run is. It asks with
  `open` only while it is shown (`start`): a kept or hidden page draws what is
  kept and starts nothing. Each `folder-render:moved` for this folder asks
  again with `look`. *Run again* asks `again`; *Stop* stops the run. Leaving
  drops a run still waiting for a place (`folder_render_leave`).

  The drawing: an image fitted to the pane, with *Fit* and zoom; text in the
  code view, in the code font; Markdown in the Markdown component, its
  relative links opening files at the folder; HTML sanitized (DOMPurify) and
  drawn in a frame with `sandbox=""`, served from the backend's render scheme
  by a token under a policy with no script and no request (`render/frame.ts`).
-->
<script lang="ts" module>
  import {
    folderRender,
    folderRenderFrame,
    folderRenderLeave,
    folderRenderOutput,
    folderRenderStop,
    type FolderAsk,
    type FolderRenderView,
  } from "../../ipc/app";
  import { EVENTS } from "../../ipc";
  import { listen as tauriListen } from "@tauri-apps/api/event";

  /** What the pane reaches the backend through: the IPC, a fake in tests. */
  export interface FolderRenderPorts {
    render(at: FolderAsk): Promise<FolderRenderView>;
    stop(entityId: string, renderer: string, folder: string): Promise<void>;
    leave(entityId: string, renderer: string, folder: string): Promise<void>;
    output(token: string): Promise<Uint8Array>;
    frame(document: string): Promise<string>;
    listen(event: string, handler: (event: { payload: unknown }) => void): Promise<() => void>;
  }

  export const ipcFolderRenderPorts: FolderRenderPorts = {
    render: folderRender,
    stop: folderRenderStop,
    leave: folderRenderLeave,
    output: folderRenderOutput,
    frame: folderRenderFrame,
    listen: (event, handler) => tauriListen(event, handler),
  };

</script>

<script lang="ts">
  import { untrack } from "svelte";

  import { ipcErrorMessage } from "../../ipc";
  import type { FolderRenderer, FolderRenderMoved } from "../../ipc/app";
  import { failureLines, runningLine, tookLabel } from "../../render/folder";
  import { frameDocument, type FrameBridge } from "../../render/frame";
  import { tauriBridge } from "../../render/command";
  import { FRAME_SCHEME } from "../../render/frame";
  import { typedImageBlob } from "../../render/image";
  import type { RenderPort } from "../../render/port";
  import { renderPort } from "../../render/port";
  import { ago } from "../../shell/time";
  import TfMarkdown from "../markdown/TfMarkdown.svelte";
  import { PressHold } from "../press-hold.svelte";

  let {
    entityId,
    renderer,
    renderers = [renderer],
    folder,
    commit = null,
    paths = null,
    start = true,
    changedInside = false,
    switchTo = null,
    fileLink = null,
    title = "",
    ports = ipcFolderRenderPorts,
    bridge = tauriBridge,
    render = renderPort(),
    now = () => new Date(),
  }: {
    /** The repository or branch the folder is read at. */
    entityId: string;
    /** The renderer drawing it. */
    renderer: FolderRenderer;
    /** Every saved renderer that takes the folder, for the switch; `renderer` among them. */
    renderers?: readonly FolderRenderer[];
    /** The folder's path from the top, `""` the top. */
    folder: string;
    /** The commit the page shows; `null` reads at the mirror's tip. */
    commit?: string | null;
    /** The section the source lists every path in, for a repository with no clone. */
    paths?: string | null;
    /** Whether a run may start: the page is shown and a person opened it. */
    start?: boolean;
    /** Files under the folder are changed in the checkout, which the drawing (of the commit) leaves out. */
    changedInside?: boolean;
    /** The address that draws the folder with another renderer; `null` draws no switch. */
    switchTo?: ((id: string) => string) | null;
    /** The href that opens a repository file, for Markdown's relative links. */
    fileLink?: ((to: string, line: number | null) => string) | null;
    /** The repository's name, for the top folder's header. */
    title?: string;
    ports?: FolderRenderPorts;
    bridge?: FrameBridge;
    render?: RenderPort;
    now?: () => Date;
  } = $props();

  let view = $state<FolderRenderView | null>(null);
  let refused = $state<string | null>(null);
  /** What the drawing's token read as: an image's URL, text, or the frame's address. */
  let shown = $state<{ token: string; image?: string; text?: string; frame?: { src?: string; srcdoc?: string } } | null>(null);
  let shownFailed = $state<string | null>(null);
  /** The image's zoom: `null` fits it to the pane. */
  let zoom = $state<number | null>(null);
  /** Moves every second while a tool runs, so its time counts up. */
  let tick = $state(0);
  let askedAt = 0;

  /** *Run again* and *Stop* are one button (focus stays on it), held after a press so a double press acts once. */
  const hold = new PressHold();
  function press() {
    if (hold.held()) return;
    hold.hold();
    if (view?.running) void ports.stop(entityId, renderer.id, folder);
    else answer("again");
  }

  const ask = (kind: FolderAsk["ask"]) =>
    ports.render({ entityId, renderer: renderer.id, folder, commit, paths, ask: kind });

  function answer(kind: FolderAsk["ask"]) {
    const asked = ++askedAt;
    ask(kind).then(
      (next) => {
        if (asked !== askedAt) return;
        view = next;
        refused = null;
      },
      (cause: unknown) => {
        if (asked === askedAt) refused = ipcErrorMessage(cause);
      },
    );
  }

  // The folder, the renderer or the commit changed: ask again, starting a run where the page is shown.
  $effect(() => {
    void [entityId, renderer.id, renderer.command, folder, commit, paths];
    const kind = start ? "open" : "look";
    untrack(() => {
      view = null;
      shown = null;
      zoom = null;
      answer(kind);
    });
  });

  // A page shown again after it was kept hidden opens the render: a run starts where nothing is kept.
  let wasShown = untrack(() => start);
  $effect(() => {
    const now = start;
    if (now && !wasShown) untrack(() => answer("open"));
    wasShown = now;
  });

  // The run moved or ended: ask again.
  $effect(() => {
    const at = { entityId, renderer: renderer.id, folder };
    let live = true;
    let off: (() => void) | null = null;
    void ports
      .listen(EVENTS.folderRender, ({ payload }) => {
        const moved = payload as FolderRenderMoved;
        if (moved.entity_id !== at.entityId || moved.renderer !== at.renderer || moved.folder !== at.folder) return;
        if (moved.running !== null && view !== null) {
          // Progress: the state alone, no ask.
          view = { ...view, running: moved.running };
          return;
        }
        untrack(() => answer("look"));
      })
      .then((unlisten) => {
        if (live) off = unlisten;
        else unlisten();
      });
    return () => {
      live = false;
      off?.();
      void ports.leave(at.entityId, at.renderer, at.folder).catch(() => {});
    };
  });

  // The run's clock.
  $effect(() => {
    if (view?.running?.state !== "running") return;
    const every = setInterval(() => (tick += 1), 1000);
    return () => clearInterval(every);
  });

  // The drawing's bytes, read once per token.
  $effect(() => {
    const drawn = view?.drawn ?? null;
    if (drawn === null) {
      shown = null;
      return;
    }
    if (untrack(() => shown?.token) === drawn.token) return;
    let live = true;
    let made: string | null = null;
    shownFailed = null;
    const kind = drawn.kind;
    ports
      .output(drawn.token)
      .then(async (bytes) => {
        if (!live) return;
        if (kind.kind === "image") {
          made = URL.createObjectURL(typedImageBlob(bytes, kind.mime));
          shown = { token: drawn.token, image: made };
        } else if (kind.kind === "html") {
          const document = frameDocument("html", new TextDecoder().decode(bytes));
          if (!bridge.isTauri()) {
            shown = { token: drawn.token, frame: { srcdoc: document } };
            return;
          }
          const token = await ports.frame(document);
          if (live) shown = { token: drawn.token, frame: { src: bridge.convertFileSrc(`folder.${token}`, FRAME_SCHEME) } };
        } else if (kind.kind === "undrawable") {
          shown = { token: drawn.token };
        } else {
          shown = { token: drawn.token, text: new TextDecoder().decode(bytes) };
        }
      })
      .catch((cause: unknown) => {
        if (live) shownFailed = ipcErrorMessage(cause);
      });
    return () => {
      live = false;
      if (made !== null) URL.revokeObjectURL(made);
    };
  });

  const drawn = $derived(view?.drawn ?? null);
  const running = $derived(view?.running ?? null);
  const failure = $derived(view?.failure ?? null);
  const failed = $derived(failure === null || failure.why === "stopped" ? null : failureLines(failure));
  const stopped = $derived(failure?.why === "stopped");
  const short = (sha: string | null | undefined) => (sha ? sha.slice(0, 7) : null);
  const elapsed = $derived((void tick, running?.state === "running" ? running.elapsed_ms + tick * 1000 : 0));
  const cut = $derived(folder.lastIndexOf("/") + 1);
  /** The text's lines, at most this many drawn: a generator's 64 MB of text is not a page. */
  const LINE_CAP = 20_000;
  const lines = $derived(shown?.text === undefined ? [] : shown.text.split("\n"));
  const others = $derived(renderers.filter((r) => r.id !== renderer.id));

  // Every new token starts fitted.
  $effect(() => {
    void drawn?.token;
    zoom = null;
  });

  const STEPS = [0.25, 0.5, 0.75, 1, 1.5, 2, 3, 4];
  function step(by: 1 | -1, natural: number) {
    const at = zoom ?? natural;
    const next = by > 0 ? STEPS.find((s) => s > at + 0.001) : [...STEPS].reverse().find((s) => s < at - 0.001);
    if (next !== undefined) zoom = next;
  }
  let img = $state<HTMLImageElement | null>(null);
  /** The fitted image's scale, which a first zoom steps from. */
  const fitted = () => (img === null || img.naturalWidth === 0 ? 1 : img.clientWidth / img.naturalWidth);
</script>

<div class="tf-fileview tf-frender">
  <header>
    <span class="tf-p">{#if folder === ""}{title || "The top folder"}{:else}{#if cut > 0}<span class="tf-pdir">{folder.slice(0, cut)}</span>{/if}{folder.slice(cut)}{/if}</span>
    <span class="tf-dim tf-small tf-fmeta" role="status">
      {#if drawn !== null && view?.current}
        Drawn by {renderer.name}{#if short(drawn.commit)}&nbsp;at <span class="tf-mono">{short(drawn.commit)}</span>{/if}, {tookLabel(drawn.took_ms)}{#if drawn.from === "forge"}, read from the source{/if}
      {:else if drawn !== null}
        Drawn at <span class="tf-mono">{short(drawn.commit) ?? "an earlier commit"}</span> {ago(drawn.drawn_at, now())}{#if short(view?.commit)}; the folder is at <span class="tf-mono">{short(view?.commit)}</span> now{/if}
      {:else}
        {renderer.name}
      {/if}
    </span>
    <span class="tf-trail">
      {#if switchTo !== null && others.length > 0}
        <label class="tf-frender-as">
          <span class="tf-dim tf-small">Render with</span>
          <select class="tf-select" value={renderer.id} onchange={(event) => (location.hash = switchTo(event.currentTarget.value))}>
            {#each renderers as one (one.id)}<option value={one.id}>{one.name}</option>{/each}
          </select>
        </label>
      {/if}
      <button type="button" class="tf-btn tf-sm tf-frender-run" disabled={view === null && refused === null} onclick={press}>{running !== null ? "Stop" : "Run again"}</button>
    </span>
  </header>
  {#if refused !== null}
    <div class="tf-fpad"><div class="tf-stepfail" role="alert"><b>Could not ask for {renderer.name}.</b><span>{refused}</span></div></div>
  {:else}
    {#if running !== null}
      <div class="tf-banner tf-info" aria-live="polite">
        <span>{runningLine(renderer.name, running, view?.from ?? null)}</span>
        {#if running.state === "running"}<span class="tf-frender-clock">{tookLabel(elapsed)}</span>{/if}
      </div>
    {/if}
    {#if changedInside && drawn !== null}
      <div class="tf-banner tf-warn"><span>Drawn from the commit: what your checkout changed in this folder is not in it.</span></div>
    {/if}
    {#if failed !== null && failure !== null}
      <div class="tf-fpad">
        <div class="tf-stepfail" role="alert">
          <b>{renderer.name} could not draw this folder.</b>
          <span>{failed.line}</span>
          {#if failed.stderr !== ""}<pre>{failed.stderr}</pre>{/if}
        </div>
      </div>
    {/if}
    {#if drawn !== null}
      <div class="tf-frender-out" class:tf-frender-held={!view?.current}>
        {#if shownFailed !== null}
          <div class="tf-fpad"><div class="tf-stepfail" role="alert"><b>Could not read what {renderer.name} drew.</b><span>{shownFailed}</span></div></div>
        {:else if shown === null || shown.token !== drawn.token}
          <div class="tf-empty"><p>Reading what {renderer.name} drew…</p></div>
        {:else if drawn.kind.kind === "image" && shown.image !== undefined}
          <div class="tf-pdf-bar tf-frender-zoom">
            <div class="tf-seg" role="group" aria-label="Size">
              <button type="button" aria-pressed={zoom === null} onclick={() => (zoom = null)}>Fit</button>
              <button type="button" aria-pressed={zoom === 1} onclick={() => (zoom = 1)}>Actual size</button>
            </div>
            <button type="button" class="tf-iconbtn tf-sm" aria-label="Zoom out" title="Zoom out" onclick={() => step(-1, fitted())}>−</button>
            <span class="tf-pdf-zoom">{zoom === null ? "Fit" : `${Math.round(zoom * 100)}%`}</span>
            <button type="button" class="tf-iconbtn tf-sm" aria-label="Zoom in" title="Zoom in" onclick={() => step(1, fitted())}>+</button>
          </div>
          <div class="tf-frender-img" class:tf-fit={zoom === null}>
            <img
              bind:this={img}
              src={shown.image}
              alt="{renderer.name}'s drawing of {folder || title || 'the top folder'}"
              style:width={zoom === null || img === null ? null : `${Math.round(img.naturalWidth * zoom)}px`}
            />
          </div>
        {:else if drawn.kind.kind === "html" && shown.frame !== undefined}
          <iframe class="tf-rframe tf-frender-frame" sandbox="" title="{renderer.name}'s page" src={shown.frame.src} srcdoc={shown.frame.srcdoc}></iframe>
        {:else if drawn.kind.kind === "markdown" && shown.text !== undefined}
          <div class="tf-rendered"><TfMarkdown text={shown.text} base={folder} {fileLink} {render} /></div>
        {:else if drawn.kind.kind === "text" && shown.text !== undefined}
          <pre class="tf-code">{#each lines.slice(0, LINE_CAP) as text, i (i)}<div class="tf-l"><span>{i + 1}</span><span>{text || " "}</span></div>{/each}</pre>
          {#if lines.length > LINE_CAP}<p class="tf-gonote tf-fpad">Showing the first {LINE_CAP.toLocaleString("en-US")} of {lines.length.toLocaleString("en-US")} lines.</p>{/if}
        {:else}
          <div class="tf-empty"><p>{renderer.name} made {drawn.size.toLocaleString("en-US")} bytes that are not an image, text or a page, so knobas cannot draw them.</p></div>
        {/if}
      </div>
    {:else if view === null}
      <div class="tf-empty"><p>Asking {renderer.name}…</p></div>
    {:else if running === null && failed === null}
      <div class="tf-empty">
        <p>{stopped ? `${renderer.name} was stopped.` : `${renderer.name} has not drawn this folder yet.`}</p>
        <button type="button" class="tf-btn tf-sm tf-primary" onclick={press}>Run {renderer.name}</button>
      </div>
    {/if}
  {/if}
</div>
