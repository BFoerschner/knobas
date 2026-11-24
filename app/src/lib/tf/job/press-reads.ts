/**
 * A ticket page's reads, asked at the press on the ticket's card or row (the ux30 end gate, Windows; Fable's ruling: a
 * page's reads go with the press). The page found its ticket in three reads in a row, the sources, what the key's
 * candidates are and the ticket itself, and asked the first of them once it was built: at CPU 6x in WebView2 the card
 * drew 210 ms after the press, the last two rounds behind the build. Each is the call the page makes, with its
 * arguments, so the page's own asks find them in the answer cache (`ipc/answer-cache.ts`), on their way or kept.
 */
import { segmentKey, jobCandidates } from "./job";
import { bridgePorts, bridgeTicketBoard, type JobPorts } from "./ports";
import { cardFilters, SECTION_LIMIT } from "./sections";

type AheadPorts = Pick<JobPorts, "listSources" | "entityBriefs" | "getEntity" | "cardReads">;

/** When each ticket's reads were last asked, by id: a press and its click ask once. */
const asked = new Map<string, number>();
/** How long one press's asks stand for the next. */
const ASKED_MS = 3000;

/** Asks the reads a ticket's page waits on, for the ticket `entityId` (`job-reader.svelte.ts`'s `readJob`). */
export function readTicketAhead(entityId: string, ports: AheadPorts = bridgePorts, board: (query: string) => Promise<unknown> = bridgeTicketBoard): void {
  const last = asked.get(entityId);
  if (last !== undefined && Date.now() - last < ASKED_MS) return;
  asked.set(entityId, Date.now());
  const quiet = (read: Promise<unknown> | undefined) => void read?.catch(() => {});
  quiet(
    ports.listSources().then((listed) => {
      if (ports.entityBriefs !== undefined) quiet(ports.entityBriefs(jobCandidates(entityId, listed)));
    }),
  );
  quiet(ports.getEntity(entityId));
  quiet(ports.cardReads?.(entityId, cardFilters(entityId), SECTION_LIMIT));
  quiet(board(`under:${segmentKey(entityId)}`));
}

/** Tests only: no ticket asked. */
export function forgetTicketReads(): void {
  asked.clear();
}
