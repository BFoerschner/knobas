/**
 * `?fake-ipc`: Notes (#46): create, read, save and delete, stateful within the
 * session; one note is there from the start (#1504).
 */
import type { LinkEnd, NoteLinkInput } from "../../../ipc/entity";
import type { Handlers } from "../fake-tauri";
import { keepSeed } from "../fake-seed";
import { CORPUS, FAKE_CONTEXTS, SYNCED_AT } from "./corpus";

/* ----------------------------------------------------------------- notes */

/**
 * The notes this session has written, by id. **Fixture only.**
 *
 * A note is the one kind knobas owns rather than mirrors (#46), so there is
 * nothing in `CORPUS` to answer from: the browser walk that shows a pasted URL
 * turning into a chip (#497), and the one that presses *New note* and reads the
 * links panel (#502), both need somewhere to write one. Stateful within the
 * session, like `FAKE_CONTEXTS` in `corpus.ts` and for the same reason — a walk that
 * made a note and then could not read it back would show a bug this app does
 * not have.
 */
const NOTES = new Map<
  string,
  { title: string; body_md: string; created_at: string; born: NoteLinkInput[] }
>();

/**
 * `create_note`, minting an id the way the backend's `note:<uuid>` does, and
 * keeping the links the note was **born with** (#502). **Fixture only.**
 *
 * Kept rather than drawn: there is no link table here, so what this can show is
 * the one thing the walk is looking at — that the room and the foreground reach
 * the command as two links, and that the panel then draws a chip for each.
 * Whether the rows exist, whether they make the note a member of the context,
 * and what happens to a target no row ever carried are questions only a
 * database can answer, and `crates/knobas-app/tests/it/entity.rs` asks them there.
 */
function createNote(args: Record<string, unknown>) {
  const id = `note:${Math.random().toString(16).slice(2, 6)}`;
  const born = Array.isArray(args["links"]) ? (args["links"] as NoteLinkInput[]) : [];
  // The title and body the caller passed, and not two empty strings: *New
  // note* sends neither, but a **capture** creates its note on the first
  // keystroke and sends both (#503), and a fixture that dropped them would
  // draw an empty note over a walk whose whole point is what was typed.
  NOTES.set(id, {
    title: String(args["title"] ?? ""),
    body_md: String(args["bodyMd"] ?? ""),
    created_at: SYNCED_AT,
    born,
  });
  return noteDetail(id);
}

/**
 * Put a note in this session's store unless it is there already: the
 * backend's get-or-create for a note another command owns (the standup
 * protocol, #829), which then reads and saves through `get_note` and
 * `save_note` like any other. **Fixture only.**
 */
export function seedNote(id: string, title: string, bodyMd: string, born: NoteLinkInput[] = []): void {
  if (!NOTES.has(id)) NOTES.set(id, { title, body_md: bodyMd, created_at: SYNCED_AT, born });
}

/**
 * The one note this fixture holds from the start (#1504), so the note page
 * has something to draw under `?fake-notes-shown`: two `[[refs]]` the corpus
 * resolves and one it does not, the `captured-from` link a capture is born
 * with (its `captured-in` names a context, and this fixture's start with
 * none), and a page linked by hand.
 */
export const FIXTURE_NOTE_ID = "note:retry-backoff";
seedNote(
  FIXTURE_NOTE_ID,
  "Retry backoff, thinking aloud",
  [
    "The retry window in [[mock:PAY-231]] has to be idempotent before the scheduler goes back on.",
    "",
    "- jittered backoff, capped at 6 attempts",
    "- the 409 from the second bank: [[mock:PAY-228]] fixes it",
    "- ask about [[mock:PAY-999]] on Monday",
  ].join("\n"),
  [
    { target_id: "mock:PAY-231", relation: "captured-from" },
    { target_id: "confluence:sepa", relation: "related" },
  ],
);
keepSeed(NOTES);

/** `delete_note`: the body goes; `false` when there was nothing left to delete, as the command answers. */
function deleteNote(args: Record<string, unknown>) {
  return NOTES.delete(String(args["noteId"] ?? ""));
}

