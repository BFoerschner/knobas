/** The note page's tokeniser (#1504), the rooms UI's cases kept. */
import { expect, test } from "vitest";

import type { LinkEnd, NoteRef } from "../../ipc/entity";
import { tokenise } from "./note-body";

const END: LinkEnd = { entity_id: "mock:PAY-231", kind: "ticket", title: "Retry failed SEPA payouts", deleted_at: null };
const ref = (target_id: string, target: LinkEnd | null = END): NoteRef => ({ target_id, target });

test("a reference the backend named is a token, and the text around it stays text", () => {
  expect(tokenise("see [[mock:PAY-231]], above", [ref("mock:PAY-231")])).toEqual([
    { kind: "text", text: "see " },
    { kind: "ref", text: "[[mock:PAY-231]]", ref: ref("mock:PAY-231") },
    { kind: "text", text: ", above" },
  ]);
});

test("an unresolved reference the backend listed is a token with no target", () => {
  expect(tokenise("[[mock:NOPE-1]]", [ref("mock:NOPE-1", null)])).toEqual([
    { kind: "ref", text: "[[mock:NOPE-1]]", ref: ref("mock:NOPE-1", null) },
  ]);
});

test("brackets the backend did not name are the text they are", () => {
  for (const body of ["an unclosed [[mock:PAY-231", "[[]] and [[   ]]", "[[mock:PAY-999]]"]) {
    expect(tokenise(body, []), body).toEqual([{ kind: "text", text: body }]);
  }
  expect(tokenise("[[mock:PAY-231]] but not [[mock:PAY-999]]", [ref("mock:PAY-231")])).toEqual([
    { kind: "ref", text: "[[mock:PAY-231]]", ref: ref("mock:PAY-231") },
    { kind: "text", text: " but not [[mock:PAY-999]]" },
  ]);
});

test("a reference written with spaces inside is the same reference", () => {
  expect(tokenise("[[ mock:PAY-231 ]]", [ref("mock:PAY-231")])[0]).toMatchObject({ kind: "ref" });
});
