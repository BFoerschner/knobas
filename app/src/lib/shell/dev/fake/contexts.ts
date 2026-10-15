/**
 * `?fake-ipc`: Stored contexts (#47): list, members, create and promote, and a room's
 * Assets tile. The store itself lives in `corpus.ts`, which `list_entities`
 * reads it from.
 */
import type { Handler } from "../fake-tauri";
import { CORPUS, FAKE_CONTEXTS, fakeContext, fakeMembers } from "./corpus";

/** This feature's commands. */
export function handlers(): Record<string, Handler> {
  return {
    // Contexts (#47). Stateful within the session, so the new-context tab and
    // *Promote* can be walked in QA; membership is empty because the fixture
    // has no link graph, and `list_entities` answers a context scope with an
    // empty page for the same reason.
    list_contexts: () => FAKE_CONTEXTS.slice(),
    context_members: (args) => fakeMembers(String(args["ctxId"] ?? "")),
    create_context: (args) => {
      const row = fakeContext("adhoc", String(args["title"] ?? "Untitled"), null);
      FAKE_CONTEXTS.unshift(row);
      return row;
    },
    promote_context: (args) => {
      const anchor = String(args["entityId"] ?? "");
      const existing = FAKE_CONTEXTS.find((row) => row.anchor_id === anchor);
      if (existing) return existing;
      const entry = CORPUS.find((candidate) => candidate.entity_id === anchor);
      if (!entry) {
        throw { code: "not_found", message: `${anchor} is not in the mirror`, source_id: null };
      }
      const row = fakeContext("ticket", entry.title, anchor);
      FAKE_CONTEXTS.unshift(row);
      return row;
    },
    // The room's Assets tile (#434). Empty for `context_members`' reason: the
    // fixture has no link graph, so no context holds anything and membership
    // -- assets included -- is honestly nothing. A stored room under
    // `?fake-ipc` therefore draws the tile's empty state rather than a red
    // "command not found".
    context_assets: () => [],
  };
}
