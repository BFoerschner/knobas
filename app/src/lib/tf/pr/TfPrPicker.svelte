<!--
  *Files changed*'s commit picker (#2273, spec #2234 step 6b;
  `mockups/code-area/diff.html`, state *range*): *Commits <choice> base…head ▾*
  in the diff's place above the files. Its menu offers *All commits*, *Since
  your last review* (only to a viewer who submitted one, naming the head
  reviewed and how many commits came since), then the commits: a click picks
  one, a shift-click stretches the pick to a range, as *Commits* picks
  (`diff/address.ts`). The pick has its own address too: *Open as diff*.
  Arrow keys move, `Esc` closes, as the other menus do.
-->
<script lang="ts">
  import { untrack } from "svelte";
  import type { DiffRange } from "../../ipc/entity";
  import { layers as sharedLayers, type Layers } from "../layers.svelte";
  import { diffHash, picked, selectRange } from "../diff/address";
  import { commitsSince, pickedEnds, sinceOffered, type PrPick } from "../diff/pick";

  let {
    range,
    choice,
    onchoose,
    layers = sharedLayers,
  }: {
    range: DiffRange;
    choice: PrPick;
    onchoose: (choice: PrPick) => void;
    layers?: Layers | undefined;
  } = $props();

  let open = $state(false);
  let face = $state<HTMLButtonElement | null>(null);
  let menu = $state<HTMLDivElement | null>(null);

  const short = (sha: string) => sha.replace(/\^$/, "").slice(0, 7) + (sha.endsWith("^") ? "^" : "");
  const ends = $derived(pickedEnds(range, choice) ?? { base: range.base, head: range.head });
  const since = $derived(commitsSince(range));
  const named = $derived.by(() => {
    if (choice.kind === "all") return `All ${range.commits.length}`;
    if (choice.kind === "since") return "Since your last review";
    const { anchor, end } = choice.pick;
    if (anchor === end) return range.commits[anchor]?.title ?? "One commit";
    return `${Math.abs(anchor - end) + 1} commits`;
  });

  $effect(() => {
    if (!open) return;
    const off = untrack(() => layers.add({ kind: "dialog", close: () => close() }));
    queueMicrotask(() => menu?.querySelector<HTMLElement>("[aria-checked=true], button")?.focus());
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

  function choose(next: PrPick) {
    open = false;
    onchoose(next);
  }

  /** A click picks one commit, a shift-click stretches the pick; the menu stays open for the stretch. */
  function pickCommit(at: number, shift: boolean) {
    const next = selectRange(choice.kind === "commits" ? choice.pick : null, at, shift);
    onchoose(next === null ? { kind: "all" } : { kind: "commits", pick: next });
  }
</script>

<span class="tf-pick">
  <button
    bind:this={face}
    type="button"
    class="tf-btn tf-sm tf-pick-face"
    aria-haspopup="menu"
    aria-expanded={open}
    onclick={() => (open = !open)}
    ><span class="tf-pick-lab">Commits</span>{" "}<b>{named}</b>{" "}<span class="tf-mono tf-pick-ends">{short(ends.base)}…{short(ends.head)}</span><svg
      width="12"
      height="12"
      viewBox="0 0 16 16"
      fill="none"
      stroke="currentColor"
      stroke-width="1.8"
      aria-hidden="true"><path d="M4 6l4 4 4-4" /></svg
    ></button
  >
  {#if open}
    <!-- svelte-ignore a11y_interactive_supports_focus -->
    <div class="tf-menu" role="menu" tabindex="-1" aria-label="Commits" bind:this={menu} {onkeydown}>
      <button type="button" role="menuitemradio" aria-checked={choice.kind === "all"} class:tf-on={choice.kind === "all"} onclick={() => choose({ kind: "all" })}>
        <span class="tf-pick-tick" aria-hidden="true">{choice.kind === "all" ? "✓" : ""}</span>All commits{" "}<span class="tf-pick-kb">{range.commits.length}</span>
      </button>
      {#if sinceOffered(range) && range.reviewed}
        <button type="button" role="menuitemradio" aria-checked={choice.kind === "since"} class:tf-on={choice.kind === "since"} onclick={() => choose({ kind: "since" })}>
          <span class="tf-pick-tick" aria-hidden="true">{choice.kind === "since" ? "✓" : ""}</span><span
            >Since your last review{" "}<span class="tf-pick-sub"
              >You reviewed <span class="tf-mono">{short(range.reviewed)}</span>{since === null ? "" : ` · ${since} commit${since === 1 ? "" : "s"} since`}</span
            ></span
          >
        </button>
      {/if}
      {#if range.commits.length > 0}
        <hr />
        <div class="tf-pick-mh">One commit, or click then shift-click for a range</div>
        {#each range.commits as commit, at (commit.sha)}
          {@const on = choice.kind === "commits" && picked(choice.pick, at)}
          <button type="button" role="menuitemcheckbox" aria-checked={on} class="tf-pick-c" class:tf-on={on} onclick={(event) => pickCommit(at, event.shiftKey)}>
            <span class="tf-pick-box" aria-hidden="true">{on ? "✓" : ""}</span>
            <span class="tf-mono">{commit.sha.slice(0, 7)}</span>
            <span class="tf-pick-title">{commit.title}</span>
            {#if commit.meta}<span class="tf-pick-kb">{commit.meta}</span>{/if}
          </button>
        {/each}
      {/if}
      {#if choice.kind !== "all"}
        <div class="tf-pick-foot">
          The pick has its own address: link it, go back to it.{" "}<a role="menuitem" href={diffHash(range.repository, ends.base, ends.head)} onclick={() => (open = false)}>Open as diff</a>
        </div>
      {/if}
    </div>
  {/if}
</span>
