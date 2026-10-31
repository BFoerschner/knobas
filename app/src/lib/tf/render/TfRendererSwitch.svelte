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
  `onmakedefault`. Arrow keys move, `Esc` closes, as *Open in…* does.
-->
<script lang="ts">
  import { untrack } from "svelte";

  import { savedCommand } from "../../render/command";
  import type { Opened } from "../../render/defaults";
  import type { Renderer } from "../../render/registry";
  import { layers as sharedLayers, type Layers } from "../layers.svelte";
  import { settingsHash } from "../settings/settings";

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
  let menu = $state<HTMLDivElement | null>(null);

  /** What a built-in's second line says beyond "built in". */
  const BLURB: Record<string, string> = { source: "highlighted", mermaid: "strict security", d2: "WASM", graphviz: "viz.js WASM" };

  const candidate = $derived.by((): Renderer | null => {
    if (shown.id !== opened.renderer?.id) return shown;
    if (shown.id !== "source") return opened.offered.find((r) => r.id === "source") ?? null;
    return opened.offered.find((r) => r.id !== shown.id) ?? null;
  });

  $effect(() => {
    if (!open) return;
    const off = untrack(() => layers.add({ kind: "dialog", close: () => close() }));
    queueMicrotask(() => menu?.querySelector<HTMLElement>("button.tf-on, button")?.focus());
    const outside = (event: MouseEvent) => {
      const target = event.target as Node | null;
      if (target && !menu?.contains(target) && !face?.contains(target)) open = false;
    };
    window.addEventListener("click", outside, true);
    return () => {
      off();
      window.removeEventListener("click", outside, true);
    };
  });

  function close() {
    open = false;
    face?.focus();
  }

  function onkeydown(event: KeyboardEvent) {
    const items = [...(menu?.querySelectorAll<HTMLElement>("button, a") ?? [])];
    const at = items.indexOf(document.activeElement as HTMLElement);
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      close();
    } else if (event.key === "ArrowDown" && items.length > 0) {
      event.preventDefault();
      items[(at + 1) % items.length]?.focus();
    } else if (event.key === "ArrowUp" && items.length > 0) {
      event.preventDefault();
      items[(at - 1 + items.length) % items.length]?.focus();
    }
  }

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
    ><span class="tf-rsw-lab">Render as</span>{" "}<b>{shown.name}</b><svg width="12" height="12" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.8" aria-hidden="true"
      ><path d="M4 6l4 4 4-4" /></svg
    ></button
  >
  {#if open}
    <!-- svelte-ignore a11y_interactive_supports_focus -->
    <div class="tf-menu" role="menu" tabindex="-1" aria-label="Render as" bind:this={menu} {onkeydown}>
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
          {#if isDefault}{" "}<span class="tf-rsw-tag">DEFAULT FOR {opened.pattern ?? pattern}</span>{:else if command !== null}{" "}<span class="tf-rsw-tag">COMMAND</span>{/if}
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
    </div>
  {/if}
</span>
