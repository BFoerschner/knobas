<!--
  A file's *Rendered* diff (#2279; ux26's stage, `docs/decisions/2026-11-20-a-comparison-is-drawn-rendered-and-may-end-
  at-the-working-tree.md` rulings 2 and 3): the file drawn by its renderer twice, before and after, or the one side an
  added or deleted file has, in three parts that stand at their final heights from the frame the card turns:

  - **the caption row** (`.tf-rcap`, 29 px): each side's name over its half, *before* and *after* each followed by
    where it is read (`main at 4f2a91c`, *working tree*), with the 3 px inset bars; at its right end an image's modes
    (*Side by side | Swipe | Onion*) and *Render as* where more than one renderer can draw the comparison;
  - **the stage** (`.tf-rstage`): its height comes from the kind of renderer alone (320 px for an image, between 360
    and 640 px for a document), never from what lands in it. It is one scroller holding both columns, so they scroll
    together, on the compositor (`--under`). A side not read yet draws skeleton bars in its half, a side that failed
    says why there, and the other side stays;
  - **the foot** (`.tf-rfoot`, 31 px): what is going on (reading, an image's sizes), and *Show all of it*, which lets
    the stage take its content's own height and is the person's press, so it may move what is under the card.

  The view reads the sides (`load`) and keeps them (`sideOf`), each as bytes read at the clone first; a text renderer
  decodes them. Each renderer draws as the file viewer draws it: Markdown through the sanitizer, a diagram, a page and
  a command's output each in its frame, an image as an `<img>` of a blob URL (an SVG sanitized first,
  `render/image.ts`), let go when the stage goes. A PDF is compared page by page (`TfPdfDiff`, #2282) once both sides
  are read.

  **Changed blocks** (ruling 3's second half): in a Markdown comparison each top-level block that holds a changed line
  (`changed`, the diff's own lines) is marked on its own side, a 3 px bar and a tint that fades out (`.tf-rchg`). The
  stage opens scrolled to the first, `n` and `p` step through them (`nextChange`), both columns moving together, and the
  foot counts them. Where the file's lines are cut or not all read, the first are marked and the foot says so.
-->
<script lang="ts">
  import type { Attachment } from "svelte/attachments";
  import { untrack } from "svelte";
  import { folderRenderFrame } from "../../ipc/app";
  import { ipcCommands, type CommandPort } from "../../render/command";
  import type { Opened } from "../../render/defaults";
  import { isDiagramEngine } from "../../render/diagram";
  import { renderPort, type RenderPort } from "../../render/port";
  import { imageBlob } from "../../render/image";
  import type { Renderer } from "../../render/registry";
  import TfMarkdown from "../markdown/TfMarkdown.svelte";
  import TfCommandOutput from "../render/TfCommandOutput.svelte";
  import TfDiagram from "../render/TfDiagram.svelte";
  import TfHtmlPage from "../render/TfHtmlPage.svelte";
  import TfPdfDiff from "../render/TfPdfDiff.svelte";
  import TfRendererSwitch from "../render/TfRendererSwitch.svelte";
  import { imageMode as sharedImageMode, type ImageModeChoice } from "./image-mode.svelte";
  import { meetsLines, type ImageMode, type LineRanges, type RenderedPlan, type Side, type SideGot } from "./rendered";

  let {
    plan,
    path,
    sides,
    sideOf,
    load,
    ends = null,
    choices = [],
    opened = null,
    pattern = "",
    onpick = () => {},
    onmakedefault = () => {},
    imageMode = sharedImageMode,
    render = renderPort(),
    commands = ipcCommands,
    frame = folderRenderFrame,
    epoch = 0,
    changed = null,
    partial = false,
  }: {
    plan: RenderedPlan;
    path: string;
    /** The sides to draw: the plan's, less one the reader found absent. */
    sides: readonly Side[];
    sideOf: (side: Side) => SideGot | undefined;
    load: (side: Side) => void;
    /** Where each side is read, after its name in the caption (`main at 4f2a91c`, *working tree*); none, the names alone. */
    ends?: { old: string; new: string } | null;
    /** The renderers that can draw this comparison: *Render as* is offered where there are two or more. */
    choices?: readonly Renderer[];
    /** What takes the file and what it opens with, for *Render as*'s menu. */
    opened?: Opened | null;
    /** The pattern *Use … by default* writes. */
    pattern?: string;
    onpick?: (renderer: Renderer) => void;
    onmakedefault?: (renderer: Renderer) => void;
    /** An image's mode, one choice app-wide for the visit. */
    imageMode?: ImageModeChoice;
    render?: RenderPort;
    commands?: CommandPort;
    /** Keeps a page's sanitized document for its frame (`TfHtmlPage`): the IPC, a fake in tests. */
    frame?: (document: string) => Promise<string>;
    /** Counts the working tree's states seen: each new one asks the sides again; what is drawn stays until they land. */
    epoch?: number;
    /** The lines the change touched on each side, for a Markdown comparison's changed blocks; none, nothing is marked. */
    changed?: { old: LineRanges; new: LineRanges } | null;
    /** The lines are cut or not all read: the first changes are marked, and the foot says so. */
    partial?: boolean;
  } = $props();

  // ---- Changed blocks (ux26, ruling 3) ----

  let stage = $state<HTMLElement | null>(null);
  /** The changes as the stage steps through them: each marked block's top in the stage, ones level with another merged. */
  let changes = $state.raw<number[]>([]);
  /** The change `n` or `p` is on, from 0; -1 before the first is landed on. */
  let at = $state(-1);
  /** The stage was put at the first change once, as it opened. */
  let landed = false;
  const marksOn = $derived(changed !== null && plan.renderer.id === "markdown");
  /** Reads where the marked blocks stand, in the stage's own scroll. */
  function readChanges() {
    const box = stage;
    if (box === null) return;
    const top = box.getBoundingClientRect().top - box.scrollTop;
    const tops = [...box.querySelectorAll<HTMLElement>(".tf-rchg")].map((el) => Math.round(el.getBoundingClientRect().top - top)).sort((a, b) => a - b);
    const merged: number[] = [];
    for (const t of tops) if (merged.length === 0 || t - merged.at(-1)! > 8) merged.push(t);
    if (merged.length !== changes.length || merged.some((t, n) => t !== changes[n])) changes = merged;
  }
  /** Marks a side's changed blocks as it is drawn, and again as a long text draws its later pieces. */
  const marking = (side: Side): Attachment<HTMLElement> => (node) => {
    const ranges = changed?.[side] ?? [];
    if (!marksOn) return;
    const apply = () => {
      for (const block of node.querySelectorAll<HTMLElement>(".tf-md > [data-md-lines]")) {
        const span = /^(\d+)-(\d+)$/.exec(block.dataset["mdLines"] ?? "");
        const on = span !== null && meetsLines(Number(span[1]), Number(span[2]), ranges);
        if (block.classList.contains("tf-rchg") !== on) block.classList.toggle("tf-rchg", on);
      }
      readChanges();
      if (!landed && changes.length > 0 && stage !== null && !open) {
        landed = true;
        at = 0;
        stage.scrollTop = Math.max(0, changes[0]! - 12);
      }
    };
    // Read nothing it writes: the observer, not the attachment, runs it again.
    untrack(apply);
    const watch = new MutationObserver(apply);
    watch.observe(node, { childList: true, subtree: true });
    return () => watch.disconnect();
  };
  /**
   * `n` (`step` 1) and `p` (-1): the stage scrolls to the next or previous change, both columns with it; false when there
   * is none that way. `entering`: from another file, so its first going down, its last going up.
   */
  export function nextChange(step: 1 | -1, entering = false): boolean {
    readChanges();
    const to = entering ? (step === 1 ? 0 : changes.length - 1) : at + step;
    if (!marksOn || to < 0 || to >= changes.length || stage === null) return false;
    at = to;
    if (open) scrollPageTo(changes[to]!);
    else stage.scrollTop = Math.max(0, changes[to]! - 12);
    return true;
  }
  /** With the stage shown whole, the page scrolls to the change instead, under the band. */
  function scrollPageTo(top: number) {
    const box = stage;
    const page = box?.closest<HTMLElement>(".tf-main, .tf-bmain");
    if (!box || !page) return;
    page.scrollTop += box.getBoundingClientRect().top + top - page.getBoundingClientRect().top - 120;
  }

  $effect(() => {
    void epoch;
    for (const side of plan.sides) load(side);
  });

  const NAME = { old: "before", new: "after" } as const;
  const id = $derived(plan.renderer.id);
  const image = $derived(id === "image");
  /** An image changed on both sides has three modes; one side has none. */
  const twoImages = $derived(image && sides.length === 2);
  const shownMode = $derived<ImageMode>(twoImages ? imageMode.current : "sides");
  const overlaid = $derived(shownMode !== "sides");
  /** The columns the stage draws: one for a PDF (its own two), a swipe or an onion, else one a side. */
  const columns = $derived(id === "pdf" || overlaid ? 1 : sides.length);

  /** A side's bytes once read, `null` where there is no file on that side; `undefined` while it is read. */
  const bytesOf = (side: Side): Uint8Array | null | undefined => {
    const got = sideOf(side);
    if (got === undefined || got.kind === "failed") return undefined;
    if (got.kind === "absent") return null;
    return typeof got.content === "string" ? new TextEncoder().encode(got.content) : got.content;
  };
  const decoder = new TextDecoder("utf-8", { fatal: false });
  const textOf = (content: string | Uint8Array) => (typeof content === "string" ? content : decoder.decode(content));

  /** Each image side's blob URL, made once its bytes are in and revoked with the stage or a new read. */
  let urls = $state<Partial<Record<Side, string>>>({});
  $effect(() => {
    if (!image) return;
    const made: Partial<Record<Side, string>> = {};
    for (const side of plan.sides) {
      const got = sideOf(side);
      if (got?.kind !== "drawn") continue;
      const bytes = typeof got.content === "string" ? new TextEncoder().encode(got.content) : got.content;
      made[side] = URL.createObjectURL(imageBlob(bytes, path));
    }
    urls = made;
    return () => Object.values(made).forEach((url) => URL.revokeObjectURL(url!));
  });
  /** An image's size once it loaded, by side. */
  let sizes = $state<Partial<Record<Side, string>>>({});
  const measure = (side: Side) => (event: Event) => {
    const img = event.currentTarget as HTMLImageElement;
    if (img.naturalWidth > 0) sizes = { ...sizes, [side]: `${img.naturalWidth} × ${img.naturalHeight}` };
  };

  /** Where the swipe cuts, or how opaque the onion's after side is, in percent. */
  let cut = $state(50);
  /** Through the CSSOM, as the clip and the opacity change: the CSP takes no inline style. */
  const over = (mode: ImageMode, at: number): Attachment<HTMLElement> => (node) => {
    if (mode === "onion") {
      node.style.removeProperty("clip-path");
      node.style.setProperty("opacity", String(at / 100));
    } else {
      node.style.removeProperty("opacity");
      node.style.setProperty("clip-path", `inset(0 0 0 ${at}%)`);
    }
  };

  /** Every side drawn has answered, drawn or failed: *Show all of it* can be pressed. */
  const settled = $derived(sides.every((side) => sideOf(side) !== undefined));
  let open = $state(false);

  const failure = (side: Side) => {
    const got = sideOf(side);
    return got?.kind === "failed" ? got.why : null;
  };
  /** What the foot says: the read that is still out, or an image's sizes. */
  const status = $derived.by(() => {
    const unread = sides.filter((side) => sideOf(side) === undefined);
    if (unread.length === 2) return "Reading both sides…";
    if (unread[0] === "old") return "Reading the side before…";
    if (unread[0] === "new") return ends?.new === "working tree" ? "Reading the working tree's side…" : "Reading the side after…";
    if (image) return sides.flatMap((side) => (sizes[side] ? [`${NAME[side]} ${sizes[side]}`] : [])).join(", ");
    if (marksOn && changes.length > 0) {
      const count = `Change ${Math.max(1, at + 1)} of ${changes.length}`;
      return partial ? `${count}. The first changes are marked: Lines has them all.` : count;
    }
    return "";
  });
  const caption = (side: Side) => (ends ? `${NAME[side]} ${ends[side]}` : NAME[side]);
  const IMAGE_MODES: { mode: ImageMode; label: string }[] = [
    { mode: "sides", label: "Side by side" },
    { mode: "swipe", label: "Swipe" },
    { mode: "onion", label: "Onion" },
  ];
</script>

{#snippet drawn(side: Side, content: string | Uint8Array)}
  {#if image}
    {#if urls[side]}<img src={urls[side]} alt="{path} {NAME[side]}" onload={measure(side)} />{/if}
  {:else if id === "markdown"}
    <TfMarkdown text={textOf(content)} {render} lines={marksOn} />
  {:else if id === "html"}
    <TfHtmlPage html={textOf(content)} title="{path} {NAME[side]}, drawn as a page" {frame} room=".tf-rstage" extra="tf-rstage-frame" />
  {:else if isDiagramEngine(id)}
    <TfDiagram engine={id} code={textOf(content)} {render} />
  {:else}
    <TfCommandOutput renderer={plan.renderer} text={textOf(content)} {commands} />
  {/if}
{/snippet}

{#snippet waiting(side: Side)}
  <div class="tf-skel tf-skel-lines tf-rwait" aria-busy="true" aria-label="Reading {path} {NAME[side]}">
    <span class="tf-skel-bar"></span><span class="tf-skel-bar"></span><span class="tf-skel-bar"></span>
  </div>
{/snippet}

<div class="tf-rcap" class:tf-rcap-one={columns === 1}>
  {#if columns === 1 && sides.length === 2}
    <span class="tf-rcap-side tf-rcap-both" title="{caption('old')} · {caption('new')}"><b>before</b> {ends?.old ?? ""} <b class="tf-rcap-to">after</b> {ends?.new ?? ""}</span>
  {:else}
    {#each sides as side (side)}
      <span class="tf-rcap-side tf-rcap-{side}" title={caption(side)}><b>{NAME[side]}</b>{#if ends}{" "}{ends[side]}{/if}</span>
    {/each}
  {/if}
  {#if twoImages || choices.length > 1}
    <span class="tf-rcap-acts">
      {#if twoImages}
        <span class="tf-seg" role="group" aria-label="Show {path}'s two images">
          {#each IMAGE_MODES as m (m.mode)}<button type="button" aria-pressed={shownMode === m.mode} onclick={() => imageMode.set(m.mode)}>{m.label}</button>{/each}
        </span>
      {/if}
      {#if choices.length > 1 && opened !== null}
        <TfRendererSwitch opened={{ ...opened, offered: [...choices] }} shown={plan.renderer} {pattern} {onpick} {onmakedefault} />
      {/if}
    </span>
  {/if}
</div>
<div class="tf-rstage" bind:this={stage} class:tf-rstage-img={image} class:tf-rstage-one={columns === 1} class:tf-rstage-open={open} data-renderer={id}>
  {#if id === "pdf"}
    {@const failed = sides.find((side) => failure(side) !== null)}
    {@const [old, nu] = [plan.sides.includes("old") ? bytesOf("old") : null, plan.sides.includes("new") ? bytesOf("new") : null]}
    <div class="tf-rside">
      {#if failed !== undefined}
        <p class="tf-stepfail" role="alert">The side {failed === "old" ? "before" : "after"} could not be read: {failure(failed)}</p>
      {:else if old !== undefined && nu !== undefined}
        <TfPdfDiff before={old} after={nu} {path} {render} />
      {:else}
        {@render waiting(sides[0] ?? "new")}
      {/if}
    </div>
  {:else if overlaid}
    {@const failed = sides.find((side) => failure(side) !== null)}
    <div class="tf-rside tf-rside-over">
      {#if failed !== undefined}
        <p class="tf-stepfail" role="alert">The side {failed === "old" ? "before" : "after"} could not be read: {failure(failed)}</p>
      {:else if urls.old && urls.new}
        <div class="tf-swipe">
          <div class="tf-swipe-box">
            <img src={urls.old} alt="{path} before" onload={measure("old")} />
            <div class="tf-swipe-after" class:tf-onion={shownMode === "onion"} {@attach over(shownMode, cut)}><img src={urls.new} alt="{path} after" onload={measure("new")} /></div>
          </div>
          <label class="tf-swipe-bar"
            ><span>before</span><input
              type="range"
              min="0"
              max="100"
              bind:value={cut}
              aria-label={shownMode === "onion" ? `How much of ${path} after shows over before` : `Swipe between ${path} before and after`}
            /><span>after</span></label
          >
        </div>
      {:else}
        {@render waiting("old")}
      {/if}
    </div>
  {:else}
    {#each sides as side (side)}
      {@const got = sideOf(side)}
      <div class="tf-rside tf-rside-{side}" class:tf-rside-doc={!image} class:tf-rside-img={image} {@attach marking(side)}>
        {#if got?.kind === "drawn"}
          {@render drawn(side, got.content)}
        {:else if got?.kind === "failed"}
          <p class="tf-stepfail" role="alert">The side {side === "old" ? "before" : "after"} could not be read: {got.why}</p>
        {:else}
          {@render waiting(side)}
        {/if}
      </div>
    {/each}
  {/if}
</div>
<div class="tf-rfoot">
  <span class="tf-rfoot-say" aria-live="polite">{status}</span>
  <button
    type="button"
    class="tf-btn tf-sm tf-ghost tf-rfoot-all"
    aria-disabled={settled ? undefined : "true"}
    title={settled ? undefined : "Still drawing"}
    onclick={() => {
      if (settled) open = !open;
    }}>{open ? "Show less" : "Show all of it"}</button
  >
</div>
