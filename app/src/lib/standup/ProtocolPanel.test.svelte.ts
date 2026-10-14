/**
 * The protocol panel: what a reader sees, what it asks before publishing, and
 * what pressing the buttons sends (issue #289, spec #272 stories 64-69).
 *
 * The seam is spec #272's: **a rendered view in, user-visible text and the
 * calls it made out.** Nothing here reaches into the component's state, and
 * nothing here re-tests the backend — what a publish *does* is
 * `crates/knobas-app/tests/it/protocol_ipc.rs`'s, and what is asserted here is
 * which question the reader is asked first and what the answer is turned into.
 *
 * The one rule this file is really about is story 68: **with two Confluence
 * sources the dialog has no preselection.** A panel that quietly aimed at the
 * first configured wiki would look identical in a screenshot and would publish
 * the team's standup into the wrong instance.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { ActionItemTicket, NoteDetail, Protocol, PublishTarget } from "../ipc/entity";
import type { QueuedWrite } from "../ipc/sources";
import { createFollower } from "../shell/follow-write";
import { createRouter } from "../shell/router.svelte";
import { toasts } from "../shell/toasts.svelte";
import { paste } from "../shell/test-paste";
import ProtocolPanel from "./ProtocolPanel.svelte";

const DAY = "2026-09-03";
const NOTE = "note:6f1e";
const BODY = "## Attendees\n\n- Mara\n\n## Action items\n\n- [ ] Ask Ines\n";

function protocol(over: Partial<Protocol> = {}): Protocol {
  return {
    day: DAY,
    note_id: NOTE,
    page_title: DAY,
    publication: null,
    ...over,
  };
}

function note(body = BODY): NoteDetail {
  return {
    note: {
      id: NOTE,
      title: "Standup 2026-09-03",
      body_md: body,
      created_at: "2026-09-03T07:00:00Z",
      updated_at: "2026-09-03T07:00:00Z",
    },
    refs: [],
    links: [],
  };
}

const CONFLUENCE = {
  adapter_kind: "confluence",
  write_ops: ["comment", "create_page"],
};

function wiki(id: string) {
  return { id, display_name: id.toUpperCase(), adapter_kind: "confluence", enabled: true };
}

let target: HTMLDivElement;
let app: Record<string, unknown> | undefined;

/** The queue as the panel's follower sees it (#779): its open rows, and the `activity:new` handlers. */
let open: QueuedWrite[] = [];
const handlers: (() => void)[] = [];
function moved() {
  for (const handler of [...handlers]) handler();
}
function queued(id: number, state: QueuedWrite["state"], detail: string | null = null): QueuedWrite {
  return { id, state, detail, op: "create_page", entity_id: "wiki:98400" } as QueuedWrite;
}

interface Calls {
  published: { day: string; target: PublishTarget | undefined }[];
  saved: { title: string; body: string }[];
  filed: { project: string; type: string; title: string }[];
}

