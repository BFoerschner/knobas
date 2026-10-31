/**
 * An item's page (#1201): the block view its own source answers, for an item
 * whose kind no surface draws, such as a custom role's (ADR-0029). The view
 * here is the one the WASM fixture's `view` answers for a survey (#1202).
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import type { EntityDetail, SourceView, ViewAction } from "../../ipc/entity";
import type { EntityRefresh, WriteOpPayload } from "../../ipc/sources";
import { refreshProbe, type RefreshProbe } from "../refresh.fixture";
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
    refreshes: true,
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
 * The source action the WASM fixture's survey view offers (#1407): a write
 * outside every role's vocabulary, its id fixed, one field the source asks for.
 */
const BOOK_REPAIR: ViewAction = {
  write_op: "source_action",
  label: "Book repair…",
  fields: [{ name: "day", label: "Day", kind: { type: "text", suggestions: [] }, value: null, required: true, placeholder: "Tuesday" }],
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
  probe = refreshProbe();
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

/** The page's *Refresh* and *Sync now*, and what they were asked. */
let probe: RefreshProbe;

/** The page's reads as given; its writes land in {@link writes}. */
async function open(reads: Omit<ItemPorts, "submitWrite">, id = ID, back: { hash: string; name: string } | null = null) {
  const ports: ItemPorts = {
    refresh: probe.ports,
    ...reads,
    submitWrite: (payload) => {
      writes.push(payload);
      return Promise.resolve({ id: 1 } as Awaited<ReturnType<ItemPorts["submitWrite"]>>);
    },
  };
  page = mount(TfItem, { target, props: { id, ready: true, back, ports } });
  await settle();
}

/** The survey's view with `actions` as its toolbar and `extra` blocks in its section. */
function surveyView(actions: ViewAction[], extra: SourceView["sections"][number]["blocks"] = []): SourceView {
  const [survey] = SURVEY_VIEW.sections;
  return { sections: [{ ...survey!, blocks: [...survey!.blocks, ...extra] }], actions };
}

const toolbar = () => [...target.querySelectorAll(".tf-fhead .tf-acts button:not(.tf-refresh)")].map((button) => button.textContent?.trim());

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
  expect(show!.className).toBe("tf-btn tf-sm");
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

test("the header runs the source's actions, then a ghost Open in, then Refresh last (#2288)", async () => {
  await open({
    getEntity: () => Promise.resolve(detail({ web_url: "https://surveys.example/1" })),
    entityView: () => Promise.resolve(surveyView([BOOK_REPAIR])),
  });

  const acts = [...target.querySelectorAll(".tf-fhead .tf-acts > *")].map((el) => [el.textContent?.trim(), el.className]);
  expect(acts).toEqual([
    ["Book repair", expect.stringContaining("tf-btn")],
    ["Open in Harbour surveys", "tf-btn tf-ghost"],
    ["Refresh", "tf-btn tf-refresh"],
  ]);
});

test("the header puts the source's primary action first, then the others in its order (#2288)", async () => {
  const close: ViewAction = { ...BOOK_REPAIR, label: "Close survey…", values: { id: "close" }, primary: true };
  const later: ViewAction = { ...BOOK_REPAIR, label: "Flag…", values: { id: "flag" } };
  await open({ getEntity: () => Promise.resolve(detail()), entityView: () => Promise.resolve(surveyView([BOOK_REPAIR, close, later])) });

  const acts = [...target.querySelectorAll(".tf-fhead .tf-acts > button")].map((el) => el.textContent?.trim());
  expect(acts.slice(0, 3)).toEqual(["Close survey", "Book repair", "Flag"]);
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
  expect(dialog.querySelector("h2")?.textContent?.trim()).toBe("Book repair on Quay crane 4 inspection");
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
  expect(toasts.items.at(-1)?.text).toBe("Queued: book repair on Quay crane 4 inspection.");
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
  // In the page body, so small (Controls: three sizes, chosen by place; #2288).
  expect(section.querySelector("button")!.className).toBe("tf-btn tf-sm");
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

test("the breadcrumb is the kind's label, and the hint names where Esc goes when the shell says it goes somewhere (#1408, #2228)", async () => {
  await open({ getEntity: () => Promise.resolve(detail()), entityView: () => Promise.resolve(SURVEY_VIEW) }, ID, { hash: "#/builds", name: "Builds" });

  const crumbs = target.querySelector('nav[aria-label="Breadcrumb"]')!;
  expect(crumbs.querySelector(".tf-jobchip")).toBeNull();
  expect(crumbs.querySelector("strong")?.textContent).toBe("Survey");
  expect(crumbs.querySelector(".tf-esc")?.textContent?.replace(/\s+/g, " ").trim()).toBe("Back to Builds Esc");
});

test("where Esc does nothing, the breadcrumb is the kind's label alone; the item's title stays in the header (#1408, #2228)", async () => {
  await open({ getEntity: () => Promise.resolve(detail()), entityView: () => Promise.resolve(SURVEY_VIEW) });

  const crumbs = target.querySelector('nav[aria-label="Breadcrumb"]')!;
  expect(crumbs.querySelector(".tf-jobchip")).toBeNull();
  expect(crumbs.querySelector("strong")?.textContent).toBe("Survey");
  expect(crumbs.querySelector(".tf-esc")).toBeNull();
  expect(crumbs.textContent).not.toContain("Quay crane 4 inspection");
  expect(target.querySelector("h1")?.textContent).toBe("Quay crane 4 inspection");
});

test("an item the mirror cannot read still draws the breadcrumb, named for an item", async () => {
  await open({
    getEntity: () => Promise.reject({ code: "not_found", message: "surveys:gone is not in the mirror", source_id: null }),
    entityView: () => Promise.resolve(null),
  }, "surveys:gone");

  expect(target.querySelector('nav[aria-label="Breadcrumb"] strong')?.textContent).toBe("Item");
});

// -- Refresh (#2107) -----------------------------------------------------------

const refreshButton = () => target.querySelector<HTMLButtonElement>(".tf-fhead .tf-refresh");
const banners = () => [...target.querySelectorAll(".tf-withdrawn, .tf-turnedoff")].map((banner) => banner.textContent?.trim() ?? "");

/** Reads that answer `first`, then each of `later` in turn, counting the detail reads. */
function reads(first: EntityDetail, ...later: EntityDetail[]) {
  const answers = [first, ...later];
  const count = { detail: 0, view: 0 };
  return {
    count,
    ports: {
      getEntity: () => Promise.resolve(answers[Math.min(count.detail++, answers.length - 1)]!),
      entityView: () => (count.view++, Promise.resolve(SURVEY_VIEW)),
    },
  };
}

test("Refresh reads the item, says Refreshing… meanwhile, and settles without blanking", async () => {
  const { ports, count } = reads(detail(), detail({ row: { ...detail().row, title: "Quay crane 4 inspection, signed off" } }));
  await open(ports);
  let answer!: (value: EntityRefresh) => void;
  probe.answer = () => new Promise((resolve) => (answer = resolve));

  expect(refreshButton()?.textContent).toBe("Refresh");
  refreshButton()!.click();
  flushSync();
  expect(probe.refreshed).toEqual([ID]);
  expect(refreshButton()?.textContent).toBe("Refreshing…");
  expect(refreshButton()?.disabled).toBe(true);
  // The detail stays drawn while the read is out.
  expect(target.querySelector("h1")?.textContent).toBe("Quay crane 4 inspection");
  expect(target.querySelector(".tf-item-text")?.textContent).toBe(BODY);

  answer({ outcome: "refreshed" });
  await settle();
  expect(count.detail).toBe(2);
  expect(count.view).toBe(2);
  expect(target.querySelector("h1")?.textContent).toBe("Quay crane 4 inspection, signed off");
  expect(refreshButton()?.textContent).toBe("Refresh");
  expect(target.querySelector(".tf-refresh-failed")).toBeNull();
});

test("a tombstoned item keeps Refresh, and one that comes back loses the withdrawn banner", async () => {
  const gone = detail({ deleted_at: "2026-10-25T08:00:00Z" });
  await open(reads(gone, detail()).ports);
  expect(banners().some((line) => line.startsWith("Withdrawn upstream"))).toBe(true);
  expect(refreshButton()?.textContent).toBe("Refresh");

  refreshButton()!.click();
  await settle();
  expect(probe.refreshed).toEqual([ID]);
  expect(banners()).toEqual([]);
});

test("gone draws the withdrawn banner over the detail", async () => {
  await open(reads(detail(), detail({ deleted_at: "2026-10-25T08:00:00Z" })).ports);
  expect(banners()).toEqual([]);
  probe.answer = () => Promise.resolve({ outcome: "gone" });

  refreshButton()!.click();
  await settle();
  expect(banners().some((line) => line.startsWith("Withdrawn upstream"))).toBe(true);
  expect(target.querySelector("h1")?.textContent).toBe("Quay crane 4 inspection");
});

test("a failed refresh says why in one line and keeps the detail as it was", async () => {
  const { ports, count } = reads(detail());
  await open(ports);
  probe.answer = () => Promise.resolve({ outcome: "failed", reason: "connection refused" });

  refreshButton()!.click();
  await settle();
  expect(target.querySelector(".tf-refresh-failed")?.textContent).toBe("Couldn't refresh: connection refused");
  expect(count.detail).toBe(1);
  expect(target.querySelector("h1")?.textContent).toBe("Quay crane 4 inspection");
  expect(target.querySelector(".tf-item-text")?.textContent).toBe(BODY);
  expect(refreshButton()?.textContent).toBe("Refresh");
});

test("an adapter that does not read one item offers Sync now, which runs the item's source", async () => {
  const { ports, count } = reads(detail({ refreshes: false }));
  await open(ports);
  expect(refreshButton()?.textContent).toBe("Sync now");

  refreshButton()!.click();
  await settle();
  expect(probe.synced).toEqual(["surveys"]);
  expect(probe.refreshed).toEqual([]);
  expect(refreshButton()?.textContent).toBe("Syncing…");

  probe.end("surveys");
  await settle();
  expect(refreshButton()?.textContent).toBe("Sync now");
  expect(count.detail).toBe(2);
});

test("an instance that answers not offered gets its source's run, and Sync now from then", async () => {
  await open(reads(detail()).ports);
  probe.answer = () => Promise.resolve({ outcome: "not_offered" });

  refreshButton()!.click();
  await settle();
  expect(probe.synced).toEqual(["surveys"]);
  expect(refreshButton()?.textContent).toBe("Syncing…");
});

test("a turned-off source's item draws neither button, and its banner names the remedy", async () => {
  await open(reads(detail({ source: { ...detail().source, enabled: false } })).ports);
  expect(refreshButton()).toBeNull();
  expect(banners()).toEqual([
    "Its source is turned off: Harbour surveys is disabled, so this item is hidden everywhere else. Re-enable the source to bring it back.",
  ]);
});

test("a source run the not-offered answer asked for and was refused says why in a toast", async () => {
  await open(reads(detail()).ports);
  probe.answer = () => Promise.resolve({ outcome: "not_offered" });
  probe.run = () => Promise.reject({ code: "not_ready", message: "the engine is starting", source_id: null });

  refreshButton()!.click();
  await settle();
  expect(toasts.items.map((toast) => toast.text)).toEqual(["Could not sync: the engine is starting"]);
  expect(refreshButton()?.textContent).toBe("Sync now");
});

test("a text block the source marks Markdown is drawn by the Markdown component; the plain one beside it as written (#2261)", async () => {
  await open({
    getEntity: () => Promise.resolve(detail()),
    entityView: () => Promise.resolve(surveyView([], [{ type: "text", text: "## Findings\n\n- rust on **leg 2**", markdown: true }])),
  });
  expect(target.querySelector(".tf-item-textp .tf-md h2")?.textContent).toBe("Findings");
  expect(target.querySelector(".tf-item-textp .tf-md li strong")?.textContent).toBe("leg 2");
  expect(target.querySelector(".tf-item-text")?.textContent).toBe(BODY);
});
