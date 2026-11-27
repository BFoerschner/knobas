<!--
  The page of a pull request being created (#2584): `#/pr/new/<write id>`,
  where every *Create pull request* lands (`creating.svelte.ts`).

  - **While the create is out**: the pull request page's own header and tabs,
    drawn from what was typed (its title, head into base) with *Queued*, then
    *Sent*, where the state chip stands, and one line under the tabs. The
    tabs open nothing yet.
  - **Once the read-back names it** (ADR-0031): the pull request's address in
    place of this one (`replaceAddress`, so `Back` never returns here), where
    the header and the tabs stand where they stood. A page kept hidden waits
    until it shows, so a create never pulls the reader off another page.
  - **A create that won't go**: the source's words, and what the write queue
    offers for that row: *Send mine anyway* (*Send again* for a send that may
    have arrived, #1966) for a held one, and *Discard*; *Cancel* for one
    waiting on its source.
  - `Esc` goes to the repository, as from the pull request (#2559).
-->
<script lang="ts" module>
  import type { EntityDetail } from "../../ipc/entity";
  import type { WriteFollower } from "../../shell/follow-write";

  /** What the page reads and does, injectable so a test needs no bridge. */
  export interface CreatingPorts {
    follower: WriteFollower;
    getEntity: (entityId: string) => Promise<Pick<EntityDetail, "row">>;
    applyHeldWrite: (id: number) => Promise<void>;
    discardWrite: (id: number) => Promise<void>;
  }
</script>

<script lang="ts">
  import { ipcErrorMessage } from "../../ipc";
  import { getEntity } from "../../ipc/entity";
  import { applyHeldWrite, discardWrite, type QueuedWrite } from "../../ipc/sources";
  import { mayHaveLanded, willNotGo, writeFollower } from "../../shell/follow-write";
  import { push } from "../../shell/toasts.svelte";
  import { noteWithdrawn, onWithdrawn, openWriteQueue, unconfirmedSend } from "../../shell/write-queue.svelte";
  import type { EscTarget } from "../escape";
  import { pageVisible } from "../kept-pages.svelte";
  import { replaceAddress } from "../opener";
  import { parentPage } from "../page-name.svelte";
  import { repoHash } from "../repo/repo";
  import TfCrumbs, { type Crumb } from "../TfCrumbs.svelte";
  import TfNothingHere from "../TfNothingHere.svelte";
  import TfStateChip from "../TfStateChip.svelte";
  import { creatingOf, type CreatePr } from "./creating.svelte";
  import { PR_TABS, prHash } from "./pr";

  let {
    write,
    back = null,
    ports = {
      follower: writeFollower(),
      getEntity: (entityId) => getEntity(entityId),
      applyHeldWrite: (id) => applyHeldWrite(id),
      discardWrite: (id) => discardWrite(id),
    },
  }: {
    /** The create's write id, from the address. */
    write: number;
    back?: EscTarget | null;
    ports?: CreatingPorts;
  } = $props();

  const noted = $derived(creatingOf(write));
  /** The queue's open row: `null` until the first read, `undefined` once it lists none. */
  let row = $state<QueuedWrite | null | undefined>(null);
  let withdrawn = $state(false);
  let busy = $state(false);
  let repoName = $state<string | null>(null);

  $effect(() => {
    const id = write;
    const stopFollow = ports.follower.follow(id, (heard) => (row = heard));
    const stopWithdrawn = onWithdrawn(id, () => (withdrawn = true));
    return () => {
      stopFollow();
      stopWithdrawn();
    };
  });

  /** What was typed: this window's note, else the queue's row (after a reload). */
  const pr = $derived.by<CreatePr | null>(() => {
    if (noted !== undefined) return { entity: noted.repo, title: noted.title, head: noted.head, base: noted.base };
    const queued = (row?.payload as { CreatePullRequest?: CreatePr } | undefined)?.CreatePullRequest;
    return queued ?? null;
  });
  const repo = $derived(pr?.entity ?? null);

  $effect(() => {
    if (repo === null) return;
    const id = repo;
    let live = true;
    ports.getEntity(id).then(
      (found) => live && (repoName = found.row.title),
      () => {},
    );
    return () => (live = false);
  });

  // Once the read-back names what it made, that is the page; a hidden kept page goes once it shows.
  const shown = pageVisible();
  $effect(() => {
    const made = noted?.made ?? null;
    if (made !== null && shown()) replaceAddress(prHash(made));
  });

  const stopped = $derived(row != null && willNotGo(row) ? row : null);
  const unsure = $derived(row != null && mayHaveLanded(row) ? row : null);
  const chip = $derived.by(() => {
    if (withdrawn) return { text: "Discarded", tone: "muted" };
    if (stopped !== null) return stopped.state === "refused" ? { text: "Refused", tone: "err" } : { text: "Held", tone: "warn" };
    if (unsure !== null) return { text: "Held", tone: "warn" };
    return row === undefined ? { text: "Sent", tone: "info" } : { text: "Queued", tone: "info" };
  });
  const line = $derived.by(() => {
    if (withdrawn) return "Discarded. No pull request was created.";
    if (unsure !== null) {
      const source = unconfirmedSend(unsure)?.source ?? "the source";
      return `${source} did not answer, so it may have arrived. Check ${source}, then send it again or discard it.`;
    }
    if (stopped !== null) return `${stopped.state === "refused" ? "The source refused it" : "It was held"}${stopped.detail ? `: ${stopped.detail}` : "."}`;
    if (row === undefined) return "Sent. Waiting for the source to say which pull request it made…";
    return row?.detail ? `Creating this pull request… Waiting: ${row.detail}` : "Creating this pull request…";
  });
  /** A held row's *Send mine anyway*, as the queue panel draws it; none for a refused one. */
  const resend = $derived.by(() => {
    const held = stopped ?? unsure;
    if (held === null || held.state !== "held" || !held.source_enabled) return null;
    return unsure !== null ? "Send again" : "Send mine anyway";
  });
  const discardable = $derived(!withdrawn && row != null);

  async function act(run: () => Promise<void>) {
    if (busy) return;
    busy = true;
    try {
      await run();
    } catch (cause) {
      push({ text: `The write queue did not take it: ${ipcErrorMessage(cause)}`, tone: "err" });
    } finally {
      busy = false;
    }
  }

  const crumbs = $derived<Crumb[]>([
    { name: "Code", href: "#/code" },
    ...(repo !== null && repoName !== null ? [{ name: repoName, href: repoHash(repo), code: true }] : []),
    { name: "New pull request" },
  ]);
  parentPage(() => (repo === null || repoName === null ? null : { hash: repoHash(repo), name: repoName }));
</script>

<div class="tf-full">
  <TfCrumbs parts={crumbs} {back} />
  {#if pr === null}
    {#if row === undefined}
      <TfNothingHere message="knobas has no pull request on its way here. It may have been sent before knobas restarted." href="#/code/prs" action="All pull requests" />
    {/if}
  {:else}
    <header class="tf-fhead tf-holds">
      <div>
        <h1>PR {pr.title}</h1>
        <div class="tf-sub">
          <TfStateChip class="tf-prstate" text={chip.text} tone={chip.tone} />{" "}<span class="tf-mono">{pr.head}</span>{" "}into <span class="tf-mono">{pr.base}</span>
        </div>
      </div>
      <div class="tf-acts"></div>
    </header>
    <nav class="tf-tabs" aria-label="Pull request">
      {#each PR_TABS as entry (entry.id)}
        <a aria-disabled="true" aria-current={entry.id === "overview" ? "page" : undefined}>{entry.label}</a>
      {/each}
    </nav>
    <section class="tf-prcreating">
      <p class="tf-prline" class:tf-dim={stopped === null && unsure === null} role="status">{line}</p>
      {#if resend !== null || discardable || stopped !== null || unsure !== null}
        <div class="tf-acts">
          {#if resend !== null}
            <button type="button" class="tf-btn tf-sm" disabled={busy} onclick={() => act(() => ports.applyHeldWrite(write))}>{resend}</button>
          {/if}
          {#if discardable && (stopped !== null || unsure !== null || row?.wait_reason)}
            <button
              type="button"
              class="tf-btn tf-sm tf-danger"
              disabled={busy}
              onclick={() =>
                act(async () => {
                  await ports.discardWrite(write);
                  noteWithdrawn(write);
                })}>{stopped !== null || unsure !== null ? "Discard" : "Cancel"}</button
            >
          {/if}
          {#if stopped !== null || unsure !== null}
            <button type="button" class="tf-btn tf-sm tf-ghost" onclick={openWriteQueue}>Open write queue</button>
          {/if}
        </div>
      {:else if row === undefined && repo !== null && !withdrawn}
        <a class="tf-btn tf-sm tf-ghost" href={repoHash(repo, "prs")}>Pull requests</a>
      {/if}
    </section>
  {/if}
</div>
