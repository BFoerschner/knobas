<!--
  One field row of the API explorer (ux31, section 8): a parameter, a header or a property. Its name, its type
  (a ref's name pressed goes to the schema's page), where it goes and whether it is required; under it its words,
  its line of facts, and *read only*, *write only* or *Deprecated*. The caret opens what it holds in place, one
  indent in under a hairline guide (`TfOasShape`), once the worker answered its shape, so it opens whole. A ref to
  a schema the row sits inside does not open again: it says so, and its name follows.
-->
<script lang="ts">
  import type { Snippet } from "svelte";

  import { factsOf, hasInside, nameParts, refName, summary } from "../../render/openapi-view";
  import TfOasShape from "./TfOasShape.svelte";
  import TfOasWords from "./TfOasWords.svelte";
  import { innerRef, type OasState } from "./oas.svelte";

  interface Props {
    st: OasState;
    name: string;
    node: unknown;
    base: string;
    /** What its caret and *Show all* are kept by. */
    k: string;
    required?: boolean;
    where?: string;
    words?: string;
    example?: unknown;
    /** An example made from the schema, where the spec gives none (a parameter's). */
    made?: unknown;
    deprecated?: boolean;
    /** The refs the row sits inside: one of them opening again would never end. */
    trail?: readonly string[];
    prose?: Snippet<[string]> | null;
    /** Goes to a named schema's page. */
    onfollow: (name: string) => void;
  }

  let {
    st,
    name,
    node,
    base,
    k,
    required = false,
    where = "",
    words = "",
    example = undefined,
    made = undefined,
    deprecated = false,
    trail = [],
    prose = null,
    onfollow,
  }: Props = $props();

  type Obj = Record<string, unknown>;
  const own = $derived((typeof node === "object" && node !== null && !Array.isArray(node) ? node : {}) as Obj);
  const line = $derived(summary(node));
  const inner = $derived(innerRef(node));
  const cycle = $derived(inner !== null && trail.includes(`${base}#${inner}`));
  const can = $derived(!cycle && hasInside(node));
  const open = $derived(can && st.expand.has(k));
  const shape = $derived(open ? st.shapes.get(k) : undefined);
  // A ref's facts are its schema's, drawn when it opens; a plain node's are its own.
  const facts = $derived(factsOf(inner === null ? node : null, example));
  const named = $derived(line.ref === null ? null : st.named(base, line.ref));
  const away = $derived(line.ref === null || named !== null ? null : st.hrefOf(base, line.ref));
  const label = $derived(line.ref === null ? "" : refName(line.ref));
  const dep = $derived(deprecated || own.deprecated === true);

  /**
   * A press on the row opens or shuts it; with Alt (Option) it opens or shuts everything below. A press on a ref, a
   * link or *Show all* is theirs, and a drag that selected a name (what people copy) toggles nothing.
   */
  function press(event: MouseEvent) {
    if (!can) return;
    const target = event.target as Element | null;
    if (target?.closest("button, a") !== null && target?.closest("button, a") !== event.currentTarget) return;
    if ((window.getSelection()?.toString() ?? "") !== "") return;
    toggle(event.altKey);
  }

  function toggle(all: boolean) {
    if (all) {
      if (open) st.closeAll(k);
      else void st.openAll(k, base, node, trail);
    } else st.toggleShape(k, base, node);
  }

  /** The rows of this page, in reading order: the keys move among them. */
  function rowsOf(from: HTMLElement): HTMLElement[] {
    const page = from.closest(".tf-oas-page") ?? from.closest(".tf-oas-inline") ?? document;
    return [...page.querySelectorAll<HTMLElement>(".tf-oas-fr")];
  }

  /** → opens (or steps in), ← shuts (or goes to the parent), ↑ ↓ move, Enter and Space toggle, Alt for all below. */
  function keys(event: KeyboardEvent) {
    if (event.metaKey || event.ctrlKey || event.target !== event.currentTarget) return;
    const row = event.currentTarget as HTMLElement;
    const go = (to: HTMLElement | null | undefined) => {
      if (to == null) return;
      to.focus();
      to.scrollIntoView?.({ block: "nearest" });
    };
    if (event.key === "ArrowRight") {
      if (can && !open) toggle(event.altKey);
      else if (open) go(row.parentElement?.querySelector<HTMLElement>(":scope > .tf-oas-in .tf-oas-fr"));
    } else if (event.key === "ArrowLeft") {
      if (open) toggle(event.altKey);
      else go(row.closest(".tf-oas-in")?.parentElement?.querySelector<HTMLElement>(":scope > .tf-oas-fr"));
    } else if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      const all = rowsOf(row);
      go(all[all.indexOf(row) + (event.key === "ArrowDown" ? 1 : -1)]);
    } else if ((event.key === "Enter" || event.key === " ") && can) toggle(event.altKey);
    else return;
    event.preventDefault();
  }
