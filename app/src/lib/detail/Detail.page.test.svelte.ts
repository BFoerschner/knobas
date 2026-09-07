/**
 * The page detail's **section edit** and its **comment** (#286).
 *
 * The rule the section rule exists to serve, asserted through the component
 * rather than through the pure function beside it: a reader is offered an edit
 * exactly where the rule says one is possible, the edit that goes out is the
 * **whole** body with everything else untouched, and where the rule refuses,
 * the wiki is offered instead of a disabled button.
 *
 * Its own file rather than more of `Detail.test.svelte.ts`: that one is about
 * the three answers a read can have, and this needs a loaded kind registry, a
 * Confluence payload and a queue to watch.
 */
import { flushSync, mount, unmount } from "svelte";
import { beforeEach, expect, test, vi } from "vitest";

import type { EntityDetail } from "../ipc/entity";
import type { SourceDescriptor } from "../ipc/sources";

/** Plain functions, not `vi.fn` — see the note in `shell/Tile.test.svelte.ts`. */
const queued: unknown[] = [];
const opened: string[] = [];

vi.mock("../ipc/entity", () => ({
  listContexts: () => Promise.resolve([]),
  contextMembers: () => Promise.resolve([]),
  createContext: () => Promise.reject(new Error("no context creation in this test")),
  promoteContext: () => Promise.reject(new Error("no promotion in this test")),
  getEntity: () => Promise.resolve(entity()),
  unlink: () => Promise.resolve(),
  createLink: () => Promise.resolve({}),
  miniBoard: () => Promise.reject(new Error("a page has no board")),
  submitWrite: (payload: unknown) => {
    queued.push(payload);
    return Promise.resolve({});
  },
}));

vi.mock("../ipc/search", () => ({
  search: () => Promise.reject(new Error("the picker is not this file's business")),
  launcherHome: () => Promise.reject(new Error("the dialog never loads the board")),
  noFilters: () => ({ sources: [], kinds: [], updated_within_days: null, mine: false, authors: [] }),
}));

let writeOps: string[] = ["comment", "update_page", "create_page"];

vi.mock("../ipc/sources", () => ({
  listAdapters: (): Promise<SourceDescriptor[]> =>
    Promise.resolve([
      {
        id: "confluence",
        adapter_kind: "confluence",
        name: "Tidewater Confluence",
        capabilities: ["Write"],
        adapter_version: "0",
        auth_methods: [],
        accepts_account: false,
        // A getter, because the registry caches its descriptors on the first
        // `load()` — see the same note in `Detail.status.test.svelte.ts`.
        get write_ops() {
          return writeOps;
        },
        entity_kinds: [],
        payload_paths: [],
        config_schema: {},
      },
    ]),
}));

vi.mock("../shell/open-external", () => ({
  openExternal: (url: string) => {
    opened.push(url);
    return Promise.resolve();
  },
}));

const { default: Detail } = await import("./Detail.svelte");
const { toasts } = await import("../shell/toasts.svelte");
const { kindRegistry } = await import("../shell/kind-registry.svelte");

/**
 * The seeded fixture page's shape: a preamble nobody titled, two sections of
 * prose, and one that holds a macro. Every claim below is about *this* body,
 * so the macro has to be in it rather than described.
 */
const STORAGE =
  "<p>the design, in outline.</p>" +
  "<h2>Backoff policy</h2><p>base 30 s, factor 2.</p>" +
  "<h2>Rollout</h2><p>behind a flag.</p>" +
  '<h2>Runbook</h2><ac:structured-macro ac:name="info"><ac:rich-text-body><p>call ops</p>' +
  "</ac:rich-text-body></ac:structured-macro>";

let entity: () => EntityDetail = () => page();

function page(over: Partial<EntityDetail> = {}, payload: Record<string, unknown> = {}): EntityDetail {
  return {
    row: {
      entity_id: "confluence:98307",
      kind: "page",
      source_id: "confluence",
      title: "SEPA payout retry design",
      updated_at: "2026-08-22T11:48:00Z",
      synced_at: "2026-08-22T14:30:00Z",
      path: null,
    },
    source: {
      id: "confluence",
      display_name: "Tidewater Confluence",
      adapter_kind: "confluence",
      enabled: true,
    },
    kind_info: null,
    body_text: "the stripped text nobody should see here",
    author: "mara.lindqvist",
    payload: {
      id: "98307",
      body: { storage: { value: STORAGE, representation: "storage" } },
      version: { number: 3, when: "2026-08-22T11:48:00Z", by: { username: "knobas" } },
      ...payload,
    },
    web_url: "http://127.0.0.1:8090/display/ENG/SEPA+payout+retry+design",
    deleted_at: null,
    links: [],
    activity: [],
    ...over,
  };
}

