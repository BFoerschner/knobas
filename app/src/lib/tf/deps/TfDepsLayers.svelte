<!--
  *Layers…* (#873, R-11.13): the shared layers, top to bottom (add, rename
  with the uniqueness refusal in place, remove with a toast, reorder); *What
  goes where*, every kind with its count and its layer; and *In the view
  <name>*, what only this view keeps. The first two are written at once and
  say "Every view uses it"; the last changes the view's draft until *Save
  view*.
-->
<script lang="ts">
  import { tick } from "svelte";

  import { ipcErrorMessage } from "../../ipc";
  import type { DepGroup, DepLayer, DepTiles, Dependencies } from "../../ipc/assets";
  import { push } from "../../shell/toasts.svelte";
  import { kindOf } from "./deps";
  import { layerIn } from "./draw";
  import type { LayerPorts } from "./ports";
  import type { DepViewState } from "./views.svelte";
  import { FOLD_AT, freeLayerName, GROUPS, TILES } from "./views";

  let {
    read,
    views,
    ports,
    onlayers,
    onfoldreset,
    onclose,
  }: {
    read: Dependencies;
    views: DepViewState;
    ports: LayerPorts;
    /** The layers as the backend now holds them. */
    onlayers: (layers: DepLayer[]) => void;
    /** The view's *Folded* changed: drop the session's own fold of that layer. */
    onfoldreset: (layer: string) => void;
    onclose: () => void;
  } = $props();

  let errors = $state<Record<string, string>>({});
  let panel = $state<HTMLElement | null>(null);

  const draft = $derived(views.draft);
  const counts = $derived.by(() => {
    const by = new Map<string, number>();
    for (const item of read.items) by.set(item.kind, (by.get(item.kind) ?? 0) + 1);
    return by;
  });
  const layerOfKind = (kind: string) => read.layers.find((layer) => layer.kinds.includes(kind))?.id ?? "";
  const shared = (text: string) => push({ text: `${text}. Every view uses it.` });

  async function write(run: () => Promise<DepLayer[]>, said?: string) {
    try {
      onlayers(await run());
      if (said) shared(said);
      return true;
    } catch (error) {
      push({ text: ipcErrorMessage(error), tone: "err" });
      return false;
    }
  }

  async function rename(layer: DepLayer, input: HTMLInputElement) {
    const name = input.value.trim();
    if (name === layer.name) return;
    try {
      onlayers(await ports.renameDepLayer(layer.id, name));
      errors = { ...errors, [layer.id]: "" };
      shared(`Renamed ${layer.name} to ${name}`);
    } catch (error) {
      errors = { ...errors, [layer.id]: ipcErrorMessage(error) };
      if (name === "") input.value = layer.name;
      else {
        input.focus();
        input.select();
      }
    }
  }

  async function add() {
    const before = new Set(read.layers.map((layer) => layer.id));
    if (!(await write(() => ports.createDepLayer(freeLayerName(read.layers.map((layer) => layer.name)))))) return;
    await tick();
    const added = read.layers.find((layer) => !before.has(layer.id));
    const input = added ? panel?.querySelector<HTMLInputElement>(`[data-layer-name="${CSS.escape(added.id)}"]`) : null;
    input?.focus();
    input?.select();
  }

  function remove(layer: DepLayer) {
    const left = layer.kinds.map((kind) => kindOf(read.kinds, kind).label);
    void write(() => ports.deleteDepLayer(layer.id), `Removed ${layer.name}${left.length ? `; ${left.join(", ")} now sit in no layer` : ""}`);
  }

  function move(index: number, by: number) {
    const ids = read.layers.map((layer) => layer.id);
    [ids[index], ids[index + by]] = [ids[index + by]!, ids[index]!];
    void write(() => ports.orderDepLayers(ids));
  }

  function place(kind: string, layerId: string) {
    const label = kindOf(read.kinds, kind).label;
    const to = read.layers.find((layer) => layer.id === layerId);
    void write(() => ports.placeDepKind(kind, to?.id ?? null), `${label} ${to ? `now sits in ${to.name}` : "now sits in no layer"}`);
  }
</script>

