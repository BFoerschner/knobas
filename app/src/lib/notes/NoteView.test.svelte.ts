import { flushSync, mount, unmount } from "svelte";
import { beforeEach, expect, test, vi } from "vitest";

import type { LinkEnd, LinkEntry, NoteDetail } from "../ipc/entity";
import type { SearchResponse } from "../ipc/search";
import { paste } from "../shell/test-paste";

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

/**
 * The mirror, as the resolver sees it: the URLs it holds, and what they name
 * (#496). One entry, because the question a paste asks is binary.
 *
 * Keyed on the URL **as pasted**, fragment and all. The normalisation that
 * makes `#comment-42` irrelevant is the resolver's own — one rule, written
 * once as SQL in `knobas_core::web_url`, applied to both sides and checked
 * against a real database in `crates/knobas-app/tests/it/url_resolve.rs`. A
 * fixture that re-implemented it here would be a second spelling of it, in a
 * second language, that nothing forces to agree. What this file is entitled to
 * check is the frontend's own half: that what reaches the resolver is what the
 * reader pasted, verbatim.
 */
const MIRROR: Record<string, { entity_id: string; kind: string }> = {
  "https://jira.example/browse/PAY-231#comment-42": { entity_id: "mock:PAY-231", kind: "ticket" },
};

/** Every URL `resolve_url` was asked about, in order. */
const resolved: string[] = [];

/**
 * Set while a test wants the resolver's answer to still be in flight.
 *
 * The two things the swap is guarded against — the reader typing on, and the
 * editor closing — are both *during the round trip*, and a resolver that
 * answers on the next microtask has no during.
 */
let held: Promise<void> | null = null;
let release: () => void = () => {};

/** Set while a test wants `resolve_url` to reject rather than answer. */
let refuses = false;

function hold() {
  held = new Promise((resolve) => {
    release = () => {
      held = null;
      resolve();
    };
  });
}

/**
 * What the save reconciles the body's `[[refs]]` to — `note::reconcile_refs`,
 * standing in for it.
 *
 * The panel redraws its chips from the answer to the save rather than from a
 * read of its own, so a fixture that returned the refs it was opened with
 * would show a note whose body says one thing and whose chips say another —
 * which is exactly the mismatch a paste-to-reference has to be checked
 * against.
 *
 * `shell/dev/fake-tauri.ts` spells the same rule for the browser walk, and the
 * two deliberately do not share — see the note on `noteDetail` there for why
 * neither the dev harness nor a shared module is somewhere this can live.
 */
function refsOf(bodyMd: string): NoteDetail["refs"] {
  return [...bodyMd.matchAll(/\[\[([^\]]+)\]\]/g)].map((found) => {
    const targetId = found[1]!.trim();
    const known = KNOWN[targetId];
    return { target_id: targetId, target: known ?? null };
  });
}

/** The corpus the mirror holds, by entity id — what a chip draws from. */
const KNOWN: Record<string, LinkEnd> = {
  "mock:PAY-231": {
    entity_id: "mock:PAY-231",
    kind: "ticket",
    title: "Payments retry storm",
    deleted_at: null,
  },
};

