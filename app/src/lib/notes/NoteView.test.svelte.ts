import { flushSync, mount, unmount } from "svelte";
import { beforeEach, expect, test, vi } from "vitest";

import type { LinkEntry, NoteDetail } from "../ipc/entity";
import type { SearchResponse } from "../ipc/search";

/**
 * The note editor, driven the way a person drives it.
 *
 * The autosave is real — a `setTimeout` — so every test passes
 * `saveAfterMs: 0` and then lets the macrotask queue turn. Faking the clock
 * instead would test the fake.
 */

// ---------------------------------------------------------------- the backend

const saves: { noteId: string; title: string; bodyMd: string }[] = [];
const deletes: string[] = [];
const unlinks: string[] = [];
let saveFails: unknown = null;
let stored: NoteDetail;

function detail(over: Partial<NoteDetail> = {}): NoteDetail {
  return {
    note: {
      id: "note:7f2c",
      title: "SEPA retry investigation",
      body_md: "the counter starts at zero",
      created_at: "2026-08-22T11:48:00Z",
      updated_at: "2026-08-22T14:30:00Z",
    },
    refs: [],
    links: [],
    ...over,
  };
}

vi.mock("../ipc/entity", () => ({
  getNote: async () => stored,
  saveNote: async (noteId: string, title: string, bodyMd: string) => {
    saves.push({ noteId, title, bodyMd });
    if (saveFails) throw saveFails;
    stored = { ...stored, note: { ...stored.note, title, body_md: bodyMd } };
    return stored;
  },
  deleteNote: async (noteId: string) => {
    deletes.push(noteId);
    return true;
  },
  unlink: async (linkId: string) => {
    unlinks.push(linkId);
  },
}));

const ANSWER: SearchResponse = {
  interpreted: {
    text: "pay",
    prefix: null,
    filters: { sources: [], kinds: [], authors: [], updated_within_days: null, mine: false },
    unknown_tokens: [],
  },
  groups: [
    {
      kind: "ticket",
      label: "Ticket",
      plural: "Tickets",
      monogram: "TI",
      total: 1,
      hits: [
        {
          entity_id: "mock:PAY-231",
          kind: "ticket",
          source_id: "mock",
          title: "Payments retry storm",
          updated_at: "2026-08-22T11:48:00Z",
          synced_at: "2026-08-22T14:30:00Z",
          rank: 1,
          snippet: [],
        },
      ],
    },
  ],
  total: 1,
  took_ms: 3,
};

// `noFilters` is re-exported through the `../ipc` barrel and `Session` calls
// it on every query, so a mock of this module that omits it takes the search
// down inside the session rather than in the component (the same reason
// `LinkDialog.test.svelte.ts` declares it).
vi.mock("../ipc/search", () => ({
  search: async () => ANSWER,
  launcherHome: async () => ({ smart_lists: [], recent: [] }),
  noFilters: () => ({ sources: [], kinds: [], updated_within_days: null, mine: false, authors: [] }),
}));

const toasts: string[] = [];
vi.mock("../shell/toasts.svelte", () => ({
  push: (toast: { text: string }) => toasts.push(toast.text),
}));

const { default: NoteView } = await import("./NoteView.svelte");

beforeEach(() => {
  saves.length = 0;
  deletes.length = 0;
  unlinks.length = 0;
  toasts.length = 0;
  saveFails = null;
  stored = detail();
});

// ------------------------------------------------------------------ the harness

/** Let every queued promise and zero-delay timer run. */
async function settle() {
  for (let turn = 0; turn < 6; turn += 1) {
    await new Promise((resolve) => setTimeout(resolve, 0));
    flushSync();
  }
}

function render(saveAfterMs = 0) {
  const target = document.createElement("div");
  document.body.append(target);
  const onclose = vi.fn();
  const onnavigate = vi.fn();
  const app = mount(NoteView, {
    target,
    props: {
      entityId: "note:7f2c",
      contextLabel: "All work",
      onclose,
      onnavigate,
      saveAfterMs,
    },
  });
  flushSync();
  return {
    target,
    onclose,
    onnavigate,
    text: () => (target.textContent ?? "").replace(/\s+/g, " "),
    button: (label: string) =>
      [...target.querySelectorAll<HTMLButtonElement>("button")].find(
        (node) => node.textContent?.trim() === label,
      ),
    title: () => target.querySelector<HTMLInputElement>("input.name")!,
    editor: () => target.querySelector<HTMLTextAreaElement>("textarea"),
    options: () => [...target.querySelectorAll<HTMLButtonElement>('[role="option"]')],
    /** Enter edit mode and type `text` into the body, caret at the end. */
    async type(text: string) {
      this.button("Edit")?.click();
      flushSync();
      const area = this.editor()!;
      area.value = text;
      area.setSelectionRange(text.length, text.length);
      area.dispatchEvent(new Event("input", { bubbles: true }));
      flushSync();
      await settle();
    },
    done: () => {
      unmount(app);
      target.remove();
    },
  };
}

