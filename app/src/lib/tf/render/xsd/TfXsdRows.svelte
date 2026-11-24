<!--
  What an element or a type holds, as rows (the XSD design, section 10): attributes first, then the elements in the
  order they must stand, an extension's base before its own, groups opened, a choice as *One of these* with every
  branch a row. A row is the API explorer's field grid: caret, name, type, notes, and *how often* where the API
  explorer says *required*; under it its documentation's first paragraph and a line of facts.

  The whole row opens what it holds in place, one step in under a guide, read from the worker the first time
  (`XsdState.open`); Alt-click opens or shuts everything under it. A row already on the way down says *It holds
  itself.* and does not open again. Keys on a row: `→` opens or steps in, `←` shuts or goes to its holder, `↑` `↓`
  move, `Enter` or `Space` toggle, with Alt for everything under it.
-->
<script lang="ts">
  import type { Row } from "../../../render/xsd-answer";
  import TfXsdName from "./TfXsdName.svelte";
  import TfXsdOcc from "./TfXsdOcc.svelte";
  import TfXsdRows from "./TfXsdRows.svelte";
  import TfXsdRuns from "./TfXsdRuns.svelte";
  import TfXsdType from "./TfXsdType.svelte";
  import type { XsdState } from "./xsd.svelte";

  let { st, at, rows, ongo }: { st: XsdState; at: string; rows: readonly Row[]; ongo: (key: string, label: string) => void } = $props();
  /** A choice's branches in its select: a schema's own list, written by hand. */
  const BRANCHES_SHOWN = 200;

  /** A list of more than 80 rows draws 80 and *Show the other* (section 10); shown whole, at most a set's bound. */
  const ROWS_SHOWN = 80;
  const ROWS_ALL = 4000;
  const whole = $derived(st.more.has(`rows:${at}`));

  function press(event: MouseEvent, id: string) {
    const target = event.target as Element | null;
    if (target?.closest("button, a, select") !== null && target?.closest("button, a, select") !== event.currentTarget) return;
    if ((window.getSelection()?.toString() ?? "") !== "") return;
    void st.toggle(id, event.altKey);
  }

  function rowsOf(from: HTMLElement): HTMLElement[] {
    const page = from.closest(".tf-oas-page") ?? document;
    return [...page.querySelectorAll<HTMLElement>(".tf-oas-fr[tabindex='0']")];
  }

  function keys(event: KeyboardEvent, id: string, can: boolean) {
    if (event.metaKey || event.ctrlKey || event.target !== event.currentTarget) return;
    const row = event.currentTarget as HTMLElement;
    const open = st.expand.has(id);
    const go = (to: HTMLElement | null | undefined) => {
      if (to == null) return;
      to.focus();
      to.scrollIntoView?.({ block: "nearest" });
    };
    if (event.key === "ArrowRight") {
      if (can && !open) void st.toggle(id, event.altKey);
      else if (open) go(row.parentElement?.querySelector<HTMLElement>(":scope > .tf-oas-in .tf-oas-fr[tabindex='0']"));
    } else if (event.key === "ArrowLeft") {
      if (open) void st.toggle(id, event.altKey);
      else go(row.closest(".tf-oas-in")?.parentElement?.querySelector<HTMLElement>(":scope > .tf-oas-fr"));
    } else if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      const all = rowsOf(row);
      go(all[all.indexOf(row) + (event.key === "ArrowDown" ? 1 : -1)]);
    } else if ((event.key === "Enter" || event.key === " ") && can) void st.toggle(id, event.altKey);
    else return;
    event.preventDefault();
  }
</script>

