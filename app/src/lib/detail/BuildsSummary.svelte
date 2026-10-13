<!--
  The builds knobas mirrors for the commit this entity stands at (#710), as
  one line between the view's toolbar and its tabs (#732): a pull request's
  head commit, a branch's tip, a commit itself.

  **Where the question is asked.** Whether the code is green is the next
  thing asked of it, on every tab, so the answer sits above the tabs rather
  than under a whole diff. These rows are a mirror fact (`get_entity`'s
  `builds`, read through the declared `commit` and `revision` paths), not the
  source's view: drawn whether or not the view answered, and where there is no
  view at all the detail draws this line under its header. Gitea's own commit
  statuses are the view's *Checks*; these are knobas' TeamCity builds of the
  same commit.

  Quiet: absent when nothing ran on the commit. A count per state, and the
  only colour a failing count's and a running one's. Clicking the line opens
  the builds in place, in the order the read gave (failing first, then
  running, then the rest), each line opening its build.
-->
<script lang="ts">
  import type { EntityRow, Tone } from "../ipc/entity";
  import EntityLine from "../shell/EntityLine.svelte";
  import { hashFor } from "../shell/router.svelte";

  let {
    builds,
    onopen,
    open = $bindable(false),
  }: {
    builds: EntityRow[];
    /** Navigate to an address (spec §2). */
    onopen: (hash: string) => void;
    /** Whether the list is open; the detail keeps it per entity. */
    open?: boolean;
  } = $props();

  /**
   * The counts the line draws, in the read's own order, with the colour class
   * each one takes (`app.css`): a failure's and a run's, and nothing for the rest.
   */
  const WORDS: { word: string; tones: (Tone | null)[]; cls: string }[] = [
    { word: "failing", tones: ["err"], cls: "fail" },
    { word: "warning", tones: ["warn"], cls: "" },
    { word: "running", tones: ["info"], cls: "link" },
    { word: "passed", tones: ["ok"], cls: "" },
    { word: "other", tones: ["muted", null], cls: "" },
  ];

  const counts = $derived(
    WORDS.map(({ word, tones, cls }) => ({
      word,
      cls,
      n: builds.filter((row) => tones.includes(row.state?.tone ?? null)).length,
    })).filter((count) => count.n > 0),
  );

  const uid = $props.id();

  function openBuild(row: EntityRow) {
    onopen(hashFor({ view: "room", ctx: "all", detail: { kind: row.kind, entityId: row.entity_id } }));
  }
</script>

{#if builds.length > 0}
  <div class="bs">
    <button
      class="bs-line"
      aria-expanded={open}
      aria-controls="bs-list-{uid}"
      onclick={() => (open = !open)}
    >
      <span class="bs-lab">Builds</span>
      {#each counts as count (count.word)}
        <span class="bs-sep" aria-hidden="true">{" · "}</span>
        <span class="bs-n {count.cls}">{count.n} {count.word}</span>
      {/each}
    </button>
    {#if open}
      <div class="bs-list" id="bs-list-{uid}">
        {#each builds as row (row.entity_id)}
          <EntityLine {row} onopen={openBuild} showSource={false} />
        {/each}
      </div>
    {/if}
  </div>
{/if}

<style>
  /*
    One hairline-bounded strip at the toolbar's inset, mono like the tab
    strip under it. Muted until a count is a failure or a run.
  */
  .bs {
    border-bottom: 1px solid var(--hair);
  }

  .bs-line {
    display: flex;
    align-items: center;
    width: 100%;
    padding: 6px 14px;
    font: 400 12px var(--mono);
    color: var(--muted);
    text-align: left;
  }

  .bs-line:hover {
    color: var(--text);
  }

  .bs-line:focus-visible {
    outline-offset: -2px;
  }

  /* The caret is drawn, not said: `aria-expanded` says it. */
  .bs-line::before {
    content: "▸";
    width: 10px;
    margin-right: 6px;
    color: var(--faint);
  }

  .bs-line[aria-expanded="true"]::before {
    content: "▾";
  }

  .bs-lab {
    color: var(--text);
  }

  .bs-sep {
    white-space: pre;
    color: var(--faint);
  }

  .bs-list {
    padding: 0 6px 6px;
  }
</style>
