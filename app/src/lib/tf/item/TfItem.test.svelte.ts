/**
 * An item's page (#1201): the block view its own source answers, for an item
 * whose kind no surface draws, such as a custom role's (ADR-0029). The view
 * here is the one the WASM fixture's `view` answers for a survey (#1202).
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import type { EntityDetail, SourceView, ViewAction } from "../../ipc/entity";
import type { WriteOpPayload } from "../../ipc/sources";
import { toasts } from "../../shell/toasts.svelte";
import TfItem, { type ItemPorts } from "./TfItem.svelte";

const ID = "surveys:survey-1";
const BODY = "quaywright inspection of quay crane 4: hoist brake worn, booked for Tuesday";

/** The mirror's row of a fixture survey, as `get_entity` answers it. */
function detail(over: Partial<EntityDetail> = {}): EntityDetail {
  return {
    row: { entity_id: ID, kind: "survey", source_id: "surveys", title: "Quay crane 4 inspection", updated_at: null, synced_at: "", path: null, state: null },
    source: { id: "surveys", display_name: "Harbour surveys", adapter_kind: "wasm-fixture", enabled: true },
    kind_info: { label: "Survey", plural: "Surveys", monogram: "SV" },
    body_text: BODY,
    author: "harbourmaster",
    payload: { key: "survey-1", title: "<b>never drawn</b>" },
    web_url: null,
    deleted_at: null,
    links: [],
    activity: [],
    builds: [],
    status: null,
    issue_type: null,
    priority: null,
    estimate_seconds: null,
    ...over,
  } as EntityDetail;
}

/** The fixture's survey view: the surveyor as a fact, the body as text. */
const SURVEY_VIEW: SourceView = {
  sections: [
    {
      id: "survey",
      title: "Quay crane 4 inspection",
      count: null,
      deferred: false,
      blocks: [
        { type: "facts", facts: [{ label: "Surveyor", value: "harbourmaster", tone: null }] },
        { type: "text", text: BODY, markdown: false },
      ],
    },
  ],
  actions: [],
};

/**
 * The fixture's source action on a survey (#1407): a write outside every
 * role's vocabulary, its id fixed, one field the source asks for.
 */
const BOOK_REPAIR: ViewAction = {
  write_op: "source_action",
  label: "Book repair…",
  fields: [{ name: "day", label: "Day", kind: { type: "text", suggestions: [] }, value: null, required: true, placeholder: null }],
  confirm: "Book a repair crew for this survey's finding?",
  primary: false,
  danger: false,
  entity: null,
  values: { id: "book_repair" },
  disabled: null,
  with_settings: false,
  line_comments: "none",
};

let target: HTMLElement;
let page: ReturnType<typeof mount> | null = null;
/** What the page handed the write queue. */
let writes: WriteOpPayload[] = [];

beforeEach(() => {
  target = document.createElement("div");
  document.body.appendChild(target);
  writes = [];
  toasts.items = [];
});

afterEach(() => {
  if (page) unmount(page);
  page = null;
  target.remove();
});

async function settle() {
  for (let turn = 0; turn < 5; turn += 1) await new Promise((resolve) => setTimeout(resolve, 0));
  flushSync();
}

/** The page's reads as given; its writes land in {@link writes}. */
async function open(reads: Omit<ItemPorts, "submitWrite">, id = ID, job: string | null = null) {
  const ports: ItemPorts = {
    ...reads,
    submitWrite: (payload) => {
      writes.push(payload);
      return Promise.resolve({ id: 1 } as Awaited<ReturnType<ItemPorts["submitWrite"]>>);
    },
  };
  page = mount(TfItem, { target, props: { id, ready: true, job, ports } });
  await settle();
}

/** The survey's view with `actions` as its toolbar and `extra` blocks in its section. */
function surveyView(actions: ViewAction[], extra: SourceView["sections"][number]["blocks"] = []): SourceView {
  const [survey] = SURVEY_VIEW.sections;
  return { sections: [{ ...survey!, blocks: [...survey!.blocks, ...extra] }], actions };
}

const toolbar = () => [...target.querySelectorAll(".tf-fhead .tf-acts button")].map((button) => button.textContent?.trim());

test("an item opens the block view its source answers: its facts and its text", async () => {
  const asked: [string, string | null][] = [];
  await open({
    getEntity: () => Promise.resolve(detail()),
    entityView: (id, section) => (asked.push([id, section]), Promise.resolve(SURVEY_VIEW)),
  });

  expect(asked).toEqual([[ID, null]]);
  expect(target.querySelector("h1")?.textContent).toBe("Quay crane 4 inspection");
  expect(target.textContent).toContain("Survey · Harbour surveys");
  const facts = [...target.querySelectorAll("dl dt")].map((dt) => [dt.textContent, dt.nextElementSibling?.textContent]);
  expect(facts).toEqual([["Surveyor", "harbourmaster"]]);
  expect(target.querySelector(".tf-item-text")?.textContent).toBe(BODY);
  expect(target.textContent).not.toContain("Showing the mirror");
  // The payload is never read (ADR-0007).
  expect(target.textContent).not.toContain("never drawn");
});

test("a source with no view for the kind draws the mirror's text, with no line", async () => {
  await open({ getEntity: () => Promise.resolve(detail()), entityView: () => Promise.resolve(null) });

  expect(target.querySelector("h1")?.textContent).toBe("Quay crane 4 inspection");
  expect(target.querySelector(".tf-item-text")?.textContent).toBe(BODY);
  expect(target.querySelector("dl")).toBeNull();
  expect(target.textContent).not.toContain("Showing the mirror");
});

