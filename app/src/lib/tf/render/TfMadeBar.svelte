<!--
  The made bar (ADR-0046): the strip above the file view at every `rendered`
  address, saying the generated folder's maker, from which commit, how many
  files and bytes, and where the run is. One row that never wraps and never
  changes height: the run's states change its words and its colour (info,
  warn, err) in place, never its box. On the right, fixed widths: *Render
  with* (only when more than one renderer takes the folder; it takes its room
  from the status), *Open in…* (its room held until a folder is kept), *Run
  again* or *Stop* (one button, as wide as the longer word), *Show the real
  files*.

  *What it said* and *N left out* open a panel under the bar on the person's
  press: the failure's line and what the tool wrote on stderr, or the paths
  left out, each with its reason.
-->
<script lang="ts">
  import type { FolderRenderer, FolderRenderView, MadeLeftOut, MadeLeftOutWhy } from "../../ipc/app";
  import { failureLines, runningLine, sizeLabel, tookLabel } from "../../render/folder";
  import { ago } from "../../shell/time";
  import { thousands } from "../counted";
  import TfPath from "../TfPath.svelte";
  import type { Snippet } from "svelte";

  let {
    folder,
    title = "",
    renderer,
    name,
    renderers = [],
    switchTo = null,
    onpick = () => {},
    view,
    refused = null,
    elapsed = 0,
    made = null,
    changedInside = false,
    busy = false,
    openIn,
    onpress,
    onreal,
    now = () => new Date(),
  }: {
    /** The grafted folder's path, `""` the top. */
    folder: string;
    /** The repository's name, for the top folder. */
    title?: string;
    /** The renderer's id. */
    renderer: string;
    /** Its name, as the person gave it. */
    name: string;
    /** Every saved renderer that takes the folder, for *Render with*. */
    renderers?: readonly FolderRenderer[];
    /** The address that draws the folder with another renderer. */
    switchTo?: ((id: string) => string) | null;
    /** *Render with* chose a renderer: the folder's pick (ux26), kept before the address changes. */
    onpick?: (renderer: string) => void;
    /** The run's answer; `null` while it is asked. */
    view: FolderRenderView | null;
    /** Why the ask was refused. */
    refused?: string | null;
    /** The tool's time so far, while it runs. */
    elapsed?: number;
    /** The generated folder shown: how many files, their bytes, what was left out. */
    made?: { files: number; bytes: number; leftOut: readonly MadeLeftOut[]; leftOutTotal: number } | null;
    /** The checkout changed files under the folder, which the generated folder (of the commit) leaves out. */
    changedInside?: boolean;
    /** A run goes, or its new generated folder is still being read: the bar says so to assistive technology. */
    busy?: boolean;
    /** *Open in…*, drawn in its held room. */
    openIn?: Snippet;
    /** *Run again* or *Stop*. */
    onpress: () => void;
    /** *Show the real files*. */
    onreal: () => void;
    now?: () => Date;
  } = $props();

  const short = (sha: string | null | undefined) => (sha ? sha.slice(0, 7) : null);
  const drawn = $derived(view?.drawn ?? null);
  const running = $derived(view?.running ?? null);
  const failure = $derived(view?.failure ?? null);
  const failed = $derived(failure === null || failure.why === "stopped" ? null : failureLines(failure));

  /** The bar's words and its colour. */
  const status = $derived.by((): { text: string; tone: "info" | "warn" | "err" } => {
    if (refused !== null) return { text: `Could not ask ${name}: ${refused}`, tone: "err" };
    if (view === null) return { text: `Asking ${name}…`, tone: "info" };
    if (running !== null) {
      if (running.state === "running") return { text: `Running ${name}… ${tookLabel(elapsed)}`, tone: "info" };
      return { text: runningLine(name, running, view.from), tone: "info" };
    }
    if (failed !== null) {
      return drawn !== null
        ? { text: `The last run failed. This is what it made at ${short(drawn.commit) ?? "an earlier commit"}.`, tone: "err" }
        : { text: `${name} could not make this folder.`, tone: "err" };
    }
    if (drawn !== null && view.current) {
      const at = short(drawn.commit);
      const counted = made === null ? "" : `, ${thousands(made.files)} ${made.files === 1 ? "file" : "files"}, ${sizeLabel(made.bytes)}`;
      const from = drawn.from === "forge" ? ", read from the source" : "";
      return { text: `Made by ${name}${at === null ? "" : ` at ${at}`}${counted}, ${tookLabel(drawn.took_ms)}${from}`, tone: "info" };
    }
    if (drawn !== null) {
      const now_ = short(view.commit);
      return {
        text: `Made at ${short(drawn.commit) ?? "an earlier commit"} ${ago(drawn.drawn_at, now())}.${now_ === null ? "" : ` The folder is at ${now_} now.`}`,
        tone: "warn",
      };
    }
    if (failure?.why === "stopped") return { text: `${name} was stopped.`, tone: "info" };
    return { text: `${name} has not made this folder yet.`, tone: "info" };
  });
  /** The checkout's changes are said in the made line's own slot. */
  const changedNote = $derived(changedInside && drawn !== null && view?.current && running === null && failed === null ? ". Your checkout's changes here are not in it." : "");
  const others = $derived(renderers.filter((r) => r.id !== renderer));
  const leftOut = $derived(made !== null && made.leftOutTotal > 0 && running === null && failed === null ? made.leftOutTotal : 0);

  /** The panel under the bar: what the tool said, or what was left out. */
  let panel = $state<"said" | "left" | null>(null);
  $effect(() => {
    // A new answer closes a panel it no longer has.
    if ((panel === "said" && failed === null) || (panel === "left" && leftOut === 0)) panel = null;
  });

  const WHY: Record<MadeLeftOutWhy, string> = {
    link: "a link, not followed",
    special: "not a file",
    git: "a .git folder",
    name: "a name Windows does not take",
    not_utf8: "a name that is not text",
    case_twin: "the same name as another but for case",
    too_deep: "too deep",
  };