<aside class="tf-dp-panel" aria-label="Layers" bind:this={panel}>
  <div class="tf-dp-ph">
    <span class="tf-dp-mk">Layers</span>
    <button type="button" class="tf-btn tf-sm tf-ghost" aria-label="Close" onclick={onclose}>✕</button>
  </div>
  <div class="tf-dp-ps">
    <h4>The layers, top to bottom</h4>
    <p class="tf-dim">The same in every view. Each name is used once, so a layer always holds the same things.</p>
    {#each read.layers as layer, index (layer.id)}
      <div class="tf-dp-lrow">
        <span class="tf-dp-arrows">
          <button type="button" class="tf-iconbtn tf-sm" disabled={index === 0} aria-label="Move {layer.name} up" onclick={() => move(index, -1)}>↑</button>
          <button type="button" class="tf-iconbtn tf-sm" disabled={index === read.layers.length - 1} aria-label="Move {layer.name} down" onclick={() => move(index, 1)}>↓</button>
        </span>
        <input type="text" value={layer.name} aria-label="Name of the layer" data-layer-name={layer.id} onchange={(e) => void rename(layer, e.currentTarget)} />
        <button type="button" class="tf-iconbtn tf-sm" aria-label="Remove {layer.name}" title="Remove the layer from every view; what was in it goes to no layer" onclick={() => remove(layer)}>✕</button>
        <span class="tf-dp-lkinds tf-dim">{layer.kinds.length ? layer.kinds.map((kind) => kindOf(read.kinds, kind).label).join(", ") : "Nothing yet: place kinds below"}</span>
        {#if errors[layer.id]}<p class="tf-dp-lerr" role="alert">{errors[layer.id]}</p>{/if}
      </div>
    {/each}
    <button type="button" class="tf-btn tf-sm" onclick={() => void add()}>+ Add a layer</button>
  </div>
  <div class="tf-dp-ps">
    <h4>What goes where</h4>
    <p class="tf-dim">Every kind sits in exactly one layer, or in none. A reverse proxy runs on a VM like a service does, so it is in Services.</p>
    <table class="tf-dp-kt">
      <tbody>
        {#each read.kinds as kind (kind.id)}
          <tr>
            <td><span class="tf-dp-mono">{kind.monogram}</span></td>
            <td>{kind.label}{#if counts.get(kind.id)}{" "}<span class="tf-dim">{counts.get(kind.id)!.toLocaleString("en")}</span>{/if}</td>
            <td>
              <select aria-label="Layer for {kind.label}" value={layerOfKind(kind.id)} onchange={(e) => place(kind.id, e.currentTarget.value)}>
                <option value="">No layer</option>
                {#each read.layers as layer (layer.id)}<option value={layer.id}>{layer.name}</option>{/each}
              </select>
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
  </div>
  {#if draft !== null}
    <div class="tf-dp-ps">
      <h4>In the view {views.current?.name ?? "this view"}</h4>
      <p class="tf-dim">Only these settings belong to the view. <b>Save view</b> keeps them.</p>
      {#each read.layers as layer (layer.id)}
        {@const shown = layerIn(draft, layer.id)}
        <div class="tf-dp-vrow">
          <b>{layer.name}</b>
          <label class="tf-ck"
            ><input type="checkbox" checked={!shown.hidden} onchange={(e) => views.change((d) => void (d.layers[layer.id] = { ...layerIn(d, layer.id), hidden: !e.currentTarget.checked }))} /> Show</label
          >
          <label class="tf-ck"
            ><input
              type="checkbox"
              checked={shown.folded}
              onchange={(e) => {
                const folded = e.currentTarget.checked;
                views.change((d) => void (d.layers[layer.id] = { ...layerIn(d, layer.id), folded }));
                onfoldreset(layer.id);
              }}
            /> Folded</label
          >
          <label
            >Group by <select
              aria-label="Group {layer.name} by"
              value={shown.group}
              onchange={(e) => {
                const group = e.currentTarget.value as DepGroup;
                views.change((d) => void (d.layers[layer.id] = { ...layerIn(d, layer.id), group }));
              }}
            >
              {#each GROUPS as option (option.value)}<option value={option.value}>{option.label}</option>{/each}
            </select></label
          >
        </div>
      {/each}
      <label
        >Tiles <select aria-label="Tiles" value={draft.tiles} onchange={(e) => { const tiles = e.currentTarget.value as DepTiles; views.change((d) => void (d.tiles = tiles)); }}>
          {#each TILES as option (option.value)}<option value={option.value}>{option.label}</option>{/each}
        </select></label
      >
      <label
        >Fold groups with no problem when a layer holds more than <input
          type="number"
          aria-label="Fold count"
          min="1"
          max="500"
          class="tf-dp-foldat"
          value={draft.fold_at}
          onchange={(e) => { const at = Number(e.currentTarget.value) || FOLD_AT; views.change((d) => void (d.fold_at = at)); }}
        /></label
      >
    </div>
  {/if}
  <p class="tf-dim tf-dp-pnote">Changes to the layers and to what goes where apply to every view at once, and are kept straight away.</p>
</aside>
