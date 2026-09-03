/**
 * The slide-over, and the three answers it has to be able to draw.
 *
 * A found entity, a **not_found** one — which is an ordinary event for a deep
 * link into a corpus that has not synced yet, not an error dialog — and a
 * malformed address. All three are panels; none of them is a blank aside.
 */
import { flushSync, mount, unmount } from "svelte";
import { beforeEach, expect, test, vi } from "vitest";

import type { EntityDetail, LinkEntry } from "../ipc/entity";

/** A plain function, not a `vi.fn` — see the note in `shell/Tile.test.svelte.ts`. */
const calls: string[] = [];
let answer: (entityId: string) => Promise<EntityDetail> = () =>
  Promise.resolve(detail());

/** Link ids handed to `unlink`, and whether the command refuses. */
const unlinked: string[] = [];
let unlinkFails = false;

/** Every `create_link` the *Link to…* dialog issued. */
const created: { fromId: string; toId: string }[] = [];

vi.mock("../ipc/entity", () => ({
  // The ticket detail's status select (#179) reads the granted board and
  // queues through the write queue. Not what this file is about, so both
  // answer with nothing.
  miniBoard: () => Promise.resolve({ columns: [], sources: [] }),
  submitWrite: () => Promise.reject(new Error("no write in this test")),
  // Contexts (#47): the store imports these at module level, so every mock of
  // this module has to define them even where no context is ever made.
  listContexts: () => Promise.resolve([]),
  contextMembers: () => Promise.resolve([]),
  createContext: () =>
    Promise.reject(new Error("no context creation in this test")),
  promoteContext: () => Promise.reject(new Error("no promotion in this test")),
  getEntity: (entityId: string) => {
    calls.push(entityId);
    return answer(entityId);
  },
  unlink: async (linkId: string) => {
    unlinked.push(linkId);
    if (unlinkFails)
      throw { code: "not_found", message: "no such link", source_id: null };
  },
  createLink: async (fromId: string, toId: string) => {
    created.push({ fromId, toId });
    return {};
  },
}));

/** The dialog's target picker. One fixed answer is enough here: this file is
    about the detail's wiring, not about the picker (`LinkDialog.test`). */
vi.mock("../ipc/search", () => ({
  search: async () => ({
    interpreted: { prefix: "none", text: "", segments: [], filters: null },
    groups: [
      {
        kind: "page",
        label: "Page",
        plural: "Pages",
        monogram: "PG",
        total: 1,
        hits: [
          {
            entity_id: "mock:ENG-SEPA",
            kind: "page",
            source_id: "mock",
            updated_at: null,
            synced_at: "2026-08-28T09:30:00Z",
            path: null,
            title: "SEPA retry runbook",
            rank: 1,
            snippet: [],
          },
        ],
      },
    ],
    total: 1,
    took_ms: 2,
  }),
  launcherHome: () =>
    Promise.reject(new Error("the dialog never loads the board")),
  noFilters: () => ({
    sources: [],
    kinds: [],
    updated_within_days: null,
    mine: false,
    authors: [],
  }),
}));

/** What the OS opener was handed, and whether it refused. */
const opened: string[] = [];
let openerFails = false;

vi.mock("@tauri-apps/plugin-opener", () => ({
  openUrl: async (url: string) => {
    opened.push(url);
    if (openerFails) throw new Error("no handler");
  },
}));

const { default: Detail } = await import("./Detail.svelte");
const { toasts } = await import("../shell/toasts.svelte");
const { linkChanges } = await import("./links.svelte");

function detail(over: Partial<EntityDetail> = {}): EntityDetail {
  return {
    row: {
      entity_id: "mock:PAY-231",
      kind: "ticket",
      source_id: "mock",
      title: "Retry failed SEPA payouts",
      updated_at: "2026-08-22T11:48:00Z",
      synced_at: "2026-08-22T14:30:00Z",
      // A ticket sits nowhere: the ADR-0007 miss, and the default this file's
      // other tests are written against.
      path: null,
    },
    source: {
      id: "mock",
      display_name: "Tidewater (mock)",
      adapter_kind: "mock",
      enabled: true,
    },
    kind_info: null,
    body_text: "Payouts to two SEPA banks fail with a 409 on retry.",
    author: "mara",
    payload: { key: "PAY-231", status: "In Progress" },
    web_url: null,
    deleted_at: null,
    links: [],
    activity: [],
    ...over,
  };
}

function render(props: { entityId?: string; kind?: string | null } = {}) {
  const target = document.createElement("div");
  document.body.append(target);
  const onclose = vi.fn();
  const onnavigate = vi.fn();
  const app = mount(Detail, {
    target,
    props: {
      entityId: props.entityId ?? "mock:PAY-231",
      kind: props.kind ?? "ticket",
      contextLabel: "All work",
      onclose,
      onnavigate,
    },
  });
  flushSync();
  return {
    target,
    onclose,
    onnavigate,
    /**
     * The panel's text with runs of whitespace collapsed.
     *
     * `textContent` reproduces the newlines and indentation of the *template*,
     * so a sentence that happens to wrap across two source lines contains a
     * newline and eight spaces in the middle. That is a fact about how the
     * markup is formatted, not about what the reader sees.
     */
    text: () => (target.textContent ?? "").replace(/\s+/g, " "),
    done: () => {
      unmount(app);
      target.remove();
    },
  };
}

beforeEach(() => {
  calls.length = 0;
  opened.length = 0;
  unlinked.length = 0;
  created.length = 0;
  openerFails = false;
  unlinkFails = false;
  answer = () => Promise.resolve(detail());
});

/** The header's *Open in browser*, if the panel is showing one. */
function openButton(target: HTMLElement) {
  return [...target.querySelectorAll<HTMLButtonElement>(".d-h button")].find(
    (button) => button.textContent?.includes("Open in browser"),
  );
}

