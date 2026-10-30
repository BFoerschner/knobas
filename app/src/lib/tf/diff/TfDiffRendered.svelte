<!--
  A file's *Rendered* diff (#2279, spec #2234 step 6e): the file drawn by
  its renderer twice, before and after side by side, or the one side an
  added or deleted file has. An image can also be swiped across: the after
  side over the before, cut where the slider stands. The layout is the
  before/after pane of `mockups/code-area/diff.html`.

  The view reads the sides (`load`) and keeps them (`sideOf`): the texts are
  the ones expanded context reads, so switching back and forth asks nothing
  again. Each renderer draws as the file viewer draws it: Markdown through
  the sanitizer, a diagram and a command's output in a sandboxed frame, an
  image as an `<img>` of a blob URL (an SVG sanitized first, `render/image.ts`),
  let go when the pane goes.
-->
<script lang="ts">
  import { ipcCommands, type CommandPort } from "../../render/command";
  import { isDiagramEngine } from "../../render/diagram";
  import { renderPort, type RenderPort } from "../../render/port";
  import { imageBlob } from "../../render/image";
  import TfMarkdown from "../markdown/TfMarkdown.svelte";
  import TfCommandOutput from "../render/TfCommandOutput.svelte";
  import TfDiagram from "../render/TfDiagram.svelte";
  import type { RenderedPlan, Side, SideGot } from "./rendered";

  let {
    plan,
    path,
    sides,
    swipe,
    sideOf,
    load,
    render = renderPort(),
    commands = ipcCommands,
  }: {
    plan: RenderedPlan;
    path: string;
    /** The sides to draw: the plan's, less one the reader found absent. */
    sides: readonly Side[];
    /** Swiped across instead of side by side (an image changed on both sides). */
    swipe: boolean;
    sideOf: (side: Side) => SideGot | undefined;
    load: (side: Side) => void;
    render?: RenderPort;
    commands?: CommandPort;
  } = $props();

  $effect(() => {
    for (const side of plan.sides) load(side);
  });

  const NAME = { old: "before", new: "after" } as const;
  const id = $derived(plan.renderer.id);
  const textOf = (content: string | Uint8Array) => (typeof content === "string" ? content : new TextDecoder().decode(content));
  /** Each image side's blob URL, made once its bytes are in and revoked with the pane or a new read. */
  let urls = $state<Partial<Record<Side, string>>>({});
  $effect(() => {
    if (id !== "image") return;
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
  /** Where the swipe cuts, in percent from the left: before to its left, after to its right. */
  let cut = $state(50);
  const clip = (node: HTMLElement) => {
    node.style.setProperty("clip-path", `inset(0 0 0 ${cut}%)`);
  };
</script>

{#snippet drawn(side: Side, content: string | Uint8Array)}
  {#if id === "image"}
    {#if urls[side]}<img src={urls[side]} alt="{path} {NAME[side]}" onload={measure(side)} />{/if}
  {:else if id === "markdown"}
    <TfMarkdown text={textOf(content)} {render} />
  {:else if isDiagramEngine(id)}
    <TfDiagram engine={id} code={textOf(content)} {render} />
  {:else}
    <TfCommandOutput renderer={plan.renderer} text={textOf(content)} {commands} />
  {/if}
{/snippet}

{#if swipe}
  <div class="tf-swipe">
    {#if urls.old && urls.new}
      <div class="tf-swipe-box">
        <img src={urls.old} alt="{path} before" />
        <div class="tf-swipe-after" {@attach clip}><img src={urls.new} alt="{path} after" /></div>
      </div>
      <label class="tf-swipe-bar"><span>before</span><input type="range" min="0" max="100" bind:value={cut} aria-label="Swipe between {path} before and after" /><span>after</span></label>
    {:else}
      <p class="tf-dim" aria-live="polite">Reading both sides…</p>
    {/if}
  </div>
{:else}
  <div class="tf-ba" class:tf-ba-one={sides.length === 1}>
    {#each sides as side (side)}
      {@const got = sideOf(side)}
      <figure class="tf-ba-{side}">
        <figcaption>{NAME[side]}{sizes[side] ? ` · ${sizes[side]}` : ""}</figcaption>
        {#if got?.kind === "drawn"}
          <div class="tf-ba-body tf-ba-{id === 'image' ? 'img' : 'doc'}">{@render drawn(side, got.content)}</div>
        {:else if got?.kind === "failed"}
          <p class="tf-stepfail" role="alert">The {NAME[side]} side could not be read: {got.why}</p>
        {:else}
          <p class="tf-dim" aria-live="polite">Reading…</p>
        {/if}
      </figure>
    {/each}
  </div>
{/if}