test("a failed view read draws the mirror under a line naming the failure (ADR-0020 rule 2)", async () => {
  await open({
    getEntity: () => Promise.resolve(detail()),
    entityView: () => Promise.reject({ code: "unreachable", message: "Harbour surveys did not answer", source_id: "surveys" }),
  });

  expect(target.querySelector(".tf-cfail")?.textContent).toBe("Showing the mirror: Harbour surveys did not answer");
  expect(target.querySelector(".tf-item-text")?.textContent).toBe(BODY);
});

test("a deferred section is read when the reader opens it", async () => {
  const asked: (string | null)[] = [];
  const view: SourceView = {
    sections: [
      SURVEY_VIEW.sections[0]!,
      { id: "history", title: "History", count: 2, deferred: true, blocks: [] },
    ],
    actions: [],
  };
  await open({
    getEntity: () => Promise.resolve(detail()),
    entityView: (_id, section) => {
      asked.push(section);
      if (section === null) return Promise.resolve(view);
      return Promise.resolve({
        sections: [{ id: "history", title: "History", count: 2, deferred: false, blocks: [{ type: "text", text: "Booked twice", markdown: false }] }],
        actions: [],
      });
    },
  });
  expect(target.textContent).not.toContain("Booked twice");

  const show = [...target.querySelectorAll("button")].find((button) => button.textContent?.trim() === "Show History (2)");
  show!.click();
  await settle();

  expect(asked).toEqual([null, "history"]);
  expect(target.textContent).toContain("Booked twice");
});

test("an id the mirror does not hold says so", async () => {
  await open({
    getEntity: () => Promise.reject({ code: "not_found", message: "surveys:gone is not in the mirror", source_id: null }),
    entityView: () => Promise.resolve(null),
  }, "surveys:gone");

  expect(target.querySelector(".tf-cfail")?.textContent).toBe("Could not read surveys:gone: surveys:gone is not in the mirror");
});

test("a view's source action is drawn as the page's toolbar, named as the source names it (#1407)", async () => {
  await open({ getEntity: () => Promise.resolve(detail()), entityView: () => Promise.resolve(surveyView([BOOK_REPAIR])) });

  expect(toolbar()).toEqual(["Book repair"]);
});

test("a view with no actions draws no toolbar", async () => {
  await open({ getEntity: () => Promise.resolve(detail()), entityView: () => Promise.resolve(SURVEY_VIEW) });

  expect(toolbar()).toEqual([]);
});

test("pressing a source action opens its dialog and queues one SourceAction against the item, with the Queued toast (#1407)", async () => {
  await open({ getEntity: () => Promise.resolve(detail()), entityView: () => Promise.resolve(surveyView([BOOK_REPAIR])) });

  target.querySelector<HTMLButtonElement>(".tf-fhead .tf-acts button")!.click();
  flushSync();
  const dialog = document.querySelector(".tf-dialog")!;
  expect(dialog.textContent).toContain("Book a repair crew for this survey's finding?");
  const day = dialog.querySelector<HTMLInputElement>("input[type=text]")!;
  day.value = "Tuesday";
  day.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
  const footer = [...dialog.querySelectorAll<HTMLButtonElement>("footer button")];
  expect(footer.map((button) => button.textContent?.trim())).toEqual(["Cancel", "Book repair"]);
  footer[1]!.click();
  await settle();

  expect(writes).toEqual([{ SourceAction: { entity: ID, id: "book_repair", fields: { day: "Tuesday" } } }]);
  expect(document.querySelector(".tf-dialog")).toBeNull();
  expect(toasts.items.at(-1)?.text).toBe("Queued: book repair on Quay crane 4 inspection. Sends with the next sync.");
});

test("an actions block is drawn in place as the same buttons, not named as another role's (#1407)", async () => {
  await open({
    getEntity: () => Promise.resolve(detail()),
    entityView: () => Promise.resolve(surveyView([], [{ type: "actions", actions: [BOOK_REPAIR] }])),
  });

  expect(toolbar()).toEqual([]);
  const section = target.querySelector("section")!;
  const inPlace = [...section.querySelectorAll("button")].map((button) => button.textContent?.trim());
  expect(inPlace).toEqual(["Book repair"]);
  expect(target.textContent).not.toContain("drawn on its role's page");

  section.querySelector("button")!.click();
  flushSync();
  expect(document.querySelector(".tf-dialog")?.textContent).toContain("Book a repair crew for this survey's finding?");
});

test("a block of another role's type is still named, not drawn", async () => {
  await open({
    getEntity: () => Promise.resolve(detail()),
    entityView: () => Promise.resolve(surveyView([], [{ type: "diff", files: [], truncated: false, commentable: null, gone: [] }])),
  });

  expect(target.textContent).toContain("A diff block is drawn on its role's page, not here.");
});

test("a source action the source draws disabled says why and opens nothing", async () => {
  const refused: ViewAction = { ...BOOK_REPAIR, disabled: "No crew is free this week." };
  await open({ getEntity: () => Promise.resolve(detail()), entityView: () => Promise.resolve(surveyView([refused])) });

  const button = target.querySelector<HTMLButtonElement>(".tf-fhead .tf-acts button")!;
  expect(button.getAttribute("aria-disabled")).toBe("true");
  expect(button.title).toBe("No crew is free this week.");
  button.click();
  flushSync();
  expect(document.querySelector(".tf-dialog")).toBeNull();
});
