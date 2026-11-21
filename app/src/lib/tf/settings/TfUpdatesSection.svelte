<!--
  Settings › Updates (ADR-0052, decisions D13; the design system's *The update box*).

  The page's top is one box: the version on this computer, the newest on the chosen channel, and the track between them,
  which is the download's progress. Its rooms never change: three lines at each end, one reading under the track, two
  lines of what it says, and two buttons whose width is their longest word's (each holds all its words, the others
  hidden), shown or hidden with `visibility`. So a check, a download or a failure landing while the page is open changes
  words and fills, and moves nothing.

  The second button turns from *Downloading…* into *Restart to update* with no press; for `BY_POINTER_MS` after any such
  change of its meaning a press on it does nothing (the press rule).

  Under the box, in this order so nothing is added above anything drawn: *Check for updates*, *Which versions* (its line
  has room for the longest reading), *Last update*, and *What's new*, a box of one height that scrolls inside.

  Everything comes from `update_state`, which needs no database: the page is usable in the frame it opens.
-->
<script lang="ts">
  import TfDialog from "./TfDialog.svelte";
  import TfMarkdown from "../markdown/TfMarkdown.svelte";
  import { BY_POINTER_MS } from "../press-guard";
  import { draftCount } from "../drafts.svelte";
  import { timer as sharedTimer } from "../../shell/timer.svelte";
  import { routeWeb } from "../../shell/open-web";
  import { sitesUnsaved } from "../../ipc/site";
  import type { UpdateNotes } from "../../ipc/update";
  import { layers as sharedLayers, type Layers } from "../layers.svelte";
  import { updates as sharedUpdates, type Updates } from "../update.svelte";
  import { CHANNEL_NAMES, FIRST_WORDS, SECOND_WORDS, channelLine, lastUpdateLine, updateBox, type BoxButton } from "./updates";

  let {
    store = sharedUpdates,
    now = () => new Date(),
    layers = sharedLayers,
    losses = realLosses,
    open = (url: string) => routeWeb(url),
  }: {
    store?: Pick<Updates, "state" | "check" | "setCheck" | "setChannel" | "download" | "cancel" | "install" | "later" | "notes">;
    now?: () => Date;
    layers?: Layers;
    /** What a restart would lose: unsent drafts, a site's unsaved changes, a running timer. */
    losses?: () => Promise<string[]>;
    /** Opens a GitHub page in the browser. */
    open?: (url: string) => Promise<void>;
  } = $props();

  /** The three things a restart loses, as the question lists them. */
  async function realLosses(): Promise<string[]> {
    const lost: string[] = [];
    const drafts = draftCount();
    if (drafts > 0) lost.push(drafts === 1 ? "1 draft you have not sent" : `${drafts} drafts you have not sent`);
    try {
      for (const site of await sitesUnsaved()) lost.push(`Unsaved changes in ${site.source}'s site`);
    } catch {
      // No site state: nothing to list.
    }
    if (sharedTimer.current !== null) lost.push(`The timer stops now, at ${sharedTimer.elapsed ?? "0:00"}`);
    return lost;
  }

  const upd = $derived(store.state);
  const box = $derived(upd === null ? null : updateBox(upd, now()));
  const checking = $derived(upd?.phase.phase === "checking");

  /** When the second button last changed its meaning without a press, and whether the last change was ours. */
  let changedAt = Number.NEGATIVE_INFINITY;
  let pressed = false;
  let lastMeaning = "";
  $effect.pre(() => {
    const meaning = box === null ? "" : `${box.second.does}:${box.second.label}:${box.second.shown}`;
    if (meaning === lastMeaning) return;
    if (lastMeaning !== "" && !pressed) changedAt = performance.now();
    pressed = false;
    lastMeaning = meaning;
  });

  let asking = $state<string[] | null>(null);

  async function first(button: BoxButton & { does: "cancel" | "later" | null }) {
    pressed = true;
    if (button.does === "cancel") await store.cancel();
    else if (button.does === "later") await store.later();
  }

  async function second() {
    if (box === null || upd === null) return;
    if (performance.now() - changedAt < BY_POINTER_MS) return;
    pressed = true;
    const does = box.second.does;
    if (does === "download") await store.download();
    else if (does === "page") await open(upd.offer?.page_url ?? upd.newest?.page_url ?? "");
    else if (does === "install") {
      const lost = await losses();
      if (lost.length > 0) asking = lost;
      else await store.install();
    }
  }

  /**
   * The track's fill, as a share of its width: set through the element's style object, which the CSP allows (an inline
   * `style` attribute it does not). A `<progress>` drew as a tall block in WKWebView.
   */
  function filled(share: number) {
    return (node: HTMLElement) => {
      node.style.width = `${Math.round(Math.min(1, Math.max(0, share)) * 1000) / 10}%`;
    };
  }

  /** Every word a button ever holds, the current one first: the button is as wide as the widest. */
  const words = (current: string, all: readonly string[]) => [current, ...all.filter((word) => word !== current)];

  /** The notes shown: the offered version's, else the running one's. */
  const notesOf = $derived(upd === null ? null : (upd.offer?.version ?? upd.current));
  let notes = $state<UpdateNotes | null>(null);
  let notesFailed = $state(false);
  $effect(() => {
    const version = notesOf;
    if (version === null || !upd?.set_up) return;
    let dead = false;
    notesFailed = false;
    void store.notes(version).then(
      (read) => {
        if (!dead) notes = read;
      },
      () => {
        if (!dead) notesFailed = true;
      },
    );
    return () => {
      dead = true;
    };
  });

  const last = $derived(upd === null ? null : lastUpdateLine(upd.last, now()));
  const channel = $derived(upd === null ? null : channelLine(upd.channel));
