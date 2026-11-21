<!--
  Settings › Appearance › Theme's preview (ADR-0055, the decisions' 3.11): a small knobas made of the app's own parts,
  static, drawn in the theme under edit whatever the window wears. The stage carries the theme's colours as plain
  tokens (`scopeValues`), so it draws a theme no slot holds and one of the other side. Its code colours are the
  theme's own where it has them, else those drawn for that side.

  Every part names the colours it is drawn in, and what it stands on, in `data-keys`, nearest first: what *Pick from
  the preview* lists (`TfThemeSection.test.svelte.ts` holds that every row's colour is named somewhere).

  **Pick from the preview** (3.11): a toggle over the stage. While on, the pointer is a crosshair and the part under
  it is outlined; a press opens a menu at the pointer listing the colours of that part and of what it stands on,
  nearest first, each with its swatch, name and hex. A pick ends the mode and hands the key to the editor, which
  scrolls to its row and opens its swatch. `Esc` ends the mode and leaves focus on the toggle.
-->
<script lang="ts">
  import { untrack } from "svelte";

  import { layers } from "../layers.svelte";
  import type { Colours } from "../theme/app-derive";
  import { isAppKey, NAME, type AppKey } from "../theme/app-roles";
  import { cssVars } from "../theme/paint";
  import type { Theme } from "../theme/themes";
  import { scopeValues } from "../themes.svelte";
  import TfMenu from "../TfMenu.svelte";

  let { theme, code, colours, note, onpick }: { theme: Theme; code: Theme; colours: Colours; note: string; onpick?: (key: AppKey) => void } = $props();

  let picking = $state(false);
  let toggle = $state<HTMLButtonElement | null>(null);
  let menuOpen = $state(false);
  let at = $state<{ x: number; y: number } | null>(null);
  let offered = $state<AppKey[]>([]);
  let hit: Element | null = null;

  function mark(next: Element | null): void {
    if (hit === next) return;
    hit?.classList.remove("tf-apt-hit");
    hit = next;
    hit?.classList.add("tf-apt-hit");
  }

  function end(focus: boolean): void {
    picking = false;
    menuOpen = false;
    mark(null);
    if (focus) toggle?.focus();
  }

  $effect(() => {
    if (!picking) return;
    return untrack(() => layers.add({ kind: "panel", close: () => end(true) }));
  });

  /** The colours of the part under `target` and of what it stands on, nearest first, each once. */
  export function keysAt(target: Element, stage: Element): AppKey[] {
    const out: AppKey[] = [];
    for (let el: Element | null = target; el && stage.contains(el); el = el.parentElement)
      for (const key of el.getAttribute("data-keys")?.split(" ") ?? []) if (isAppKey(key) && !out.includes(key) && colours[key as keyof Colours] !== undefined) out.push(key);
    return out;
  }

  function moved(event: PointerEvent): void {
    if (!picking || menuOpen) return;
    const stage = event.currentTarget as Element;
    mark((event.target as Element).closest("[data-keys]"));
    if (hit && !stage.contains(hit)) mark(null);
  }

  function pressed(event: MouseEvent): void {
    if (!picking) return;
    event.preventDefault();
    event.stopPropagation();
    offered = keysAt(event.target as Element, event.currentTarget as Element);
    at = { x: event.clientX, y: event.clientY };
    menuOpen = offered.length > 0;
  }

  function pick(key: AppKey): void {
    end(false);
    onpick?.(key);
  }
</script>

<div class="tf-apt-pv">
  <div class="tf-apt-pvh">
    {#if onpick}
      <button type="button" class="tf-btn tf-sm" aria-pressed={picking} bind:this={toggle} onclick={() => (picking ? end(false) : (picking = true))}>Pick from the preview</button>
    {/if}
    <span class="tf-dim">{note}</span>
  </div>
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions: the pick is a pointer's way in; every colour is also a row of the list -->
  <div
    class="tf-apt-stage"
    class:tf-apt-picking={picking}
    data-keys="paper"
    onpointermove={moved}
    onpointerleave={() => mark(null)}
    onclickcapture={pressed}
    {@attach cssVars(() => scopeValues(theme, code, colours))}
  >
    <div class="tf-apt-bar" data-keys="sheet rule">
      <span class="tf-apt-nav" aria-current="page" data-keys="ink sheet2">Tickets</span><span class="tf-apt-nav" data-keys="ink2 sheet">Code</span><span class="tf-apt-nav" data-keys="ink2 sheet">Builds</span><span class="tf-apt-nav" data-keys="ink2 sheet">Docs</span>
      <span class="tf-apt-find" data-keys="muted field rule">Search everything</span>
      <span class="tf-btn tf-sm tf-primary" data-keys="onPrimary primary">Create ticket</span>
    </div>
    <div class="tf-apt-grid">
      <div class="tf-col">
        <h3 data-keys="ink2 paper">In progress <span class="tf-count">2</span></h3>
        <div class="tf-stack">
          <div class="tf-cards">
            <div class="tf-tcard tf-card tf-sel" data-keys="card cardInk">
              <div class="tf-thead"><span class="tf-k" data-keys="cardInk card">PAY-231</span><span class="tf-tkind" data-keys="cardInk2 card card2">Story</span></div>
              <span class="tf-s" data-keys="cardInk card">Settle the batch before the ledger closes</span>
              <div class="tf-meta">
                <span class="tf-chip tf-warn" data-keys="cardWarn cardWarnBg card">Stale</span><span class="tf-lab" data-keys="cardInk card">backend</span><span class="tf-av tf-me" data-keys="sheet ink">BF</span>
              </div>
            </div>
            <div class="tf-tcard" data-keys="sheet rule">
              <div class="tf-thead"><span class="tf-k" data-keys="ink sheet">PAY-240</span><span class="tf-tkind tf-dim" data-keys="ink2 sheet">Bug</span></div>
              <span class="tf-s" data-keys="ink sheet">Retry queue drops the last receipt</span>
              <div class="tf-meta">
                <span class="tf-chip tf-err" data-keys="err errBg sheet">Failed</span><span class="tf-chip tf-warn" data-keys="warn warnBg sheet">Slow</span><span class="tf-lab" data-keys="ink2 rule sheet">retry</span><span class="tf-av" data-keys="ink2 sheet2">MK</span>
              </div>
            </div>
          </div>
        </div>
      </div>
      <div class="tf-apt-right">
        <div class="tf-apt-rail">
          <span class="tf-apt-nav" aria-current="true" data-keys="onFill fill onInkOk">All tickets</span><span class="tf-apt-nav" data-keys="ink paper">Mine</span><span class="tf-apt-nav" data-keys="ink paper">This sprint</span>
          <span class="tf-seg tf-apt-segr" data-keys="fill onFill rule"><button type="button" tabindex="-1" aria-pressed="true">Board</button><button type="button" tabindex="-1" aria-pressed="false">List</button></span>
        </div>
        <table class="tf-tbl" data-keys="sheet rule2">
          <thead><tr><th data-keys="ink2 sheet2">Key</th><th data-keys="ink2 sheet2">Title</th><th data-keys="ink2 sheet2">Build</th><th data-keys="ink2 sheet2">Updated</th></tr></thead>
          <tbody>
            <tr class="tf-row">
              <td class="tf-keycell"><span class="tf-mono" data-keys="ink sheet">PAY-244</span></td>
              <td class="tf-apt-t" data-keys="ink sheet">Ledger export skips <mark class="tf-apt-mark" data-keys="mark ink">void</mark>ed rows</td>
              <td><span class="tf-chip tf-ok" data-keys="ok okBg sheet">Passed</span></td>
              <td class="tf-dim tf-num" data-keys="ink2 sheet">10:05</td>
            </tr>
            <tr class="tf-row tf-apt-hover">
              <td class="tf-keycell" data-keys="sheet2"><span class="tf-mono" data-keys="ink sheet2">PAY-243</span></td>
              <td class="tf-apt-t" data-keys="ink sheet2">Batch window honours the bank holiday list</td>
              <td data-keys="sheet2"><span class="tf-chip tf-info" data-keys="info infoBg sheet2">Running</span></td>
              <td class="tf-dim tf-num" data-keys="ink2 sheet2">yesterday</td>
            </tr>
            <tr class="tf-row">
              <td class="tf-keycell"><span class="tf-mono" data-keys="ink sheet">PAY-239</span></td>
              <td class="tf-apt-t" data-keys="ink sheet">Receipt PDF shows the wrong currency sign</td>
              <td><span class="tf-chip tf-muted" data-keys="muted mutedBg sheet">Unknown</span></td>
              <td class="tf-dim tf-num" data-keys="ink2 sheet">Wed 14:32</td>
            </tr>
          </tbody>
        </table>
        <div class="tf-diff tf-apt-diff" data-keys="sheet rule">
          <div class="tf-ln"><span class="tf-no">41</span><span>  <span class="tf-hl-kw">while</span> (tries &lt; <span class="tf-hl-con">MAX_TRIES</span>) {"{"}</span></div>
          <div class="tf-ln tf-del" data-keys="errBg ink"><span class="tf-no">42</span><span>    <span class="tf-hl-fn">send</span>(batch, <span class="tf-dw" data-keys="errWord ink">250</span>);</span></div>
          <div class="tf-ln tf-add" data-keys="okBg ink"><span class="tf-no">42</span><span>    <span class="tf-hl-fn">send</span>(batch, <span class="tf-aw" data-keys="okWord ink">backoff(tries)</span>);  <span class="tf-hl-com">// retry later</span></span></div>
          <div class="tf-ln"><span class="tf-no">43</span><span>    <span class="tf-hl-ty">Receipt</span> r = <span class="tf-hl-fn">settle</span>(<span class="tf-hl-str">"retry {"{}"}"</span>);</span></div>
        </div>
      </div>
    </div>
    <div class="tf-apt-low">
      <div class="tf-dialog tf-apt-dialog" data-keys="sheet rule">
        <header><h2 data-keys="ink sheet">Log time on PAY-231</h2></header>
        <div class="tf-apt-dbody">
          <span class="tf-dim" data-keys="ink2 sheet">Today, 1h 20m tracked. See the <span class="tf-apt-link" data-keys="link sheet">worklog</span>. <span class="tf-apt-sel" data-keys="selection ink">Selected</span> words.</span>
          <span class="tf-apt-field tf-apt-focused" data-keys="focus ink field rule">1h 20m</span>
        </div>
        <div class="tf-apt-dfoot">
          <span class="tf-btn tf-sm tf-ghost" data-keys="ink sheet">Cancel</span><span class="tf-btn tf-sm tf-primary" data-keys="onPrimary primary">Log 1h 20m</span>
        </div>
      </div>
      <div class="tf-apt-float">
        <div class="tf-apt-ln" data-keys="sheet rule">
          <div class="tf-apt-lnq" data-keys="ink sheet">retr</div>
          <div class="tf-apt-lnr tf-on" data-keys="ink sheet2"><span><mark data-keys="mark ink">Retr</mark>y queue drops the last receipt</span><small data-keys="ink2 sheet2">PAY-240</small></div>
          <div class="tf-apt-lnr" data-keys="ink sheet"><span>payout/<mark data-keys="mark ink">retr</mark>y/RetryQueue.java</span><small data-keys="ink2 sheet">payments</small></div>
        </div>
        <div class="tf-toast tf-apt-toast" data-keys="sheet ink"><span>Logged 1h 20m on PAY-231.</span><button type="button" tabindex="-1">Undo</button></div>
      </div>
    </div>
  </div>
</div>
<TfMenu bind:open={menuOpen} opener={toggle} {at} label="Colours here" className="tf-apt-pickmenu" focusItem={false}>
  {#each offered as key (key)}
    <button type="button" role="menuitem" onclick={() => pick(key)}
      ><i {@attach cssVars(() => ({ background: colours[key as keyof Colours] ?? "" }))}></i><span>{NAME[key]}</span><small>{colours[key as keyof Colours]}</small></button
    >
  {/each}
</TfMenu>