</script>

<div class="tf-madebar" aria-busy={busy} class:tf-warn={status.tone === "warn"} class:tf-err={status.tone === "err"}>
  {#if folder === ""}<span class="tf-p">{title || "The top folder"}</span>{:else}<TfPath path={folder} class="tf-p" />{/if}
  <span role="status" title="{status.text}{changedNote}">{status.text}{changedNote}</span>
  {#if leftOut > 0}
    <button type="button" class="tf-btn tf-sm tf-ghost tf-trail" aria-expanded={panel === "left"} onclick={() => (panel = panel === "left" ? null : "left")}
      >{thousands(leftOut)} left out</button
    >
  {/if}
  {#if failed !== null}
    <button type="button" class="tf-btn tf-sm tf-ghost tf-trail" aria-expanded={panel === "said"} onclick={() => (panel = panel === "said" ? null : "said")}
      >What it said</button
    >
  {/if}
  {#if switchTo !== null && others.length > 0}
    <label class="tf-trail tf-madebar-as">
      <span class="tf-small">Render with</span>
      <select class="tf-select" aria-label="Render with" title={name} value={renderer} onchange={(event) => {
            // Choosing is picking, kept for this folder (ux26).
            onpick(event.currentTarget.value);
            location.hash = switchTo(event.currentTarget.value);
          }}>
        {#each renderers as one (one.id)}<option value={one.id}>{one.name}</option>{/each}
      </select>
    </label>
  {/if}
  <span class="tf-trail tf-madebar-open">{@render openIn?.()}</span>
  <button type="button" class="tf-btn tf-sm tf-trail tf-madebar-run" disabled={view === null && refused === null} onclick={onpress}>{running !== null ? "Stop" : "Run again"}</button>
  <button type="button" class="tf-btn tf-sm tf-trail" onclick={onreal}>Show the real files</button>
</div>
{#if panel === "said" && failed !== null}
  <div class="tf-madebar-more">
    <div class="tf-stepfail" role="alert">
      <b>{failed.line}</b>
      {#if failed.stderr !== ""}<pre>{failed.stderr}</pre>{:else}<span>It wrote nothing on stderr.</span>{/if}
    </div>
  </div>
{:else if panel === "left" && made !== null}
  <div class="tf-madebar-more">
    <ul class="tf-madebar-left">
      {#each made.leftOut as one, i (i)}<li><span class="tf-mono" title={one.path}>{one.path}</span><span>{WHY[one.why]}</span></li>{/each}
    </ul>
    {#if made.leftOutTotal > made.leftOut.length}<p class="tf-small">and {thousands(made.leftOutTotal - made.leftOut.length)} more</p>{/if}
  </div>
{/if}
