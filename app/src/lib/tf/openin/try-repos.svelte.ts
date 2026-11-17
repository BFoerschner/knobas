/**
 * **Try it on**'s repositories, for the application page and a folder renderer's page: every repository the mirror holds
 * whose checkout is on this disk, read `CHECKOUT_BATCH` at a time as Code reads them, the preferred ones first. The
 * first batch with a checkout picks one (the first preferred found, else the first found); later batches only add to the
 * list.
 *
 * The list is raw state, so a repository already listed stays the same object as batches come in. As deep state each
 * batch's new array wrapped every row anew, the picked repository was a new object each time, and a page that follows it
 * read its files, commits and pull requests again from *Reading…*, once per batch: with 65 checkouts among many more on
 * the forge, its rows flickered for the first seconds (Björn, 2026-11-17, v3.2.9-ux17: "keeps spazzing around for the
 * first few seconds running through the repositories").
 */
import { ipcErrorMessage } from "../../ipc";
import { CHECKOUT_BATCH, type TryReads, type TryRepo } from "./try-reads";

export class TryRepos {
  /** The repositories with a checkout here, in the order found; `null` while the first batch is out. */
  list = $state.raw<TryRepo[] | null>(null);
  /** Still reading checkouts after the first batch. */
  more = $state(false);
  failed = $state<string | null>(null);
  /** The picked repository's id: the select's value. */
  picked = $state<string | null>(null);
  readonly repo = $derived(this.list?.find((one) => one.entityId === this.picked) ?? null);
  /** The picked repository's id alone, for what follows the pick and nothing else. */
  readonly pickedId = $derived(this.repo?.entityId ?? null);

  /**
   * Reads the list once. `preferred` comes first, in its order; `stop` ends the read between batches (the page was
   * left), keeping what was found.
   */
  async read(reads: Pick<TryReads, "repos" | "checkouts">, preferred: readonly (string | null)[], stop: () => boolean = () => false): Promise<void> {
    let rows;
    try {
      rows = await reads.repos();
    } catch (cause) {
      this.failed = ipcErrorMessage(cause);
      this.list = [];
      return;
    }
    const wanted = preferred.filter((id): id is string => id !== null);
    const order = [...wanted.filter((id) => rows.some((row) => row.entity_id === id)), ...rows.map((row) => row.entity_id).filter((id) => !wanted.includes(id))];
    const byId = new Map(rows.map((row) => [row.entity_id, row]));
    const found: TryRepo[] = [];
    this.more = true;
    for (let start = 0; start < order.length && !stop(); start += CHECKOUT_BATCH) {
      try {
        const answers = await reads.checkouts(order.slice(start, start + CHECKOUT_BATCH));
        for (const answer of answers) {
          const row = byId.get(answer.entityId);
          if (answer.path !== null && row) found.push({ entityId: row.entity_id, title: row.title, sourceId: row.source_id, path: answer.path, clonesRoot: answer.clonesRoot });
        }
      } catch {
        // A batch that can't be read leaves its repositories out rather than claiming a checkout.
      }
      // A batch that found nothing new draws nothing new.
      if (this.list === null || found.length !== this.list.length) this.list = [...found];
      if (this.picked === null) this.picked = wanted.find((id) => found.some((one) => one.entityId === id)) ?? found[0]?.entityId ?? null;
    }
    if (this.list === null || found.length !== this.list.length) this.list = [...found];
    this.more = false;
  }
}
