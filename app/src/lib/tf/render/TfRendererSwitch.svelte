<!--
  The renderer switcher (#2263, spec #2234 story 43; `mockups/code-area/file.html`,
  state *switcher*): *Render as <name> ▾* in a file view's header. Its menu
  lists every renderer that takes the open file (`render/defaults.ts`'s
  `openWith`), ticks the one drawing it, tags the default for the pattern and
  each command renderer, and offers to make a renderer the default:

  - the one shown, when it is not the default;
  - else Source, when the default is something else (the mockup's
    *Use Source for *.mmd by default*);
  - else, the default being Source, the first other renderer;
  - else (Source takes the file alone) nothing.

  *Renderers and defaults…* goes to Settings › Renderers. Picking a renderer
  only changes this view (`onpick`); the default changes only through
  `onmakedefault`. `TfMenu` draws it.
-->
<script lang="ts">
  import { savedCommand } from "../../render/command";
  import type { Opened } from "../../render/defaults";
  import type { Renderer } from "../../render/registry";
  import { layers as sharedLayers, type Layers } from "../layers.svelte";
  import { settingsHash } from "../settings/settings";
  import TfMenu from "../TfMenu.svelte";

  let {
    opened,
    shown,
    pattern,
    onpick,
    onmakedefault,
    layers = sharedLayers,
  }: {
    /** What takes the file and what it opens with. */
    opened: Opened;
    /** The renderer drawing the file now. */
    shown: Renderer;
    /** The pattern *Make default* writes and the menu names. */
    pattern: string;
    onpick: (renderer: Renderer) => void;
    onmakedefault: (renderer: Renderer) => void;
    layers?: Layers | undefined;
  } = $props();

  let open = $state(false);
  let face = $state<HTMLButtonElement | null>(null);

  /** What a built-in's second line says beyond "built in". */
  const BLURB: Record<string, string> = { source: "highlighted", mermaid: "strict security", d2: "WASM", graphviz: "viz.js WASM" };

  const candidate = $derived.by((): Renderer | null => {
    if (shown.id !== opened.renderer?.id) return shown;
    if (shown.id !== "source") return opened.offered.find((r) => r.id === "source") ?? null;
    return opened.offered.find((r) => r.id !== shown.id) ?? null;
  });

  function pick(renderer: Renderer) {
    open = false;
    onpick(renderer);
  }

  function makeDefault(renderer: Renderer) {
    open = false;
    onmakedefault(renderer);
  }
</script>

<span class="tf-rsw">
  <button
    bind:this={face}
    type="button"
    class="tf-btn tf-sm tf-rsw-face"
    aria-haspopup="menu"
    aria-expanded={open}
    onclick={() => (open = !open)}
    ><span class="tf-rsw-lab">Render as</span>{" "}<b>{shown.name}</b><svg class="tf-caret" width="12" height="12" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.8" aria-hidden="true"
      ><path d="M4 6l4 4 4-4" /></svg
    ></button
  >
  <TfMenu bind:open opener={face} label="Render as" className="tf-rsw-m" focusOn="button.tf-on" {layers}>
      <div class="tf-rsw-mh">Renderers that accept {pattern}</div>
      {#each opened.offered as r (r.id)}
        {@const command = savedCommand(r)}
        {@const isDefault = r.id === opened.renderer?.id}
        <button type="button" role="menuitemradio" aria-checked={r.id === shown.id} class:tf-on={r.id === shown.id} onclick={() => pick(r)}>
          <span class="tf-rsw-tick" aria-hidden="true">{r.id === shown.id ? "✓" : ""}</span>
          <span class="tf-rsw-name"
            >{r.name}{" "}{#if command !== null}<span class="tf-rsw-sub tf-mono">{command.command}</span>{:else}<span class="tf-rsw-sub"
                >built in{BLURB[r.id] ? ` · ${BLURB[r.id]}` : ""}</span
              >{/if}</span
          >
          {#if isDefault}{" "}<span class="tf-rsw-tag">Default for {opened.pattern ?? pattern}</span>{:else if command !== null}{" "}<span class="tf-rsw-tag">COMMAND</span>{/if}
        </button>
      {/each}
      <hr />
      {#if candidate !== null}
        {@const target = candidate}
        <button type="button" role="menuitem" onclick={() => makeDefault(target)}
          ><span class="tf-rsw-tick" aria-hidden="true"></span>Use {target.name} for {pattern} by default</button
        >
      {/if}
      <a role="menuitem" href={settingsHash("renderers")} onclick={() => (open = false)}
        ><span class="tf-rsw-tick" aria-hidden="true"></span>Renderers and defaults…{" "}<span class="tf-rsw-kb">Settings › Renderers</span></a
      >
  </TfMenu>
</span>
