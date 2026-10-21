/**
 * `?fake-ipc`: Stored contexts (#47): list and promote. The store itself lives
 * in `corpus.ts`, which `list_entities` reads it from.
 */
import type { Handler } from "../fake-tauri";
import { CORPUS, FAKE_CONTEXTS, fakeContext } from "./corpus";

/** This feature's commands. */
export function handlers(): Record<string, Handler> {
  return {
    // Contexts (#47). Stateful within the session, so *Promote* can be walked
    // in QA; `list_entities` answers a context scope with an empty page
    // because the fixture has no link graph.
    list_contexts: () => FAKE_CONTEXTS.slice(),
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
  };
}