test("draws the title, the provenance and the projected payload", async () => {
  const screen = render();
  await vi.waitFor(() =>
    expect(screen.text()).toContain("Retry failed SEPA payouts"),
  );
  flushSync();

  expect(calls).toEqual(["mock:PAY-231"]);
  expect(screen.text()).toContain("Tidewater (mock)");
  expect(screen.text()).toContain("mara");
  // The §3a projection, not a per-kind view.
  expect(screen.text()).toContain("Status");
  expect(screen.text()).toContain("In Progress");
  expect(screen.target.querySelector(".d-h .crumb")?.textContent).toContain(
    "All work",
  );

  screen.done();
});

/**
 * Body text is a source system's prose, and it is *text*.
 *
 * The mutation this survives is `{@html detail.body_text}`, which renders
 * identically for every payload in this fixture and turns a ticket description
 * into a script-injection path (gotcha 7).
 */
test("body text and title are interpolated, never parsed as markup", async () => {
  answer = () =>
    Promise.resolve(
      detail({
        row: { ...detail().row, title: "<b>bold</b> ticket" },
        body_text: "<script>alert(1)</script>",
        payload: { desc: "<img onerror=x>" },
      }),
    );
  const screen = render();
  await vi.waitFor(() =>
    expect(screen.text()).toContain("<script>alert(1)</script>"),
  );
  flushSync();

  expect(screen.text()).toContain("<b>bold</b> ticket");
  expect(screen.text()).toContain("<img onerror=x>");
  // The only `<b>` in the panel is the crumb's own separator: nothing from the
  // payload became an element.
  expect(
    [...screen.target.querySelectorAll("b")].map((b) => b.textContent),
  ).toEqual(["›"]);
  expect(screen.target.querySelector("script")).toBeNull();
  expect(screen.target.querySelector("img")).toBeNull();

  screen.done();
});

/**
 * A deep link into a corpus that has not synced yet is a normal event.
 *
 * It gets a panel that names the id, says what happened, and offers the way
 * out — not a blank aside and not a toast the reader has already missed.
 */
test("not_found renders a panel with the id and a way out", async () => {
  answer = () =>
    Promise.reject({
      code: "not_found",
      message: "mock:NOPE-1 is not in the local index",
      source_id: null,
    });

  const screen = render({ entityId: "mock:NOPE-1" });
  await vi.waitFor(() =>
    expect(screen.text()).toContain("Not in the local index"),
  );
  flushSync();

  expect(screen.text()).toContain("NOPE-1");
  expect(screen.text()).toContain("mock:NOPE-1 is not in the local index");
  expect(screen.target.querySelector(".d-b")).not.toBeNull();

  const close = [...screen.target.querySelectorAll("button")].find(
    (button) => button.textContent?.trim() === "Close",
  );
  close?.click();
  expect(screen.onclose).toHaveBeenCalledOnce();

  screen.done();
});

/** A mistyped address says *that*, rather than claiming the entity is missing. */
test("invalid and not_found say different things", async () => {
  answer = () =>
    Promise.reject({
      code: "invalid",
      message: "no-colon has no ':' separator",
      source_id: null,
    });

  const screen = render({ entityId: "no-colon" });
  await vi.waitFor(() =>
    expect(screen.text()).toContain("not an entity address"),
  );
  expect(screen.text()).not.toContain("Not in the local index");
  screen.done();
});

/** The close button is the same call the Esc ladder makes. */
test("the header's x closes the panel", async () => {
  const screen = render();
  await vi.waitFor(() =>
    expect(screen.text()).toContain("Retry failed SEPA payouts"),
  );

  screen.target.querySelector<HTMLButtonElement>(".d-h .x")?.click();
  expect(screen.onclose).toHaveBeenCalledOnce();

  screen.done();
});

/**
 * Focus enters the panel on open and returns to the opener on close.
 *
 * Both halves, because a panel that took focus and never gave it back leaves
 * a keyboard reader at the top of the document with no way to say where they
 * were.
 */
test("focus moves into the panel and back to whatever opened it", async () => {
  const opener = document.createElement("button");
  document.body.append(opener);
  opener.focus();
  expect(document.activeElement).toBe(opener);

  const screen = render();
  flushSync();
  expect(screen.target.querySelector(".d-h")).toBe(document.activeElement);

  screen.done();
  expect(document.activeElement).toBe(opener);
  opener.remove();
});

/** An undeclared kind gets the generic view, headed by the title-cased kind. */
test("an undeclared kind is browsable — §3a's promise", async () => {
  answer = () =>
    Promise.resolve(
      detail({
        row: {
          entity_id: "mock:INC-1",
          kind: "incident",
          source_id: "mock",
          title: "Payout queue backed up",
          updated_at: "2026-08-22T08:05:00Z",
          synced_at: "2026-08-22T14:30:00Z",
          path: null,
        },
        kind_info: null,
        payload: {
          id: "INC-1",
          severity: "SEV2",
          affected_services: ["payout", "ledger"],
        },
      }),
    );

  const screen = render({ entityId: "mock:INC-1", kind: "incident" });
  await vi.waitFor(() =>
    expect(screen.text()).toContain("Payout queue backed up"),
  );
  flushSync();

  expect(screen.target.querySelector(".d-h .crumb")?.textContent).toContain(
    "Incident",
  );
  expect(screen.text()).toContain("Severity");
  expect(screen.text()).toContain("SEV2");
  // The nested value is summarised, and its JSON is behind a disclosure.
  expect(screen.text()).toContain("2 items");
  expect(screen.target.querySelector("details")).not.toBeNull();

  screen.done();
});