function render(
  answer: Protocol,
  over: {
    stored?: PublishTarget | null;
    sources?: ReturnType<typeof wiki>[];
    body?: string;
    /** Make the stored-target read fail, the way a database that is down does. */
    targetReadFails?: boolean;
    /** Make every save fail, the way a note deleted underneath does. */
    saveFails?: boolean;
    /** What a publish answers; a sent, linked page by default. */
    published?: Protocol;
    /** What filing a ticket answers; a filed, linked ticket by default. */
    filed?: ActionItemTicket;
    /** What each `action_item_ticket` read answers, in turn. */
    ticketReads?: ActionItemTicket[];
  } = {},
) {
  const calls: Calls = { published: [], saved: [], filed: [] };
  location.hash = "#/standup";
  const router = createRouter();
  let current = answer;
  app = mount(ProtocolPanel, {
    target,
    props: {
      day: DAY,
      router,
      saveAfterMs: 1,
      ports: {
        standupProtocol: () => Promise.resolve(current),
        getNote: () => Promise.resolve(note(over.body ?? BODY)),
        saveNote: (_id: string, title: string, body: string) => {
          calls.saved.push({ title, body });
          return over.saveFails
            ? Promise.reject({ code: "internal", message: "the note would not save" })
            : Promise.resolve(note(body));
        },
        standupPublishTarget: () =>
          over.targetReadFails
            ? Promise.reject({ code: "internal", message: "the setting would not read" })
            : Promise.resolve(over.stored ?? null),
        listSources: () => Promise.resolve((over.sources ?? [wiki("wiki")]) as never),
        listAdapters: () => Promise.resolve([CONFLUENCE] as never),
        search: () =>
          Promise.resolve({
            interpreted: { text: "", prefix: null, filters: {}, unknown_tokens: [] },
            groups: [
              {
                kind: "page",
                hits: [
                  {
                    entity_id: "wiki:98400",
                    kind: "page",
                    source_id: "wiki",
                    updated_at: null,
                    synced_at: "2026-09-03T07:00:00Z",
                    title: "Standup protocols",
                    path: "Engineering",
                    rank: 1,
                    snippet: [],
                  },
                ],
              },
            ],
            total: 1,
            took_ms: 1,
            coverage: [],
          } as never),
        publishStandupProtocol: (day: string, chosen?: PublishTarget) => {
          calls.published.push({ day, target: chosen });
          current = over.published ?? protocol({
            publication: {
              write_id: 1,
              state: "sent",
              detail: null,
              page_entity_id: "wiki:98411",
              linked: true,
            },
          });
          return Promise.resolve(current);
        },
        listProjects: () =>
          Promise.resolve([{ source_id: "tracker", key: "PAY", name: "Payments" }]),
        createActionItemTicket: (
          _note: string,
          project: string,
          ticketType: string,
          title: string,
        ) => {
          calls.filed.push({ project, type: ticketType, title });
          return Promise.resolve(
            over.filed ?? {
              write_id: 3,
              state: "sent",
              detail: null,
              ticket_entity_id: "tracker:PAY-999",
              linked: true,
            },
          );
        },
        actionItemTicket: () => {
          const next = over.ticketReads?.shift();
          return next ? Promise.resolve(next) : Promise.reject(new Error("no read set up"));
        },
        follower: createFollower({
          listen: (_event, handler) => {
            handlers.push(handler);
            return Promise.resolve(() => handlers.splice(handlers.indexOf(handler), 1));
          },
          pendingWrites: () => Promise.resolve(open),
        }),

      },
    },
  });
  flushSync();
  /** What the next `standup_protocol` read answers. */
  const reads = (next: Protocol) => {
    current = next;
  };
  return { router, calls, reads };
}

function text(): string {
  return (target.textContent ?? "").replace(/\s+/g, " ").trim();
}

function button(label: string): HTMLButtonElement | undefined {
  return [...target.querySelectorAll("button")].find(
    (candidate) => candidate.textContent?.trim() === label,
  );
}

/**
 * Let every promise the panel is waiting on resolve, then render.
 *
 * A count rather than a timer: the chains here are microtasks all the way
 * down -- *Publish* reads the stored target, saves the body, reads the note
 * back and only then sends -- and a fixed number of ticks is what makes the
 * assertion about the panel rather than about how fast the runner is. Ten is
 * comfortably past the longest chain; a shorter one would fail on the publish
 * path and pass everywhere else, which is how a flake starts.
 */
async function settle() {
  for (let tick = 0; tick < 10; tick += 1) await Promise.resolve();
  flushSync();
}

beforeEach(() => {
  open = [];
  handlers.length = 0;
  toasts.items = [];
  target = document.createElement("div");
  document.body.append(target);
});

afterEach(() => {
  if (app) unmount(app);
  app = undefined;
  target.remove();
  vi.useRealTimers();
});

test("the protocol's body is on screen and editable", async () => {
  render(protocol());
  await settle();
  const editor = target.querySelector<HTMLTextAreaElement>("textarea");
  expect(editor?.value).toContain("## Attendees");
  expect(editor?.disabled).toBe(false);
});