/** `get_note`. A note this session did not write is a `not_found`, as it is. */
function getNote(args: Record<string, unknown>) {
  const id = String(args["noteId"] ?? "");
  if (!NOTES.has(id)) {
    throw { code: "not_found", message: `there is no note ${id}`, source_id: null };
  }
  return noteDetail(id);
}

/** `save_note`: idempotent, and the answer carries the reconciled refs. */
function saveNote(args: Record<string, unknown>) {
  const id = String(args["noteId"] ?? "");
  const existing = NOTES.get(id);
  if (!existing) {
    throw { code: "not_found", message: `there is no note ${id}`, source_id: null };
  }
  NOTES.set(id, {
    ...existing,
    title: String(args["title"] ?? ""),
    body_md: String(args["bodyMd"] ?? ""),
  });
  return noteDetail(id);
}

/**
 * One note: its `[[refs]]` resolved against what this fixture holds, and the
 * links it was born with drawn the way the panel draws them. **Fixture only.**
 *
 * A **second, fixture-only** implementation of `knobas_core::note`'s
 * `parse_refs` and `reconcile_refs`, on the same terms as {@link resolveUrl}
 * below: the rule that decides what a reference *is* lives in Rust and is
 * tested against a real database, and there is no database here. What this owes
 * is the outcomes a walk has to be able to see — a body naming an entity comes
 * back with a chip to draw and a body naming nothing does not (#497), and a
 * note born in a room comes back with the links it was born with (#502).
 *
 * `notes/NoteView.test.svelte.ts` stands in for the same backend rule, and the
 * two deliberately do not share: this module is the dev harness, and
 * `shell/house-rules.test.ts` pins `App.svelte` as its only importer outside
 * `shell/dev/`. Lifting the scanner somewhere both could reach would put a
 * second `[[…]]` scanner into app code, which is what `notes/note-body.ts`
 * exists to refuse.
 */
function noteDetail(id: string) {
  const note = NOTES.get(id)!;
  const refs = [...note.body_md.matchAll(/\[\[([^\]]+)\]\]/g)].map((found) => {
    const targetId = found[1]!.trim();
    return { target_id: targetId, target: noteLinkEnd(targetId) };
  });
  const links = note.born
    .map((born) => ({ born, other: noteLinkEnd(born.target_id) }))
    .filter((drawn): drawn is { born: NoteLinkInput; other: LinkEnd } => drawn.other !== null)
    .map(({ born, other }, index) => ({
      link: {
        id: `link:born-${id}-${index}`,
        from_id: id,
        to_id: born.target_id,
        relation: born.relation,
        // `manual`, because that is what the command writes and because the
        // panel refuses to unlink an `implied` row with a message about a
        // `[[reference]]` — which would be false about a capture link.
        origin: "manual" as const,
        note: null,
        created_by: "user",
        created_at: SYNCED_AT,
        confirmed_at: SYNCED_AT,
        rule: null,
        rule_class: null,
        reason: null,
      },
      other,
    }));
  return {
    note: {
      id,
      title: note.title,
      body_md: note.body_md,
      created_at: note.created_at,
      updated_at: SYNCED_AT,
    },
    refs,
    links,
  };
}

/**
 * The other end of a note's ref or born link, out of whatever this fixture
 * holds — the mirror's corpus, or a context this session made.
 *
 * `null` where nothing carries the id — an unresolved ref, and for a born link
 * no link at all: the same failure direction the real command has.
 */
function noteLinkEnd(entityId: string): LinkEnd | null {
  const entry = CORPUS.find((candidate) => candidate.entity_id === entityId);
  if (entry) {
    return {
      entity_id: entityId,
      kind: entry.kind,
      title: entry.title,
      deleted_at: entry.deleted_at,
    };
  }
  const context = FAKE_CONTEXTS.find((candidate) => candidate.id === entityId);
  if (context) {
    return { entity_id: entityId, kind: "ctx", title: context.title, deleted_at: null };
  }
  return null;
}

/** This feature's commands. */
export function handlers(): Handlers {
  return {
    create_note: (args) => createNote(args),
    get_note: (args) => getNote(args),
    save_note: (args) => saveNote(args),
    delete_note: (args) => deleteNote(args),
  };
}