{#each whole ? rows.slice(0, ROWS_ALL) : rows.slice(0, ROWS_SHOWN) as row (row.id)}
  {#if row.k === "attr"}
    <div class="tf-oas-f">
      <div class="tf-oas-fr">
        <span class="tf-oas-tog"></span>
        <span class="tf-oas-fname"><span class="tf-xsd-pre">{row.name.prefix}</span>{row.name.local}</span>
        <span class="tf-oas-ftype"><TfXsdType type={row.type} {ongo} /></span>
        <span class="tf-oas-fnotes">{#if row.notes.length > 0}<span><TfXsdRuns runs={row.notes} /></span>{/if}</span>
        <TfXsdOcc occ={row.occ} />
        {#if row.docu !== null}<p class="tf-oas-fd">{#if row.docu.lang !== null}<span class="tf-lab tf-xsd-lang">{row.docu.lang}</span>{/if}{row.docu.text}</p>{/if}
        {#if row.facts.length > 0}<div class="tf-oas-fd tf-oas-facts">{#each row.facts as fact, i (i)}<span><TfXsdRuns runs={fact} /></span>{/each}</div>{/if}
      </div>
    </div>
  {:else if row.k === "el"}
    {@const open = row.can && st.expand.has(row.id)}
    {@const inner = st.kids.get(row.id)}
    <div class="tf-oas-f" aria-expanded={row.can ? open : undefined}>
      <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
      <div
        class="tf-oas-fr"
        class:tf-oas-can={row.can}
        data-k={row.id}
        role={row.can ? "button" : undefined}
        tabindex={row.can ? 0 : -1}
        aria-expanded={row.can ? open : undefined}
        aria-label={row.can ? `${open ? "Close" : "Open"} ${row.name.local}` : undefined}
        onclick={(e) => press(e, row.id)}
        onkeydown={(e) => keys(e, row.id, row.can)}
      >
        <span class="tf-oas-tog">{#if row.can}<svg viewBox="0 0 10 10" fill="none" stroke="currentColor" stroke-width="1.6" aria-hidden="true"><path d="M3 2l4 3-4 3" /></svg>{/if}</span>
        <span class="tf-oas-fname">{#if row.lost}<span class="tf-xsd-lostname">{row.name.prefix}{row.name.local}</span>{:else}<TfXsdName name={row.name} {ongo} cls="tf-xsd-nref" />{/if}</span>
        <span class="tf-oas-ftype">{#if row.lost}<span class="tf-xsd-lost">not found here</span>{:else}<TfXsdType type={row.type} {ongo} />{#if row.again} <span class="tf-xsd-own">It holds itself.</span>{/if}{/if}</span>
        <span class="tf-oas-fnotes">{#if row.notes.length > 0}<span><TfXsdRuns runs={row.notes} /></span>{/if}</span>
        <TfXsdOcc occ={row.occ} />
        {#if row.docu !== null}<p class="tf-oas-fd">{#if row.docu.lang !== null}<span class="tf-lab tf-xsd-lang">{row.docu.lang}</span>{/if}{row.docu.text}</p>{/if}
        {#if row.facts.length > 0}<div class="tf-oas-fd tf-oas-facts">{#each row.facts as fact, i (i)}<span><TfXsdRuns runs={fact} /></span>{/each}</div>{/if}
      </div>
      {#if open}
        <div class="tf-oas-in">
          {#if inner === undefined}<p class="tf-oas-fnote" role="status">Reading…</p>
          {:else if inner.length === 0}<p class="tf-oas-fnote">It holds nothing.</p>
          {:else}<div class="tf-oas-fs"><TfXsdRows {st} at={row.id} rows={inner} {ongo} /></div>{/if}
        </div>
      {/if}
    </div>
  {:else if row.k === "group"}
    <div class="tf-xsd-grp">
      <div class="tf-xsd-gl">
        <b>{row.words}</b>
        {#if row.from !== null}<span>from the group <TfXsdName name={row.from} {ongo} /></span>{/if}
        {#if row.choice !== null && st.made !== null}
          {@const id = row.choice.id}
          {@const passed = st.made.example.choices.find((c) => c.id === id)}
          {#if passed !== undefined}
            <label class="tf-xsd-takes"
              >The example takes <select aria-label="The example takes" value={passed.picked} onchange={(e) => st.pick(id, Number((e.currentTarget as HTMLSelectElement).value))}
                >{#each passed.names.slice(0, BRANCHES_SHOWN) as name, i (i)}<option value={i}>{name}</option>{/each}</select
              ></label
            >
          {/if}
        {/if}
        <TfXsdOcc occ={row.occ} />
      </div>
      <div class="tf-oas-in"><div class="tf-oas-fs"><TfXsdRows {st} at={row.id} rows={row.items} {ongo} /></div></div>
    </div>
  {:else if row.k === "any"}
    <div class="tf-oas-f">
      <div class="tf-oas-fr">
        <span class="tf-oas-tog"></span>
        <span class="tf-oas-fname tf-xsd-own">any element</span>
        <span class="tf-oas-ftype tf-xsd-own">{row.words}</span>
        <span class="tf-oas-fnotes"></span>
        <TfXsdOcc occ={row.occ} />
      </div>
    </div>
  {:else}
    <p class="tf-oas-fnote"><TfXsdRuns runs={row.runs} />{#if row.lost !== null}<span class="tf-xsd-lostname">{row.lost.prefix}{row.lost.local}</span> <span class="tf-xsd-lost">not found here</span>{/if}</p>
  {/if}
{/each}
{#if !whole && rows.length > ROWS_SHOWN}<p class="tf-oas-sub"><button type="button" class="tf-oas-more" onclick={() => st.more.add(`rows:${at}`)}>Show the other {(rows.length - ROWS_SHOWN).toLocaleString("en")}</button></p>{/if}
