/**
 * An item's page (#1201): the block view its own source answers, for an item
 * whose kind no surface draws, such as a custom role's (ADR-0029). The view
 * here is the one the WASM fixture's `view` answers for a survey (#1202).
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import type { EntityDetail, SourceView } from "../../ipc/entity";
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

let target: HTMLElement;
let page: ReturnType<typeof mount> | null = null;

beforeEach(() => {
  target = document.createElement("div");
  document.body.appendChild(target);
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

async function open(ports: ItemPorts, id = ID) {
  page = mount(TfItem, { target, props: { id, ready: true, ports } });
  await settle();
}

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
