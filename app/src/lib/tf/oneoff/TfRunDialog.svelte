<!--
  The run dialog (ADR-0060, section 13): the file viewer's dialog with a run line.

  - **Title** the command's name, **sub line** what it ran on and where that is on disk.
  - **Actions** on the title line: *Run again* or *Stop* as one button as wide as the longer word, *Copy*,
    *Download*, *More*, then the Close.
  - **The run line**, 36 px, never wraps or changes height: how the run stands with the exit code as a figure, the
    time, the command as it was run, and a stepper through this command's runs on this thing. A 2 px stripe slides
    along its foot while it runs, the dialog's one motion.
  - **The view bar**: *Output* and *Problems n*, the counts, and *stdout and stderr*, *Wrap*, *Follow*.
  - **The stage**, its height from the window, never from what a program printed.

  It paints in the frame of the press, from what the page knows, before the program has started. Closing it leaves
  a run running. Arrival focus is the output's scroller, so `Enter` presses nothing and the arrows scroll.

  Keys while it stands: `R` runs it again, `⌘.` stops it, `N` and `P` step through the places, `Enter` opens the
  one stood at, `1` and `2` switch the views, `Esc` closes.
-->
<script lang="ts">
  import "../css/oneoff.css";
  import type { OneOffPlace } from "../../ipc/oneoff";
  import { openObject } from "../../ipc/entity";
  import { layers as sharedLayers, type Layers } from "../layers.svelte";
  import TfDialog from "../settings/TfDialog.svelte";
  import TfMenu from "../TfMenu.svelte";
  import { keyHint } from "../../shell/platform";
  import { push } from "../../shell/toasts.svelte";
  import { openWeb } from "../../shell/open-web";
  import { lineHash, renderedHash } from "../repo/repo";
  import { folderRenderOpen } from "../../ipc/app";
  import { folderRenderers } from "../../render/folders.svelte";
  import { clock, many, outputText, problems, problemsText, resultWord, shown, size, stateOf, targetLabel, took, type Problem } from "../../render/oneoff";
  import { oneoffs as sharedStore, type OneOffs, type Run } from "../../render/oneoff.svelte";
  import TfRunLines from "./TfRunLines.svelte";
  import TfCommand from "../TfCommand.svelte";
  import TfRunProblems from "./TfRunProblems.svelte";
  import TfRunResult from "./TfRunResult.svelte";
  import TfRunFiles from "./TfRunFiles.svelte";
  import TfRunBatch from "./TfRunBatch.svelte";

  let {
    run,
    store = sharedStore,
    layers = sharedLayers,
    navigate = (hash: string) => (location.hash = hash),
  }: {
    run: Run;
    store?: OneOffs;
    layers?: Layers;
    /** Goes to a page of the app (a file at a line). */
    navigate?: (hash: string) => void;
  } = $props();

  let now = $state(Date.now());
  $effect(() => {
    if (!run.running) return;
    const tick = setInterval(() => (now = Date.now()), 250);
    return () => clearInterval(tick);
  });

  const batchFailed = $derived.by(() => {
    void run.version;
    return (run.batch ?? []).filter((item) => item.ended !== null && !(item.ended.how === "exit" && item.ended.code === 0)).length;
  });
  const st = $derived.by(() => {
    const base = stateOf(run.refused !== null ? { how: "not_started", reason: run.refused } : run.ended);
    if (run.batch === null || run.ended === null || run.ended.how !== "exit") return base;
    const n = run.batch.length;
    return batchFailed > 0 ? { tone: "err" as const, words: `${batchFailed} of ${n} exited with an error`, code: null } : { tone: "ok" as const, words: `All ${n} exited with 0`, code: null };
  });
  /** A folder renderer's name, for a generated folder's run. */
  const rendererName = (id: string) => folderRenderers.list.find((r) => r.id === id)?.name ?? id;
  /** A generated folder's run: which renderer made which folder (ADR-0060 amended 2026-11-25). */
  const madeOf = $derived(run.target.kind === "made" || run.target.kind === "made_files" ? { renderer: run.target.renderer, folder: run.target.folder } : null);
  const sub = $derived(
    madeOf !== null
      ? run.batch !== null
        ? `${targetLabel(run.target, run.repo, rendererName)}, one run a file, ${4} at a time`
        : targetLabel(run.target, run.repo, rendererName)
      : run.batch !== null
        ? `${run.batch.length} files in ${run.repo}, one run a file, ${4} at a time`
        : `${run.target.kind === "file" ? `${run.target.path} in ${run.repo}` : run.target.kind === "folder" && run.target.path !== "" ? `${run.target.path}/ in ${run.repo}` : run.repo}, your working tree${run.branch !== null ? ` on ${run.branch}` : ""}`,
  );
  /** What a run on a generated folder changed there, in the warn tone where the counts stand (Fable, 2026-11-25). */
  const changedSaid = $derived(
    madeOf === null || run.madeChanged === null || run.madeChanged.length === 0
      ? null
      : `It changed ${many(run.madeChanged.length, "generated file", "generated files")}. They are knobas's copy of what ${rendererName(madeOf.renderer)} made, so ${madeOf.folder === "" ? run.repo : madeOf.folder} is rendered again.`,
  );
  const ran = $derived(run.ended !== null ? `after ${took(run.elapsed)}` : run.refused !== null ? "" : clock(Math.max(run.elapsed, now - run.startedAt)));
  const findings = $derived.by(() => {
    void run.version;
    return problems([...run.found.values()].sort((a, b) => a.n - b.n));
  });
  const findingCount = $derived(findings.reduce((n, g) => n + g.list.length, 0));
  const both = $derived.by(() => {
    void run.version;
    return run.wrote.out && run.wrote.err;
  });
  const result = $derived(run.result);
  let filePicked = $state(0);
  const counts = $derived.by(() => {
    const out: string[] = [];
    if (run.batch !== null) {
      void run.version;
      const ended = run.batch.filter((item) => item.ended !== null).length;
      return `${ended} of ${many(run.batch.length, "run", "runs")} ended${batchFailed > 0 ? `, ${batchFailed} with an error` : ""}.`;
    }
    if (run.view === "result" && result?.from === "one") {
      return `${result.name === "stdout" ? "Its stdout" : result.name}, ${size(result.size)}${result.kind.kind === "page" ? ", drawn without its scripts" : ""}${result.overruled ? `. Said to be ${result.overruled}, it is not.` : "."}`;
    }
    if (run.view === "files" && result?.from === "files") {
      return `${many(result.files.length, "file", "files")}, ${size(result.bytes)}${result.left_out_total > 0 ? `. ${result.left_out_total} left out` : ""}.`;
    }
    if (run.total > 0) out.push(`${many(run.total, "line", "lines")}, ${size(run.bytes)}`);
    if (findingCount > 0) out.push(`${many(findingCount, "problem", "problems")} in ${many(findings.length, "file", "files")}`);
    if (run.dropped > 0) out.push(`${run.dropped.toLocaleString("en")} lines not kept`);
    if (result?.from === "refused") out.push(`Nothing it made is kept: ${result.why}`);
    return out.length > 0 ? `${out.join(". ")}.` : "";
  });
  const history = $derived(store.history(run));
  const at = $derived(run.id === null ? 0 : Math.max(0, history.findIndex((r) => r.id === run.id)));
  const words = $derived(run.argv.length > 0 ? run.argv : [run.command.command]);
  const quiet = $derived.by(() => {
    void run.version;
    return run.lines.length === 0;
  });

  /** The views the bar offers, in order: the result first, then Output, then Problems. */
  const views = $derived.by(() => {
    const list: { id: "output" | "problems" | "result" | "files"; label: string; count: number | null }[] = [];
    if (result?.from === "one") list.push({ id: "result", label: resultWord(result.kind), count: null });
    if (result?.from === "files") list.push({ id: "files", label: "Files", count: result.files.length });
    list.push({ id: "output", label: "Output", count: null });
    if (findingCount > 0) list.push({ id: "problems", label: "Problems", count: findingCount });
    return list;
  });
  function pick(view: "output" | "problems" | "result" | "files") {
    run.view = view;
    run.picked = true;
  }

  let moreOpen = $state(false);
  let moreFace = $state<HTMLButtonElement | null>(null);

  function again() {
    if (run.running) store.stop(run);
    else store.again(run);
  }

  async function copy() {
    const text = run.view === "problems" ? problemsText(findings) : outputText(run.stream === "both" ? run.lines : run.lines.filter((l) => l.stream === run.stream));
    await store.copy(text).then(
      () => push({ text: run.view === "problems" ? `Copied ${many(findingCount, "problem", "problems")}` : `Copied ${many(run.lines.length, "line", "lines")}` }),
      () => push({ text: "Could not copy: the clipboard refused it", tone: "err" }),
    );
  }

  function download(colours: boolean) {
    moreOpen = false;
    const resultShown = (run.view === "result" && result?.from === "one") || (run.view === "files" && result?.from === "files");
    const file = run.view === "files" && result?.from === "files" ? (result.files[filePicked]?.path ?? null) : null;
    (resultShown ? store.saveResult(run, file) : store.saveRun(run, colours)).then(
      (path) => push({ text: `Saved to ${path}` }),
      (error: unknown) => push({ text: `Could not save it: ${(error as { message?: string } | null)?.message ?? String(error)}`, tone: "err" }),
    );
  }

  /** A place pressed: the file at its line behind the dialog, and the run one press away. */
  function openPlace(place: OneOffPlace | Problem, wide: boolean) {
    const line = place.line ?? 1;
    // A run on a generated folder: its places are the generated files, opened as the folder's own (Fable, 2026-11-25).
    if (madeOf !== null) {
      if (wide) {
        const token = run.made?.token ?? null;
        if (token !== null)
          void folderRenderOpen(null, token, place.path, line).catch((error: unknown) =>
            push({ text: `Could not open ${place.path}: ${(error as { message?: string } | null)?.message ?? String(error)}`, tone: "err" }),
          );
        return;
      }
      leaveFor(renderedHash(run.entityId, madeOf.renderer, madeOf.folder, place.path, { line }), place.path, line);
      return;
    }
    if (wide) {
      void openObject(run.entityId, null, { kind: "file", path: place.path, line, at: null }).catch((error: unknown) =>
        push({ text: `Could not open ${place.path}: ${(error as { message?: string } | null)?.message ?? String(error)}`, tone: "err" }),
      );
      return;
    }
    leaveFor(lineHash(run.entityId, place.path, line), place.path, line);
  }

  /** Closes the dialog for the page at `hash`, the run one press away. */
  function leaveFor(hash: string, path: string, line: number) {
    const place = { path };
    const id = run.id;
    const view = run.view;
    store.close();
    navigate(hash);
    if (id !== null) {
      push({
        text: `${place.path} at line ${line}, from ${run.command.name}`,
        action: {
          label: "Back to it",
          run: () =>
            void store.reopen(id).then((back) => {
              if (back !== null) back.view = view;
            }),
        },
        subject: `oneoff-place:${id}`,
      });
    }
  }

  /** The lines that hold a place, in order: what `N` and `P` step through. */
  function step(by: 1 | -1) {
    const placed = run.view === "problems" ? findings.flatMap((g) => g.list.map((p) => p.n)) : run.lines.filter((l) => (l.places ?? []).some((p) => p.line !== null)).map((l) => l.n);
    // (The places are read only on a press of N or P, never on a batch.)
    if (placed.length === 0) return;
    const now = run.at === null ? -1 : placed.indexOf(run.at);
    const next = now < 0 ? (by === 1 ? 0 : placed.length - 1) : Math.min(placed.length - 1, Math.max(0, now + by));
    run.at = placed[next] ?? null;
    run.follow = false;
  }

  function openAt() {
    if (run.at === null) return;
    const line = run.lines.find((l) => l.n === run.at);
    const place = line?.places?.find((p) => p.line !== null) ?? line?.places?.[0];
    if (place) openPlace(place, false);
  }

  function onkeydown(event: KeyboardEvent) {
    const on = event.target instanceof Element ? event.target : null;
    if (event.defaultPrevented || on?.closest("input, textarea, select, [contenteditable], [role=menu]")) return;
    const mod = event.metaKey || event.ctrlKey;
    if (mod && event.key === ".") {
      event.preventDefault();
      if (run.running) store.stop(run);
      return;
    }
    if (mod || event.altKey) return;
    if (event.key === "r" || event.key === "R") {
      event.preventDefault();
      if (!run.running) store.again(run);
    } else if (event.key === "n" || event.key === "N") {
      event.preventDefault();
      step(1);
    } else if (event.key === "p" || event.key === "P") {
      event.preventDefault();
      step(-1);
    } else if (event.key === "Enter" && run.at !== null && !on?.closest("button, a")) {
      event.preventDefault();
      openAt();
    } else if (/^[1-4]$/.test(event.key)) {
      const view = views[Number(event.key) - 1];
      if (view !== undefined) pick(view.id);
    }
  }