/**
 * The adapter's own words win when it has any.
 *
 * The backend resolves `kind_info` now (`commands/entity.rs::kind_info_for`,
 * pinned by `tests/entity.rs`), so this is the path every configured source's
 * detail takes; the null-`kind_info` test above is what covers the fallback.
 */
test("a declared kind_info names the header", async () => {
  answer = () =>
    Promise.resolve(
      detail({
        kind_info: {
          id: "ticket",
          label: "Issue",
          plural: "Issues",
          monogram: "IS",
          full_sync_exhaustive: true,
        },
      }),
    );

  const screen = render();
  await vi.waitFor(() =>
    expect(screen.text()).toContain("Retry failed SEPA payouts"),
  );
  flushSync();

  expect(screen.target.querySelector(".d-h .crumb")?.textContent).toContain(
    "Issue",
  );
  expect(screen.target.querySelector(".mg")?.textContent).toBe("IS");

  screen.done();
});

/** A slow answer for an entity the reader has left must not land. */
test("a superseded read is discarded", async () => {
  let resolveFirst!: (value: EntityDetail) => void;
  answer = () =>
    new Promise<EntityDetail>((resolve) => (resolveFirst = resolve));

  const target = document.createElement("div");
  document.body.append(target);
  const props = $state({
    entityId: "mock:PAY-231",
    kind: "ticket" as string | null,
    contextLabel: "All work",
    onclose: vi.fn(),
    onnavigate: vi.fn(),
  });
  const app = mount(Detail, { target, props });
  flushSync();

  answer = () =>
    Promise.resolve(
      detail({
        row: { ...detail().row, entity_id: "mock:PAY-228", title: "Second" },
      }),
    );
  props.entityId = "mock:PAY-228";
  flushSync();
  await vi.waitFor(() => expect(target.textContent ?? "").toContain("Second"));

  resolveFirst(detail());
  await Promise.resolve();
  flushSync();

  expect(target.textContent ?? "").not.toContain("Retry failed SEPA payouts");
  expect(target.textContent ?? "").toContain("Second");

  unmount(app);
  target.remove();
});

/**
 * §5a: an entity the source withdrew is still readable, and says so.
 *
 * The banner is the whole point — without it a withdrawn ticket is
 * indistinguishable from a live one, and the reason knobas kept it (links and
 * notes may point at it) is the reason the reader is looking at it.
 */
test("a withdrawn entity opens with a banner that says why it is still here", async () => {
  answer = () =>
    Promise.resolve(
      detail({
        row: {
          entity_id: "mock:PAY-198",
          kind: "ticket",
          source_id: "mock",
          title: "Legacy payout reconciliation (withdrawn)",
          updated_at: null,
          synced_at: "2026-08-22T14:30:00Z",
          path: null,
        },
        deleted_at: "2026-08-22T12:00:00Z",
        web_url: null,
      }),
    );

  const screen = render({ entityId: "mock:PAY-198" });
  await vi.waitFor(() => expect(screen.text()).toContain("Withdrawn upstream"));
  flushSync();

  expect(screen.target.querySelector(".prompt")).not.toBeNull();
  expect(screen.text()).toContain("links and notes may point at it");
  // ...and it is still a readable item, not just a banner.
  expect(screen.text()).toContain("Legacy payout reconciliation (withdrawn)");

  screen.done();
});

/** A live entity carries no banner at all. */
test("a live entity has no withdrawn banner", async () => {
  const screen = render();
  await vi.waitFor(() =>
    expect(screen.text()).toContain("Retry failed SEPA payouts"),
  );
  flushSync();

  expect(screen.target.querySelector(".prompt")).toBeNull();
  expect(screen.text()).not.toContain("Withdrawn upstream");

  screen.done();
});

/**
 * Issue #204: the other reason a reader hides an entity. A disabled source's
 * item opens by direct address exactly as a tombstoned one does, and gets the
 * same treatment — a banner naming the fact and the remedy. Without it the
 * item opens looking entirely ordinary while every list pretends it does not
 * exist.
 */
test("a disabled source's entity opens with a banner naming the remedy", async () => {
  answer = () =>
    Promise.resolve(
      detail({
        source: {
          id: "mock",
          display_name: "Tidewater (mock)",
          adapter_kind: "mock",
          enabled: false,
        },
      }),
    );

  const screen = render();
  await vi.waitFor(() =>
    expect(screen.text()).toContain("source is turned off"),
  );
  flushSync();

  expect(screen.text()).toContain("Tidewater (mock)");
  expect(screen.text()).toContain("Re-enable");
  // Turned off is not withdrawn — the two banners never borrow each other's
  // words, or the reader is told upstream did something the user did.
  expect(screen.text()).not.toContain("Withdrawn upstream");
  // ...and it is still a readable item, not just a banner.
  expect(screen.text()).toContain("Retry failed SEPA payouts");

  screen.done();
});

/**
 * #204's miss direction, from the ruling's own tests clause: an entity from
 * an enabled source must carry no marker.
 */
test("an enabled source's entity has no turned-off banner", async () => {
  const screen = render();
  await vi.waitFor(() =>
    expect(screen.text()).toContain("Retry failed SEPA payouts"),
  );
  flushSync();

  expect(screen.text()).not.toContain("source is turned off");

  screen.done();
});

/**
 * The three states stay three: a withdrawn entity of an *enabled* source
 * shows the withdrawal alone. (When both facts hold, both banners show —
 * they are independent facts with independent remedies.)
 */
test("a withdrawn entity of an enabled source does not claim the source is off", async () => {
  answer = () =>
    Promise.resolve(
      detail({
        deleted_at: "2026-08-22T12:00:00Z",
      }),
    );

  const screen = render();
  await vi.waitFor(() => expect(screen.text()).toContain("Withdrawn upstream"));
  flushSync();

  expect(screen.text()).not.toContain("source is turned off");

  screen.done();
});

