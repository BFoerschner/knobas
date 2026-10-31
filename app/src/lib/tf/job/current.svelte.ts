/**
 * The job on screen (#886, R-1.4): the entity id the job page resolved its
 * address to, or `null` where no job is showing or it is still being read.
 *
 * `#/job/<key>` carries a key a person reads, and only the page knows which
 * source the key resolved under, so the page reports it here and
 * `App.svelte`'s foreground reads it. A window singleton, like `layers`.
 */
let entityId = $state<string | null>(null);

export const currentJob = {
  get entityId() {
    return entityId;
  },
  /** Report the resolved job; the answer clears it, for the page's unmount. */
  set(id: string | null): () => void {
    entityId = id;
    return () => {
      if (entityId === id) entityId = null;
    };
  },
};

/**
 * The job a page stands for (#1500, re-sourced by spec #2227): on the Job page
 * the job on screen (`null` while its address is still being resolved),
 * everywhere else the chosen job (`chosen.svelte.ts`), with the ticket's
 * entity id; `null` when no job is chosen. The breadcrumb no longer draws it
 * (#2228); the name is kept. The shell reports it; the launcher's *Link to
 * <key>* row is offered against it, and ⌘T falls back to it. Never the newest
 * open job: a guessed job would take links to the wrong ticket in silence.
 */
let crumb = $state<{ key: string; entityId: string } | null>(null);

export const breadcrumbJob = {
  get job() {
    return crumb;
  },
  set(job: { key: string; entityId: string } | null): void {
    crumb = job;
  },
};

/** The other end of a link the launcher asked the job's dialog for. */
export interface LinkTarget {
  entity_id: string;
  title: string;
  kind: string;
}

let asked = $state<{ key: string; target: LinkTarget } | null>(null);

/**
 * *Link to <key>* from the launcher (#1500): the launcher never writes the
 * link. It asks here, and the job page with that key opens its own link
 * dialog with the result preset, so the relation is picked where every other
 * link from the job is.
 */
export const linkRequest = {
  ask(key: string, target: LinkTarget): void {
    asked = { key, target };
  },
  /** The target asked for `key`, handed over once; `null` when none waits. */
  take(key: string): LinkTarget | null {
    if (asked === null || asked.key !== key) return null;
    const { target } = asked;
    asked = null;
    return target;
  },
  /** Forget a request nobody took: the reader left before the job drew. */
  drop(): void {
    asked = null;
  },
};
