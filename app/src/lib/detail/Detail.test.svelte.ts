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
let answer: (entityId: string) => Promise<EntityDetail> = () => Promise.resolve(detail());

/** Link ids handed to `unlink`, and whether the command refuses. */
const unlinked: string[] = [];
let unlinkFails = false;

/** Every `create_link` the *Link to…* dialog issued. */
const created: { fromId: string; toId: string }[] = [];

vi.mock("../ipc/entity", () => ({
  getEntity: (entityId: string) => {
    calls.push(entityId);
    return answer(entityId);
  },
  unlink: async (linkId: string) => {
    unlinked.push(linkId);
    if (unlinkFails) throw { code: "not_found", message: "no such link", source_id: null };
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
  launcherHome: () => Promise.reject(new Error("the dialog never loads the board")),
  noFilters: () => ({ sources: [], kinds: [], updated_within_days: null, mine: false, authors: [] }),
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
    },
    source: { id: "mock", display_name: "Tidewater (mock)", adapter_kind: "mock" },
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
  return [...target.querySelectorAll<HTMLButtonElement>(".d-h button")].find((button) =>
    button.textContent?.includes("Open in browser"),
  );
}

test("draws the title, the provenance and the projected payload", async () => {
  const screen = render();
  await vi.waitFor(() => expect(screen.text()).toContain("Retry failed SEPA payouts"));
  flushSync();

  expect(calls).toEqual(["mock:PAY-231"]);
  expect(screen.text()).toContain("Tidewater (mock)");
  expect(screen.text()).toContain("mara");
  // The §3a projection, not a per-kind view.
  expect(screen.text()).toContain("Status");
  expect(screen.text()).toContain("In Progress");
  expect(screen.target.querySelector(".d-h .crumb")?.textContent).toContain("All work");

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
  await vi.waitFor(() => expect(screen.text()).toContain("<script>alert(1)</script>"));
  flushSync();

  expect(screen.text()).toContain("<b>bold</b> ticket");
  expect(screen.text()).toContain("<img onerror=x>");
  // The only `<b>` in the panel is the crumb's own separator: nothing from the
  // payload became an element.
  expect([...screen.target.querySelectorAll("b")].map((b) => b.textContent)).toEqual(["›"]);
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
  await vi.waitFor(() => expect(screen.text()).toContain("Not in the local index"));
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
    Promise.reject({ code: "invalid", message: "no-colon has no ':' separator", source_id: null });

  const screen = render({ entityId: "no-colon" });
  await vi.waitFor(() => expect(screen.text()).toContain("not an entity address"));
  expect(screen.text()).not.toContain("Not in the local index");
  screen.done();
});

/** The close button is the same call the Esc ladder makes. */
test("the header's x closes the panel", async () => {
  const screen = render();
  await vi.waitFor(() => expect(screen.text()).toContain("Retry failed SEPA payouts"));

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
        },
        kind_info: null,
        payload: { id: "INC-1", severity: "SEV2", affected_services: ["payout", "ledger"] },
      }),
    );

  const screen = render({ entityId: "mock:INC-1", kind: "incident" });
  await vi.waitFor(() => expect(screen.text()).toContain("Payout queue backed up"));
  flushSync();

  expect(screen.target.querySelector(".d-h .crumb")?.textContent).toContain("Incident");
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
 * `kind_info` is null throughout M1 (task 21 fills it), so this is the path
 * that would otherwise ship untested and break silently the day it is wired.
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
  await vi.waitFor(() => expect(screen.text()).toContain("Retry failed SEPA payouts"));
  flushSync();

  expect(screen.target.querySelector(".d-h .crumb")?.textContent).toContain("Issue");
  expect(screen.target.querySelector(".mg")?.textContent).toBe("IS");

  screen.done();
});