/** A hydrated link entry, as `get_entity` now hands them over. */
function link(over: {
  id: string;
  from?: string;
  to?: string;
  relation?: string;
  origin?: LinkEntry["link"]["origin"];
  note?: string | null;
  otherKind?: string;
  otherTitle?: string;
  otherDeleted?: string | null;
}): LinkEntry {
  const from = over.from ?? "mock:PAY-231";
  const to = over.to ?? "mock:payout-service#142";
  return {
    link: {
      id: over.id,
      from_id: from,
      to_id: to,
      relation: over.relation ?? "implements",
      origin: over.origin ?? "manual",
      note: over.note ?? null,
      created_by: "mara",
      created_at: "2026-08-22T12:00:00Z",
      confirmed_at: "2026-08-22T12:00:00Z",
      rule: null,
      rule_class: null,
      reason: null,
    },
    other: {
      entity_id: from === "mock:PAY-231" ? to : from,
      kind: over.otherKind ?? "pr",
      title: over.otherTitle ?? "Retry SEPA payouts behind the gateway",
      deleted_at: over.otherDeleted ?? null,
    },
  };
}

/**
 * The empty panel invites linking rather than explaining that linking is
 * impossible — which is what it said through M1, and is no longer true.
 */
test("the links panel is present, empty, and invites linking", async () => {
  const screen = render();
  await vi.waitFor(() => expect(screen.text()).toContain("Linked items"));
  flushSync();

  expect(screen.text()).toContain("Nothing linked yet");
  expect(screen.text()).toContain("Link this to");
  // The M1 caveat lived in a `title` attribute, where `textContent` cannot see
  // it, so the markup is what is asserted.
  expect(screen.target.innerHTML).not.toContain("M2");

  screen.done();
});

/**
 * A link shows the *other* end whichever way it was drawn, by title and kind.
 *
 * The far end's id is deliberately **not** what the row says: the whole point
 * of the hydrated read is that the reader recognises what they linked.
 */
test("a link renders the far end by title, in either direction", async () => {
  answer = () =>
    Promise.resolve(
      detail({
        links: [
          link({
            id: "11111111-1111-1111-1111-111111111111",
            to: "mock:payout-service#142",
            relation: "implements",
            otherTitle: "Retry SEPA payouts behind the gateway",
          }),
          link({
            id: "22222222-2222-2222-2222-222222222222",
            from: "mock:ENG-SEPA",
            to: "mock:PAY-231",
            relation: "documents",
            note: "the retry storm postmortem",
            otherKind: "page",
            otherTitle: "SEPA retry runbook",
          }),
        ],
      }),
    );

  const screen = render();
  await vi.waitFor(() => expect(screen.text()).toContain("implements"));
  flushSync();

  expect(screen.text()).toContain("Retry SEPA payouts behind the gateway");
  expect(screen.text()).toContain("SEPA retry runbook");
  expect(screen.text()).toContain("the retry storm postmortem");
  // The second link was drawn *at* this entity, so it reads inverted.
  expect(screen.text()).toContain("documented by");
  expect(screen.text()).not.toContain("Nothing linked yet");
  // Neither row names the entity being viewed.
  const cells = [...screen.target.querySelectorAll(".lopen")].map((n) =>
    n.textContent?.trim(),
  );
  expect(cells.join(" ")).not.toContain("mock:PAY-231");

  screen.done();
});

/** Story 8: a row is navigation, and the shell is the only navigator. */
test("clicking a linked row asks the shell for the other end's address", async () => {
  answer = () =>
    Promise.resolve(
      detail({
        links: [
          link({
            id: "11111111-1111-1111-1111-111111111111",
            to: "mock:payout-service#142",
            otherKind: "pr",
            otherTitle: "Retry SEPA payouts",
          }),
        ],
      }),
    );

  const screen = render();
  await vi.waitFor(() => expect(screen.text()).toContain("Retry SEPA payouts"));
  flushSync();

  const row = [
    ...screen.target.querySelectorAll<HTMLButtonElement>(".lopen"),
  ][0]!;
  row.click();
  flushSync();

  expect(screen.onnavigate).toHaveBeenCalledWith(
    "#/pr/mock:payout-service%23142",
  );

  screen.done();
});

/**
 * Story 11: one action, and the panel is right afterwards **without the
 * reader reopening the detail**.
 *
 * The re-read is what is asserted, not a locally spliced array: the links
 * array is the backend's answer, and a panel that edited its own copy would
 * disagree with the next read.
 */
test("unlinking removes the row without reopening the detail", async () => {
  const kept = link({
    id: "22222222-2222-2222-2222-222222222222",
    otherTitle: "Kept link",
  });
  const doomed = link({
    id: "11111111-1111-1111-1111-111111111111",
    to: "mock:payout-service#142",
    otherTitle: "Doomed link",
  });
  let links = [doomed, kept];
  answer = () => Promise.resolve(detail({ links }));

  const screen = render();
  await vi.waitFor(() => expect(screen.text()).toContain("Doomed link"));
  flushSync();

  // The backend is what forgets the link; the panel re-reads.
  links = [kept];
  const unlinkButton = [
    ...screen.target.querySelectorAll<HTMLButtonElement>("button"),
  ].find((button) => button.textContent?.includes("Unlink"))!;
  unlinkButton.click();

  await vi.waitFor(() => expect(screen.text()).not.toContain("Doomed link"));
  flushSync();

  expect(unlinked).toEqual(["11111111-1111-1111-1111-111111111111"]);
  expect(screen.text()).toContain("Kept link");
  expect(calls).toEqual(["mock:PAY-231", "mock:PAY-231"]);
  expect(screen.onclose).not.toHaveBeenCalled();

  screen.done();
});

