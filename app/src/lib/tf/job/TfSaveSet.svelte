<!--
  *Save as a link set…* (#869, R-7.9), from the Connections drawer:
  prototype-6's `openSaveSet`. A name, the ticket's own repositories,
  pipelines, pages, monitors and assets, all ticked (pull requests, branches
  and commits are not offered, and it says why), and *Apply it to*: only this
  ticket, every ticket under its epic with today's count, every sub-task with
  today's count, or every ticket in its project. *Save link set* writes a new
  set through `save_link_set` and hands it back with its one rule.
-->
<script lang="ts">
  import { ipcErrorMessage } from "../../ipc";
  import type { EntityDetail, EntityRow, LinkEnd, LinkSetDetail, LinkSetDraft, LinkSetRule, RuleTicket } from "../../ipc/entity";
  import type { Layers } from "../layers.svelte";
  import TfDialog from "../settings/TfDialog.svelte";
  import { epicOf, offered, projectOf, savedDraft, savedName } from "../sets/apply";
  import { KIND_LABEL } from "../sets/sets";
  import { keyOf } from "./job";

  let {
    ticket,
    links,
    ports,
    layers,
    onsaved,
    onclose,
  }: {
    ticket: Pick<EntityRow, "entity_id" | "source_id" | "path" | "title">;
    /** The far ends of the links drawn on the ticket. */
    links: LinkEnd[];
    ports: {
      saveLinkSet: (draft: LinkSetDraft) => Promise<LinkSetDetail>;
      previewLinkSetRule: (rule: LinkSetRule) => Promise<RuleTicket[]>;
      getEntity: (entityId: string) => Promise<EntityDetail>;
    };
    layers: Layers;
    onsaved: (set: LinkSetDetail) => void;
    onclose: () => void;
  } = $props();

  // svelte-ignore state_referenced_locally
  // Read once: the dialog is opened for one ticket and closed before another.
  const { key, epic, project, offers } = {
    key: keyOf(ticket.entity_id),
    epic: epicOf(ticket),
    project: projectOf(ticket),
    offers: offered(links),
  };

  // svelte-ignore state_referenced_locally
  let name = $state(savedName(ticket.title));
  let ticked = $state(new Set(offers.map((offer) => offer.entity_id)));
  let applyTo = $state<"manual" | "epic" | "parent" | "project">(epic ? "epic" : "manual");
  let epicTitle = $state<string | null>(null);
  let epicCount = $state<number | null>(null);
  let subtasks = $state<number | null>(null);
  let failure = $state<string | null>(null);
  let writing = $state(false);

  $effect(() => {
    let live = true;
    void ports.previewLinkSetRule({ kind: "parent", value: ticket.entity_id }).then(
      (rows) => live && (subtasks = rows.length),
      () => {},
    );
    if (epic !== null) {
      void ports.previewLinkSetRule({ kind: "epic", value: epic }).then(
        (rows) => live && (epicCount = rows.length),
        () => {},
      );
      void ports.getEntity(epic).then(
        (detail) => live && detail.row.title !== keyOf(epic) && (epicTitle = detail.row.title),
        () => {},
      );
    }
    return () => {
      live = false;
    };
  });

  function rule(): LinkSetRule {
    if (applyTo === "epic" && epic !== null) return { kind: "epic", value: epic };
    if (applyTo === "project" && project !== null) return { kind: "project", value: project };
    if (applyTo === "parent") return { kind: "parent", value: ticket.entity_id };
    return { kind: "manual", value: ticket.entity_id };
  }

  function toggle(id: string, on: boolean) {
    const next = new Set(ticked);
    if (on) next.add(id);
    else next.delete(id);
    ticked = next;
  }

  async function save() {
    if (writing) return;
    writing = true;
    failure = null;
    const picked = offers.filter((offer) => ticked.has(offer.entity_id));
    try {
      onsaved(await ports.saveLinkSet(savedDraft(name.trim() || "New link set", key, picked, rule())));
    } catch (error) {
      failure = ipcErrorMessage(error);
    } finally {
      writing = false;
    }
  }
</script>

<TfDialog title="Save {key}’s connections as a link set" {onclose} {layers}>
  {#snippet body()}
    <div class="tf-formgrid">
      <span>Name</span><input type="text" aria-label="Name" bind:value={name} />
    </div>
    <div class="tf-pane-h">What goes in</div>
    <div class="tf-picks" role="group" aria-label="What goes in">
      {#each offers as offer (offer.entity_id)}
        <label
          ><input
            type="checkbox"
            checked={ticked.has(offer.entity_id)}
            onchange={(event) => toggle(offer.entity_id, event.currentTarget.checked)}
          />
          {offer.title}<span class="tf-dim">{KIND_LABEL[offer.kind]}</span></label
        >
      {:else}
        <p class="tf-dim tf-picks-more">Nothing linked on {key} can go in a link set.</p>
      {/each}
    </div>
    <p class="tf-dim tf-small">PRs, branches and commits belong to this one ticket, so they are not offered.</p>
    <div class="tf-pane-h">Apply it to</div>
    <div class="tf-radio" role="radiogroup" aria-label="Apply it to">
      <label><input type="radio" name="tf-ssr" value="manual" bind:group={applyTo} /> <span>Only {key}</span></label>
      {#if epic !== null}
        <label
          ><input type="radio" name="tf-ssr" value="epic" bind:group={applyTo} />
          <span
            >Every ticket under epic {keyOf(epic)}{epicTitle ? ` ${epicTitle}` : ""}<small
              >{epicCount ?? "…"} tickets today, and new ones as they are created</small
            ></span
          ></label
        >
      {/if}
      <label
        ><input type="radio" name="tf-ssr" value="parent" bind:group={applyTo} />
        <span>Every sub-task of {key}<small>{subtasks ?? "…"} today</small></span></label
      >
      {#if project !== null}
        <label
          ><input type="radio" name="tf-ssr" value="project" bind:group={applyTo} />
          <span>Every ticket in project {keyOf(project)}</span></label
        >
      {/if}
    </div>
    {#if failure}<p class="tf-fail" role="alert">{failure}</p>{/if}
    <p class="tf-dim tf-stays">Link sets stay in knobas and are never written to a source.</p>
  {/snippet}
  {#snippet footer()}
    <button type="button" class="tf-btn tf-ghost" onclick={onclose}>Cancel</button>
    <button type="button" class="tf-btn tf-primary" disabled={writing} onclick={() => void save()}>Save link set</button>
  {/snippet}
</TfDialog>
