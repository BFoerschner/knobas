<!--
  *New route* / *Edit route* — the pane's route editor (issue #432, spec #427
  stories 12, 13 and 14).

  One dialog for both, because they ask for the same four things and a second
  component would be the copy that drifts: what it is called, where it answers,
  who can reach it, and which asset it lands on. Which of the two it is, is
  whether a `route` was handed in.

  ## The target is picked by walking, not typed

  A route lands on an asset, and an asset is a tree of unknown depth — so the
  picker is `MoveDialog`'s: one read per level, `assetTree(parentId)`, standing
  where the route is exposed because that is where its target usually is. What
  differs is that choosing and walking are the same click here: clicking a row
  makes it the target *and* opens what it holds, so landing on a container two
  levels down is two clicks and no mode.

  **Landing on nothing is a first-class choice**, not an empty field: an
  endpoint that lands on nothing knobas knows is a route the model holds on
  purpose (a health path, a database port), and *Clear* is how a reader says
  so.

  ## What is not here

  The **asset that exposes** the route. A route is the address of the thing
  that answers it, so re-exposing one elsewhere is a different route with a
  different history — `assets::edit_route` has no such edit and neither has
  this. And the route's **properties**: `RouteEdit`'s property arm is on the
  wire and the import writes through it, but the editor that draws a key, a
  kind and a value is the pane's own and is built around an asset's schema;
  generalising it is a ticket, not a paragraph.
-->
<script lang="ts">
  import { ipcErrorMessage } from "../ipc";
  import type {
    assetTree,
    AssetRow,
    createRoute,
    deleteRoute,
    editRoute,
    RouteEdit,
    RouteRow,
    Visibility,
  } from "../ipc/assets";
  import { latestRead } from "../shell/latest-read";
  import Modal from "../shell/Modal.svelte";

  let {
    asset,
    heldBy,
    route = null,
    tree,
    create,
    edit,
    remove,
    onclose,
    onsaved,
    ondeleted,
  }: {
    /** The asset that exposes the route — the pane's own. */
    asset: AssetRow;
    /** Its ancestors, outermost first: where the target picker opens. */
    heldBy: AssetRow[];
    /** The route being edited, or `null` to expose a new one. */
    route?: RouteRow | null;
    tree: typeof assetTree;
    create: typeof createRoute;
    edit: typeof editRoute;
    remove: typeof deleteRoute;
    onclose: () => void;
    /** The route was written. The Tree re-reads; this dialog is done. */
    onsaved: (row: RouteRow) => void;
    ondeleted: () => void;
  } = $props();

  // svelte-ignore state_referenced_locally
  // Read once, at init: this is what the dialog *opens* on, and re-deriving it
  // would overwrite what the reader has typed the next time the pane re-read.
  let name = $state(route?.name ?? "");
  // svelte-ignore state_referenced_locally
  let url = $state(route?.url ?? "");
  // svelte-ignore state_referenced_locally
  let visibility = $state<Visibility>(route?.visibility ?? "internal");
  // svelte-ignore state_referenced_locally
  let target = $state<{ id: string; name: string } | null>(
    route?.target_id === null || route?.target_id === undefined
      ? null
      : { id: route.target_id, name: route.target_name ?? route.target_id },
  );

  // svelte-ignore state_referenced_locally
  // The picker opens where the route is exposed, which is where its target
  // usually is — `MoveDialog`'s reason for seeding these two together.
  let crumb = $state<AssetRow[]>([...heldBy, asset]);
  // svelte-ignore state_referenced_locally
  let at = $state<string | null>(asset.id);
  let rows = $state<AssetRow[]>([]);
  let loaded = $state(false);
  let failure = $state<string | null>(null);
  let writing = $state(false);

  const columnRead = latestRead<AssetRow[]>();

  const editing = $derived(route !== null);
  /** Unique per instance, so two stacked dialogs cannot share a field id. */
  const fieldId = `route-${Math.random().toString(36).slice(2, 9)}`;

  $effect(() => {
    const parent = at;
    void columnRead(() => tree(parent), {
      ok: (answer) => {
        rows = answer;
        loaded = true;
      },
      fail: (cause) => {
        rows = [];
        failure = ipcErrorMessage(cause);
        loaded = true;
      },
    });
  });

  /** Stand at the foot of `path` — the top of the estate when it is empty. */
  function stand(path: AssetRow[]) {
    crumb = path;
    at = path.at(-1)?.id ?? null;
    failure = null;
  }

  /** One click: this row is the target, and what it holds is the next level. */
  function choose(row: AssetRow) {
    target = { id: row.id, name: row.name };
    stand([...crumb, row]);
  }

  function onfieldkeydown(event: KeyboardEvent) {
    if (event.key !== "Enter") return;
    event.preventDefault();
    void submit();
  }

  /**
   * The edits this dialog would send — every field, and the backend drops the
   * ones that changed nothing.
   *
   * Deliberately not a diff computed here: `assets::edit_route` already writes
   * no line for an edit that changes nothing, and a second copy of that
   * comparison in the frontend is the copy that goes stale — it would have to
   * know that a name is trimmed and a URL is not.
   */
  function edits(): RouteEdit[] {
    return [
      { field: "name", value: name },
      { field: "url", value: url },
      { field: "target", value: target?.id ?? null },
      { field: "visibility", value: visibility },
    ];
  }

  async function submit() {
    if (writing || name.trim() === "" || url.trim() === "") return;
    writing = true;
    failure = null;
    try {
      const written =
        route === null
          ? await create(asset.id, name, url, target?.id ?? null, visibility)
          : await edit(route.id, edits());
      onsaved(written);
    } catch (rejection) {
      // In place, and in the backend's own words: a URL with no scheme and a
      // target that is not there are refused by name.
      failure = ipcErrorMessage(rejection);
    } finally {
      writing = false;
    }
  }

  async function drop() {
    if (writing || route === null) return;
    writing = true;
    failure = null;
    try {
      await remove(route.id);
      ondeleted();
    } catch (rejection) {
      failure = ipcErrorMessage(rejection);
    } finally {
      writing = false;
    }
  }