/**
 * An `implied` link is drawn by knobas from a `[[reference]]` in a note's
 * body (#46), so it is visible from *both* ends -- and from this end too,
 * tombstoning it would be undone by that note's next autosave, silently.
 * The refusal names the note, because the reference lives there and this
 * panel cannot edit it.
 */
test("unlinking a note's reference from the target's end is refused, with the reason", async () => {
  answer = () =>
    Promise.resolve(
      detail({
        links: [
          link({
            id: "33333333-3333-3333-3333-333333333333",
            from: "note:7f2c",
            to: "mock:PAY-231",
            relation: "references",
            origin: "implied",
            otherKind: "note",
            otherTitle: "SEPA retry investigation",
          }),
        ],
      }),
    );

  const screen = render();
  await vi.waitFor(() =>
    expect(screen.text()).toContain("SEPA retry investigation"),
  );
  flushSync();

  [...screen.target.querySelectorAll<HTMLButtonElement>("button")]
    .find((button) => button.textContent?.includes("Unlink"))!
    .click();

  await vi.waitFor(() =>
    expect(toasts.items.map((toast) => toast.text).join(" ")).toMatch(
      /Remove the reference/,
    ),
  );
  flushSync();

  expect(unlinked, "the row was never tombstoned").toEqual([]);
  expect(screen.text()).toContain("SEPA retry investigation");
  expect(calls, "nothing to re-read").toEqual(["mock:PAY-231"]);

  screen.done();
  toasts.items = [];
});

/** A refused unlink says so and leaves the row where it was. */
test("an unlink that fails is reported and changes nothing", async () => {
  unlinkFails = true;
  answer = () =>
    Promise.resolve(
      detail({
        links: [
          link({
            id: "11111111-1111-1111-1111-111111111111",
            otherTitle: "Still here",
          }),
        ],
      }),
    );

  const screen = render();
  await vi.waitFor(() => expect(screen.text()).toContain("Still here"));
  flushSync();

  [...screen.target.querySelectorAll<HTMLButtonElement>("button")]
    .find((button) => button.textContent?.includes("Unlink"))!
    .click();

  await vi.waitFor(() =>
    expect(toasts.items.map((toast) => toast.text).join(" ")).toMatch(
      /no such link/,
    ),
  );
  flushSync();

  expect(screen.text()).toContain("Still here");
  expect(calls, "a refused unlink does not re-read").toEqual(["mock:PAY-231"]);

  toasts.items = [];
  screen.done();
});

/**
 * #54's criterion, literally: *the created link appears in the panel without
 * reopening the detail*.
 *
 * The whole path in one test, because that is what the criterion is: the
 * header's button opens the dialog, the dialog writes, and the panel shows the
 * row afterwards — with the slide-over never having closed.
 */
test("a link made in the dialog appears in the panel without reopening the detail", async () => {
  let links: LinkEntry[] = [];
  answer = () => Promise.resolve(detail({ links }));

  const screen = render();
  await vi.waitFor(() => expect(screen.text()).toContain("Nothing linked yet"));
  flushSync();

  const openDialog = [
    ...screen.target.querySelectorAll<HTMLButtonElement>(".d-h button"),
  ].find((button) => button.textContent?.includes("Link to"))!;
  openDialog.click();
  flushSync();
  expect(screen.target.querySelector('[role="dialog"]')).not.toBeNull();

  const picker = screen.target.querySelector<HTMLInputElement>(
    'input[placeholder="Search everything"]',
  )!;
  picker.value = "sepa";
  picker.dispatchEvent(new Event("input", { bubbles: true }));
  await vi.waitFor(() =>
    expect(
      screen.target.querySelectorAll('[role="option"]').length,
    ).toBeGreaterThan(0),
  );
  picker.dispatchEvent(
    new KeyboardEvent("keydown", {
      key: "Enter",
      bubbles: true,
      cancelable: true,
    }),
  );
  flushSync();

  // What the backend will answer once the write lands.
  links = [
    link({
      id: "33333333-3333-3333-3333-333333333333",
      to: "mock:ENG-SEPA",
      relation: "related",
      otherKind: "page",
      otherTitle: "SEPA retry runbook",
    }),
  ];
  [...screen.target.querySelectorAll<HTMLButtonElement>("button")]
    .find((button) => button.textContent?.trim() === "Link")!
    .click();

  // The re-read, not the dialog's own echo of the picked target: the title is
  // on screen either way, so what is waited for is the second `get_entity`.
  await vi.waitFor(() => expect(calls).toHaveLength(2));
  flushSync();

  expect(created).toEqual([{ fromId: "mock:PAY-231", toId: "mock:ENG-SEPA" }]);
  expect(screen.text()).toContain("SEPA retry runbook");
  // The panel is the backend's answer, re-read — not a locally spliced array.
  expect(calls).toEqual(["mock:PAY-231", "mock:PAY-231"]);
  expect(screen.text()).not.toContain("Nothing linked yet");
  // The dialog is done and the detail never closed.
  expect(screen.target.querySelector('[role="dialog"]')).toBeNull();
  expect(screen.text(), "the new row reads under its own heading").toContain(
    "related to",
  );
  expect(screen.onclose).not.toHaveBeenCalled();
  expect(screen.text()).toContain("Retry failed SEPA payouts");

  screen.done();
});

/**
 * The Esc ladder, from the detail's side: the dialog is rung 1, so one press
 * closes it and leaves the slide-over standing.
 */