// -------------------------------------------------------------------- the tests

/** Stories 1 and 3: the note is there to read and to edit. */
test("a note opens showing what was written in it", async () => {
  const screen = render();
  await settle();
  expect(screen.title().value).toBe("SEPA retry investigation");
  expect(screen.text()).toContain("the counter starts at zero");
  screen.done();
});

/**
 * Story 2: no explicit save. A pause after the last keystroke is what writes,
 * so nothing typed can be lost to a closed window.
 */
test("typing saves after a pause, with no save button anywhere", async () => {
  const screen = render();
  await settle();
  expect(screen.button("Save")).toBeUndefined();

  await screen.type("the counter starts at zero, not one");
  expect(saves).toEqual([
    {
      noteId: "note:7f2c",
      title: "SEPA retry investigation",
      bodyMd: "the counter starts at zero, not one",
    },
  ]);
  screen.done();
});

/** Story 5: a note that changed subject can say so. */
test("renaming the note saves the new title", async () => {
  const screen = render();
  await settle();
  const field = screen.title();
  field.value = "Off-by-one in the retry counter";
  field.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
  await settle();

  expect(saves.at(-1)?.title).toBe("Off-by-one in the retry counter");
  screen.done();
});

/**
 * A failed save keeps what was typed and says so — the one thing an autosave
 * must never do is lose the text while claiming to have saved it.
 */
test("a save that fails says so and keeps the text", async () => {
  const screen = render();
  await settle();
  saveFails = { code: "internal", message: "the database went away" };

  await screen.type("something worth keeping");
  expect(screen.text()).toContain("the database went away");
  expect(screen.text()).toContain("what you typed is still here");
  expect(screen.editor()!.value).toBe("something worth keeping");
  screen.done();
});

/**
 * Story 6: `[[` is the launcher. Typing one opens the same picker over
 * everything knobas knows, and accepting writes the reference.
 */
test("typing [[ offers what knobas knows, and accepting writes the reference", async () => {
  const screen = render();
  await settle();

  await screen.type("off-by-one in [[pay");
  // The picker is the launcher's `Session`, so it carries the launcher's 90 ms
  // debounce. Waited out with a real timer armed after it, the way
  // `Launcher.test.svelte.ts` waits out the same one.
  await new Promise((resolve) => setTimeout(resolve, 120));
  await settle();
  const options = screen.options();
  expect(options).toHaveLength(1);
  expect(options[0]!.textContent).toContain("Payments retry storm");

  options[0]!.click();
  flushSync();
  await settle();

  expect(screen.editor()!.value).toBe("off-by-one in [[mock:PAY-231]]");
  expect(saves.at(-1)?.bodyMd).toBe("off-by-one in [[mock:PAY-231]]");
  screen.done();
});

/** A caret that is not inside a `[[` is not completing anything. */
test("ordinary typing offers nothing", async () => {
  const screen = render();
  await settle();
  await screen.type("just prose about the retry counter");
  await new Promise((resolve) => setTimeout(resolve, 120));
  await settle();
  expect(screen.options()).toHaveLength(0);
  screen.done();
});

/**
 * Story 11 from this end: the note's own panel lists what it points at and
 * what points at it — the same panel #53 built, filled by the same read.
 */
test("the note draws its links panel from what the read returned", async () => {
  const entry: LinkEntry = {
    link: {
      id: "11111111-1111-4111-8111-111111111111",
      from_id: "note:7f2c",
      to_id: "mock:PAY-231",
      relation: "references",
      origin: "implied",
      note: null,
      created_by: "user",
      created_at: "2026-08-22T14:30:00Z",
    },
    other: {
      entity_id: "mock:PAY-231",
      kind: "ticket",
      title: "Payments retry storm",
      deleted_at: null,
    },
  };
  stored = detail({ links: [entry] });
  const screen = render();
  await settle();
  expect(screen.text()).toContain("Payments retry storm");
  screen.done();
});

/**
 * Story 2's keyboard corner: Escape closes through the shell's `window`
 * handler and unmounts the focused field, and the DOM fires no blur for an
 * element that is removed -- so the pause that would have written never
 * comes. The teardown flushes a pending timer instead of dropping it, or
 * everything typed since the last flush would go with the window.
 */
