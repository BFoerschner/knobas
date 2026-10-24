/**
 * `?fake-ipc`: Time > Standup: the digest, the protocol, its publication and the action-item
 * tickets (#829; Settings > Standup, #831).
 *
 * Answers `standup_digest`, `standup_protocol`, `publish_standup_protocol`,
 * `standup_publish_target`, `set_standup_publish_target`,
 * `create_action_item_ticket` and `action_item_ticket`.
 *
 * **Shape from prototype-6's `tmStandup`**: yesterday three lines (PAY-231
 * tracked time and a push, PR #142, an edit to the retry design page), today
 * two, one blocker (PAY-228 blocked by OPS-77). Titles come from `CORPUS`
 * where the entity is in the mirror, so a line cannot name a ticket
 * differently from the board.
 *
 * **Where this diverges from prototype-6 and the mock adapter:**
 *
 * - The *Infrastructure* list (#932) carries the alert's monitor and duration
 *   ("payout-api health check was down 12 min, from 03:10"); prototype-6's
 *   "vm-pay-01 restarted at 03:12" is not drawn, since the mirror holds state
 *   changes and not restarts.
 * - Prototype-6 draws the protocol as an attendee list with notes per person
 *   and action items; `Protocol` is a note id, a page title and a
 *   `publication`. Those people and items are the note's body, seeded here
 *   into `notes.ts`'s store (`seedNote`) the way the backend makes the note
 *   on first read, and then read and saved through the notes commands. The
 *   body is prototype-6's: four of five people there, two notes, two action
 *   items. Lena Hoffmann, absent in the prototype, is not in it: the body
 *   records who was there, not who was not.
 * - Prototype-6's first action item already has OPS-131, and so does this
 *   one: the protocol note carries the `related` link the backend draws to a
 *   ticket it filed, and `OPS-131` in the corpus is titled by the item's words,
 *   which is how the tab matches the two (#1433). The second item has none
 *   until *Create ticket*.
 * - The publish target is the mock source and a page id, not "Confluence,
 *   Payments > Standups": the fixture has no Confluence source (`sources.ts`).
 * - A publish and an action-item ticket start `pending` and read `sent` on the
 *   second ask, which is what the write queue does between a click and a
 *   sync; nothing is written anywhere.
 */
import type { Handlers } from "../fake-tauri";
import type { ActionItemTicket, DigestLine, Protocol, PublishTarget } from "../../../ipc/entity";
import { CORPUS } from "./corpus";
import { seedNote } from "./notes";

/** The ticket filed for the protocol's first action item, titled by its words (#1433). */
const FILED_TICKET = "mock:OPS-131";

/** The mirror's title for an entity, else the given one. */
function titled(entityId: string, fallback: string): string {
  return CORPUS.find((row) => row.entity_id === entityId)?.title ?? fallback;
}

function line(
  entity_id: string | null,
  kind: string | null,
  fallback: string,
  verb: string,
  reason: string,
  at: string,
): DigestLine {
  return {
    entity_id,
    kind,
    title: entity_id === null ? fallback : titled(entity_id, fallback),
    source: "mock",
    verb,
    reason,
    at,
  };
}

/** prototype-6's protocol, in the shape `knobas_app::protocol::template_of` starts. */
function protocolBody(day: string): string {
  return [
    `# Standup ${day}`,
    "",
    "## Attendees",
    "",
    "- Mara Lindqvist",
    "- Jonas Becker",
    "- Priya Nair",
    "- Tomasz Wójcik",
    "",
    "## Notes",
    "",
    "- Mara Lindqvist: PAY-231 retries done, review open. PAY-240 next.",
    "- Tomasz Wójcik: Runbook update this afternoon.",
    "",
    "## Action items",
    "",
    "- [ ] Update the on-call runbook for the new ledger topology (Tomasz Wójcik)",
    "- [ ] Decide the manual-review SLA (Priya Nair)",
    "",
  ].join("\n");
}

/** `HH:MM` on `day`, as the instant the browser's clock reads it. */
function on(day: string, hour: number, minute: number): string {
  const at = new Date(`${day}T00:00:00`);
  at.setHours(hour, minute, 0, 0);
  return at.toISOString();
}

