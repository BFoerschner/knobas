<!--
  Link sets (#866, R-7.6): `#/sets`, prototype-6's `renderSets()`. The intro,
  *New link set*, the filter bar (text over names and members, *Applies by*
  chips), the table: each set's name and description, what is in it per kind,
  one line per rule, and the tickets it shows on now with how many are yours,
  then R-7.3's sub-task switch, which is about every ticket and not about these
  rows, so it stands below them. A row opens the set's editor, `#/sets/<id>`
  (`TfSetEditor.svelte`), which this page draws in its place. With no sets the
  empty block holds the one *New link set*, and there is nothing to filter.

  *New link set* opens an editor with nothing saved yet (`NEW_SET`): the first
  change saves it, so leaving at once leaves no empty row.

  Over `link_sets`, `save_link_set` (the hint's *Add to …*),
  `link_set_hints` (#870, R-7.10's card), `parent_links` and
  `set_parent_links`. Sets stay in knobas: nothing here writes to a source.
-->
<script lang="ts">
  import { ipcErrorMessage } from "../../ipc";
  import type { LinkSetHint, LinkSetRow } from "../../ipc/entity";
  import { push, undoable } from "../../shell/toasts.svelte";
  import type { EscTarget } from "../escape";
  import TfCrumbs from "../TfCrumbs.svelte";
  import TfWaiting from "../TfWaiting.svelte";
  import { waitPhase } from "../waiting";
  import FilterBar from "../filter/FilterBar.svelte";
  import TfNoMatch from "../filter/TfNoMatch.svelte";
  import { sessionFilter } from "../filter/filter-state.svelte";
  import type { FilterConfig } from "../filter/filter";
  import Marked from "../filter/Marked.svelte";
  import { draftOf } from "./apply";
  import { firstHint, notNow } from "./hints.svelte";
  import { defaultPorts, type SetPorts } from "./ports";
  import { APPLIES_BY, contentLines, hintTickets, keyWords, NEW_SET, ruleWords, visibleSets } from "./sets";
  import TfKey from "../TfKey.svelte";
  import TfSetEditor from "./TfSetEditor.svelte";

  let {
    ready,
    rest,
    ports = defaultPorts,
    back = null,
  }: {
    /** Whether the database answers yet: the reads wait for it. */
    ready: boolean;
    /** What follows `sets` in the address: a set's id, or nothing. */
    rest: string[];
    ports?: SetPorts;
    /** Where `Esc` goes, which the breadcrumb's hint names; `null` where it does nothing. */
    back?: Pick<EscTarget, "name"> | null;
  } = $props();

  const editing = $derived(rest[0] ? decodeURIComponent(rest[0]) : null);

  let sets = $state<LinkSetRow[] | null>(null);
  let failed = $state<string | null>(null);
  /** Moves on *Try again*, which the read below follows. */
  let attempt = $state(0);
  let inherit = $state<boolean | null>(null);
  /** R-7.10's hints (#870); the card draws the first one not put away. */
  let hints = $state<LinkSetHint[]>([]);
  let adding = $state(false);
  const hint = $derived(firstHint(hints));

  $effect(() => {
    void attempt;
    if (!ready || editing !== null) return;
    let live = true;
    failed = null;
    ports.linkSets().then(
      (rows) => {
        if (live) sets = rows;
      },
      (error: unknown) => {
        if (live) failed = ipcErrorMessage(error);
      },
    );
    // A hint is an offer: a failed read draws no card and says nothing.
    ports.linkSetHints().then(
      (rows) => {
        if (live) hints = rows;
      },
      () => {},
    );
    ports.parentLinks().then(
      (on) => {
        if (live) inherit = on;
      },
      () => {},
    );
    return () => {
      live = false;
    };
  });

  const filter = sessionFilter("sets");
  const config: FilterConfig = {
    id: "sets",
    placeholder: "Filter link sets",
    noun: "link sets",
    one: "link set",
    chips: [{ key: "by", label: "Applies by", options: APPLIES_BY.map(({ value, label }) => ({ value, label })) }],
  };
  const rows = $derived(sets === null ? [] : visibleSets(sets, filter.text, filter.chips["by"] ?? []));

  async function switchInherit(on: boolean) {
    try {
      inherit = await ports.setParentLinks(on);
      push({ text: on ? "Sub-tasks show their parent’s connections again" : "Sub-tasks show only their own connections" });
    } catch (error) {
      push({ text: ipcErrorMessage(error), tone: "err" });
    }
  }

  function newSet() {
    location.hash = `#/sets/${NEW_SET}`;
  }

  /** *Add to …*: the set saved whole with the item added, then the list and the hints read again. */
  async function addHinted(offer: LinkSetHint) {
    if (adding) return;
    adding = true;
    try {
      const set = sets?.find((row) => row.id === offer.set_id) ?? (await ports.linkSet(offer.set_id));
      const draft = draftOf(set);
      draft.members.push({ kind: offer.member.kind, entity_id: offer.member.entity_id });
      await ports.saveLinkSet(draft);
      undoable(`Added ${offer.title} to ${offer.set_name}`, () => void takeBack(offer));
      await reread();
    } catch (error) {
      push({ text: ipcErrorMessage(error), tone: "err" });
    } finally {
      adding = false;
    }
  }

  async function reread() {
    const [rows, offered] = await Promise.all([ports.linkSets(), ports.linkSetHints().catch(() => [])]);
    sets = rows;
    hints = offered;
  }

  /** *Undo* on *Add to …*: the set as it stands now, without the item, so the offer comes back. */
  async function takeBack(offer: LinkSetHint) {
    try {
      const draft = draftOf(await ports.linkSet(offer.set_id));
      draft.members = draft.members.filter((member) => member.entity_id !== offer.member.entity_id);
      await ports.saveLinkSet(draft);
      await reread();
    } catch (error) {
      push({ text: `Could not undo it: ${ipcErrorMessage(error)}`, tone: "err" });
    }
  }

  /** A row opens its set; a click on a link inside it already did its own thing. */
  function open(event: MouseEvent, id: string) {
    if ((event.target as Element | null)?.closest("a, button")) return;
    location.hash = `#/sets/${encodeURIComponent(id)}`;
  }
</script>

<div class="tf-full">
{#if editing !== null}
  <TfSetEditor {ready} id={editing} {ports} {back} />
{:else}
  <TfCrumbs parts={[{ name: "Link sets" }]} {back} sectionTop />
  <header class="tf-fhead">
    <div>
      <h1>Link sets</h1>
      <div class="tf-sub tf-sets-intro">
        Set it up once. A link set is a named group of repos, pipelines, pages, assets and monitors. A rule shows it on
        every ticket under an epic, every sub-task of a ticket or every ticket in a project. Jobs read it live: nothing
        is copied onto tickets and nothing is written to Jira.
      </div>
    </div>
    {#if sets !== null && sets.length > 0}
      <div class="tf-acts">
        <button type="button" class="tf-btn tf-primary" onclick={newSet}>New link set</button>
      </div>
    {/if}
  </header>
  {#if sets === null || sets.length > 0}
    <FilterBar {config} {filter} shown={sets === null ? null : rows.length} total={sets?.length ?? null} />
  {/if}
  <div class="tf-cbody tf-sets">
    {#if hint !== null}
      {@const offer = hint}
      <div class="tf-hint-card">
        <span
          >You linked <b>{offer.title}</b> by hand to {#each keyWords(hintTickets(offer)) as word, w (w)}{#if word.key}<TfKey value={word.text} />{:else}{word.text}{/if}{/each}. Add it to <b>{offer.set_name}</b>?</span
        >
        <span class="tf-trail">
          <button type="button" class="tf-btn tf-sm" disabled={adding} onclick={() => void addHinted(offer)}
            >Add to {offer.set_name}</button
          >
          <button type="button" class="tf-btn tf-sm tf-ghost" onclick={() => notNow(offer)}>Not now</button>
        </span>
      </div>
    {/if}
    {#if failed !== null && sets === null}
      <div class="tf-empty">
        <p class="tf-ifail">Could not read the link sets: {failed}</p>
        <button type="button" class="tf-btn" onclick={() => attempt++}>Try again</button>
      </div>
    {:else if sets === null}
      <TfWaiting phase={waitPhase({ drawn: false, reading: true, failed: false })} columns={4}><span></span></TfWaiting>
    {:else}
      {#if rows.length > 0}
        <table class="tf-tbl tf-sets-tbl">
          <thead><tr><th>Link set</th><th>What is in it</th><th>Applies to</th><th class="tf-sets-now">Tickets now</th></tr></thead>
          <tbody>
            {#each rows as set (set.id)}
              <tr class="tf-row" onclick={(event) => open(event, set.id)}>
                <td
                  ><a class="tf-set-name" href="#/sets/{encodeURIComponent(set.id)}"><Marked text={set.name} typed={filter.text} /></a
                  >{#if set.description}<span class="tf-sub">{set.description}</span>{/if}</td
                >
                <td>{#if set.members.length + set.patterns.length > 0}{@const lines = contentLines(set)}{#each lines as line, at (line)}<span class="tf-sets-item">{line}{at < lines.length - 1 ? "," : ""}</span>{" "}{/each}{:else}<span class="tf-dim">nothing yet</span>{/if}</td>
                <td>
                  {#each set.rules as rule, at (at)}<span class="tf-sets-rule">{#each ruleWords(rule) as word, w (w)}{#if word.key}<TfKey value={word.text} />{:else}{word.text}{/if}{/each}</span>{:else}<span class="tf-dim"
                      >nothing yet</span
                    >{/each}
                </td>
                <td class="tf-num tf-sets-now">{set.tickets} <span class="tf-dim">({set.mine} yours)</span></td>
              </tr>
            {/each}
          </tbody>
        </table>
      {:else if sets.length === 0}
        <div class="tf-empty">
          <p>No link sets yet. A set names the repos, pipelines, pages, assets and monitors a kind of work needs, once.</p>
          <button type="button" class="tf-btn tf-primary" onclick={newSet}>New link set</button>
        </div>
      {:else}
        <TfNoMatch noun="link sets" {filter} />
      {/if}
    {/if}
    {#if inherit !== null}
      <label class="tf-toggle">
        <input type="checkbox" checked={inherit} onchange={(event) => void switchInherit(event.currentTarget.checked)} />
        <span
          ><b>Sub-tasks show their parent ticket’s connections</b><span class="tf-dim tf-toggle-sub"
            >A sub-task shows what is linked on its parent, marked as coming from it. Works for every parent, no set
            needed.</span
          ></span
        >
      </label>
    {/if}
  </div>
{/if}
</div>
