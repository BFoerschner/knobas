/**
 * A pull request knobas is creating (#2584): every *Create* lands on the new
 * pull request's page, not on the list, and that page exists before the
 * pull request does.
 *
 * A create is a queued write, so the pull request has no number until its
 * source answers. Until then the page is addressed by the write
 * (`#/pr/new/<write id>`, {@link creatingHash}) and drawn from what was typed.
 * Once the write lands, its read-back (ADR-0031: `after_landing` refreshes the
 * item a create's receipt names) tells `item:refreshed` with the write's id
 * and the entity it made, which is recorded here as `made`; the page then
 * takes the pull request's own address in place.
 *
 * Kept for the window's life, never written anywhere: the queue is the record
 * (`queued.svelte.ts`'s rule). The listener starts with the first create, so
 * a read-back that lands before the page draws is not missed. After a reload
 * the page reads the queue's row for what was typed; a write already sent by
 * then has no `made` here, and the page says so.
 */
import { EVENTS } from "../../ipc";
import { windowListen } from "../../ipc/shared-listen";
import type { RefreshEnded } from "../../ipc/sources";
import type { EchoListen } from "../queued.svelte";

/** What a `CreatePullRequest` carries, as the page draws it. */
export interface CreatePr {
  entity: string;
  title: string;
  head: string;
  base: string;
}

/** One create this window queued. */
export interface Creating {
  write: number;
  repo: string;
  title: string;
  head: string;
  base: string;
  /** The pull request's entity id once the read-back named it, else `null`. */
  made: string | null;
}

const creating: Record<number, Creating> = $state({});
let unlisten: Promise<() => void> | null = null;

/** The address of the page of the create queued as write `write`. */
export function creatingHash(write: number): string {
  return `#/pr/new/${write}`;
}

/** The write a create's address names (`new/<write id>` after `pr`), or `null` for any other. */
export function readCreatingRoute(rest: readonly string[]): number | null {
  if (rest[0] !== "new" || rest[1] === undefined || !/^\d+$/.test(rest[1])) return null;
  return Number(rest[1]);
}

/** The create queued as write `write` in this window, if it was. */
export function creatingOf(write: number): Creating | undefined {
  return creating[write];
}

/** A `CreatePullRequest` was queued as write `write`: answers the address its page has until the source answers. */
export function noteCreate(write: number, pr: CreatePr, listen: EchoListen = (event, handler) => windowListen(event, handler)): string {
  creating[write] = { write, repo: pr.entity, title: pr.title, head: pr.head, base: pr.base, made: null };
  unlisten ??= listen(EVENTS.itemRefreshed, ({ payload }) => {
    const ended = payload as RefreshEnded;
    const one = ended.write_id === null ? undefined : creating[ended.write_id];
    // A read-back that fell back on the create's target (no id stamped) names no pull request.
    if (one === undefined || ended.ended !== "refreshed" || ended.entity_id === one.repo) return;
    one.made = ended.entity_id;
  }).catch(() => {
    // No bridge to hear it on (a test without `listen`): the page stays on *Sent*.
    unlisten = null;
    return () => {};
  });
  return creatingHash(write);
}

/** Tests only: forget every create and stop listening. */
export function forgetCreates(): void {
  for (const key of Object.keys(creating)) delete creating[Number(key)];
  void unlisten?.then((off) => off());
  unlisten = null;
}