</script>

<header class="tf-stg-h tf-stg-hline">
  <div>
    <h1>Updates</h1>
    <p>knobas looks on GitHub for a newer version. It sends nothing about you or your data.</p>
  </div>
  <div class="tf-acts">
    <button class="tf-btn tf-upd-check" disabled={upd === null || !upd.set_up || checking || upd.phase.phase === "downloading" || upd.phase.phase === "installing"} onclick={() => void store.check()}>
      <span class="tf-upd-words"><span class:tf-upd-off={checking}>Check now</span><span class:tf-upd-off={!checking} aria-hidden={!checking}>Checking…</span></span>
    </button>
  </div>
</header>

<div class="tf-upd" data-testid="update-box">
  <div class="tf-upd-end">
    <span class="tf-upd-lab">{box?.here.label ?? "On this computer"}</span>
    <b class="tf-upd-v">{box?.here.version ?? " "}</b>
    <span class="tf-upd-sub">{box?.here.sub ?? " "}</span>
  </div>
  <div class="tf-upd-way">
    <div class="tf-upd-track" class:tf-go={box?.track.go} class:tf-full={box?.track.fill === 1} class:tf-part={box !== null && box.track.fill !== null && box.track.fill < 1} class:tf-bad={box?.track.bad}>
      {#if box !== null && box.track.fill !== null}<i class="tf-upd-fill" {@attach filled(box.track.fill)}></i>{/if}
    </div>
    <span class="tf-upd-n" class:tf-ok={box?.reading.tone === "ok"} class:tf-err={box?.reading.tone === "err"}>{box?.reading.text || " "}</span>
  </div>
  <div class="tf-upd-end tf-upd-new">
    <span class="tf-upd-lab">{box?.there.label ?? " "}</span>
    <b class="tf-upd-v" class:tf-quiet={box?.there.quiet}>{box?.there.version || " "}</b>
    <span class="tf-upd-sub">{box?.there.sub || " "}</span>
  </div>
  <p class="tf-upd-says" class:tf-err={box?.says.err} role={box?.says.err ? "alert" : "status"}>
    {box?.says.line ?? ""}{#if box?.says.dim}<br /><span class="tf-dim">{box.says.dim}</span>{/if}
  </p>
  <div class="tf-upd-acts">
    {#if box?.page && upd}
      <span class="tf-upd-rest"><a href={upd.offer?.page_url ?? ""} onclick={(event) => { event.preventDefault(); void open(upd.offer?.page_url ?? ""); }}>Open the download page</a></span>
    {/if}
    {#if box}
      <button class="tf-btn tf-ghost tf-upd-b1" class:tf-unseen={!box.first.shown} disabled={box.first.disabled || !box.first.shown} aria-label={box.first.label} onclick={() => void first(box.first)}>
        <span class="tf-upd-words">{#each words(box.first.label, FIRST_WORDS) as word, at (word)}<span class:tf-upd-off={at > 0} aria-hidden={at > 0}>{word}</span>{/each}</span>
      </button>
      <button class="tf-btn tf-primary tf-upd-b2" class:tf-unseen={!box.second.shown} disabled={box.second.disabled || !box.second.shown} aria-label={box.second.label} onclick={() => void second()}>
        <span class="tf-upd-words">{#each words(box.second.label, SECOND_WORDS) as word, at (word)}<span class:tf-upd-off={at > 0} aria-hidden={at > 0}>{word}</span>{/each}</span>
      </button>
    {/if}
  </div>
</div>

<div class="tf-stg-row tf-upd-first">
  <label class="tf-stg-l" for="tf-upd-check">Check for updates<small>When knobas starts and every 6 hours. A newer version shows here and as a dot on the Settings cog. Nothing downloads or installs until you say so.</small></label>
  <div class="tf-stg-c">
    <button id="tf-upd-check" class="tf-stg-sw" role="switch" aria-label="Check for updates" aria-checked={upd?.check ?? true} disabled={upd === null || !upd.set_up} onclick={() => upd && void store.setCheck(!upd.check)}><i></i></button>
  </div>
</div>
<div class="tf-stg-row">
  <div class="tf-stg-l">Which versions<small>Releases are finished versions. Pre-release builds come more often, before a release is finished.</small></div>
  <div class="tf-stg-c tf-stg-inl">
    <div class="tf-seg" role="group" aria-label="Which versions">
      {#each ["releases", "pre_releases"] as const as choice (choice)}
        <button aria-pressed={upd?.channel === choice} disabled={upd === null || !upd.set_up} onclick={() => upd?.channel !== choice && void store.setChannel(choice)}>{CHANNEL_NAMES[choice]}</button>
      {/each}
    </div>
    <span class="tf-upd-chan" class:tf-warn={channel?.warn}>{channel?.text ?? ""}</span>
  </div>
</div>
<div class="tf-stg-row">
  <div class="tf-stg-l">Last update</div>
  <div class="tf-stg-c">
    {#if last}
      <span><span class="tf-mono">{last.from}</span> to <span class="tf-mono">{last.to}</span>, {last.when}.{#if last.backup}{" "}Backed up first as <span class="tf-mono">{last.backup}</span>.{/if}</span>
    {:else}
      <span class="tf-dim">None on this computer yet.</span>
    {/if}
  </div>
</div>

<section class="tf-stg-sec">
  <div class="tf-upd-h2">
    <h2>What's new in <span class="tf-mono">{notesOf ?? ""}</span></h2>
    {#if notes?.page_url ?? upd?.newest?.page_url}
      {@const page = notes?.page_url ?? upd?.newest?.page_url ?? ""}
      <a href={page} onclick={(event) => { event.preventDefault(); void open(page); }}>Open this release on GitHub</a>
    {/if}
  </div>
  <div class="tf-upd-notes" tabindex="-1">
    {#if upd !== null && !upd.set_up}
      <p class="tf-dim">The notes of each release are on GitHub.</p>
    {:else if notes?.markdown}
      <TfMarkdown text={notes.markdown} />
    {:else if notesFailed || (notes !== null && notes.markdown === null)}
      <p class="tf-dim">knobas could not read the notes from GitHub. The link above opens the release.</p>
    {/if}
  </div>
</section>

{#if asking !== null}
  <TfDialog title="Restart to update to {upd?.offer?.version ?? ''}?" onclose={() => (asking = null)} {layers}>
    {#snippet body()}
      <p class="tf-dlg-p">knobas closes, installs the update and opens again. This will be lost:</p>
      <ul class="tf-upd-lost">
        {#each asking ?? [] as line (line)}<li>{line}</li>{/each}
      </ul>
    {/snippet}
    {#snippet footer()}
      <button class="tf-btn tf-ghost" onclick={() => (asking = null)}>Not now</button>
      <button
        class="tf-btn tf-primary"
        onclick={() => {
          asking = null;
          void store.install();
        }}>Restart and update</button
      >
    {/snippet}
  </TfDialog>
{/if}
