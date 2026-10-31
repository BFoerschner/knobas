/**
 * **What is in front of the reader** in the ticket-first shell (#1102): the
 * entity passive attribution records and a capture's `captured-from` names.
 * `CONTEXT.md`'s **Passive attribution** is the rule: the open detail, else
 * the asset in the Tree's pane, else the open job's ticket, else nothing.
 *
 * Pure, and read from the address alone, so it answers at once on every
 * address change and never passes through a gap while a page reads.
 *
 * - **The open detail** is an entity page: a ticket page (`#/ticket/<id>`,
 *   spec #2227, so reading a ticket is recorded against it, not the chosen
 *   job), `#/pr/<id>`, `#/repo/<id>`,
 *   `#/build/…<run>`, `#/page/<id>`, and `#/branch/<id>` and `#/commit/<id>`,
 *   which send the reader on to the repository page that draws them (#1440)
 *   and name the entity where its repository is unknown. A tab
 *   or a place inside the page (`#/pr/<id>/files`, `#/repo/<id>/commits/<sha>`,
 *   `#/build/<pipeline>/<run>/tests`, `#/page/<id>/3`) is the same entity, so
 *   moving between the tabs of one page keeps one run of observations.
 * - **The asset in the Tree's pane** is `#/asset/<id>`: the Tree's selection
 *   lives in its address. `#/assets` and `#/route/<id>` select no asset (the
 *   route's asset is only known once the Tree has read it), so they are
 *   nothing, as the rooms UI had it.
 * - **The open job** is the job page's resolved ticket, which the page
 *   reports to `currentJob`; its tabs keep that one ticket.
 *
 * Lists (`#/builds/<pipeline>`, `#/docs`, Tickets, Time) and the Tree with
 * nothing selected are nothing, never the last job's ticket.
 */
import type { EntityTarget } from "../ipc/time";
import { canBeTarget } from "../shell/timer";
import type { TfRoute } from "./route";

/** First segments whose first id is the entity the page draws. A head joins only when a page draws it. */
const ENTITY_HEADS = new Set(["pr", "repo", "page", "asset", "branch", "commit"]);

/** The entity id an address's page is the detail of, or `null`. */
function detailOf(route: TfRoute): string | null {
  // The ticket page (spec #2227) is its ticket's detail, whatever job is chosen.
  if (route.view === "ticket") return route.id;
  if (route.view !== "full") return null;
  const { head, rest } = route;
  // `#/build/<run>` is replaced by `#/build/<pipeline>/<run>[/<tab>]`; the run is the detail either way.
  if (head === "build") return (rest.length === 1 ? rest[0] : rest[1]) || null;
  if (ENTITY_HEADS.has(head)) return rest[0] || null;
  return null;
}

/** What is in front of the reader on `route`, with `job` the job page's resolved ticket. */
export function foregroundOf(route: TfRoute, job: string | null): EntityTarget | null {
  const detail = detailOf(route);
  const id = detail ?? job;
  if (id === null || !canBeTarget({ entityId: id })) return null;
  return { kind: "entity", entity_id: id };
}