test("Esc in the dialog closes the dialog and not the detail", async () => {
  const screen = render();
  await vi.waitFor(() => expect(screen.text()).toContain("Linked items"));
  flushSync();

  [...screen.target.querySelectorAll<HTMLButtonElement>(".d-h button")]
    .find((button) => button.textContent?.includes("Link to"))!
    .click();
  flushSync();

  // The shell's global ladder, stood up for real: `installKeys` binds `window`,
  // so a key that reaches it here is a key that would unwind the slide-over as
  // well as the dialog on one press.
  const reachedTheShell: string[] = [];
  const listener = (event: KeyboardEvent) => reachedTheShell.push(event.key);
  window.addEventListener("keydown", listener);

  const dialog = screen.target.querySelector<HTMLElement>('[role="dialog"]')!;
  dialog.dispatchEvent(
    new KeyboardEvent("keydown", {
      key: "Escape",
      bubbles: true,
      cancelable: true,
    }),
  );
  flushSync();
  window.removeEventListener("keydown", listener);

  expect(screen.target.querySelector('[role="dialog"]')).toBeNull();
  expect(reachedTheShell, "one keystroke must not unwind two ladders").toEqual(
    [],
  );
  expect(screen.onclose).not.toHaveBeenCalled();

  screen.done();
});

/**
 * #55's other half, from the detail's side.
 *
 * The launcher can link this entity while the slide-over is open, and the
 * write happens in the shell. A panel that then kept saying "Nothing linked
 * yet" would be showing a state the app has already left.
 */
test("a link drawn from outside the detail brings the panel up to date", async () => {
  let links: LinkEntry[] = [];
  answer = () => Promise.resolve(detail({ links }));

  const screen = render();
  await vi.waitFor(() => expect(screen.text()).toContain("Nothing linked yet"));
  flushSync();

  links = [
    link({
      id: "44444444-4444-4444-4444-444444444444",
      to: "mock:ENG-SEPA",
      otherKind: "page",
      otherTitle: "Linked from the launcher",
    }),
  ];
  linkChanges.count += 1;
  flushSync();

  await vi.waitFor(() =>
    expect(screen.text()).toContain("Linked from the launcher"),
  );
  expect(calls).toEqual(["mock:PAY-231", "mock:PAY-231"]);
  expect(screen.text()).not.toContain("Nothing linked yet");

  screen.done();
});

/** The entity's own history rides along with the read. */
test("history arrives with the entity rather than in a second call", async () => {
  answer = () =>
    Promise.resolve(
      detail({
        activity: [
          {
            id: 1,
            at: "2026-08-22T14:30:00Z",
            actor: "sync:mock",
            verb: "synced",
            entity_id: "mock:PAY-231",
            detail: { upserted: 9 },
          },
        ],
      }),
    );

  const screen = render();
  await vi.waitFor(() => expect(screen.text()).toContain("History"));
  flushSync();

  expect(calls).toEqual(["mock:PAY-231"]);
  expect(screen.text()).toContain("synced by mock");

  screen.done();
});

// -- Open in browser --------------------------------------------------------

/**
 * The button exists only when there is somewhere to go (P5).
 *
 * Both directions. A `web_url` of `null` is the ordinary state for a source
 * with no per-item page and for anything withdrawn upstream, and a disabled
 * button there would claim a page exists.
 */
test("Open in browser appears exactly when the adapter reported a url", async () => {
  const screen = render();
  await vi.waitFor(() =>
    expect(screen.text()).toContain("Retry failed SEPA payouts"),
  );
  flushSync();
  expect(openButton(screen.target), "no url, so no button").toBeUndefined();
  screen.done();

  answer = () =>
    Promise.resolve(
      detail({ web_url: "https://127.0.0.1:8443/browse/PAY-231" }),
    );
  const withUrl = render();
  await vi.waitFor(() => expect(openButton(withUrl.target)).toBeDefined());
  flushSync();

  openButton(withUrl.target)?.click();
  await vi.waitFor(() =>
    expect(opened).toEqual(["https://127.0.0.1:8443/browse/PAY-231"]),
  );

  withUrl.done();
});

/**
 * A refused scheme is reported, not swallowed.
 *
 * `web_url` is a remote system's data; a source configured with an `ftp://`
 * base URL produces one, and a button that silently did nothing would look
 * like a bug in knobas rather than like something a person can fix.
 */
test("a url the guard refuses never reaches the opener, and says so", async () => {
  answer = () => Promise.resolve(detail({ web_url: "file:///etc/passwd" }));
  const screen = render();
  await vi.waitFor(() => expect(openButton(screen.target)).toBeDefined());
  flushSync();

  openButton(screen.target)?.click();
  await vi.waitFor(() =>
    expect(toasts.items.map((toast) => toast.text).join(" ")).toMatch(
      /Could not open the link/,
    ),
  );
  expect(opened, "file:// reached the OS opener").toEqual([]);
  expect(toasts.items.map((toast) => toast.text).join(" ")).toMatch(/file:/);

  toasts.items = [];
  screen.done();
});

/** ...and so is a failure from the OS itself. */
test("a failure from the opener becomes a toast", async () => {
  openerFails = true;
  answer = () =>
    Promise.resolve(detail({ web_url: "https://127.0.0.1:8443/x" }));
  const screen = render();
  await vi.waitFor(() => expect(openButton(screen.target)).toBeDefined());
  flushSync();

  openButton(screen.target)?.click();
  await vi.waitFor(() =>
    expect(toasts.items.map((toast) => toast.text).join(" ")).toMatch(
      /no handler/,
    ),
  );
  expect(opened).toEqual(["https://127.0.0.1:8443/x"]);

  toasts.items = [];
  screen.done();
});