export function handlers(): Handlers {
  let target: PublishTarget | null = { source_id: "mock", parent: "mock:ENG-STANDUPS" };
  const published = new Map<string, Protocol["publication"]>();
  let nextWrite = 801;
  /** Writes by id, and how many times each was asked about. */
  const asked = new Map<number, number>();
  /** Each create's title, as the backend answers it back (#1951). */
  const titles = new Map<number, string>();

  const protocol = (day: string): Protocol => {
    // The backend's `related` link from the protocol to the ticket it filed.
    seedNote(`note:standup-${day}`, `Standup ${day}`, protocolBody(day), [{ target_id: FILED_TICKET, relation: "related" }]);
    return protocolOf(day);
  };
  const protocolOf = (day: string): Protocol => ({
    day,
    note_id: `note:standup-${day}`,
    page_title: day,
    publication: published.get(day) ?? null,
    owed_tickets: [],
  });
  /** A re-read of a pending publication finds it sent, as the queue would. */
  const settle = (day: string): Protocol => {
    const publication = published.get(day);
    if (publication?.state === "pending") {
      published.set(day, { ...publication, state: "sent", page_entity_id: "confluence:sepa", linked: true });
    }
    return protocol(day);
  };

  return {
    standup_digest: (args) => {
      const today = args["today"] as { day: string };
      const earlier = (args["earlier"] as { day: string }[] | undefined) ?? [];
      const before = earlier.at(-1)?.day ?? null;
      const when = before ?? today.day;
      return {
        yesterday_day: before,
        yesterday:
          before === null
            ? []
            : [
                line("mock:PAY-231", "ticket", "Retry failed SEPA payouts", "tracked", "4h 30m; pushed 3 commits to the pull request", on(when, 17, 40)),
                line("mock:payout-service#142", "pr", "PR #142", "merged", "PAY-228 moved to In Review", on(when, 15, 5)),
                line("confluence:sepa", "page", "SEPA retry design", "edited", "Backoff policy", on(when, 11, 20)),
              ],
        today: [
          line("mock:PAY-231", "ticket", "Retry failed SEPA payouts", "tracking", "timer running", on(today.day, 9, 40)),
          line("mock:PAY-198", "ticket", "Payout dashboard latency", "assigned", "assigned to you today", on(today.day, 8, 12)),
        ],
        blockers: [
          line("mock:PAY-228", "ticket", "Fix ledger drift", "blocked", "blocked by OPS-77 On-call runbook outdated", on(today.day, 8, 0)),
        ],
        // prototype-6: "payout-api health check was down 12 min, 03:10 to 03:22". The
        // reason is the duration; the clock is drawn from `at`. Its restart half has
        // no source: the mirror records a state change, not a restart.
        infrastructure: [
          line("mock:payout-api", "monitor", "payout-api health check", "down", "was down 12 min", on(when, 3, 10)),
        ],
      };
    },
    standup_protocol: (args) => settle(String(args["day"])),
    publish_standup_protocol: (args) => {
      const day = String(args["day"]);
      const write_id = nextWrite++;
      asked.set(write_id, 0);
      published.set(day, { write_id, state: "pending", detail: null, page_entity_id: null, linked: false });
      return protocol(day);
    },
    standup_publish_target: () => target,
    set_standup_publish_target: (args) => {
      target = args["target"] as PublishTarget;
      return target;
    },
    create_action_item_ticket: (args): ActionItemTicket => {
      const write_id = nextWrite++;
      asked.set(write_id, 0);
      titles.set(write_id, String(args["title"]).trim());
      return { write_id, title: titles.get(write_id)!, state: "pending", may_have_landed: false, detail: null, ticket_entity_id: null, linked: false };
    },
    action_item_ticket: (args): ActionItemTicket => {
      const write_id = Number(args["writeId"]);
      const count = (asked.get(write_id) ?? 0) + 1;
      asked.set(write_id, count);
      const title = titles.get(write_id) ?? "";
      return count < 2
        ? { write_id, title, state: "pending", may_have_landed: false, detail: null, ticket_entity_id: null, linked: false }
        : { write_id, title, state: "sent", may_have_landed: false, detail: null, ticket_entity_id: "mock:OPS-77", linked: true };
    },
  };
}