test("its action items each offer a ticket, and nothing else does", async () => {
  render(protocol(), {
    body: "## Notes\n\n- not an action item\n\n## Action items\n\n- [ ] Ask Ines\n",
  });
  await settle();
  const items = [...target.querySelectorAll("li")];
  expect(items).toHaveLength(1);
  expect(items[0]?.textContent).toContain("Ask Ines");
  expect(text()).not.toContain("not an action item");
});

test("a stored target publishes with no dialog at all", async () => {
  const { calls } = render(protocol(), {
    stored: { source_id: "wiki", parent: "wiki:98400" },
  });
  await settle();
  button("Publish to Confluence")?.click();
  await settle();

  expect(calls.published).toEqual([{ day: DAY, target: undefined }]);
  expect(text()).not.toContain("Where do standup protocols go?");
});

test("with two Confluence sources the dialog asks which, and will not publish until told", async () => {
  const { calls } = render(protocol(), { sources: [wiki("wiki"), wiki("wiki2")] });
  await settle();
  button("Publish to Confluence")?.click();
  await settle();

  expect(text()).toContain("Where do standup protocols go?");
  const select = target.querySelector<HTMLSelectElement>("select");
  expect(select?.value).toBe("");
  expect([...(select?.options ?? [])].map((option) => option.value)).toEqual([
    "",
    "wiki",
    "wiki2",
  ]);
  // Nothing has been chosen, so nothing can be sent.
  expect(button("Publish")?.disabled).toBe(true);
  expect(calls.published).toEqual([]);
});

test("with one Confluence the dialog preselects it and still asks for the parent", async () => {
  const { calls } = render(protocol());
  await settle();
  button("Publish to Confluence")?.click();
  await settle();

  expect(target.querySelector<HTMLSelectElement>("select")?.value).toBe("wiki");
  // The parent is nobody's to guess either.
  expect(button("Publish")?.disabled).toBe(true);

  const box = target.querySelector<HTMLInputElement>("input[type=search]")!;
  box.value = "standup";
  box.dispatchEvent(new Event("input", { bubbles: true }));
  await settle();
  [...target.querySelectorAll("button")]
    .find((candidate) => candidate.textContent?.includes("Standup protocols"))
    ?.click();
  await settle();

  button("Publish")?.click();
  await settle();
  expect(calls.published).toEqual([
    { day: DAY, target: { source_id: "wiki", parent: "wiki:98400" } },
  ]);
});

test("publishing sends what is on screen, not what the last pause happened to save", async () => {
  const { calls } = render(protocol(), {
    stored: { source_id: "wiki", parent: "wiki:98400" },
  });
  await settle();
  const editor = target.querySelector<HTMLTextAreaElement>("textarea")!;
  editor.value = "## Attendees\n\n- Mara\n- Jonas\n";
  editor.dispatchEvent(new Event("input", { bubbles: true }));
  button("Publish to Confluence")?.click();
  await settle();

  expect(calls.saved.at(-1)?.body).toContain("Jonas");
  expect(calls.published).toHaveLength(1);
});

test("a published protocol offers its page instead of the button", async () => {
  render(
    protocol({
      publication: {
        write_id: 1,
        state: "sent",
        detail: null,
        page_entity_id: "wiki:98411",
        linked: true,
      },
    }),
  );
  await settle();
  // One page per date: there is nothing left to press, so the button is gone
  // rather than disabled.
  expect(button("Publish to Confluence")).toBeUndefined();
  expect(text()).toContain(`Published as ${DAY}`);
});

test("a publication still on the queue says so and offers no second publish", async () => {
  render(
    protocol({
      publication: {
        write_id: 1,
        state: "pending",
        detail: "the wiki did not answer",
        page_entity_id: null,
        linked: false,
      },
    }),
  );
  await settle();
  expect(button("Publish to Confluence")).toBeUndefined();
  expect(text()).toContain("the write is on the queue");
  expect(text()).toContain("the wiki did not answer");
});