function render() {
  const target = document.createElement("div");
  document.body.append(target);
  const props = $state({
    entityId: "confluence:98307",
    kind: "page",
    contextLabel: "All work",
    onclose: vi.fn(),
    onnavigate: vi.fn(),
  });
  const app = mount(Detail, { target, props });
  flushSync();
  return {
    target,
    buttons: () =>
      [...target.querySelectorAll<HTMLButtonElement>("button")].map((b) =>
        (b.textContent ?? "").trim(),
      ),
    button: (label: string) =>
      [...target.querySelectorAll<HTMLButtonElement>("button")].find(
        (b) => (b.textContent ?? "").trim() === label,
      ),
    editors: () => [...target.querySelectorAll<HTMLTextAreaElement>("textarea")],
    text: () => (target.textContent ?? "").replace(/\s+/g, " "),
    done: () => {
      unmount(app);
      target.remove();
    },
  };
}

function press(button: HTMLButtonElement | undefined) {
  expect(button, "the button this test presses is not on screen").toBeDefined();
  button!.click();
  flushSync();
}

function type(area: HTMLTextAreaElement, text: string) {
  area.value = text;
  area.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
}

beforeEach(async () => {
  queued.length = 0;
  opened.length = 0;
  toasts.items = [];
  writeOps = ["comment", "update_page", "create_page"];
  entity = () => page();
  await kindRegistry.load();
});

/**
 * One *Edit section* per editable section, and none over the macro one — which
 * is the refusal rule reaching the screen.
 */
test("offers an edit for each macro-free section and the wiki for the rest", async () => {
  const screen = render();
  await vi.waitFor(() => expect(screen.buttons()).toContain("Edit section"));

  expect(screen.buttons().filter((b) => b === "Edit section")).toHaveLength(2);
  expect(screen.text()).toContain("This section has a macro, so knobas will not rewrite it.");
  // *Open in browser* twice: the page header's and the refused section's. The
  // refusal offers somewhere to go rather than a disabled button.
  expect(screen.buttons().filter((b) => b === "Open in browser")).toHaveLength(2);

  screen.done();
});

/**
 * **The whole body goes out.** Confluence's content `PUT` replaces the record,
 * so an `UpdatePage` carrying only the edited section would delete the rest of
 * the page — the macro section included, which is the one this app cannot
 * rebuild at all.
 */
test("a section edit queues the whole body with the mirrored version", async () => {
  const screen = render();
  await vi.waitFor(() => expect(screen.button("Edit section")).toBeDefined());

  press(screen.button("Edit section"));
  const area = screen.editors()[0]!;
  expect(area.value, "the editor did not open on the section as it stands").toBe(
    "base 30 s, factor 2.",
  );
  type(area, "base 45 s, factor 3.");
  press(screen.button("Queue edit"));
  await vi.waitFor(() => expect(queued).toHaveLength(1));

  expect(queued[0]).toEqual({
    UpdatePage: {
      entity: "confluence:98307",
      // The version the mirror holds, which is the version the reader was
      // looking at — not the one to write.
      base_version: 3,
      body:
        "<p>the design, in outline.</p>" +
        "<h2>Backoff policy</h2><p>base 45 s, factor 3.</p>" +
        "<h2>Rollout</h2><p>behind a flag.</p>" +
        '<h2>Runbook</h2><ac:structured-macro ac:name="info"><ac:rich-text-body><p>call ops</p>' +
        "</ac:rich-text-body></ac:structured-macro>",
    },
  });
  expect(toasts.items.at(-1)?.text).toContain("Edit to Backoff policy queued");

  screen.done();
});