/**
 * A page opens at its address, shows its ancestor path, and shows its body as
 * **text** (#284).
 *
 * The body half was the criterion's "for now" in #284 and the markup renderer
 * landed in #285 — but this fixture's *source* is the mock adapter, so it
 * still takes the plain-text arm, and that is what keeps it meaningful: it is
 * now the direction *"anything but a Confluence source shows `body_text`"*.
 * The words are drawn as text and never as markup, which is roadmap §4 gotcha
 * 7 and is why the escaped tag below has to come out as the characters the
 * author typed rather than as an element. The Confluence arm is
 * `a Confluence page renders its markup, its macro placeholders and its
 * comments` at the bottom of this file.
 */
test("a page shows its ancestor path and its body as text", async () => {
  answer = () =>
    Promise.resolve(
      detail({
        row: {
          entity_id: "confluence:98307",
          kind: "page",
          source_id: "confluence",
          title: "SEPA payout retry design",
          updated_at: "2026-08-22T10:40:00Z",
          synced_at: "2026-08-22T14:30:00Z",
          // Joined by `knobas_core::ancestor_path_read!` and carried on the
          // row, so the panel and the launcher draw the same string. The
          // payload below still holds the `ancestors` the statement read it
          // out of, which is what makes this fixture a page and not a stub.
          path: "Engineering › Payments",
        },
        body_text:
          "SEPA payout retry design\n\nBackoff policy\nbase 30 s, factor 2\n\nuse <retry> here",
        payload: {
          id: "98307",
          space: { key: "ENG", name: "Engineering" },
          ancestors: [
            { id: "65537", title: "Engineering" },
            { id: "65540", title: "Payments" },
          ],
          body: {
            storage: {
              value: "<h2>Backoff policy</h2>",
              representation: "storage",
            },
          },
        },
      }),
    );
  const screen = render({ entityId: "confluence:98307", kind: "page" });
  await vi.waitFor(() =>
    expect(screen.text()).toContain("SEPA payout retry design"),
  );
  flushSync();

  expect(calls).toEqual(["confluence:98307"]);
  expect(screen.target.querySelector(".d-path")?.textContent).toBe(
    "Engineering › Payments",
  );
  // The body, as text: the stripped words are there, and the one that looks
  // like markup arrived as characters rather than as an element.
  const body = screen.target.querySelector(".d-body");
  expect(body?.textContent).toContain("Backoff policy");
  expect(body?.textContent).toContain("use <retry> here");
  expect(body?.querySelector("retry")).toBeNull();

  screen.done();
});

/**
 * ADR-0007's failure direction, at the seam a reader actually meets: a record
 * with no ancestors draws **no path**, rather than an empty line or a guess.
 * Every ticket, build and commit in the mirror is this case.
 */
test("an item whose record names no ancestors shows no path at all", async () => {
  const screen = render();
  await vi.waitFor(() =>
    expect(screen.text()).toContain("Retry failed SEPA payouts"),
  );
  flushSync();
  expect(screen.target.querySelector(".d-path")).toBeNull();
  screen.done();
});

/**
 * The **page detail**, end to end: `get_entity`'s payload in, a rendered page
 * out (#285, criteria 2 and 3).
 *
 * The seam this covers is the one between the sanitizer and the panel. The
 * sanitizer's own DOM assertions live in `StorageBody.test.svelte.ts`; what is
 * here is that the panel finds the storage format in the payload, prefers it
 * to the normalized text, and puts the comments under it. `body_text` carries
 * a marker no rendering of the markup could produce, so "the page renders" and
 * "the text arm was taken" cannot both be true.
 */
test("a Confluence page renders its markup, its macro placeholders and its comments", async () => {
  answer = () =>
    Promise.resolve(
      detail({
        row: {
          entity_id: "confluence:98307",
          kind: "page",
          source_id: "confluence",
          title: "SEPA payout retry design",
          updated_at: "2026-08-22T10:40:00Z",
          synced_at: "2026-08-22T14:30:00Z",
          path: "Engineering › Payments",
        },
        source: {
          id: "confluence",
          display_name: "Tidewater wiki",
          adapter_kind: "confluence",
          enabled: true,
        },
        // The FTS blob. If this shows up on screen the panel took the plain
        // text arm, and no assertion about the markup below means anything.
        body_text: "STRIPPED-TEXT-ARM",
        payload: {
          id: "98307",
          space: { key: "ENG", name: "Engineering" },
          body: {
            storage: {
              value:
                "<h2>Backoff policy</h2><p>base <strong>30 s</strong>, factor 2.</p>" +
                '<ac:structured-macro ac:name="info"><ac:rich-text-body>' +
                "<p>owned by payments</p></ac:rich-text-body></ac:structured-macro>" +
                "<table><thead><tr><th>Code</th><th>Meaning</th></tr></thead>" +
                "<tbody><tr><td>503</td><td>retry</td></tr></tbody></table>" +
                "<script>alert(1)</script>",
              representation: "storage",
            },
          },
          children: {
            comment: {
              results: [
                {
                  id: "98320",
                  body: { storage: { value: "<p>@Mara can you add the <em>SLA</em>?</p>" } },
                },
                { id: "98321", body: { storage: { value: "<p>on it</p>" } } },
              ],
            },
          },
        },
      }),
    );
  const screen = render({ entityId: "confluence:98307", kind: "page" });
  await vi.waitFor(() => expect(screen.text()).toContain("Backoff policy"));
  flushSync();

  const body = screen.target.querySelector(".d-body.storage");
  expect(body, "the page detail did not render its storage format at all").not.toBeNull();
  expect(body!.querySelector("h2")?.textContent).toBe("Backoff policy");
  expect(body!.querySelector("strong")?.textContent).toBe("30 s");
  // A table, as a table (criterion 2).
  expect(body!.querySelector("table")!.rows).toHaveLength(2);
  // A macro, named, with its own body gone with it.
  expect([...body!.querySelectorAll(".ac")].map((n) => n.textContent?.trim())).toEqual([
    "info macro",
  ]);
  expect(body!.textContent).not.toContain("owned by payments");
  // gotcha 7, at the surface it is about: no script element anywhere in the
  // panel, and its body is not shown as prose in the page either.
  //
  // Scoped to the body for the prose half, deliberately: the §3a *Details*
  // projection below shows the whole raw payload as text, `body.storage.value`
  // included, so the panel's full `textContent` legitimately contains the
  // characters `<script>alert(1)</script>` -- as characters, in a disclosure,
  // which is what `payload.ts` exists to guarantee. The claim being made here
  // is about the *rendered page*.
  expect(screen.target.querySelector("script")).toBeNull();
  expect(body!.textContent).not.toContain("alert(1)");
  // The markup arm was taken, not the text arm.
  expect(screen.text(), "the panel fell back to the FTS blob").not.toContain("STRIPPED-TEXT-ARM");

  // The comments, under the body, each rendered the same way (criterion 3).
  const comments = [...screen.target.querySelectorAll(".cmt")];
  expect(comments).toHaveLength(2);
  expect(comments[0]!.textContent).toContain("can you add the SLA?");
  expect(comments[0]!.querySelector("em")?.textContent).toBe("SLA");
  expect(comments[1]!.textContent).toContain("on it");
  expect(screen.text()).toContain("Comments");
  // Under the body: the document order is body first, then comments.
  const order = [...screen.target.querySelectorAll(".d-body.storage, .cmt")];
  expect(order[0]).toBe(body);

  screen.done();
});