test("a page the mirror has not read yet says why there is no link", async () => {
  render(
    protocol({
      publication: {
        write_id: 1,
        state: "sent",
        detail: null,
        page_entity_id: "wiki:98411",
        linked: false,
      },
    }),
  );
  await settle();
  expect(text()).toContain("its link appears once the next sync has read it");
});

test("creating a ticket from an action item asks for the project first", async () => {
  const { calls } = render(protocol());
  await settle();
  button("Create ticket")?.click();
  await settle();

  expect(text()).toContain("File a ticket");
  expect(button("Create")?.disabled).toBe(true);
  expect(calls.filed).toEqual([]);

  const select = [...target.querySelectorAll("select")].at(-1)!;
  select.value = "tracker:PAY";
  select.dispatchEvent(new Event("change", { bubbles: true }));
  await settle();

  button("Create")?.click();
  await settle();
  expect(calls.filed).toEqual([
    { project: "tracker:PAY", type: "Task", title: "Ask Ines" },
  ]);
});

test("a target read that failed says so, instead of asking again", async () => {
  // "Nothing is stored" is a claim about the database. A panel that could not
  // ask has not earned it — and the visible cost of getting this wrong is a
  // dialog re-asking a question the reader already answered.
  const { calls } = render(protocol(), { targetReadFails: true });
  await settle();
  button("Publish to Confluence")?.click();
  await settle();

  expect(text()).toContain("the setting would not read");
  expect(text()).not.toContain("Where do standup protocols go?");
  expect(calls.published).toEqual([]);
});

test("a save that failed publishes nothing", async () => {
  // The page is made from the body the *backend* holds, so publishing after a
  // failed save would put a stale protocol on the wiki under today's date —
  // and a page cannot be taken back the way a keystroke can.
  const { calls } = render(protocol(), {
    stored: { source_id: "wiki", parent: "wiki:98400" },
    saveFails: true,
  });
  await settle();
  button("Publish to Confluence")?.click();
  await settle();

  expect(calls.published).toEqual([]);
  expect(text()).toContain("the note would not save");
  // And the words are still in the box, which is what makes retrying free.
  expect(target.querySelector<HTMLTextAreaElement>("textarea")?.value).toContain("Attendees");
});

/**
 * Spec #491 story 13. A protocol is published to Confluence, so what a reader
 * pastes into it has to arrive on the wiki as the link they pasted — a
 * `[[ref]]` substituted here would reach the team as a knobas-only spelling
 * nothing on that page can resolve.
 *
 * The panel edits a note through `saveNote`, which is what makes the rule
 * worth pinning rather than assuming: the note *editor* turns a resolvable URL
 * into a reference (story 12, `notes/NoteView.svelte`), and this panel edits
 * the same kind of row without inheriting that.
 */
test("a URL pasted into the protocol is left as text and published as typed", async () => {
  const link = "https://jira.example/browse/PAY-231";
  const { calls } = render(protocol(), {
    stored: { source_id: "wiki", parent: "wiki:98400" },
  });
  await settle();

  const editor = target.querySelector<HTMLTextAreaElement>("textarea")!;
  const before = editor.value;
  expect(paste(editor, link), "something took the paste over").toBe(false);
  await settle();
  expect(editor.value, "something rewrote the field without cancelling").toBe(before);

  editor.value = `${BODY}\n- context: ${link}\n`;
  editor.dispatchEvent(new Event("input", { bubbles: true }));
  button("Publish to Confluence")?.click();
  await settle();

  expect(calls.saved.at(-1)?.body).toContain(link);
  expect(calls.saved.at(-1)?.body).not.toContain("[[");
  expect(calls.published).toHaveLength(1);
});

// -- behind the answer (#779) ----------------------------------------------

const STORED = { source_id: "wiki", parent: "wiki:98400" };
const ON_ITS_WAY = protocol({
  publication: { write_id: 21, state: "pending", detail: null, page_entity_id: null, linked: false },
});
const LANDED = protocol({
  publication: { write_id: 21, state: "sent", detail: null, page_entity_id: "wiki:98411", linked: true },
});