vi.mock("../ipc/entity", () => ({
  // The ticket detail's status select (#179) reads the granted board and
  // queues through the write queue. Not what this file is about, so both
  // answer with nothing.
  miniBoard: () => Promise.resolve({ columns: [], sources: [] }),
  submitWrite: () => Promise.reject(new Error("no write in this test")),
  resolveUrl: async (url: string) => {
    resolved.push(url);
    if (held) await held;
    if (refuses) throw { code: "internal", message: "the mirror is not readable", source_id: null };
    return MIRROR[url] ?? null;
  },
  getNote: async () => stored,
  saveNote: async (noteId: string, title: string, bodyMd: string) => {
    saves.push({ noteId, title, bodyMd });
    if (saveFails) throw saveFails;
    stored = { ...stored, note: { ...stored.note, title, body_md: bodyMd }, refs: refsOf(bodyMd) };
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
          path: null,
          rank: 1,
          snippet: [],
        },
      ],
    },
  ],
  total: 1,
  took_ms: 3,
  coverage: [],
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
  resolved.length = 0;
  held = null;
  refuses = false;
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
    /** Paste into the body at the caret, and answer whether it was handled. */
    async paste(text: string) {
      const handled = paste(this.editor()!, text);
      flushSync();
      await settle();
      return handled;
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
      // Confirmed, as everything `entries_of` can return is: these fixtures
      // ride the panel's read, which is `knobas.confirmed_link` since 0007.
      confirmed_at: "2026-08-22T14:30:00Z",
      rule: null,
      rule_class: null,
      reason: null,
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
      // Confirmed, as everything `entries_of` can return is: these fixtures
      // ride the panel's read, which is `knobas.confirmed_link` since 0007.
      confirmed_at: "2026-08-22T14:30:00Z",
      rule: null,
      rule_class: null,
      reason: null,
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
      // Confirmed, as everything `entries_of` can return is: these fixtures
      // ride the panel's read, which is `knobas.confirmed_link` since 0007.
      confirmed_at: "2026-08-22T14:30:00Z",
      rule: null,
      rule_class: null,
      reason: null,
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
      // Confirmed, as everything `entries_of` can return is: these fixtures
      // ride the panel's read, which is `knobas.confirmed_link` since 0007.
      confirmed_at: "2026-08-22T14:30:00Z",
      rule: null,
      rule_class: null,
      reason: null,
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

/* ------------------------------------------------ pasting a URL (#497) */

/**
 * Story 12: a source URL pasted into a note body is the entity, so the paste
 * writes the reference rather than the address — and from there the existing
 * chip pipeline draws it and the backlink exists.
 *
 * The pasted URL carries a fragment, because a link out of chat usually does
 * and dropping it is half of what the resolver's normalisation is for (#496).
 */
test("a pasted URL the mirror holds becomes the entity's reference", async () => {
  const screen = render();
  await settle();
  await screen.type("caused by ");

  const handled = await screen.paste("https://jira.example/browse/PAY-231#comment-42");
  expect(handled, "the platform's own paste would have written the URL too").toBe(true);
  expect(resolved).toEqual(["https://jira.example/browse/PAY-231#comment-42"]);

  const area = screen.editor()!;
  expect(area.value).toBe("caused by [[mock:PAY-231]]");
  // The caret is past the reference, so the next word is typed after the chip
  // and not inside it.
  expect(area.selectionStart).toBe("caused by [[mock:PAY-231]]".length);
  expect(area.selectionEnd).toBe(area.selectionStart);

  // What the backend was asked to save, and what it reconciled.
  expect(saves.at(-1)?.bodyMd).toBe("caused by [[mock:PAY-231]]");
  expect(stored.refs).toEqual([
    {
      target_id: "mock:PAY-231",
      target: {
        entity_id: "mock:PAY-231",
        kind: "ticket",
        title: "Payments retry storm",
        deleted_at: null,
      },
    },
  ]);

  // And the chip is what the reader sees, drawn by the pipeline #46 built.
  screen.button("Done")?.click();
  flushSync();
  expect(screen.text()).toContain("Payments retry storm");
  screen.done();
});

/**
 * Story 12's other half. A link to something the mirror does not hold is
 * still a link the writer meant to keep, so it stays exactly as pasted — the
 * miss is never a swallowed paste.
 */
test("a pasted URL the mirror does not hold stays the URL, as text", async () => {
  const screen = render();
  await settle();
  await screen.type("see also ");

  await screen.paste("https://jira.example/browse/NOPE-9");

  const area = screen.editor()!;
  expect(area.value).toBe("see also https://jira.example/browse/NOPE-9");
  expect(area.selectionStart).toBe(area.value.length);
  expect(saves.at(-1)?.bodyMd).toBe("see also https://jira.example/browse/NOPE-9");
  // Nothing became a reference, so nothing is a chip.
  expect(stored.refs).toEqual([]);
  screen.done();
});

/**
 * The gate in front of the resolver, from the direction that costs something:
 * a paste that is not an absolute URL is an ordinary paste, and an ordinary
 * paste is the platform's. Preventing one would put this component in charge
 * of every clipboard in the editor — line endings, selections and all — for a
 * question it had already answered "no" to.
 */
test("pasting something that is not a URL is left to the platform", async () => {
  const screen = render();
  await settle();
  await screen.type("");

  const handled = await screen.paste("the counter starts at zero");
  expect(handled).toBe(false);
  expect(resolved).toEqual([]);
  screen.done();
});

/**
 * The swap happens a round trip after the paste, and in that gap the reader
 * owns the body. Writing on is the ordinary thing to do there, and the words
 * that follow the link have to survive the swap — the reference replaces the
 * URL, not the sentence around it.
 */
test("writing on through the round trip keeps the words and still gets the reference", async () => {
  const screen = render();
  await settle();
  await screen.type("caused by ");

  hold();
  await screen.paste("https://jira.example/browse/PAY-231#comment-42");
  const area = screen.editor()!;
  expect(area.value).toBe("caused by https://jira.example/browse/PAY-231#comment-42");

  // The reader keeps writing while the mirror is still being asked.
  area.value = `${area.value}, and again at noon`;
  area.setSelectionRange(area.value.length, area.value.length);
  area.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();

  release();
  await settle();
  expect(area.value).toBe("caused by [[mock:PAY-231]], and again at noon");
  expect(saves.at(-1)?.bodyMd).toBe("caused by [[mock:PAY-231]], and again at noon");
  // And the caret is still where the reader left it — at the end of the words
  // they were writing, not pulled back to the end of the reference. A swap
  // that moved it would put their next keystroke in the middle of the
  // sentence.
  expect(area.selectionStart, "the swap pulled the caret out of the sentence").toBe(
    area.value.length,
  );
  expect(area.selectionEnd).toBe(area.selectionStart);
  screen.done();
});

/**
 * The third way the mirror can decline to turn a URL into a reference, after
 * "not a URL" and "not in the mirror": the read itself refused. The reader is
 * owed the same thing in all three — the link they pasted, where they pasted
 * it — and a resolver that failed must never swallow a paste.
 */
test("a resolver that refuses leaves the pasted URL in the body", async () => {
  const screen = render();
  await settle();
  await screen.type("caused by ");

  refuses = true;
  await screen.paste("https://jira.example/browse/PAY-231#comment-42");

  const area = screen.editor()!;
  expect(resolved).toEqual(["https://jira.example/browse/PAY-231#comment-42"]);
  expect(area.value).toBe("caused by https://jira.example/browse/PAY-231#comment-42");
  expect(area.selectionStart).toBe(area.value.length);
  expect(saves.at(-1)?.bodyMd).toBe("caused by https://jira.example/browse/PAY-231#comment-42");
  screen.done();
});

/**
 * The edit the swap must not make. A reader who took the pasted URL back out
 * — selected it and typed over it, or deleted the line — has a body that no
 * longer holds what the mirror was asked about, and splicing a reference in at
 * the offset the URL used to be at would cut a hole in what replaced it. So
 * the swap asks whether the span still holds that URL, and lets the answer go
 * unused when it does not.
 */
test("an edit that takes the pasted URL back out leaves the answer unused", async () => {
  const screen = render();
  await settle();
  await screen.type("caused by ");

  hold();
  await screen.paste("https://jira.example/browse/PAY-231#comment-42");
  const area = screen.editor()!;

  // Second thoughts: the whole line goes.
  area.value = "caused by the retry counter";
  area.setSelectionRange(area.value.length, area.value.length);
  area.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();

  release();
  await settle();
  expect(area.value).toBe("caused by the retry counter");
  expect(saves.at(-1)?.bodyMd).toBe("caused by the retry counter");
  screen.done();
});

/**
 * The same gap, closed from the other side: *Done* unmounts the textarea, and
 * an answer that arrived after it must not write through a field the panel no
 * longer has. What was saved is what the reader left behind.
 */
test("leaving the editor mid-round-trip writes nothing back", async () => {
  const screen = render();
  await settle();
  await screen.type("caused by ");

  hold();
  await screen.paste("https://jira.example/browse/PAY-231#comment-42");
  screen.button("Done")?.click();
  flushSync();
  await settle();

  release();
  await settle();
  expect(screen.editor()).toBeNull();
  expect(saves.at(-1)?.bodyMd).toBe(
    "caused by https://jira.example/browse/PAY-231#comment-42",
  );
  screen.done();
});