</script>

<svelte:window {onkeydown} />

<TfDialog title={run.command.name} subtitle={sub} onclose={() => store.close()} {layers} className="tf-viewer tf-oo-dialog" closeButton>
  {#snippet actions()}
    <button
      type="button"
      class="tf-btn tf-oo-again"
      disabled={run.batch === null && run.refused === null && run.running && run.id === null}
      title={run.running ? keyHint("Stop it and everything it started (⌘.)") : "Run it again on the thing as it is now (R)"}
      onclick={again}
      ><!-- Both words in one cell (nothing-moves): the button is as wide as the wider, whichever it says. --><span
        class:tf-oo-unsaid={!run.running}
        aria-hidden={!run.running}>Stop</span
      ><span class:tf-oo-unsaid={run.running} aria-hidden={run.running}>Run again</span></button
    >
    <button type="button" class="tf-btn" disabled={quiet || run.view === "result" || run.view === "files"} title={run.view === "result" || run.view === "files" ? "Download it instead" : "Copy what this view shows, as text"} onclick={() => void copy()}>
      <svg class="tf-vglyph" viewBox="0 0 16 16" aria-hidden="true"><rect x="5.5" y="5.5" width="8" height="8" rx="1.5" /><path d="M10.5 3.5v-.5a1 1 0 0 0-1-1h-6a1 1 0 0 0-1 1v6a1 1 0 0 0 1 1h.5" /></svg>
      Copy
    </button>
    <button type="button" class="tf-btn" disabled={(quiet && result === null) || run.id === null} title={run.view === "result" || run.view === "files" ? "Save what it made to Downloads" : "Save what it printed to Downloads"} onclick={() => download(false)}>
      <svg class="tf-vglyph" viewBox="0 0 16 16" aria-hidden="true"><path d="M8 2.5v7.5M4.75 6.75 8 10l3.25-3.25M3 13.5h10" /></svg>
      Download
    </button>
    <button bind:this={moreFace} type="button" class="tf-iconbtn" aria-label="More" title="More" aria-haspopup="menu" aria-expanded={moreOpen} onclick={(event) => { event.stopPropagation(); moreOpen = !moreOpen; }}>
      <svg viewBox="0 0 16 16" aria-hidden="true"><circle cx="3.5" cy="8" r="1.2" /><circle cx="8" cy="8" r="1.2" /><circle cx="12.5" cy="8" r="1.2" /></svg>
    </button>
    <TfMenu bind:open={moreOpen} opener={moreFace} label="More" {layers}>
      <a role="menuitem" href="#/settings/commands/{encodeURIComponent(run.command.id)}" onclick={() => { moreOpen = false; store.close(); }}>Edit this command</a>
      <button role="menuitem" onclick={() => { moreOpen = false; void store.copy(shown(words)).then(() => push({ text: "Copied the command line" })); }}>Copy the command line</button>
      <button role="menuitem" disabled={quiet || run.id === null} onclick={() => download(true)}>Download with the colour codes</button>
    </TfMenu>
  {/snippet}
  {#snippet body()}
    <div class="tf-oo-line" data-running={run.running ? "" : undefined}>
      <span class="tf-oo-state tf-{st.tone}" role="status"
        ><span class="tf-dot {st.tone === 'muted' ? '' : st.tone === 'info' ? 'tf-run' : `tf-${st.tone}`}"></span><span
          >{st.words}{#if st.code !== null}{" "}<span class="tf-oo-code">{st.code}</span>{/if}</span
        ></span
      >
      <span class="tf-oo-took" title={run.ended !== null ? "How long it ran" : "Running for"}>{ran}</span>
      <!-- As a command line is drawn everywhere (TfCommand): the program's folders give way first, its file stays. -->
      <TfCommand command={shown(words)} class="tf-oo-argv" />
      {#if history.length > 1}
        <span class="tf-oo-hist">
          <button type="button" class="tf-iconbtn tf-sm" aria-label="The run before" title="The run before" disabled={at >= history.length - 1} onclick={() => void store.reopen(history[at + 1]!.id)}
            ><svg viewBox="0 0 16 16" aria-hidden="true"><path d="M10 3.5 5.5 8l4.5 4.5" /></svg></button
          >
          <span>{at === 0 ? (run.running ? "Now" : "This run") : "An earlier run"}, {at + 1} of {history.length}</span>
          <button type="button" class="tf-iconbtn tf-sm" aria-label="The run after" title="The run after" disabled={at === 0} onclick={() => void store.reopen(history[at - 1]!.id)}
            ><svg viewBox="0 0 16 16" aria-hidden="true"><path d="m6 3.5 4.5 4.5L6 12.5" /></svg></button
          >
        </span>
      {/if}
    </div>
    <div class="tf-oo-bar">
      <div class="tf-seg" role="group" aria-label="What to show">
        {#each views as view, i (view.id)}
          <button type="button" aria-pressed={run.view === view.id} title={String(i + 1)} onclick={() => pick(view.id)}
            >{view.label}{#if view.count !== null}{" "}<span class="tf-count">{view.count}</span>{/if}</button
          >
        {/each}
      </div>
      {#if changedSaid !== null}
        <!-- In the counts' own cell, so the bar keeps its height when a run ends (nothing-moves). -->
        <span class="tf-oo-counts tf-warn" role="status" title={`${changedSaid} ${counts}`}>{changedSaid}</span>
      {:else}
        <span class="tf-oo-counts">{counts}</span>
      {/if}
      {#if run.view === "output" && !quiet}
        {#if both}
          <select class="tf-select" aria-label="Which stream" bind:value={run.stream}>
            <option value="both">stdout and stderr</option>
            <option value="out">stdout</option>
            <option value="err">stderr</option>
          </select>
        {/if}
        <button type="button" class="tf-btn tf-sm tf-ghost" aria-pressed={run.wrap} onclick={() => (run.wrap = !run.wrap)}>Wrap</button>
        <!-- Kept after the run ends, so nothing beside it moves (nothing-moves): there is no newest line to follow then. -->
        <button type="button" class="tf-btn tf-sm tf-ghost" aria-pressed={run.running && run.follow} disabled={!run.running} title={run.running ? "Keep the newest line in view" : "It ended: there is no newer line"} onclick={() => (run.follow = !run.follow)}>Follow</button>
      {/if}
    </div>
    <div class="tf-oo-stage" data-shows={run.view}>
      {#if run.refused !== null || run.ended?.how === "not_started"}
        <!-- svelte-ignore a11y_no_noninteractive_tabindex: a scroller takes focus so the arrows scroll it (arrival focus, design system Dialogs) -->
        <div class="tf-oo-quiet tf-err" tabindex="0" role="status">
          <p><b>{words[0]} did not start.</b></p>
          <p>{run.refused ?? (run.ended?.how === "not_started" ? run.ended.reason : "")}</p>
          <div class="tf-empty-acts">
            <a class="tf-btn tf-sm" href="#/settings/commands/{encodeURIComponent(run.command.id)}" onclick={() => store.close()}>Edit this command</a>
          </div>
        </div>
      {:else if run.batch !== null}
        <TfRunBatch {run} onplace={openPlace} onurl={(href) => openWeb(href)} />
      {:else if run.view === "result" && result?.from === "one"}
        {#key run.id}
          <TfRunResult kind={result.kind} bytes={() => store.resultBytes(run)} name={result.name} bytesSize={result.size} />
        {/key}
      {:else if run.view === "files" && result?.from === "files"}
        {#key run.id}
          <TfRunFiles {result} bytes={(file) => store.resultBytes(run, file)} bind:picked={filePicked} />
        {/key}
      {:else if quiet}
        <!-- svelte-ignore a11y_no_noninteractive_tabindex: a scroller takes focus so the arrows scroll it (arrival focus, design system Dialogs) -->
        <div class="tf-oo-quiet" tabindex="0" role="status">
          <p>{run.ended === null ? "Nothing printed yet." : `It printed nothing${run.ended.how === "exit" ? ` and exited with ${run.ended.code}` : ""}.`}</p>
        </div>
      {:else if run.view === "problems"}
        <TfRunProblems {run} onproblem={(p) => openPlace(p, false)} />
      {:else}
        <!-- The dialog is for what the command printed: it arrives there, not on the stream's select before it (F-453). -->
        <TfRunLines {run} onplace={openPlace} onurl={(href) => openWeb(href)} data-arrive />
      {/if}
    </div>
  {/snippet}
</TfDialog>