function ticket(over: Partial<ActionItemTicket> = {}): ActionItemTicket {
  return { write_id: 33, state: "pending", detail: null, ticket_entity_id: null, linked: false, ...over };
}

async function fileOne() {
  button("Create ticket")?.click();
  await settle();
  const select = [...target.querySelectorAll("select")].at(-1)!;
  select.value = "tracker:PAY";
  select.dispatchEvent(new Event("change", { bubbles: true }));
  await settle();
  button("Create")?.click();
  await settle();
}

test("a publish closes on the queue's answer, draws the page as on its way, and names it when it lands", async () => {
  const { reads } = render(protocol(), { stored: STORED, published: ON_ITS_WAY });
  await settle();
  open = [queued(21, "pending")];
  button("Publish to Confluence")?.click();
  await settle();
  // On its way: the title, marked, and no link yet.
  expect(text()).toContain(`Publishing ${DAY}`);
  expect(text()).toContain("on its way");
  expect(text()).not.toContain("Published as");

  // The write lands and the sync brings the page in: the next read names it.
  open = [];
  reads(LANDED);
  moved();
  await settle();
  expect(text()).toContain(`Published as ${DAY}`);
  expect(handlers).toHaveLength(0);
});

test("a refused publish loses its echo, says why, and Edit and resend opens the dialog again", async () => {
  const { reads } = render(protocol(), { stored: STORED, published: ON_ITS_WAY });
  await settle();
  open = [queued(21, "pending")];
  button("Publish to Confluence")?.click();
  await settle();

  open = [queued(21, "refused", "a page with this title already exists")];
  // A refused write is no publication: the read answers none.
  reads(protocol());
  moved();
  await settle();
  expect(text()).not.toContain("on its way");
  expect(button("Publish to Confluence")).toBeDefined();
  const told = toasts.items.filter((t) => t.tone === "err");
  expect(told.map((t) => t.text)).toEqual(["Publish was refused: a page with this title already exists"]);
  expect(told[0]!.more?.map((a) => a.label)).toEqual(["Edit and resend"]);

  told[0]!.more![0]!.run();
  await settle();
  expect(text()).toContain("Where do standup protocols go?");
});

test("a ticket's dialog closes on the queue's answer, the ticket is drawn on its way, and its key fills in", async () => {
  render(protocol(), {
    filed: ticket(),
    ticketReads: [ticket({ state: "sent", ticket_entity_id: "tracker:PAY-999", linked: true })],
  });
  await settle();
  open = [queued(33, "pending")];
  await fileOne();
  expect(text()).not.toContain("File a ticket");
  const filed = () => target.querySelector(".pr-t")?.textContent?.replace(/\s+/g, " ").trim() ?? "";
  expect(filed()).toContain("Ask Ines");
  expect(filed()).toContain("on its way");
  expect(filed()).not.toContain("PAY-999");

  open = [];
  moved();
  await settle();
  expect(filed()).toContain("tracker:PAY-999");
  expect(filed()).not.toContain("on its way");
  expect(handlers).toHaveLength(0);
});

test("a refused ticket loses its echo, says why, and Edit and resend reopens the dialog with what was typed", async () => {
  render(protocol(), { filed: ticket() });
  await settle();
  open = [queued(33, "pending")];
  await fileOne();

  open = [{ ...queued(33, "refused", "Issue type is required"), op: "create_ticket" }];
  moved();
  await settle();
  expect(target.querySelector(".pr-t")).toBeNull();
  const told = toasts.items.filter((t) => t.tone === "err");
  expect(told.map((t) => t.text)).toEqual(["Ticket was refused: Issue type is required"]);

  told[0]!.more![0]!.run();
  await settle();
  expect(text()).toContain("File a ticket");
  expect([...target.querySelectorAll("select")].at(-1)!.value).toBe("tracker:PAY");
  expect(target.querySelector<HTMLInputElement>("input[aria-label='Ticket type']")!.value).toBe("Task");
  expect(target.querySelector(".dlg .sub")?.textContent).toBe("Ask Ines");
});
