<!--
  An API spec in a comparison's *Rendered* stage (ux31, section 13): the inline form (`TfOasInline`) on each side, held
  to the operations and schemas that differ, each marked *Added*, *Removed*, *Changed* or *Changed through Pet*. The
  marks come from the spec worker (`changes`: `render/openapi-changes.ts`), which compares by JSON with refs left as
  refs, so a reordered document is no change. Both sides open the same rows, so a changed operation opens before and
  after at once. `n` and `p` step through the marks (`nextChange`), opening each on both sides. With nothing but
  formatting changed the stage says *The same API on both sides*. There is no request column: which side's server, and
  whose secret, would be a guess.
-->
<script lang="ts">
  import { untrack } from "svelte";
  import { SvelteSet } from "svelte/reactivity";
  import TfOasInline, { type InlineMark } from "./TfOasInline.svelte";
  import { SpecSession, specEngine, type SpecEngine } from "../../render/openapi-port";
  import { sameApi, type SpecChanges } from "../../render/openapi-changes";

  let {
    before,
    after,
    path,
    engine = null,
    onsay = () => {},
  }: {
    /** The side before's text, `null` for an added file. */
    before: string | null;
    /** The side after's text, `null` for a deleted file. */
    after: string | null;
    path: string;
    engine?: SpecEngine | null;
    /** What the stage's foot says: the change stepped to, or why there are no marks. */
    onsay?: (words: string) => void;
  } = $props();

  let changes = $state.raw<SpecChanges | null>(null);
  let failed = $state<string | null>(null);
  $effect(() => {
    const [old, nu, root] = [before, after, path];
    changes = null;
    failed = null;
    if (old === null || nu === null) return;
    const session = new SpecSession(untrack(() => engine) ?? specEngine(), null);
    let live = true;
    void session
      .open(root, nu)
      .then((opened) => (opened.ok ? session.changes(root, old) : { ok: false as const, error: opened.error }))
      .then((got) => {
        if (!live) return;
        if (got.ok) changes = got.changes;
        else failed = got.error;
      });
    return () => {
      live = false;
      session.close();
    };
  });

  /** Each side's marks, by row key. */
  const marks = $derived.by((): { old: Map<string, InlineMark>; new: Map<string, InlineMark> } | null => {
    const c = changes;
    if (c === null) return null;
    const old = new Map<string, InlineMark>();
    const nu = new Map<string, InlineMark>();
    for (const k of c.ops.removed) old.set(k, { mark: "removed" });
    for (const k of c.ops.added) nu.set(k, { mark: "added" });
    for (const { key, via } of c.ops.changed) {
      const m: InlineMark = via === undefined ? { mark: "changed" } : { mark: "changed", via };
      old.set(key, m);
      nu.set(key, m);
    }
    for (const n of c.schemas.removed) old.set(`schemas/${n}`, { mark: "removed" });
    for (const n of c.schemas.added) nu.set(`schemas/${n}`, { mark: "added" });
    for (const n of c.schemas.changed) {
      old.set(`schemas/${n}`, { mark: "changed" });
      nu.set(`schemas/${n}`, { mark: "changed" });
    }
    return { old, new: nu };
  });
  /** The marks in the order `n` steps them: the operations, then the schemas, as the lists draw them. */
  const steps = $derived.by((): string[] => {
    const c = changes;
    if (c === null) return [];
    return [...c.ops.changed.map((x) => x.key), ...c.ops.added, ...c.ops.removed, ...c.schemas.changed, ...c.schemas.added, ...c.schemas.removed];
  });

  const open = new SvelteSet<string>();
  let at = $state(-1);
  let oldSide = $state<ReturnType<typeof TfOasInline> | null>(null);
  let newSide = $state<ReturnType<typeof TfOasInline> | null>(null);

  $effect(() => {
    const n = steps.length;
    const c = changes;
    onsay(failed !== null ? `The marks could not be worked out: ${failed}` : c === null ? (before !== null && after !== null ? "Comparing the two sides…" : "") : sameApi(c) ? "The same API on both sides" : `Change ${Math.max(1, at + 1)} of ${n}`);
  });

  /** `n` (1) and `p` (-1): the next or previous mark opened on both sides and brought into view; false where none. */
  export function nextChange(step: 1 | -1, entering = false): boolean {
    const to = entering ? (step === 1 ? 0 : steps.length - 1) : at + step;
    if (to < 0 || to >= steps.length) return false;
    at = to;
    const k = steps[to]!;
    open.add(k);
    oldSide?.revealKey(k);
    newSide?.revealKey(k);
    return true;
  }
</script>

<div class="tf-oasd" class:tf-oasd-one={before === null || after === null}>
  {#if before !== null && after !== null && marks === null}
    <!-- The sides wait for the marks, so neither draws its whole list and then shrinks to what changed. -->
    {#if failed !== null}<p class="tf-stepfail" role="alert">{failed}</p>{:else}<p class="tf-oas-wait" role="status">Comparing the two sides…</p>{/if}
  {:else}
    {#if before !== null}
      <TfOasInline bind:this={oldSide} text={before} {path} {engine} fill marks={marks?.old ?? null} {open} label="API before" />
    {/if}
    {#if after !== null}
      <TfOasInline bind:this={newSide} text={after} {path} {engine} fill marks={marks?.new ?? null} {open} label="API after" />
    {/if}
  {/if}
</div>