</script>

<div class="tf-oas-f" aria-expanded={can ? open : undefined}>
  <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
  <div
    class="tf-oas-fr"
    class:tf-oas-can={can}
    data-k={k}
    role={can ? "button" : undefined}
    tabindex={can ? 0 : -1}
    aria-expanded={can ? open : undefined}
    aria-label={can ? `${open ? "Close" : "Open"} ${name}` : undefined}
    onclick={press}
    onkeydown={keys}
  >
    <span class="tf-oas-tog">{#if can}<svg viewBox="0 0 10 10" fill="none" stroke="currentColor" stroke-width="1.6" aria-hidden="true"><path d="M3 2l4 3-4 3" /></svg>{/if}</span>
    <span class="tf-oas-fname" class:tf-oas-dep={dep}>{#each nameParts(name) as part, i (i)}{#if i > 0}<wbr />{/if}{part}{/each}</span>
    <span class="tf-oas-ftype"
      >{line.type}{#if line.ref !== null}{line.type !== "" ? " " : ""}{#if named !== null}<button type="button" class="tf-oas-ref" title="Open {named}" onclick={() => onfollow(named)}>{label}</button
        >{:else if away !== null}<a class="tf-oas-ref" href={away}>{label}</a>{:else}<span class="tf-oas-code">{label}</span>{/if}{/if}{#if line.nullable}, or null{/if}</span
    >
    {#if where !== "" || own.readOnly === true || own.writeOnly === true || dep}
      <span class="tf-oas-fnotes"
        >{#if where !== ""}<span>{where}</span>{/if}{#if own.readOnly === true}<span>read only</span>{/if}{#if own.writeOnly === true}<span>write only</span>{/if}{#if dep}<span class="tf-oas-warn">Deprecated</span>{/if}</span
      >
    {/if}
    {#if required}<span class="tf-oas-freq"><span class="tf-lab">required</span></span>{/if}
    {#if words !== ""}<div class="tf-oas-fd"><TfOasWords text={words} {st} {k} max={420} {prose} /></div>{/if}
    {#if facts.length > 0 || (made !== undefined && made !== null && example === undefined)}
      <div class="tf-oas-fd tf-oas-facts">
        {#each facts as fact, n (n)}<span>{#each fact as run, m (m)}{#if run.code}<code>{run.text}</code>{:else}{run.text}{/if}{/each}</span>{/each}
        {#if made !== undefined && made !== null && example === undefined}<span>Example <code>{typeof made === "string" ? made : JSON.stringify(made)}</code> <span class="tf-oas-exfrom">made from the schema</span></span>{/if}
      </div>
    {/if}
    {#if cycle && inner !== null}
      <div class="tf-oas-fd">It holds itself: follow {#if named !== null}<button type="button" class="tf-oas-ref" onclick={() => onfollow(named)}>{label}</button>{:else}<b>{refName(inner)}</b>{/if} to read it.</div>
    {/if}
  </div>
  {#if open}
    <div class="tf-oas-in">
      {#if shape !== undefined}
        <TfOasShape {st} {shape} {k} trail={inner === null ? trail : [...trail, `${base}#${inner}`]} {prose} {onfollow} />
      {:else}
        {@const got = inner === null ? undefined : st.ref(base, inner)}
        {#if got !== undefined && "reading" in got}<p class="tf-oas-fnote" role="status">Reading {got.reading}…</p>
        {:else if got !== undefined && "error" in got}<p class="tf-oas-fnote">{got.error}</p>{/if}
      {/if}
    </div>
  {/if}
</div>