</script>

<Modal title={editing ? "Edit route" : "New route"} subtitle={`On ${asset.name}`} {onclose}>
  {#snippet body()}
    <div class="fields">
      <div class="fld">
        <label class="l" for="{fieldId}-name">Name</label>
        <!-- `Modal` focuses the first focusable control, which is this one. -->
        <input
          class="inp"
          id="{fieldId}-name"
          type="text"
          autocomplete="off"
          placeholder="Gitea"
          bind:value={name}
          onkeydown={onfieldkeydown}
        />
      </div>

      <div class="fld">
        <label class="l" for="{fieldId}-url">URL or endpoint</label>
        <input
          class="inp"
          id="{fieldId}-url"
          type="text"
          autocomplete="off"
          placeholder="https://gitea.local/"
          bind:value={url}
          onkeydown={onfieldkeydown}
        />
        <p class="hint">Any scheme: <code>https://</code>, <code>postgres://</code>, <code>ssh://</code>.</p>
      </div>

      <div class="fld">
        <label class="l" for="{fieldId}-vis">Visibility</label>
        <select class="inp" id="{fieldId}-vis" bind:value={visibility} onkeydown={onfieldkeydown}>
          <option value="internal">Internal</option>
          <option value="public">Public</option>
        </select>
      </div>

      <div class="fld">
        <span class="l">Lands on</span>
        <p class="lands">
          {#if target}
            <span class="nm">{target.name}</span>
            <button class="link" onclick={() => (target = null)}>Clear</button>
          {:else}
            <span class="faint">Nothing — an endpoint knobas has no asset for</span>
          {/if}
        </p>

        <p class="crumb mono">
          <button class="link" disabled={crumb.length === 0} onclick={() => stand([])}>
            Estate
          </button>
          {#each crumb as step, index (step.id)}
            <span class="sep" aria-hidden="true">/</span>
            <button
              class="link"
              disabled={index === crumb.length - 1}
              onclick={() => stand(crumb.slice(0, index + 1))}
            >
              {step.name}
            </button>
          {/each}
        </p>

        <div class="lvl">
          <ul class="opts" aria-label="Where it lands">
            {#each rows as row (row.id)}
              <li>
                <button
                  class="opt"
                  class:on={target?.id === row.id}
                  title="Land on {row.name}"
                  onclick={() => choose(row)}
                >
                  <span class="mg">{row.monogram}</span>
                  <span class="nm">{row.name}</span>
                  {#if row.has_children}
                    <span class="chev" aria-hidden="true">›</span>
                  {/if}
                </button>
              </li>
            {/each}
          </ul>
          {#if loaded && rows.length === 0}
            <p class="empty">Nothing here.</p>
          {/if}
        </div>
      </div>

      {#if failure}
        <p class="fail" role="alert">{failure}</p>
      {/if}
    </div>
  {/snippet}

  {#snippet footer()}
    {#if editing}
      <button class="btn danger" disabled={writing} onclick={() => void drop()}>Delete</button>
    {/if}
    <button class="btn" onclick={onclose}>Cancel</button>
    <button
      class="btn pri"
      disabled={writing || name.trim() === "" || url.trim() === ""}
      onclick={() => void submit()}
    >
      {writing ? "Saving…" : editing ? "Save" : "Expose"}
    </button>
  {/snippet}
</Modal>

<style>
  .fields {
    display: grid;
    gap: 12px;
  }

  .fld .l {
    display: block;
    font: 500 10px var(--mono);
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--faint);
    margin-bottom: 4px;
  }

  .hint {
    margin-top: 6px;
    font-size: 11px;
    color: var(--muted);
  }

  .lands {
    display: flex;
    align-items: baseline;
    gap: 8px;
    margin: 0 0 6px;
    font-size: 12px;
  }

  .lands .nm {
    color: var(--text);
  }

  .crumb {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: 4px;
    margin: 0 0 6px;
    font-size: 11px;
    color: var(--muted);
  }

  .crumb button:disabled {
    color: var(--text);
    cursor: default;
  }

  /* Bounded, for `MoveDialog`'s reason: a level with fifty rows must not push
     the footer out of reach. */
  .lvl {
    max-height: 200px;
    overflow: auto;
    border: 1px solid var(--hair);
    border-radius: 2px;
  }

  .opts {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .opt {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    height: var(--row);
    padding: 0 8px 0 10px;
    text-align: left;
    color: var(--text);
    font-size: 12px;
  }

  .opt:hover {
    background: var(--raised);
  }

  .opt.on {
    background: var(--raised);
    box-shadow: inset 2px 0 0 var(--link);
  }

  .opt .nm {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .opt .mg {
    flex: none;
    font: 500 10px var(--mono);
    color: var(--faint);
  }

  .opt .chev {
    flex: none;
    color: var(--faint);
  }

  .empty {
    padding: 8px 10px;
    font-size: 12px;
    color: var(--muted);
  }
</style>
