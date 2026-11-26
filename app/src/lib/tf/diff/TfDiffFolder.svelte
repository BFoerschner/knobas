<!--
  A grouped folder's bar in a comparison (ux26, `docs/decisions/2026-11-20-a-comparison-is-drawn-rendered-and-may-end-at-
  the-working-tree.md` ruling 8; the decision list's section 9, the mock's tags 9 and 10): the made bar (`.tf-madebar`),
  one row of at least 42 px that never wraps, whose words and tone change with the two runs and never its box. Left to
  right: the folder's path (at most 40 % of the row), the status (one line, whole in its title), *Render with* where
  more than one renderer takes the folder, *Run* / *Run again* / *Stop* (one button: *Run* runs the sides that want it,
  *Run again* both anew), *Real files*.

  Under it, inset with the info rail, until the cards of what the two made stand there: a note box of exactly 96 px. A
  side that failed says why under the bar, its stderr whole, when the failure is the answer to a press of *Run* here (it
  lands with the answer's frame); a failure kept from before is said in the bar, and *What it said* opens its stderr on
  the person's press, as the made bar's does, so nothing drawn moves when a look answers.
-->
<script lang="ts">
  import type { FolderRenderer, FolderRunState, ReadFrom } from "../../ipc/app";
  import type { CompareState, FolderCompare, SideState } from "../../render/folder-compare.svelte";
  import type { FolderRun } from "../../render/folder-run.svelte";
  import { failureLines, tookLabel } from "../../render/folder";
  import { thousands } from "../counted";
  import TfPath from "../TfPath.svelte";
  import { untrack } from "svelte";

  let {
    folder,
    title = "",
    renderer,
    renderers = [],
    compare,
    lists = false,
    ends,
    onpick = () => {},
    onreal,
  }: {
    /** The grouped folder's path, `""` the top. */
    folder: string;
    /** The repository's name, for the top folder. */
    title?: string;
    renderer: FolderRenderer;
    /** Every saved renderer that takes the folder, for *Render with*. */
    renderers?: readonly FolderRenderer[];
    compare: FolderCompare;
    /** Cards of what the two made stand under the bar (the view lands them in one frame): the note box goes. */
    lists?: boolean;
    /** Each end in words: `main at 4f2a91c`, `your working tree`. */
    ends: { old: string; new: string };
    /** *Render with* chose a renderer: the folder's pick (ruling 10). */
    onpick?: (renderer: string) => void;
    /** *Real files*: the folder's own files back, for the visit. */
    onreal: () => void;
  } = $props();

  const name = $derived(renderer.name);
  const phase = $derived<CompareState>(compare.state);
  const sides = $derived({ old: compare.sideOf("old"), new: compare.sideOf("new") });
  const SIDE = { old: "Before", new: "After" } as const;

  /** A side's run in a few words, without the renderer's name (the bar names it). */
  function sideWords(side: "old" | "new", now: SideState, run: FolderRun): string {
    switch (now) {
      case "kept":
        return "kept";
      case "absent":
        return "no folder";
      case "asking":
        return "asking";
      case "none":
        return "not rendered";
      case "stale":
        return "made from an earlier state";
      case "failed":
        return "failed";
      case "running":
        return runningWords(run.view?.running ?? { state: "queued" }, run.view?.from ?? null, run.elapsed);
    }
    return side;
  }
  const from = (where: ReadFrom | null) => (where === "forge" ? "the source" : where === "working_tree" ? "your working tree" : "your clone");
  function runningWords(running: FolderRunState, where: ReadFrom | null, elapsed: number): string {
    switch (running.state) {
      case "queued":
        return "waiting";
      case "copying":
        return running.total === 0 ? `copying the folder from ${from(where)}…` : `copying ${thousands(running.done)} of ${thousands(running.total)} files from ${from(where)}…`;
      case "running":
        return `running… ${tookLabel(elapsed)}`;
      case "keeping":
        return running.total === 0 ? "keeping what it made…" : `keeping ${thousands(running.done)} of ${thousands(running.total)} files…`;
    }
  }
  /** An end as the status names it on its own. */
  const endName = (side: "old" | "new") => ends[side];
  const files = (count: number) => `${thousands(count)} ${count === 1 ? "file" : "files"}`;

  /** The failed sides' lines, and their stderr for the block under the bar. */
  const failures = $derived(
    (["old", "new"] as const).flatMap((side) => {
      if (sides[side] !== "failed") return [];
      const run = side === "old" ? compare.before : compare.after;
      if (run.refused !== null) return [{ side, line: `Could not ask ${name}: ${run.refused}`, stderr: "" }];
      const failure = run.view?.failure;
      if (failure == null) return [];
      const said = failureLines(failure);
      return [{ side, line: said.line, stderr: said.stderr }];
    }),
  );

  /** The bar's words and its tone. */
  const status = $derived.by((): { text: string; tone: "info" | "warn" | "err" } => {
    switch (phase) {
      case "asking":
        return { text: `Asking ${name}…`, tone: "info" };
      case "idle": {
        const not = (["old", "new"] as const).filter((side) => sides[side] === "none" || sides[side] === "absent");
        const want = not.filter((side) => sides[side] === "none");
        const which = want.length === 2 ? "both sides" : endName(want[0] ?? "new");
        return { text: `Not rendered yet for ${which}.`, tone: "info" };
      }
      case "running":
        return {
          text: `${SIDE.old}: ${ended(sideWords("old", sides.old, compare.before))} ${SIDE.new}: ${ended(sideWords("new", sides.new, compare.after))}`,
          tone: "info",
        };
      case "comparing":
        return { text: "Comparing what it made…", tone: "info" };
      case "failed": {
        if (compare.compareFailed !== null) return { text: `What it made could not be compared: ${compare.compareFailed}`, tone: "err" };
        return { text: failures.map((f) => `${SIDE[f.side]}: ${lowerFirst(f.line)}`).join(" ") || `${name} could not make this folder.`, tone: "err" };
      }
      case "stale": {
        // A side made from an earlier state is never compared (2026-11-24): with what was compared still drawn, the
        // bar warns it is out of date; with nothing drawn, it says why the box is empty. The working tree's words only
        // where its side alone is stale; a commit's side is stale when the tool changed.
        const stale = (["old", "new"] as const).filter((side) => sides[side] === "stale");
        const tree = stale.every((side) => ends[side] === "your working tree");
        if (compare.made !== null) return { text: tree ? "Made from an earlier state of your working tree." : `Made before ${name} changed.`, tone: "warn" };
        return { text: tree ? "Not rendered for your working tree as it is now." : `Not rendered since ${name} changed.`, tone: "info" };
      }
      case "same":
      case "made": {
        const made = compare.made;
        if (made === null) return { text: "", tone: "info" };
        if (made.kind === "single") return { text: made.same ? `${name} drew the same for both sides.` : `${name} drew both sides.`, tone: "info" };
        const c = made.compare;
        const eol = c.eol_only > 0 ? ` ${thousands(c.eol_only)} more ${c.eol_only === 1 ? "differs" : "differ"} only in line endings.` : "";
        if (c.files.length === 0) return { text: `${name} made the same ${files(c.after_files)} on both sides.${eol}`, tone: "info" };
        const counts = c.before_files === c.after_files ? `${files(c.after_files)} on each side` : `${files(c.before_files)} before and ${thousands(c.after_files)} after`;
        return { text: `${name} made ${counts} from ${ends.old} and ${ends.new}: ${thousands(c.files.length)} differ.${eol}`, tone: "info" };
      }
    }
  });
  /** A side's words as a sentence: an ellipsis ends one already. */
  const ended = (words: string) => (words.endsWith("…") ? words : `${words}.`);
  const lowerFirst = (text: string) => text.charAt(0).toLowerCase() + text.slice(1);

  const running = $derived(phase === "running");
  /** A press runs what is missing (*Run*), not both sides anew (*Run again*). */
  const fresh = $derived(compare.wants || !compare.anyKept);
  /** The stderr under the bar: opened by *What it said*, or by a failure that answers a press of *Run* here. */
  let said = $state(false);
  let pressed = false;
  /**
   * What the failed sides said, as the block shows it: kept while the sides run again and their answer is compared, so
   * the block holds its room until the next answer replaces it (it lands in that answer's frame, as what was made does).
   */
  let kept = $state.raw<typeof failures>([]);
  $effect(() => {
    const now = phase;
    const told = failures;
    untrack(() => {
      if (told.length > 0) kept = told;
      else if (now !== "running" && now !== "asking" && now !== "comparing" && kept.length > 0) kept = [];
      if (now === "failed" && pressed) said = true;
      if (now !== "running" && now !== "asking") pressed = false;
      if (kept.length === 0 && said) said = false;
    });
  });
  function press() {
    if (!running) pressed = true;
    compare.press();
  }
  /** The note box's words, while no cards stand under the bar. */
  const note = $derived.by(() => {
    const made = compare.made;
    if (made !== null && phase !== "running" && phase !== "failed" && !lists) {
      return made.kind === "single" ? `${name} drew the same for both sides.` : "Its generated files are the same on both sides.";
    }
    return "Its generated files show here when both sides are made.";
  });
</script>

<div class="tf-madebar tf-dfbar" aria-busy={running || phase === "comparing"} class:tf-warn={status.tone === "warn"} class:tf-err={status.tone === "err"}>
  {#if folder === ""}<span class="tf-p">{title || "The top folder"}</span>{:else}<TfPath path={folder} class="tf-p" />{/if}
  <span role="status" title={status.text}>{status.text}</span>
  {#if failures.some((f) => f.stderr !== "")}
    <button type="button" class="tf-btn tf-sm tf-ghost tf-trail" aria-expanded={said} onclick={() => (said = !said)}>What it said</button>
  {/if}
  {#if renderers.length > 1}
    <label class="tf-trail tf-madebar-as">
      <span class="tf-small">Render with</span>
      <select class="tf-select" aria-label="Render {folder || 'the top folder'} with" title={name} value={renderer.id} onchange={(event) => onpick(event.currentTarget.value)}>
        {#each renderers as one (one.id)}<option value={one.id}>{one.name}</option>{/each}
      </select>
    </label>
  {/if}
  <!-- *Run* runs the sides that want it (not rendered, made from an earlier state, failed); *Run again* both anew. -->
  <button
    type="button"
    class="tf-btn tf-sm tf-trail tf-madebar-run"
    class:tf-primary={!running && fresh}
    disabled={phase === "asking"}
    onclick={press}>{running ? "Stop" : fresh ? "Run" : "Run again"}</button
  >
  <button type="button" class="tf-btn tf-sm tf-ghost tf-trail" onclick={onreal}>Real files</button>
</div>
{#each said ? kept.filter((f) => f.stderr !== "") : [] as f (f.side)}
  <div class="tf-stepfail tf-dffail" role="alert">
    <b>{SIDE[f.side]}: {f.line}</b>
    <pre>{f.stderr}</pre>
  </div>
{/each}
{#if !lists}
  <div class="tf-dunder"><p class="tf-dfnote">{note}</p></div>
{/if}
