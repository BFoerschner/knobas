/**
 * The slide-over, and the three answers it has to be able to draw.
 *
 * A found entity, a **not_found** one — which is an ordinary event for a deep
 * link into a corpus that has not synced yet, not an error dialog — and a
 * malformed address. All three are panels; none of them is a blank aside.
 */
import { flushSync, mount, unmount } from "svelte";
import { beforeEach, expect, test, vi } from "vitest";

import type { EntityDetail } from "../ipc/entity";

/** A plain function, not a `vi.fn` — see the note in `shell/Tile.test.svelte.ts`. */
const calls: string[] = [];
let answer: (entityId: string) => Promise<EntityDetail> = () => Promise.resolve(detail());

vi.mock("../ipc/entity", () => ({
  getEntity: (entityId: string) => {
    calls.push(entityId);
    return answer(entityId);
  },
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
  const app = mount(Detail, {
    target,
    props: {
      entityId: props.entityId ?? "mock:PAY-231",
      kind: props.kind ?? "ticket",
      contextLabel: "All work",
      onclose,
    },
  });
  flushSync();
  return {
    target,
    onclose,
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
  openerFails = false;
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

/**
 * The links panel ships empty and says so honestly.
 *
 * M1 never writes `knobas.link`, so this is the only state it can be in — and
 * an empty panel with no explanation reads as a bug rather than as a milestone
 * boundary.
 */
test("the links panel is present, empty, and explains itself", async () => {
  const screen = render();
  await vi.waitFor(() => expect(screen.text()).toContain("Linked items"));
  flushSync();

  expect(screen.text()).toContain("Nothing linked yet");
  const empty = [...screen.target.querySelectorAll(".empty")].find((node) =>
    node.textContent?.includes("Nothing linked yet"),
  );
  expect(empty?.getAttribute("title")).toMatch(/M2/);

  screen.done();
});

/** A link, when there is one, shows the *other* end whichever way it was drawn. */
test("a link renders the far end, in either direction", async () => {
  answer = () =>
    Promise.resolve(
      detail({
        links: [
          {
            id: "11111111-1111-1111-1111-111111111111",
            from_id: "mock:PAY-231",
            to_id: "mock:payout-service#142",
            relation: "implements",
            origin: "manual",
            created_by: "mara",
            created_at: "2026-08-22T12:00:00Z",
          },
          {
            id: "22222222-2222-2222-2222-222222222222",
            from_id: "mock:ENG-SEPA",
            to_id: "mock:PAY-231",
            relation: "documents",
            origin: "suggested",
            created_by: "mara",
            created_at: "2026-08-21T12:00:00Z",
          },
        ],
      }),
    );

  const screen = render();
  await vi.waitFor(() => expect(screen.text()).toContain("implements"));
  flushSync();

  expect(screen.text()).toContain("mock:payout-service#142");
  expect(screen.text()).toContain("mock:ENG-SEPA");
  expect(screen.text()).not.toContain("Nothing linked yet");
  // Neither row is the entity being viewed.
  const cells = [...screen.target.querySelectorAll(".row.g4 .t")].map((n) => n.textContent?.trim());
  expect(cells).not.toContain("mock:PAY-231");

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