test("closing while a pause is still pending writes what was typed", async () => {
  const screen = render(60_000);
  await settle();

  await screen.type("the counter starts at zero, and the fix is [[mock:PAY-231]]");
  expect(saves, "the pause has not elapsed yet").toEqual([]);

  screen.done();
  expect(saves).toEqual([
    {
      noteId: "note:7f2c",
      title: "SEPA retry investigation",
      bodyMd: "the counter starts at zero, and the fix is [[mock:PAY-231]]",
    },
  ]);
});

/**
 * A `[[ref]]` link is *derived*: unlinking one would be undone by the next
 * save, silently. The panel says where the link comes from instead.
 */
test("unlinking a reference is refused, with the reason", async () => {
  const entry: LinkEntry = {
    link: {
      id: "11111111-1111-4111-8111-111111111111",
      from_id: "note:7f2c",
      to_id: "mock:PAY-231",
      relation: "references",
      origin: "implied",
      note: null,
      created_by: "user",
      created_at: "2026-08-22T14:30:00Z",
    },
    other: {
      entity_id: "mock:PAY-231",
      kind: "ticket",
      title: "Payments retry storm",
      deleted_at: null,
    },
  };
  stored = detail({ links: [entry] });
  const screen = render();
  await settle();

  screen.button("Unlink")?.click();
  await settle();

  expect(unlinks).toEqual([]);
  expect(toasts.join(" ")).toContain("Remove the reference");
  screen.done();
});

/**
 * The corner the origin check alone misses: a link drawn *by hand* under the
 * ref's own relation is the row the ref rides on (`reconcile_refs`'s
 * `on conflict ... do nothing`), so while the body still names the target,
 * unlinking it is the same silent undo one save later. A hand-drawn
 * `references` link to something the body does not name is the user's own,
 * and unlinks like any other.
 */
test("a hand-drawn link the body still names is refused; one it does not name unlinks", async () => {
  const ridden: LinkEntry = {
    link: {
      id: "22222222-2222-4222-8222-222222222222",
      from_id: "note:7f2c",
      to_id: "mock:PAY-231",
      relation: "references",
      origin: "manual",
      note: "drawn in the panel",
      created_by: "user",
      created_at: "2026-08-22T14:30:00Z",
    },
    other: {
      entity_id: "mock:PAY-231",
      kind: "ticket",
      title: "Payments retry storm",
      deleted_at: null,
    },
  };
  const unridden: LinkEntry = {
    link: {
      id: "33333333-3333-4333-8333-333333333333",
      from_id: "note:7f2c",
      to_id: "mock:OTHER-9",
      relation: "references",
      origin: "manual",
      note: null,
      created_by: "user",
      created_at: "2026-08-22T14:30:00Z",
    },
    other: {
      entity_id: "mock:OTHER-9",
      kind: "ticket",
      title: "Unrelated ticket",
      deleted_at: null,
    },
  };
  stored = detail({
    note: { ...detail().note, body_md: "see [[mock:PAY-231]]" },
    refs: [
      {
        target_id: "mock:PAY-231",
        target: { entity_id: "mock:PAY-231", kind: "ticket", title: "Payments retry storm", deleted_at: null },
      },
    ],
    links: [ridden, unridden],
  });
  const screen = render();
  await settle();

  const buttons = [...screen.target.querySelectorAll<HTMLButtonElement>("button")].filter(
    (button) => button.textContent?.trim() === "Unlink",
  );
  expect(buttons).toHaveLength(2);

  buttons[0]!.click();
  await settle();
  expect(unlinks, "the ridden row was never tombstoned").toEqual([]);
  expect(toasts.join(" ")).toContain("Remove the reference");

  buttons[1]!.click();
  await settle();
  expect(unlinks).toEqual(["33333333-3333-4333-8333-333333333333"]);

  screen.done();
});

/**
 * Story 4, with the one guard a reversible action does not get: deleting a
 * note deletes what the user wrote, and nothing re-fetches it.
 */
test("delete asks once before it happens", async () => {
  const screen = render();
  await settle();

  screen.button("Delete")!.click();
  flushSync();
  expect(deletes).toEqual([]);
  expect(screen.text()).toContain("Really delete?");

  screen.button("Really delete?")!.click();
  await settle();
  expect(deletes).toEqual(["note:7f2c"]);
  expect(screen.onclose).toHaveBeenCalled();
  screen.done();
});

/**
 * Every chip is drawn from the backend's `refs`, so this is the editor's half
 * of story 10: what could not be resolved is on screen, named.
 */
test("an unresolved reference in the body is visible as unresolved", async () => {
  stored = detail({
    note: { ...detail().note, body_md: "see [[mock:NOPE-1]]" },
    refs: [{ target_id: "mock:NOPE-1", target: null }],
  });
  const screen = render();
  await settle();
  expect(screen.text()).toContain("mock:NOPE-1");
  expect(screen.text()).toContain("unresolved");
  screen.done();
});
