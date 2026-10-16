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
