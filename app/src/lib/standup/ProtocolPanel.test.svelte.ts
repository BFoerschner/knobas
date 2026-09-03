/**
 * The protocol panel: what a reader sees, what it asks before publishing, and
 * what pressing the buttons sends (issue #289, spec #272 stories 64-69).
 *
 * The seam is spec #272's: **a rendered view in, user-visible text and the
 * calls it made out.** Nothing here reaches into the component's state, and
 * nothing here re-tests the backend — what a publish *does* is
 * `crates/knobas-app/tests/protocol_ipc.rs`'s, and what is asserted here is
 * which question the reader is asked first and what the answer is turned into.
 *
 * The one rule this file is really about is story 68: **with two Confluence
 * sources the dialog has no preselection.** A panel that quietly aimed at the
 * first configured wiki would look identical in a screenshot and would publish
 * the team's standup into the wrong instance.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { NoteDetail, Protocol, PublishTarget } from "../ipc/entity";
import { createRouter } from "../shell/router.svelte";
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
          current = protocol({
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
          return Promise.resolve({
            write_id: 3,
            ticket_entity_id: "tracker:PAY-999",
            linked: true,
          });
        },
      },
    },
  });
  flushSync();
  return { router, calls };
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