/** A slow answer for an entity the reader has left must not land. */
test("a superseded read is discarded", async () => {
  let resolveFirst!: (value: EntityDetail) => void;
  answer = () => new Promise<EntityDetail>((resolve) => (resolveFirst = resolve));

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

  answer = () => Promise.resolve(detail({ row: { ...detail().row, entity_id: "mock:PAY-228", title: "Second" } }));
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
  await vi.waitFor(() => expect(screen.text()).toContain("Retry failed SEPA payouts"));
  flushSync();

  expect(screen.target.querySelector(".prompt")).toBeNull();
  expect(screen.text()).not.toContain("Withdrawn upstream");

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
  const cells = [...screen.target.querySelectorAll(".lopen")].map((n) => n.textContent?.trim());
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

  const row = [...screen.target.querySelectorAll<HTMLButtonElement>(".lopen")][0]!;
  row.click();
  flushSync();

  expect(screen.onnavigate).toHaveBeenCalledWith("#/pr/mock:payout-service%23142");

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
  const kept = link({ id: "22222222-2222-2222-2222-222222222222", otherTitle: "Kept link" });
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
  const unlinkButton = [...screen.target.querySelectorAll<HTMLButtonElement>("button")].find(
    (button) => button.textContent?.includes("Unlink"),
  )!;
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
  await vi.waitFor(() => expect(screen.text()).toContain("SEPA retry investigation"));
  flushSync();

  [...screen.target.querySelectorAll<HTMLButtonElement>("button")]
    .find((button) => button.textContent?.includes("Unlink"))!
    .click();

  await vi.waitFor(() =>
    expect(toasts.items.map((toast) => toast.text).join(" ")).toMatch(/Remove the reference/),
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
        links: [link({ id: "11111111-1111-1111-1111-111111111111", otherTitle: "Still here" })],
      }),
    );

  const screen = render();
  await vi.waitFor(() => expect(screen.text()).toContain("Still here"));
  flushSync();

  [...screen.target.querySelectorAll<HTMLButtonElement>("button")]
    .find((button) => button.textContent?.includes("Unlink"))!
    .click();

  await vi.waitFor(() =>
    expect(toasts.items.map((toast) => toast.text).join(" ")).toMatch(/no such link/),
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

  const openDialog = [...screen.target.querySelectorAll<HTMLButtonElement>(".d-h button")].find(
    (button) => button.textContent?.includes("Link to"),
  )!;
  openDialog.click();
  flushSync();
  expect(screen.target.querySelector('[role="dialog"]')).not.toBeNull();

  const picker = screen.target.querySelector<HTMLInputElement>('input[placeholder="Search everything"]')!;
  picker.value = "sepa";
  picker.dispatchEvent(new Event("input", { bubbles: true }));
  await vi.waitFor(() =>
    expect(screen.target.querySelectorAll('[role="option"]').length).toBeGreaterThan(0),
  );
  picker.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true, cancelable: true }));
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
  expect(screen.text(), "the new row reads under its own heading").toContain("related to");
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
  dialog.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true }));
  flushSync();
  window.removeEventListener("keydown", listener);

  expect(screen.target.querySelector('[role="dialog"]')).toBeNull();
  expect(reachedTheShell, "one keystroke must not unwind two ladders").toEqual([]);
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

  await vi.waitFor(() => expect(screen.text()).toContain("Linked from the launcher"));
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
  await vi.waitFor(() => expect(screen.text()).toContain("Retry failed SEPA payouts"));
  flushSync();
  expect(openButton(screen.target), "no url, so no button").toBeUndefined();
  screen.done();

  answer = () => Promise.resolve(detail({ web_url: "https://127.0.0.1:8443/browse/PAY-231" }));
  const withUrl = render();
  await vi.waitFor(() => expect(openButton(withUrl.target)).toBeDefined());
  flushSync();

  openButton(withUrl.target)?.click();
  await vi.waitFor(() => expect(opened).toEqual(["https://127.0.0.1:8443/browse/PAY-231"]));

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
    expect(toasts.items.map((toast) => toast.text).join(" ")).toMatch(/Could not open the link/),
  );
  expect(opened, "file:// reached the OS opener").toEqual([]);
  expect(toasts.items.map((toast) => toast.text).join(" ")).toMatch(/file:/);

  toasts.items = [];
  screen.done();
});

/** ...and so is a failure from the OS itself. */
test("a failure from the opener becomes a toast", async () => {
  openerFails = true;
  answer = () => Promise.resolve(detail({ web_url: "https://127.0.0.1:8443/x" }));
  const screen = render();
  await vi.waitFor(() => expect(openButton(screen.target)).toBeDefined());
  flushSync();

  openButton(screen.target)?.click();
  await vi.waitFor(() =>
    expect(toasts.items.map((toast) => toast.text).join(" ")).toMatch(/no handler/),
  );
  expect(opened).toEqual(["https://127.0.0.1:8443/x"]);

  toasts.items = [];
  screen.done();
});
