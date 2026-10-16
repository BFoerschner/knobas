<!--
  Proposed links (#835, R-6.3, R-6.4): `#/review` and `#/review/<link id>`,
  one screen; the id only says which proposal is open.

  Left, the queue: the filter bar (text, *This job / All*, the strength chips,
  *Decided*) and a row per proposal with its strength dot, the item and its
  target. Right, the open proposal: both ends as cards, the relation as an
  editable dropdown under the strength, *Why it was proposed* with the
  detector's reason (the evidence under it is #836), and *Link them* (`Y`),
  *Not related* (`N`, remembered), *Decide later* (`S`). A decision opens the
  next proposal and the toast offers *Undo*; the write waits for the toast
  (`decisions.svelte.ts`).

  Over `room_suggestions`, `accept_suggestion`, `dismiss_suggestion` and, for
  a relation the reader changed, `create_link`. No new command.
-->
<script lang="ts">
  import { onMount } from "svelte";

  import { ipcErrorMessage } from "../../ipc";
  import { roomSuggestions, type SuggestionEntry } from "../../ipc/entity";
  import { inTextField } from "../../shell/keys";
  import { kindSingular } from "../../shell/kinds";
  import { push } from "../../shell/toasts.svelte";
  import FilterBar from "../filter/FilterBar.svelte";
  import { sessionFilter } from "../filter/filter-state.svelte";
  import type { FilterConfig } from "../filter/filter";
  import Marked from "../filter/Marked.svelte";
  import type { Layers } from "../layers.svelte";
  import { decisions as sessionDecisions, UNDO_MS, type Decisions } from "./decisions.svelte";
  import {
    aboutKey,
    claim,
    currentOf,
    endName,
    linkedToast,
    nextAfter,
    queue,
    relationLabel,
    relationOptions,
    rowNote,
    strengthOf,
    targetOf,
    toneOf,
    type ReviewPorts,
    type Strength,
    type Verdict,
  } from "./review";

  let {
    ready,
    job,
    sid,
    layers,
    ports = { roomSuggestions: (sources, ctx, limit) => roomSuggestions(sources, ctx, limit) },
    decisions = sessionDecisions,
    ongo = (hash: string) => location.replace(hash),
  }: {
    /** Whether the database answers yet. */
    ready: boolean;
    /** The job the reader was last in, as a ticket key: what *This job* keeps. */
    job: string | null;
    /** The addressed proposal's link id, `null` for `#/review`. */
    sid: string | null;
    layers: Pick<Layers, "open">;
    ports?: ReviewPorts;
    decisions?: Decisions;
    /** Open an address in place of this one, so a decision is not a history step. */
    ongo?: (hash: string) => void;
  } = $props();

  /** Enough for every proposal a mirror holds in practice; the tray's own `total` says if not. */
  const LIMIT = 500;

  let rows = $state<SuggestionEntry[] | null>(null);
  let failure = $state<string | null>(null);
  let relation = $state("related");

  const filter = sessionFilter("review");

  const config = $derived<FilterConfig>({
    id: "review",
    placeholder: "Filter proposals",
    noun: "",
    scopes: [
      ...(job !== null ? [{ id: "job", label: "This job", tag: job, query: "" }] : []),
      { id: "all", label: "All", query: "" },
    ],
    chips: [
      {
        key: "st",
        label: "Strength",
        options: [
          { value: "strong", label: "Strong" },
          { value: "indirect", label: "Indirect" },
          { value: "weak", label: "Weak" },
        ],
      },
      { key: "dec", label: "Show", options: [{ value: "decided", label: "Decided" }] },
    ],
  });

  const scope = $derived<"job" | "all">(job !== null && (filter.scope ?? "job") === "job" ? "job" : "all");

  /** What the mirror proposes, and what this session decided that the mirror no longer returns. */
  const entries = $derived.by(() => {
    if (rows === null) return null;
    const read = new Set(rows.map((entry) => entry.link.id));
    return [...rows, ...decisions.entries.filter((entry) => !read.has(entry.link.id))];
  });

  const list = $derived(
    entries === null
      ? []
      : queue(
          entries,
          decisions.verdicts,
          {
            scope,
            job,
            strengths: (filter.chips["st"] ?? []) as Strength[],
            decided: (filter.chips["dec"] ?? []).length > 0,
            text: filter.text,
          },
          sid,
        ),
  );

  const current = $derived(currentOf(list, decisions.verdicts, sid));
  const about = $derived(current === null ? null : aboutKey(current));

  $effect(() => {
    relation = current?.link.relation ?? "related";
  });

  $effect(() => {
    if (job === null && filter.scope === "job") filter.scope = null;
  });

  $effect(() => {
    if (!ready) return;
    ports
      .roomSuggestions([], null, LIMIT)
      .then((page) => {
        rows = page.rows;
        failure = null;
      })
      .catch((cause: unknown) => {
        failure = ipcErrorMessage(cause);
      });
  });

  function decide(verdict: Verdict) {
    const entry = current;
    if (entry === null) return;
    const id = entry.link.id;
    const chosen = relation;
    decisions.decide(entry, verdict, chosen, (message) => push({ text: message, tone: "err" }));
    const undo = {
      label: "Undo",
      run: () => {
        decisions.undo(id);
        ongo(`#/review/${encodeURIComponent(id)}`);
      },
    };
    if (verdict === "yes") push({ text: linkedToast(entry, relationLabel(chosen)), action: undo, ms: UNDO_MS });
    if (verdict === "no") push({ text: "Dismissed. It will not be proposed again.", action: undo, ms: UNDO_MS });
    const next = nextAfter(list, decisions.verdicts, id);
    ongo(next === null ? "#/review" : `#/review/${encodeURIComponent(next.link.id)}`);
  }

  const KEYS: Record<string, Verdict> = { y: "yes", n: "no", s: "skip" };

  onMount(() => {
    const onkey = (event: KeyboardEvent) => {
      if (event.defaultPrevented || event.metaKey || event.ctrlKey || event.altKey) return;
      const verdict = KEYS[event.key.toLowerCase()];
      if (verdict === undefined || current === null || layers.open.length > 0) return;
      if (inTextField(document.activeElement)) return;
      event.preventDefault();
      decide(verdict);
    };
    window.addEventListener("keydown", onkey);
    return () => window.removeEventListener("keydown", onkey);
  });
</script>

{#snippet card(end: SuggestionEntry["from"])}
  <div class="tf-pend">
    <span class="tf-kind">{kindSingular(end.kind)}</span>
    <span class="tf-name">{endName(end)}</span>
  </div>
{/snippet}

<div class="tf-rv">
  <aside class="tf-rvq" aria-label="Proposals">
    <FilterBar {config} {filter} shown={entries === null ? null : list.length} total={entries?.length ?? null} />
    <div class="tf-ql">
      {#each list as entry (entry.link.id)}
        {@const verdict = decisions.verdicts[entry.link.id]}
        <a
          href="#/review/{encodeURIComponent(entry.link.id)}"
          aria-current={current !== null && entry.link.id === current.link.id ? "true" : undefined}
          class:tf-decided={verdict !== undefined}
        >
          <span class="tf-dot tf-{toneOf(strengthOf(entry.link))}"></span>
          <span><Marked text={endName(targetOf(entry).item)} typed={filter.text} /></span>
          <span class="tf-s">{rowNote(entry, verdict)}</span>
        </a>
      {:else}
        {#if entries !== null}
          <div class="tf-empty">Nothing to judge here.</div>
        {/if}
      {/each}
    </div>
  </aside>
  <section class="tf-rmain">
    {#if failure !== null && rows === null}
      <div class="tf-review">
        <h1>Proposals cannot be read</h1>
        <p class="tf-dim">{failure}</p>
      </div>
    {:else if entries === null}
      <!-- Reading. -->
    {:else if current === null}
      <div class="tf-review">
        <h1>Nothing left to judge{scope === "job" ? ` for ${job}` : ""}</h1>
        <p class="tf-dim">
          New proposals appear after a sync finds a key, a shared commit or similar wording. Dismissed ones stay dismissed.
        </p>
        {#if scope === "job"}
          <button type="button" class="tf-btn tf-start" onclick={() => (filter.scope = "all")}>Show all proposals</button>
        {/if}
      </div>
    {:else}
      {@const strength = strengthOf(current.link)}
      <div class="tf-review">
        <div class="tf-review-top">
          <h1>Is this a real connection?</h1>
          <span class="tf-dim"
            >{list.indexOf(current) + 1} of {list.length}{#if about !== null}, about <a
                href="#/job/{encodeURIComponent(about)}"
                class="tf-mono">{about}</a
              >{/if}</span
          >
        </div>
        <div class="tf-pair">
          {@render card(current.from)}
          <div class="tf-bridge">
            <span class="tf-strength tf-{strength}">{strength.charAt(0).toUpperCase() + strength.slice(1)}</span>
            <select bind:value={relation} aria-label="Relation">
              {#each relationOptions(current.link.relation) as option (option.id)}
                <option value={option.id}>{option.label}</option>
              {/each}
            </select>
          </div>
          {@render card(current.to)}
        </div>
        <div class="tf-evidence">
          <h2>Why it was proposed</h2>
          <div class="tf-claim">{claim(current.link.reason ?? "")}</div>
        </div>
        <div class="tf-decide">
          <button type="button" class="tf-btn tf-yes" onclick={() => decide("yes")}>Link them <kbd>Y</kbd></button>
          <button type="button" class="tf-btn tf-no" onclick={() => decide("no")}>Not related <kbd>N</kbd></button>
          <button type="button" class="tf-btn tf-ghost" onclick={() => decide("skip")}>Decide later <kbd>S</kbd></button>
          <span class="tf-hint">Not related is remembered.</span>
        </div>
      </div>
    {/if}
  </section>
</div>