/**
 * No version in the record, no edit offered — the stated failure direction of
 * the version read. An `UpdatePage` sent against a guessed `base_version`
 * would either overwrite somebody's work or be refused for a reason the reader
 * cannot act on.
 *
 * **And the refusal does not vanish with the offer.** The section still says
 * why it will not be rewritten and still offers the wiki: a section that
 * silently offers nothing is worse than one that says why, and folding the two
 * gates into one is exactly how the sentence would have been lost.
 */
test("says why and offers the wiki when the record does not say what version it is", async () => {
  entity = () => page({}, { version: { when: "2026-08-22T11:48:00Z" } });
  const screen = render();
  await vi.waitFor(() => expect(screen.text()).toContain("Backoff policy"));

  expect(screen.buttons()).not.toContain("Edit section");
  expect(screen.text()).toContain("knobas cannot tell what version this page is at");
  // One per section, plus the page header's own.
  expect(screen.buttons().filter((b) => b === "Open in browser")).toHaveLength(4);
  // And the page still renders, macro placeholder and all.
  expect(screen.text()).toContain("info macro");

  screen.done();
});

/**
 * An adapter that declares no page write says **nothing** — no footer, no
 * sentence, no offer. A reader whose Confluence knobas cannot write to should
 * see the page it always saw, not three explanations of an absence.
 */
test("says nothing about editing when the adapter declares no page write", async () => {
  writeOps = ["comment"];
  const screen = render();
  await vi.waitFor(() => expect(screen.text()).toContain("Backoff policy"));

  expect(screen.text()).not.toContain("knobas will not rewrite it");
  expect(screen.text()).not.toContain("knobas cannot tell what version");
  expect(screen.buttons().filter((b) => b === "Open in browser")).toHaveLength(1);

  screen.done();
});

/**
 * The other gate: an adapter that does not declare `update_page`. The action
 * bar is drawn from the descriptor, and `submit_write` refuses an op the
 * source does not offer — a button that queued one would be a surface at
 * fault.
 */
test("offers no edit when the adapter does not declare update_page", async () => {
  writeOps = ["comment"];
  const screen = render();
  await vi.waitFor(() => expect(screen.text()).toContain("Backoff policy"));

  expect(screen.buttons()).not.toContain("Edit section");
  expect(screen.buttons()).toContain("Comment");

  screen.done();
});

test("a comment on a page is queued as the SPI's own Comment op", async () => {
  const screen = render();
  await vi.waitFor(() => expect(screen.button("Comment")).toBeDefined());

  press(screen.button("Comment"));
  type(screen.editors()[0]!, "who owns the runbook?");
  press(screen.button("Queue comment"));
  await vi.waitFor(() => expect(queued).toHaveLength(1));

  expect(queued[0]).toEqual({
    Comment: { entity: "confluence:98307", body: "who owns the runbook?" },
  });
  expect(toasts.items.at(-1)?.text).toContain("Comment queued");

  screen.done();
});

/**
 * A comment's byline (#286). The expansions that carry it were added to the
 * adapter for this: before them the payload had the words and nothing else.
 */
test("a comment renders who wrote it, and renders without one when nobody is named", async () => {
  entity = () =>
    page({}, {
      children: {
        comment: {
          results: [
            {
              id: "98320",
              body: { storage: { value: "<p>@Mara can you add the SLA?</p>" } },
              version: { when: "2026-08-22T12:41:00Z", by: { username: "knobas" } },
            },
            { id: "98321", body: { storage: { value: "<p>no expansions here</p>" } } },
          ],
        },
      },
    });
  const screen = render();
  await vi.waitFor(() => expect(screen.text()).toContain("add the SLA"));

  expect(screen.text()).toContain("knobas");
  expect(screen.text()).toContain("no expansions here");
  // One byline, not two: the comment nobody is named on renders its words
  // rather than an empty byline over them.
  expect(screen.target.querySelectorAll(".cmt-by")).toHaveLength(1);

  screen.done();
});

/** Cancelling leaves the page as it was and queues nothing. */
test("cancelling a section edit queues nothing", async () => {
  const screen = render();
  await vi.waitFor(() => expect(screen.button("Edit section")).toBeDefined());

  press(screen.button("Edit section"));
  type(screen.editors()[0]!, "never mind");
  press(screen.button("Cancel"));

  expect(screen.editors()).toHaveLength(0);
  expect(queued).toEqual([]);
  expect(screen.text()).toContain("base 30 s, factor 2.");

  screen.done();
});