/**
 * The gate, from the other side: a page from **another adapter** keeps its
 * body as text.
 *
 * `page` is a word two adapters may both emit and a Gitea wiki page's body is
 * Markdown, so a renderer chosen by the shape of the payload would render one
 * product's markup with another's rules — the guess ADR-0007 forbids. The
 * payload here is deliberately Confluence-shaped: the only thing that differs
 * is which adapter the source runs.
 */
test("a page from another adapter is not rendered as Confluence markup", async () => {
  answer = () =>
    Promise.resolve(
      detail({
        row: {
          entity_id: "gitea:wiki/Home",
          kind: "page",
          source_id: "gitea",
          title: "Home",
          updated_at: null,
          synced_at: "2026-08-22T14:30:00Z",
          path: null,
        },
        source: {
          id: "gitea",
          display_name: "Tidewater Gitea",
          adapter_kind: "gitea",
          enabled: true,
        },
        body_text: "Home\n\nrunbooks live here",
        payload: {
          body: { storage: { value: "<h2>Not Confluence</h2>", representation: "storage" } },
        },
      }),
    );
  const screen = render({ entityId: "gitea:wiki/Home", kind: "page" });
  await vi.waitFor(() => expect(screen.text()).toContain("runbooks live here"));
  flushSync();

  expect(screen.target.querySelector(".d-body.storage")).toBeNull();
  expect(screen.target.querySelector(".d-body h2")).toBeNull();
  // The body, and not the panel: the *Details* projection shows the raw
  // payload as text, so `Not Confluence` is legitimately on screen there --
  // inside a JSON disclosure, as characters.
  expect(
    screen.target.querySelector(".d-body")?.textContent,
    "another product's markup was rendered as Confluence's",
  ).not.toContain("Not Confluence");
  screen.done();
});

/**
 * **A page is a link end** (criterion 4, asserted rather than built).
 *
 * Nothing in the linking flow knows about kinds — `create_link` takes two
 * addresses — so what is worth witnessing is that the flow is *reachable* from
 * a page's own panel: the header offers *Link to…*, the dialog opens over a
 * page, and the write goes out with the page's address as the near end. The
 * far end is a page too here, which is the picker's one fixed answer in this
 * file, so both ends of this link are pages.
 */
test("a page detail can draw a link, with the page as the near end", async () => {
  answer = () =>
    Promise.resolve(
      detail({
        row: {
          entity_id: "confluence:98307",
          kind: "page",
          source_id: "confluence",
          title: "SEPA payout retry design",
          updated_at: null,
          synced_at: "2026-08-22T14:30:00Z",
          path: "Engineering › Payments",
        },
        source: {
          id: "confluence",
          display_name: "Tidewater wiki",
          adapter_kind: "confluence",
          enabled: true,
        },
        body_text: "SEPA payout retry design",
        payload: { id: "98307" },
      }),
    );
  const screen = render({ entityId: "confluence:98307", kind: "page" });
  await vi.waitFor(() => expect(screen.text()).toContain("Nothing linked yet"));
  flushSync();

  [...screen.target.querySelectorAll<HTMLButtonElement>(".d-h button")]
    .find((button) => button.textContent?.includes("Link to"))!
    .click();
  flushSync();

  const picker = screen.target.querySelector<HTMLInputElement>(
    'input[placeholder="Search everything"]',
  )!;
  picker.value = "sepa";
  picker.dispatchEvent(new Event("input", { bubbles: true }));
  await vi.waitFor(() =>
    expect(screen.target.querySelectorAll('[role="option"]').length).toBeGreaterThan(0),
  );
  picker.dispatchEvent(
    new KeyboardEvent("keydown", { key: "Enter", bubbles: true, cancelable: true }),
  );
  flushSync();
  [...screen.target.querySelectorAll<HTMLButtonElement>("button")]
    .find((button) => button.textContent?.trim() === "Link")!
    .click();

  await vi.waitFor(() => expect(created).toHaveLength(1));
  expect(created).toEqual([{ fromId: "confluence:98307", toId: "mock:ENG-SEPA" }]);
  screen.done();
});
