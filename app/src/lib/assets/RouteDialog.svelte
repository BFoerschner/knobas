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
  this.

  ## The properties are the reader's own, all of them

  A route has no type, so it declares no keys and every property is a custom
  one — which is what makes the editor here simpler than the pane's: there is
  no schema to ask which kind an unfilled key takes, so a row is a key, a kind
  and a value, and the four kinds come from `editing.ts` rather than from a
  list of this file's own. It is where spec story 14's *certificate expiry*
  goes: knobas does not own monitoring, so an expiry knobas **checks** is a
  Kuma monitor (M4.1) and an expiry knobas **records** is one of these.

  Emptying a value **clears** the property, which is `parseProperty`'s reading
  and `assets::edit_route`'s: a blank text is refused with *clear the property
  instead*, so a reader who empties a field means the key to go.
-->
<script lang="ts">
  import { ipcErrorMessage } from "../ipc";
  import type {
    assetTree,
    AssetRow,
    createRoute,
    deleteRoute,
    editRoute,
    PropertyKind,
    PropertyValue,
    RouteEdit,
    RouteRow,
    Visibility,
  } from "../ipc/assets";
  import { latestRead } from "../shell/latest-read";
  import Modal from "../shell/Modal.svelte";
  import { inputTypeFor, parseProperty, propertyEdit, PROPERTY_KINDS } from "./editing";

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

  /**
   * The route's properties, as rows a person edits: a key, the kind its value
   * is entered as, and the value as text.
   *
   * Seeded from the route's own list, which is all custom — a route declares
   * nothing — so the kind comes off the stored value and falls back to text
   * for a value that could not be read back. A row whose value is emptied is
   * a **clear**, and a row whose key is emptied is dropped before anything is
   * sent, because a key with nothing in it is a property the backend would
   * remove in the same breath it was added.
   */
  interface PropertyRow {
    key: string;
    kind: PropertyKind;
    value: string;
  }

  // svelte-ignore state_referenced_locally
  let properties = $state<PropertyRow[]>(
    (route?.properties ?? []).map((property) => ({
      key: property.key,
      kind: property.value?.kind ?? "text",
      value: property.value === null ? "" : String(property.value.value),
    })),
  );
  // svelte-ignore state_referenced_locally
  /** The keys the route arrived with — what a removed row has to clear. */
  const had: string[] = (route?.properties ?? []).map((property) => property.key);

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
  function edits(): RouteEdit[] | { refused: string } {
    const parsed = pairs();
    if ("refused" in parsed) return parsed;
    const kept = new Set(parsed.pairs.map(([key]) => key));
    return [
      { field: "name", value: name },
      { field: "url", value: url },
      { field: "target", value: target?.id ?? null },
      { field: "visibility", value: visibility },
      // A key the route had and this dialog no longer lists is a clear, which
      // is what `null` on the property arm means. Written from the keys rather
      // than from a *remove* flag: the row is gone, and what the wire needs is
      // its name.
      ...had.filter((key) => !kept.has(key)).map((key) => propertyEdit(key, null)),
      ...parsed.pairs.map(([key, value]) => propertyEdit(key, value)),
    ];
  }

  /**
   * The property rows as the wire carries them, or the one refusal
   * `parseProperty` makes: a number that is not a number.
   *
   * Everything else a value can be wrong about is the backend's to refuse, by
   * name and in place — `editing.ts`' rule, and the reason this is the only
   * check here.
   */
  function pairs(): { pairs: [string, PropertyValue | null][] } | { refused: string } {
    const out: [string, PropertyValue | null][] = [];
    for (const row of properties) {
      const key = row.key.trim();
      if (key === "") continue;
      const parsed = parseProperty(row.kind, row.value);
      if ("refused" in parsed) return parsed;
      out.push([key, parsed.value]);
    }
    return { pairs: out };
  }

  async function submit() {
    if (writing || name.trim() === "" || url.trim() === "") return;
    const list = edits();
    if ("refused" in list) {
      failure = list.refused;
      return;
    }
    writing = true;
    failure = null;
    try {
      const written =
        route === null
          ? await create(
              asset.id,
              name,
              url,
              target?.id ?? null,
              visibility,
              // A create carries the properties as pairs; `null` values have
              // nothing to set, so they are dropped rather than sent as a
              // clear of a key that does not exist yet.
              list.flatMap((change) =>
                change.field === "property" && change.value !== null
                  ? [[change.key, change.value] as [string, PropertyValue]]
                  : [],
              ),
            )
          : await edit(route.id, list);
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
          placeholder="https://gitea.example/"
          bind:value={url}
          onkeydown={onfieldkeydown}
        />
        <!--
          The schemes are named without their separator: `house-rules.test.ts`
          reads every `https?://` in this directory as a network reference, and
          a hint that spelled one in full would be an offence against a rule
          worth keeping for the sake of three characters.
        -->
        <p class="hint">Any scheme — <code>https</code>, <code>postgres</code>, <code>ssh</code>; a bare host is refused.</p>
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

      <div class="fld">
        <span class="l">Properties</span>
        <p class="hint">
          A route declares no keys, so these are yours — the certificate expiry among
          them. Emptying a value clears the property.
        </p>
        {#each properties as property, index (index)}
          <div class="prow">
            <input
              class="inp k"
              type="text"
              aria-label="Property key"
              autocomplete="off"
              placeholder="cert_expires"
              bind:value={property.key}
            />
            <!-- The four kinds are enumerated once, in `editing.ts`. -->
            <select class="inp" aria-label="Property kind" bind:value={property.kind}>
              {#each PROPERTY_KINDS as offered (offered.kind)}
                <option value={offered.kind}>{offered.label}</option>
              {/each}
            </select>
            <!-- `value`/`oninput` rather than `bind:value`: Svelte refuses a
                 two-way binding on an input whose `type` is dynamic. -->
            <input
              class="inp"
              type={inputTypeFor(property.kind)}
              aria-label="Property value"
              autocomplete="off"
              value={property.value}
              oninput={(event) => (property.value = event.currentTarget.value)}
            />
            <button
              class="btn sm"
              title="Remove {property.key || 'this property'}"
              onclick={() => properties.splice(index, 1)}
            >
              ×
            </button>
          </div>
        {/each}
        <button
          class="btn sm"
          onclick={() => properties.push({ key: "", kind: "text", value: "" })}
        >
          Add a property
        </button>
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

  .prow {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto minmax(0, 1fr) auto;
    gap: 6px;
    margin-bottom: 6px;
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
