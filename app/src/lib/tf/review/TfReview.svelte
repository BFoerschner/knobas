<!--
  Proposed links (#835, R-6.3, R-6.4): `#/review` and `#/review/<link id>`,
  one screen; the id only says which proposal is open.

  Left, the queue: the filter bar (text, *This job / All*, the strength chips,
  *Decided*) and a row per proposal with its strength dot, the item and its
  target. Right, the open proposal: both ends as cards, the relation as an
  editable dropdown under the strength, *Why it was proposed* with the
  detector's reason and, under it, the evidence (#836): the text it was found
  in with the match marked, *Also found* and *Speaks against it*, read for
  the open proposal from `proposal_evidence`. Then *Link them* (`Y`),
  *Not related* (`N`, remembered), *Decide later* (`S`). A decision opens the
  next proposal and the toast offers *Undo*; the write waits for the toast
  (`decisions.svelte.ts`).

  Over `room_suggestions`, `proposal_evidence`, `accept_suggestion`,
  `dismiss_suggestion` and, for a relation the reader changed, `create_link`.
-->
<script lang="ts">
  import { onMount, untrack } from "svelte";
  import { replaceAddress } from "../opener";

  import { ipcErrorMessage } from "../../ipc";
  import { proposalEvidence, roomSuggestions, type ProposalEvidence, type SuggestionEntry } from "../../ipc/entity";
  import { inTextField } from "../../shell/keys";
  import { kindSingular } from "../../shell/kinds";
  import { push } from "../../shell/toasts.svelte";
  import FilterBar from "../filter/FilterBar.svelte";
  import { sessionFilter } from "../filter/filter-state.svelte";
  import type { FilterConfig } from "../filter/filter";
  import Marked from "../filter/Marked.svelte";
  import type { Layers } from "../layers.svelte";
  import { jobHash } from "../route";
  import { decisions as sessionDecisions, UNDO_MS, type Decisions } from "./decisions.svelte";
  import {
    aboutTicket,
    claim,
    currentOf,
    endName,
    linkedToast,
    nextAfter,
    queue,
    quotesCode,
    relationLabel,
    relationOptions,
    REVIEW_LIMIT,
    rowNote,
    sideBySide,
    STRENGTHS,
    strengthOf,
    targetOf,
    type ReviewPorts,
    type Strength,
    type Verdict,
  } from "./review";
  import { pageVisible } from "../kept-pages.svelte";

  let {
    ready,
    job,
    sid,
    layers,
    ports = {
      roomSuggestions: (sources, ctx, limit) => roomSuggestions(sources, ctx, limit),
      proposalEvidence: (linkId) => proposalEvidence(linkId),
    },
    decisions = sessionDecisions,
    ongo = replaceAddress,
  }: {
    /** Whether the database answers yet. */
    ready: boolean;
    /** The chosen job (spec #2227), as a ticket key: what *This job* keeps. */
    job: string | null;
    /** The addressed proposal's link id, `null` for `#/review`. */
    sid: string | null;
    layers: Pick<Layers, "open">;
    ports?: ReviewPorts;
    decisions?: Decisions;
    /** Open an address in place of this one, so a decision is not a history step. */
    ongo?: (hash: string) => void;
  } = $props();

  let rows = $state<SuggestionEntry[] | null>(null);
  let failure = $state<string | null>(null);
  let relation = $state("related");
  /** Each proposal's evidence once read, by link id: the evidence, or why it could not be read. */
  let evidence = $state<Record<string, ProposalEvidence | { failure: string }>>({});

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
        options: (Object.keys(STRENGTHS) as Strength[]).map((value) => ({ value, label: STRENGTHS[value].label })),
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
  const about = $derived(current === null ? null : aboutTicket(current));

  $effect(() => {
    relation = current?.link.relation ?? "related";
  });

  $effect(() => {
    if (job === null && filter.scope === "job") filter.scope = null;
  });

  $effect(() => {
    if (!ready) return;
    // An answer that lands after the view is gone draws into nothing.
    let live = true;
    ports
      .roomSuggestions([], null, REVIEW_LIMIT)
      .then((page) => {
        if (!live) return;
        rows = page.rows;
        failure = null;
      })
      .catch((cause: unknown) => {
        if (live) failure = ipcErrorMessage(cause);
      });
    return () => {
      live = false;
    };
  });

  $effect(() => {
    const id = current?.link.id;
    if (!ready || id === undefined || untrack(() => evidence[id]) !== undefined) return;
    // Started on a microtask, so a port that throws as it is called is a
    // failure to read and not an effect that throws.
    Promise.resolve()
      .then(() => ports.proposalEvidence(id))
      .then((found) => {
        evidence[id] = found;
      })
      .catch((cause: unknown) => {
        evidence[id] = { failure: ipcErrorMessage(cause) };
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

  const pageShown = pageVisible();
  onMount(() => {
    const onkey = (event: KeyboardEvent) => {
      // A page keep-alive holds hidden (#2247) takes no keys.
      if (!pageShown()) return;
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
          <span class="tf-dot tf-{STRENGTHS[strengthOf(entry.link)].tone}"></span>
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
      {@const found = evidence[current.link.id]}
      <div class="tf-review">
        <div class="tf-review-top">
          <h1>Is this a real connection?</h1>
          <span class="tf-dim"
            >{list.indexOf(current) + 1} of {list.length}{#if about !== null}, about <a
                href={jobHash(about.entityId)}
                class="tf-mono">{about.key}</a
              >{/if}</span
          >
        </div>
        <div class="tf-pair">
          {@render card(current.from)}
          <div class="tf-bridge">
            <span class="tf-strength tf-{strength}">{STRENGTHS[strength].label}</span>
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
          {#if found !== undefined && "failure" in found}
            <p class="tf-dim">{found.failure}</p>
          {:else if found !== undefined}
            {#if found.excerpts.length > 0}
              <div class="tf-quotes" class:tf-sbs={sideBySide(found)}>
                {#each found.excerpts as excerpt, i (i)}
                  <blockquote class:tf-mono={quotesCode(current, excerpt.entity_id)}>
                    {#each excerpt.parts as part, j (j)}{#if part.marked}<mark>{part.text}</mark>{:else}{part.text}{/if}{/each}
                  </blockquote>
                {/each}
              </div>
            {/if}
            {#if found.support.length > 0}
              <div class="tf-against">
                <strong>Also found</strong>
                {#each found.support as line, i (i)}<span>{line}</span>{/each}
              </div>
            {/if}
            <div class="tf-against">
              <strong>Speaks against it</strong>
              {#each found.against as line, i (i)}<span>{line}</span>{/each}
            </div>
          {/if}
        </div>
        <div class="tf-decide">
          <button type="button" class="tf-btn tf-lg tf-yes" onclick={() => decide("yes")}>Link them <kbd>Y</kbd></button>
          <button type="button" class="tf-btn tf-lg tf-no" onclick={() => decide("no")}>Not related <kbd>N</kbd></button>
          <button type="button" class="tf-btn tf-lg tf-ghost" onclick={() => decide("skip")}>Decide later <kbd>S</kbd></button>
          <span class="tf-hint">Not related is remembered.</span>
        </div>
      </div>
    {/if}
  </section>
</div>
